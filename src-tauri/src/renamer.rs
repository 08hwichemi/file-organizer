//! 이름 한꺼번에 바꾸기(SPEC 3-4).
//!
//! - 폴더를 골라 그 안 파일(하위 폴더는 고를 때만) 이름을 한꺼번에 바꾼다. 확장자는 바꾸지 않는다.
//! - 바꾸는 방법: 첫 파일의 새 이름을 직접 적으면(엑셀 빠른 채우기처럼) 블록으로 알아듣고
//!   ([올해][글자 "년 "][지금 이름]) 나머지 파일에도 같은 블록을 쓴다. 블록은 화면에서 직접 고칠 수 있다.
//! - 명렬표(번호·이름 목록): 붙여 넣은 글을 그때그때 읽기만 하고 저장하지 않는다.
//! - 실행: 덮어쓰지 않는다(겹치면 `이름 (2).ext`). 서로 이름이 엇갈려도(a→b, b→a) 임시 이름을 거쳐 안전하게.
//!   바꾼 것은 모두 이동 기록에 같은 묶음 번호로 남아 한 번에 되돌린다. 파일을 지우는 코드는 없다.

use crate::commands::AppState;
use crate::{folders, history, mover, naming, search, watcher};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tauri::{AppHandle, Manager};

/// 한 번에 다루는 파일 수(이보다 많으면 앞쪽만)
pub const MAX_FILES: usize = 3000;
/// 하위 폴더를 읽는 깊이
const MAX_DEPTH: usize = 8;
/// Windows 경로 길이 한도(끝 글자 포함 260)
const MAX_PATH_CHARS: usize = 259;

// ───────────── 폴더 읽기 ─────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameFile {
    pub path: String,
    pub name: String,
    /// 고른 폴더 기준 하위 폴더 경로(바로 아래면 빈 문자열)
    pub sub_dir: String,
    /// 고친 시각(1970년부터 밀리초)
    pub modified: i64,
    pub size: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderFiles {
    pub folder: String,
    pub files: Vec<RenameFile>,
    /// 너무 많아 앞쪽만 읽었는지
    pub truncated: bool,
}

/// 고를 수 없는 폴더면 그 까닭
pub fn blocked_folder(path: &Path) -> Option<String> {
    if !path.is_dir() {
        return Some("폴더를 찾을 수 없어요.".into());
    }
    if path.parent().is_none() {
        return Some("드라이브 맨 위는 고를 수 없어요. 그 안의 폴더를 골라 주세요.".into());
    }
    if search::system_path(path) {
        return Some("Windows·프로그램 폴더 같은 시스템 폴더의 파일 이름은 바꿀 수 없어요.".into());
    }
    None
}

fn modified_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 폴더 안 파일(숨김·임시·저장 중인 파일 빼고, SPEC 5장 기준). `include_sub`면 하위 폴더까지.
pub fn list_files(folder: &Path, include_sub: bool) -> Result<FolderFiles, String> {
    if let Some(reason) = blocked_folder(folder) {
        return Err(reason);
    }
    let mut files = Vec::new();
    let mut truncated = false;
    let mut stack = vec![(folder.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if kind.is_symlink() || search::hidden_attr(&meta) {
                continue;
            }
            if kind.is_dir() {
                if include_sub && depth < MAX_DEPTH && !search::skip_dir(&name, false) {
                    stack.push((entry.path(), depth + 1));
                }
                continue;
            }
            if !kind.is_file()
                || !watcher::is_candidate_name(&name)
                || watcher::has_partial_partner(&entry.path())
                || watcher::is_decoy(&entry.path())
            {
                continue;
            }
            if files.len() >= MAX_FILES {
                truncated = true;
                break;
            }
            let path = entry.path();
            let sub_dir = dir
                .strip_prefix(folder)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            files.push(RenameFile {
                path: path.to_string_lossy().into_owned(),
                name,
                sub_dir,
                modified: modified_ms(&meta),
                size: meta.len(),
            });
        }
    }
    Ok(FolderFiles {
        folder: folder.to_string_lossy().into_owned(),
        files,
        truncated,
    })
}

// ───────────── 조각 나누기 ─────────────

/// 조각 종류
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Text,
    Number,
    Class,
    Year,
    Term,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub text: String,
    pub kind: Kind,
    /// 번호 조각의 값
    pub value: Option<u32>,
}

fn is_piece_separator(c: char) -> bool {
    c.is_whitespace() || matches!(c, '_' | '(' | ')' | '[' | ']' | '.' | ',' | '·' | '{' | '}')
}

fn all_digits(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

fn classify(text: &str) -> Token {
    let token = |kind, value| Token {
        text: text.to_string(),
        kind,
        value,
    };
    if naming::is_class_dash(text) {
        return token(Kind::Class, None);
    }
    if all_digits(text) {
        let n: Option<u32> = text.parse().ok();
        if text.len() == 4 && n.is_some_and(|y| (2000..=2099).contains(&y)) {
            return token(Kind::Year, None);
        }
        return token(Kind::Number, n);
    }
    if let Some(d) = text.strip_suffix('번').filter(|d| all_digits(d)) {
        return token(Kind::Number, d.parse().ok());
    }
    if matches!(text, "1학기" | "2학기") {
        return token(Kind::Term, None);
    }
    if text
        .strip_suffix("학년도")
        .or_else(|| text.strip_suffix('년'))
        .is_some_and(|d| all_digits(d) && (d.len() == 2 || d.len() == 4))
    {
        return token(Kind::Year, None);
    }
    token(Kind::Text, None)
}

/// 확장자를 뺀 이름을 조각으로: `_` 공백 괄호 점으로 나누고, `-`는 학급(`3-2`)일 때만 붙여 둔다(규칙 카드와 같은 기준).
pub fn tokens(stem: &str) -> Vec<Token> {
    let mut out = Vec::new();
    for raw in stem.split(is_piece_separator).filter(|s| !s.is_empty()) {
        if naming::is_class_dash(raw) {
            out.push(classify(raw));
            continue;
        }
        for part in raw.split('-').filter(|s| !s.is_empty()) {
            out.push(classify(part));
        }
    }
    out
}

/// 학년반번호 다섯 자리(30205)는 끝 두 자리가 번호
fn student_number(n: u32, digits: usize) -> u32 {
    if digits == 5 {
        n % 100
    } else {
        n
    }
}

// ───────────── 이름의 번호 ─────────────

/// 이름에 든 번호(예: `05` → 적힌 그대로 "05", 값 5)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameNumber {
    pub raw: String,
    pub value: u32,
}

fn make_number(raw: &str) -> Option<NameNumber> {
    raw.parse::<u32>().ok().map(|v| NameNumber {
        raw: raw.to_string(),
        value: student_number(v, raw.len()),
    })
}

/// 이름의 번호: 숫자만 된 조각(연도·학급·학기 빼고)이나 `5번`, 없으면 글자에 붙은 숫자(`과제05`).
/// 여럿이면 마지막. 학년반번호 다섯 자리(30205)는 끝 두 자리가 번호.
pub fn name_number(stem: &str) -> Option<NameNumber> {
    let toks = tokens(stem);
    if let Some(t) = toks.iter().rev().find(|t| t.kind == Kind::Number) {
        return make_number(t.text.trim_end_matches('번'));
    }
    for t in toks.iter().rev().filter(|t| t.kind == Kind::Text) {
        let chars: Vec<char> = t.text.chars().collect();
        let mut end = chars.len();
        while end > 0 {
            if !chars[end - 1].is_ascii_digit() {
                end -= 1;
                continue;
            }
            let mut start = end;
            while start > 0 && chars[start - 1].is_ascii_digit() {
                start -= 1;
            }
            let run: String = chars[start..end].iter().collect();
            let after: String = chars[end..].iter().collect();
            let year_like = run.len() == 4 && (2000..=2099).contains(&run.parse().unwrap_or(0));
            if !year_like && !after.starts_with("학기") && !after.starts_with("학년") {
                return make_number(&run);
            }
            end = start;
        }
    }
    None
}

// ───────────── 명렬표 ─────────────

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RosterInfo {
    /// 읽은 학생 수
    pub count: usize,
    /// 읽지 못한 줄 수(머리글 제외)
    pub ignored: usize,
    /// 두 번 나온 번호
    pub duplicates: Vec<u32>,
}

/// 번호 → 학생 이름. 저장하지 않는다(부를 때마다 붙여 넣은 글에서 읽는다).
pub struct Roster {
    pub names: HashMap<u32, String>,
    pub info: RosterInfo,
}

const HEADER_WORDS: &[&str] = &["번호", "이름", "성명", "학번", "성별", "번", "학생"];

/// 나이스·엑셀에서 복사한 "번호 이름" 두 칸(탭·쉼표·공백 구분, 순서 상관없음). 머리글 줄은 건너뛴다.
/// 학년반번호 다섯 자리(30205)는 끝 두 자리를 번호로.
pub fn parse_roster(text: &str) -> Roster {
    let mut names = HashMap::new();
    let mut info = RosterInfo::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = if line.contains('\t') {
            line.split('\t').collect()
        } else if line.contains(',') {
            line.split(',').collect()
        } else {
            line.split_whitespace().collect()
        };
        let fields: Vec<&str> = fields
            .iter()
            .map(|f| f.trim())
            .filter(|f| !f.is_empty())
            .collect();
        let number = fields
            .iter()
            .find(|f| all_digits(f.trim_end_matches('번')) && f.len() <= 6)
            .and_then(|f| {
                let d = f.trim_end_matches('번');
                d.parse::<u32>().ok().map(|n| student_number(n, d.len()))
            });
        let name = fields
            .iter()
            .find(|f| !f.chars().any(|c| c.is_ascii_digit()) && !HEADER_WORDS.contains(f));
        match (number, name) {
            (Some(n), Some(name)) if n > 0 => {
                if names.insert(n, name.to_string()).is_some() && !info.duplicates.contains(&n) {
                    info.duplicates.push(n);
                }
            }
            // 머리글(번호 칸 없음, 글자는 머리글 낱말뿐)
            (None, None) => {}
            (None, Some(_)) if fields.iter().all(|f| HEADER_WORDS.contains(f)) => {}
            _ => info.ignored += 1,
        }
    }
    info.count = names.len();
    Roster { names, info }
}

// ───────────── 블록: 새 이름을 이루는 칸 ─────────────
//
// 사용자는 첫 파일의 새 이름을 직접 적는다(예: `고다윤` → `2026년 고다윤`).
// 적은 이름을 첫 파일과 견줘 블록으로 알아듣고([올해][글자 "년 "][지금 이름]), 나머지 파일에 같은 블록을 쓴다.
// 블록은 화면에서 직접 고칠 수도 있다(그때는 고친 블록을 그대로 받는다).

/// 새 이름을 이루는 칸 하나
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "camelCase")]
pub enum Block {
    /// 적은 글자 그대로(모든 파일 같게)
    Text { text: String },
    /// 지금 이름. `from`이 있으면 그 글자를 `to`로 바꾼다(`to`가 없으면 지움).
    Name {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<String>,
    },
    /// 지금 이름의 i번째 조각(0부터)
    Piece { i: usize },
    /// 올해(예: 2026)
    Year,
    /// 오늘 날짜(예: 2026-10-16)
    Today,
    /// 파일 날짜(고친 날)
    FileDate,
    /// 이름의 번호. `plain`이면 앞의 0을 뺀다(05 → 5).
    Num { plain: bool },
    /// 순서 번호(보이는 순서대로 start부터 1씩, width 자리로 0 채움)
    Seq { start: u32, width: usize },
    /// 명렬표에서 이름의 번호로 찾은 학생 이름
    Student,
}

/// 오늘(올해·오늘 날짜 블록)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Today {
    pub year: String,
    pub date: String,
}

impl Today {
    pub fn now() -> Today {
        let now = chrono::Local::now();
        Today {
            year: now.format("%Y").to_string(),
            date: now.format("%Y-%m-%d").to_string(),
        }
    }
}

/// 이름을 지을 때 쓰는 파일 정보
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub name: String,
    pub stem: String,
    pub ext: String,
    pub pieces: Vec<String>,
    pub number: Option<NameNumber>,
    /// 고친 날(YYYY-MM-DD)
    pub date: String,
}

fn date_text(ms: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

impl FileInfo {
    pub fn new(name: &str, date: String) -> FileInfo {
        let (stem, ext) = split_ext(name);
        FileInfo {
            name: name.to_string(),
            stem: stem.to_string(),
            ext: ext.to_string(),
            pieces: tokens(stem).into_iter().map(|t| t.text).collect(),
            number: name_number(stem),
            date,
        }
    }

    fn read(path: &Path) -> FileInfo {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let date = std::fs::metadata(path)
            .map(|m| date_text(modified_ms(&m)))
            .unwrap_or_default();
        FileInfo::new(&name, date)
    }
}

fn lower_chars(s: &str) -> Vec<char> {
    s.chars().flat_map(char::to_lowercase).collect()
}

fn is_year_text(s: &str) -> bool {
    s.len() == 4 && all_digits(s) && (2000..=2099).contains(&s.parse().unwrap_or(0))
}

/// 첫 파일에 적은 새 이름을 블록으로 알아듣는다(첫 파일만 보고 — 테스트에서 알아듣는 모양을 볼 때).
#[cfg(test)]
fn infer(target: &str, first: &FileInfo, roster: &Roster, today: &Today) -> Vec<Block> {
    if target.trim().is_empty() || target == first.stem {
        return Vec::new();
    }
    edit_guess(target, first).unwrap_or_else(|| scan_guess(target, first, roster, today))
}

/// 파일 전체를 보고 알아듣는다. 일부만 지운 것은 두 가지로 읽힌다:
/// `면접자료_김태우_신소재공학` → `면접자료_김태우`는 "‘_신소재공학’ 글자 지우기"도, "첫·둘째 조각만 남기기"도 된다.
/// 학과처럼 파일마다 다른 조각을 지운 것이면 글자 지우기로는 첫 파일만 바뀌므로, 더 많은 파일이 바뀌는 쪽을 고른다
/// (같으면 글자 지우기: `_최종`처럼 같은 글자를 지운 경우).
pub fn infer_all(
    target: &str,
    files: &[FileInfo],
    roster: &Roster,
    today: &Today,
    list_word: &str,
) -> Vec<Block> {
    let Some(first) = files.first() else {
        return Vec::new();
    };
    if target.trim().is_empty() || target == first.stem {
        return Vec::new();
    }
    let scan = scan_guess(target, first, roster, today);
    let Some(edit) = edit_guess(target, first) else {
        return scan;
    };
    let changed = |blocks: &[Block]| {
        files
            .iter()
            .enumerate()
            .filter(|(i, f)| {
                render(blocks, f, *i, first, roster, today, list_word)
                    .is_ok_and(|s| !tidy(&s).is_empty() && tidy(&s) != f.stem)
            })
            .count()
    };
    if changed(&scan) > changed(&edit) {
        scan
    } else {
        edit
    }
}

/// 지금 이름에서 일부만 지웠거나, 대부분을 남기고 일부만 바꿨으면 → '지금 이름'에서 그 글자 지우기·바꾸기
fn edit_guess(target: &str, first: &FileInfo) -> Option<Vec<Block>> {
    if target.contains(first.stem.as_str()) {
        return None;
    }
    let t: Vec<char> = target.chars().collect();
    let s: Vec<char> = first.stem.chars().collect();
    let mut p = 0;
    while p < t.len() && p < s.len() && t[p] == s[p] {
        p += 1;
    }
    let mut q = 0;
    while q < t.len() - p && q < s.len() - p && t[t.len() - 1 - q] == s[s.len() - 1 - q] {
        q += 1;
    }
    let removed: String = s[p..s.len() - q].iter().collect();
    let inserted: String = t[p..t.len() - q].iter().collect();
    (!removed.is_empty() && (inserted.is_empty() || (p + q) * 2 >= s.len())).then(|| {
        vec![Block::Name {
            from: Some(removed),
            to: (!inserted.is_empty()).then_some(inserted),
        }]
    })
}

/// 적은 이름을 앞에서부터 읽으며 지금 이름의 조각·번호·날짜 등과 같은 부분을 블록으로 바꾼다.
fn scan_guess(target: &str, first: &FileInfo, roster: &Roster, today: &Today) -> Vec<Block> {
    let t: Vec<char> = target.chars().collect();
    // 알아볼 수 있는 값들(긴 것부터, 길이가 같으면 이 차례대로: 번호는 조각보다 먼저 — 모양이 다른 파일에도 쓸 수 있게)
    let mut sources: Vec<(Block, String)> = Vec::new();
    if let Some(name) = first
        .number
        .as_ref()
        .and_then(|n| roster.names.get(&n.value))
    {
        sources.push((Block::Student, name.clone()));
    }
    sources.push((
        Block::Name {
            from: None,
            to: None,
        },
        first.stem.clone(),
    ));
    if let Some(n) = &first.number {
        sources.push((Block::Num { plain: false }, n.raw.clone()));
        let plain = n.value.to_string();
        if plain != n.raw {
            sources.push((Block::Num { plain: true }, plain));
        }
    }
    if first.pieces.len() > 1 {
        for (i, p) in first.pieces.iter().enumerate() {
            sources.push((Block::Piece { i }, p.clone()));
        }
    }
    sources.push((Block::Year, today.year.clone()));
    sources.push((Block::Today, today.date.clone()));
    sources.push((Block::FileDate, first.date.clone()));
    sources.retain(|(_, text)| !text.is_empty());
    sources.sort_by_key(|(_, text)| std::cmp::Reverse(text.chars().count()));
    let sources: Vec<(Block, Vec<char>)> = sources
        .into_iter()
        .map(|(b, text)| (b, lower_chars(&text)))
        .collect();

    let lower: Vec<char> = t.iter().flat_map(|c| c.to_lowercase()).collect();
    // 소문자로 바꾸며 글자 수가 달라지는 드문 글자가 있으면 그냥 대소문자를 가린다
    let hay: Vec<char> = if lower.len() == t.len() {
        lower
    } else {
        t.clone()
    };
    let digitish = |b: &Block| {
        matches!(
            b,
            Block::Year | Block::Today | Block::FileDate | Block::Num { .. }
        )
    };
    let mut blocks: Vec<Block> = Vec::new();
    let mut literal = String::new();
    let mut i = 0;
    'scan: while i < t.len() {
        for (block, text) in &sources {
            let len = text.len();
            if i + len > t.len() || hay[i..i + len] != text[..] {
                continue;
            }
            if digitish(block) {
                let before = i > 0 && t[i - 1].is_ascii_digit();
                let after = i + len < t.len() && t[i + len].is_ascii_digit();
                if before || after {
                    continue;
                }
            }
            if !literal.is_empty() {
                blocks.push(Block::Text {
                    text: std::mem::take(&mut literal),
                });
            }
            blocks.push(block.clone());
            i += len;
            continue 'scan;
        }
        literal.push(t[i]);
        i += 1;
    }
    if !literal.is_empty() {
        blocks.push(Block::Text { text: literal });
    }

    // 남은 숫자(연도 모양 빼고)는 순서 번호로: 이름의 번호를 쓰지 않았을 때 한 번만
    if blocks.iter().any(|b| matches!(b, Block::Num { .. })) {
        return blocks;
    }
    let mut out = Vec::new();
    let mut converted = false;
    for b in blocks {
        let Block::Text { text } = &b else {
            out.push(b);
            continue;
        };
        if converted {
            out.push(b);
            continue;
        }
        // 마지막 숫자 덩어리
        let chars: Vec<char> = text.chars().collect();
        let Some(end) = chars
            .iter()
            .rposition(|c| c.is_ascii_digit())
            .map(|e| e + 1)
        else {
            out.push(b);
            continue;
        };
        let mut start = end;
        while start > 0 && chars[start - 1].is_ascii_digit() {
            start -= 1;
        }
        let run: String = chars[start..end].iter().collect();
        if is_year_text(&run) {
            out.push(b);
            continue;
        }
        let head: String = chars[..start].iter().collect();
        let tail: String = chars[end..].iter().collect();
        if !head.is_empty() {
            out.push(Block::Text { text: head });
        }
        out.push(Block::Seq {
            start: run.parse().unwrap_or(1),
            width: run.len(),
        });
        if !tail.is_empty() {
            out.push(Block::Text { text: tail });
        }
        converted = true;
    }
    out
}

/// 블록으로 파일 하나의 새 이름(확장자 빼고)을 짓는다. 지을 수 없으면 그 까닭.
pub fn render(
    blocks: &[Block],
    file: &FileInfo,
    index: usize,
    first: &FileInfo,
    roster: &Roster,
    today: &Today,
    list_word: &str,
) -> Result<String, String> {
    let mut out = String::new();
    for b in blocks {
        match b {
            Block::Text { text } => out.push_str(text),
            Block::Name { from, to } => match from.as_deref().filter(|f| !f.is_empty()) {
                Some(f) => out.push_str(&replace_ci(&file.stem, f, to.as_deref().unwrap_or(""))),
                None => out.push_str(&file.stem),
            },
            Block::Piece { i } => {
                if file.pieces.len() != first.pieces.len() {
                    return Err("이름 모양이 첫 파일과 달라요".into());
                }
                out.push_str(file.pieces.get(*i).ok_or("이름 모양이 첫 파일과 달라요")?);
            }
            Block::Year => out.push_str(&today.year),
            Block::Today => out.push_str(&today.date),
            Block::FileDate => out.push_str(&file.date),
            Block::Num { plain } => {
                let n = file.number.as_ref().ok_or("이름에 번호가 없어요")?;
                if *plain {
                    out.push_str(&n.value.to_string());
                } else {
                    out.push_str(&n.raw);
                }
            }
            Block::Seq { start, width } => {
                let n = start.saturating_add(index as u32);
                out.push_str(&format!("{n:0width$}"));
            }
            Block::Student => {
                let n = file
                    .number
                    .as_ref()
                    .ok_or(format!("이름에 번호가 없어 {list_word}에서 찾을 수 없어요"))?;
                if roster.names.is_empty() {
                    return Err(format!("{list_word}를 붙여 넣어야 해요"));
                }
                let name = roster
                    .names
                    .get(&n.value)
                    .ok_or(format!("{list_word}에 {}번이 없어요", n.value))?;
                out.push_str(name);
            }
        }
    }
    Ok(out)
}

/// 블록을 화면에 보여 줄 모양(첫 파일 기준): 종류 이름과 값
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockView {
    pub label: String,
    pub value: String,
}

fn view(b: &Block, first: &FileInfo, roster: &Roster, today: &Today, list_word: &str) -> BlockView {
    let v = |label: &str, value: String| BlockView {
        label: label.to_string(),
        value,
    };
    match b {
        Block::Text { text } => v("글자", format!("“{text}”")),
        Block::Name { from, to } => match from.as_deref().filter(|f| !f.is_empty()) {
            Some(f) => match to.as_deref().filter(|t| !t.is_empty()) {
                Some(t) => v("지금 이름", format!("‘{f}’ → ‘{t}’")),
                None => v("지금 이름", format!("‘{f}’ 지움")),
            },
            None => v("지금 이름", first.stem.clone()),
        },
        Block::Piece { i } => v(
            &format!("이름 조각 {}", i + 1),
            first.pieces.get(*i).cloned().unwrap_or_default(),
        ),
        Block::Year => v("올해", today.year.clone()),
        Block::Today => v("오늘 날짜", today.date.clone()),
        Block::FileDate => v("파일 날짜", first.date.clone()),
        Block::Num { plain } => v(
            "이름의 번호",
            match &first.number {
                Some(n) if *plain => n.value.to_string(),
                Some(n) => n.raw.clone(),
                None => "없음".into(),
            },
        ),
        Block::Seq { start, width } => v(
            "순서 번호",
            format!("{start:0width$}, {} …", start.saturating_add(1)),
        ),
        Block::Student => v(
            &format!("{list_word}의 이름"),
            first
                .number
                .as_ref()
                .and_then(|n| roster.names.get(&n.value).cloned())
                .unwrap_or_else(|| format!("{list_word} 필요")),
        ),
    }
}

/// 대소문자 무시로 `hay`가 `needle`로 시작하면 그만큼의 바이트 수
fn starts_with_ci(hay: &str, needle: &str) -> Option<usize> {
    let mut h = hay.char_indices();
    for nc in needle.chars() {
        let (_, hc) = h.next()?;
        if !hc.to_lowercase().eq(nc.to_lowercase()) {
            return None;
        }
    }
    Some(h.next().map(|(i, _)| i).unwrap_or(hay.len()))
}

/// 대소문자 무시 바꾸기
pub fn replace_ci(s: &str, find: &str, with: &str) -> String {
    if find.is_empty() {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if let Some(n) = starts_with_ci(&s[i..], find) {
            out.push_str(with);
            i += n;
        } else {
            let c = s[i..].chars().next().unwrap_or(' ');
            out.push(c);
            i += c.len_utf8();
        }
    }
    out
}

/// 겹친 공백·`_`는 하나로, 앞뒤의 공백·`_`·`-`는 뺀다.
pub fn tidy(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last: Option<char> = None;
    for c in s.chars() {
        let c = if c.is_whitespace() { ' ' } else { c };
        if (c == ' ' || c == '_') && last == Some(c) {
            continue;
        }
        out.push(c);
        last = Some(c);
    }
    out.trim_matches(|c: char| c == ' ' || c == '_' || c == '-')
        .to_string()
}

// ───────────── 미리보기 ─────────────

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RenameRequest {
    /// 바꿀 파일(체크한 것, 보이는 순서대로)
    pub files: Vec<String>,
    /// 첫 파일에 적은 새 이름(확장자 빼고)
    pub example: String,
    /// 블록을 직접 고쳤으면 그 블록(없으면 `example`로 알아듣는다)
    pub blocks: Option<Vec<Block>>,
    /// 붙여 넣은 명렬표(저장하지 않음)
    pub roster: String,
    /// 화면에서 부르는 이름: "명렬표" / "번호·이름 목록"
    pub list_word: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// 이름이 그대로
    Same,
    /// 바꿈
    Rename,
    /// 같은 이름이 있어 번호를 붙여 바꿈
    Conflict,
    /// 쓸 수 없는 이름이라 건너뜀
    Invalid,
    /// 필요한 값이 없어 건너뜀(명렬표에 없는 번호 등)
    Skip,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRow {
    pub path: String,
    pub old_name: String,
    /// 바꿀 이름(겹치면 번호까지 붙인 것). 건너뛰면 원래 이름.
    pub new_name: String,
    pub status: Status,
    pub note: Option<String>,
}

/// 첫 파일(블록 메뉴에 값을 보여 줄 때 씀)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstFile {
    pub name: String,
    pub stem: String,
    pub ext: String,
    /// 이름의 번호(적힌 그대로)
    pub number: Option<String>,
    pub file_date: String,
    pub today: String,
    pub year: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub blocks: Vec<Block>,
    pub views: Vec<BlockView>,
    pub first: Option<FirstFile>,
    /// 안내 한 줄(예: 학생 이름이면 명렬표를 붙여 넣으세요)
    pub hint: Option<String>,
    /// 명렬표 칸을 열어야 하는지
    pub open_roster: bool,
    pub roster: RosterInfo,
    pub rows: Vec<PreviewRow>,
    /// 실제로 바뀌는 파일 수
    pub changes: usize,
}

/// 같은 이름인지 견줄 때 쓰는 열쇠(Windows는 대소문자를 가리지 않는다)
fn name_key(name: &str) -> String {
    if cfg!(windows) {
        name.to_lowercase()
    } else {
        name.to_string()
    }
}

fn split_ext(name: &str) -> (&str, &str) {
    let stem = naming::stem(name);
    (stem, &name[stem.len()..])
}

/// 학생 이름처럼 보이는 글자(한글 2~4자)
fn looks_like_person(text: &str) -> Option<String> {
    let core: String = text
        .chars()
        .filter(|c| !matches!(c, '(' | ')' | '_' | ' ' | '-'))
        .collect();
    let n = core.chars().count();
    ((2..=4).contains(&n) && core.chars().all(|c| ('가'..='힣').contains(&c))).then_some(core)
}

/// 바꿀 이름을 모두 정한다(디스크는 읽기만).
pub fn plan(req: &RenameRequest, today: &Today) -> Preview {
    let list_word = if req.list_word.trim().is_empty() {
        "명렬표"
    } else {
        req.list_word.trim()
    };
    let paths: Vec<PathBuf> = req
        .files
        .iter()
        .take(MAX_FILES)
        .map(PathBuf::from)
        .collect();
    let infos: Vec<FileInfo> = paths.iter().map(|p| FileInfo::read(p)).collect();
    let roster = parse_roster(&req.roster);
    let Some(first) = infos.first() else {
        return Preview {
            blocks: Vec::new(),
            views: Vec::new(),
            first: None,
            hint: None,
            open_roster: false,
            roster: roster.info,
            rows: Vec::new(),
            changes: 0,
        };
    };
    let blocks = match &req.blocks {
        Some(b) => b.clone(),
        None => infer_all(&req.example, &infos, &roster, today, list_word),
    };
    let views = blocks
        .iter()
        .map(|b| view(b, first, &roster, today, list_word))
        .collect();

    // 안내: 학생 이름 블록인데 명렬표가 없거나, 학생 이름처럼 보이는 글자 + 번호
    let has_student = blocks.iter().any(|b| matches!(b, Block::Student));
    let has_number = blocks
        .iter()
        .any(|b| matches!(b, Block::Num { .. } | Block::Seq { .. }));
    let person = blocks.iter().find_map(|b| match b {
        Block::Text { text } => looks_like_person(text),
        _ => None,
    });
    let (hint, open_roster) = if has_student && roster.names.is_empty() {
        (
            Some(format!(
                "학생 이름은 {list_word}에서 번호로 찾아요. 아래에 {list_word}를 붙여 넣으세요."
            )),
            true,
        )
    } else if let (Some(p), true, Some(n), true) = (
        person,
        has_number,
        first.number.as_ref(),
        roster.names.is_empty(),
    ) {
        (
            Some(format!(
                "‘{p}’이(가) {}번 학생 이름이라면 {list_word}를 붙여 넣으세요. 다른 파일도 {list_word}에서 이름을 찾아 넣어요.",
                n.value
            )),
            true,
        )
    } else {
        (None, false)
    };

    // 1) 파일마다 새 이름(겹침은 아직 안 봄)
    let mut rows: Vec<PreviewRow> = Vec::new();
    for (i, (path, info)) in paths.iter().zip(&infos).enumerate() {
        let mut row = PreviewRow {
            path: path.to_string_lossy().into_owned(),
            old_name: info.name.clone(),
            new_name: info.name.clone(),
            status: Status::Same,
            note: None,
        };
        if blocks.is_empty() {
            rows.push(row);
            continue;
        }
        let built = match render(&blocks, info, i, first, &roster, today, list_word) {
            Ok(s) => tidy(&s),
            Err(note) => {
                row.status = Status::Skip;
                row.note = Some(note);
                rows.push(row);
                continue;
            }
        };
        let new_name = format!("{built}{}", info.ext);
        if built.is_empty() {
            row.status = Status::Invalid;
            row.note = Some("이름이 비어 있어요".into());
        } else if let Err(e) = folders::validate_name(&new_name) {
            row.status = Status::Invalid;
            row.note = Some(e);
        } else if new_name != info.name {
            let full = path.with_file_name(&new_name);
            if full.to_string_lossy().encode_utf16().count() > MAX_PATH_CHARS {
                row.status = Status::Invalid;
                row.note = Some("경로가 너무 길어요(260자 넘음)".into());
            } else {
                row.status = Status::Rename;
                row.new_name = new_name;
            }
        }
        rows.push(row);
    }

    // 2) 겹침: 폴더마다, 이름을 바꾸지 않는 파일·폴더의 이름과 앞서 정한 새 이름을 피해 (2) (3) …
    let moving: HashSet<PathBuf> = rows
        .iter()
        .filter(|r| r.status == Status::Rename)
        .map(|r| PathBuf::from(&r.path))
        .collect();
    let mut taken: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    for r in rows.iter_mut().filter(|r| r.status == Status::Rename) {
        let dir = Path::new(&r.path)
            .parent()
            .unwrap_or(Path::new(""))
            .to_path_buf();
        let set = taken.entry(dir.clone()).or_insert_with(|| {
            std::fs::read_dir(&dir)
                .map(|entries| {
                    entries
                        .flatten()
                        .filter(|e| !moving.contains(&e.path()))
                        .filter_map(|e| e.file_name().into_string().ok())
                        .map(|n| name_key(&n))
                        .collect()
                })
                .unwrap_or_default()
        });
        let (stem, ext) = {
            let (s, e) = split_ext(&r.new_name);
            (s.to_string(), e.to_string())
        };
        let mut candidate = r.new_name.clone();
        let mut n = 1;
        while set.contains(&name_key(&candidate)) {
            n += 1;
            candidate = format!("{stem} ({n}){ext}");
        }
        set.insert(name_key(&candidate));
        if n > 1 {
            r.status = Status::Conflict;
            r.note = Some(format!("같은 이름이 있어 ({n})을 붙여요"));
            r.new_name = candidate;
        }
    }
    let changes = rows
        .iter()
        .filter(|r| matches!(r.status, Status::Rename | Status::Conflict))
        .count();
    Preview {
        blocks,
        views,
        first: Some(FirstFile {
            name: first.name.clone(),
            stem: first.stem.clone(),
            ext: first.ext.clone(),
            number: first.number.as_ref().map(|n| n.raw.clone()),
            file_date: first.date.clone(),
            today: today.date.clone(),
            year: today.year.clone(),
        }),
        hint,
        open_roster,
        roster: roster.info,
        rows,
        changes,
    }
}

// ───────────── 실행 ─────────────

/// 임시 이름(`~`로 시작하고 `.tmp`로 끝나 감시·검색이 무시한다)
fn temp_name(i: usize) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("~이름바꾸는중-{nanos}-{i}.tmp")
}

/// 바꾼 결과 하나
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    pub from: PathBuf,
    pub to: PathBuf,
}

/// (지금 경로, 바꿀 이름)들을 차례로 바꾼다. 먼저 모두 임시 이름으로 옮긴 뒤 새 이름으로 바꿔
/// 서로 엇갈린 이름(a→b, b→a)도 덮어쓰지 않는다. 새 이름 자리가 그사이 찼으면 번호를 붙인다.
/// 열려 있는 파일 등 바꾸지 못한 것은 건너뛰고 까닭을 돌려준다.
pub fn execute(plans: &[(PathBuf, String)]) -> (Vec<Done>, Vec<String>) {
    let mut errors = Vec::new();
    let mut staged: Vec<(PathBuf, PathBuf, &str)> = Vec::new();
    for (i, (from, new_name)) in plans.iter().enumerate() {
        let name = from
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some(dir) = from.parent() else {
            continue;
        };
        match mover::move_into(from, dir, Some(&temp_name(i))) {
            Ok(temp) => staged.push((from.clone(), temp, new_name)),
            Err(e) => errors.push(format!("{name}: {e}")),
        }
    }
    let mut done = Vec::new();
    for (from, temp, new_name) in staged {
        let dir = temp.parent().unwrap_or(Path::new("")).to_path_buf();
        let name = from
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        match mover::move_into(&temp, &dir, Some(new_name)) {
            Ok(to) => done.push(Done { from, to }),
            Err(e) => {
                errors.push(format!("{name}: {e}"));
                // 원래 이름으로 돌려놓는다(그 자리가 찼으면 번호). 그것도 안 되면 임시 이름 그대로 기록해 되돌릴 수 있게.
                let to = mover::move_into(&temp, &dir, Some(&name)).unwrap_or(temp);
                if to != from {
                    done.push(Done { from, to });
                }
            }
        }
    }
    (done, errors)
}

// ───────────── 명령 ─────────────

fn db_err(e: rusqlite::Error) -> String {
    format!("기록을 저장하지 못했어요: {e}")
}

/// 폴더 안 파일 목록
#[tauri::command]
pub async fn rename_list(folder: String, include_sub: bool) -> Result<FolderFiles, String> {
    list_files(Path::new(&folder), include_sub)
}

/// 미리보기(디스크는 읽기만). 명렬표는 저장하지 않는다.
#[tauri::command]
pub async fn rename_preview(request: RenameRequest) -> Result<Preview, String> {
    Ok(plan(&request, &Today::now()))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameResult {
    /// 되돌리기용 묶음 번호
    pub batch: i64,
    pub renamed: usize,
    /// 바꾸지 못한 파일과 까닭
    pub errors: Vec<String>,
}

/// 이름 바꾸기 실행: 미리보기를 다시 계산해(그사이 바뀐 것 반영) 바꾸고, 같은 묶음으로 기록한다.
#[tauri::command]
pub async fn rename_apply(app: AppHandle, request: RenameRequest) -> Result<RenameResult, String> {
    let preview = plan(&request, &Today::now());
    let plans: Vec<(PathBuf, String)> = preview
        .rows
        .iter()
        .filter(|r| matches!(r.status, Status::Rename | Status::Conflict))
        .map(|r| (PathBuf::from(&r.path), r.new_name.clone()))
        .collect();
    let state = app.state::<AppState>();
    let (done, errors) = execute(&plans);
    // 바뀐 이름을 감시가 새 파일로 잡지 않게
    for d in &done {
        state.watcher.mark_known(d.to.clone());
    }
    let batch = {
        let conn = state
            .db
            .lock()
            .map_err(|_| "저장소를 열 수 없어요".to_string())?;
        let batch = history::next_batch(&conn).map_err(db_err)?;
        for d in &done {
            history::record_in_batch(&conn, &d.from, &d.to, batch).map_err(db_err)?;
            // 이 파일로 옮겼던 기록·정리함은 새 이름을 따라간다
            history::rewrite_file(&conn, &d.from, &d.to, batch).map_err(db_err)?;
            crate::inbox::rename_path(&conn, &d.from, &d.to).map_err(db_err)?;
        }
        batch
    };
    crate::commands::notify_inbox(&app);
    Ok(RenameResult {
        batch,
        renamed: done.len(),
        errors,
    })
}

/// 묶음 전체 되돌리기(엇갈린 이름도 임시 이름을 거쳐 원래대로)
#[tauri::command]
pub async fn rename_undo(
    app: AppHandle,
    batch: i64,
) -> Result<crate::commands::UndoManyResult, String> {
    let state = app.state::<AppState>();
    let records = {
        let conn = state
            .db
            .lock()
            .map_err(|_| "저장소를 열 수 없어요".to_string())?;
        history::batch_moves(&conn, batch).map_err(db_err)?
    };
    if records.is_empty() {
        return Err("되돌릴 기록을 찾을 수 없어요. 이미 되돌렸을 수 있어요.".into());
    }
    let plans: Vec<(PathBuf, String)> = records
        .iter()
        .map(|r| {
            let original = Path::new(&r.original_path);
            (
                PathBuf::from(&r.moved_path),
                original
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            )
        })
        .collect();
    let (done, errors) = execute(&plans);
    let conn = state
        .db
        .lock()
        .map_err(|_| "저장소를 열 수 없어요".to_string())?;
    for d in &done {
        state.watcher.mark_known(d.to.clone());
        if let Some(r) = records.iter().find(|r| Path::new(&r.moved_path) == d.from) {
            history::mark_undone(&conn, r.id).map_err(db_err)?;
            history::rewrite_file(&conn, &d.from, &d.to, batch).map_err(db_err)?;
            crate::inbox::rename_path(&conn, &d.from, &d.to).map_err(db_err)?;
        }
    }
    drop(conn);
    crate::commands::notify_inbox(&app);
    Ok(crate::commands::UndoManyResult {
        restored: done.len(),
        errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn today() -> Today {
        Today {
            year: "2026".into(),
            date: "2026-10-16".into(),
        }
    }

    fn info(name: &str) -> FileInfo {
        FileInfo::new(name, "2026-09-17".into())
    }

    fn no_roster() -> Roster {
        parse_roster("")
    }

    fn text(t: &str) -> Block {
        Block::Text { text: t.into() }
    }

    /// 첫 파일에 적은 이름으로 다른 파일들의 새 이름(확장자 포함)을 짓는다
    fn apply(names: &[&str], example: &str, roster: &Roster) -> Vec<Result<String, String>> {
        let files: Vec<FileInfo> = names.iter().map(|n| info(n)).collect();
        let blocks = infer_all(example, &files, roster, &today(), "명렬표");
        files
            .iter()
            .enumerate()
            .map(|(i, f)| {
                render(&blocks, f, i, &files[0], roster, &today(), "명렬표")
                    .map(|s| format!("{}{}", tidy(&s), f.ext))
            })
            .collect()
    }

    #[test]
    fn reads_number_in_name() {
        let n = |s: &str| name_number(s).map(|n| (n.raw, n.value));
        assert_eq!(n("05"), Some(("05".into(), 5)));
        assert_eq!(n("독후감_3-2_07"), Some(("07".into(), 7))); // 학급은 번호가 아님
        assert_eq!(n("2027_과제"), None); // 연도도 아님
        assert_eq!(n("김민지5번"), Some(("5".into(), 5)));
        assert_eq!(n("과제05_최종"), Some(("05".into(), 5)));
        assert_eq!(n("1학기 과제"), None);
        assert_eq!(n("30205_독후감").map(|x| x.1), Some(5)); // 학년반번호
                                                             // 2000~2099는 연도 모양이라 번호로 보지 않는다(사진은 순서 번호로)
        assert_eq!(n("IMG_2041"), None);
        assert_eq!(n("IMG_1041").map(|x| x.1), Some(1041));
    }

    #[test]
    fn adds_year_in_front_of_student_names() {
        // 실제로 해 본 경우: 학생 이름만 있는 파일에 "2026년 " 붙이기
        let names = ["고다윤.pdf", "김민찬.pdf"];
        let first = info(names[0]);
        assert_eq!(
            infer("2026년 고다윤", &first, &no_roster(), &today()),
            [
                Block::Year,
                text("년 "),
                Block::Name {
                    from: None,
                    to: None
                }
            ]
        );
        let out = apply(&names, "2026년 고다윤", &no_roster());
        assert_eq!(out[1].as_deref(), Ok("2026년 김민찬.pdf"));
    }

    #[test]
    fn numbered_homework_with_and_without_roster() {
        let names = ["01.hwp", "02.hwp", "03.hwp"];
        // 명렬표가 없으면 "김민지"는 글자 그대로(모두 김민지) — 화면이 명렬표를 붙여 넣으라고 알린다
        let blocks = infer("김민지(1)", &info(names[0]), &no_roster(), &today());
        assert_eq!(
            blocks,
            [text("김민지("), Block::Num { plain: true }, text(")")]
        );
        // 명렬표가 있으면 번호로 이름을 찾는다
        let roster = parse_roster("번호\t이름\n1\t김민지\n2\t이서준\n");
        let out = apply(&names, "김민지(1)", &roster);
        assert_eq!(out[1].as_deref(), Ok("이서준(2).hwp"));
        assert_eq!(out[2], Err("명렬표에 3번이 없어요".into()));
    }

    #[test]
    fn new_name_with_number_from_name_or_in_order() {
        // 이름의 번호(001 → 1)
        let out = apply(
            &["scan_001.pdf", "scan_002.pdf"],
            "가정통신문 1",
            &no_roster(),
        );
        assert_eq!(out[1].as_deref(), Ok("가정통신문 2.pdf"));
        // 앞의 0까지 그대로
        let out = apply(
            &["scan_001.pdf", "scan_002.pdf"],
            "가정통신문_001",
            &no_roster(),
        );
        assert_eq!(out[1].as_deref(), Ok("가정통신문_002.pdf"));
        // 이름의 번호와 맞지 않는 숫자는 순서 번호(사진 번호 2041은 쓰지 않음)
        let first = info("IMG_2041.jpg");
        assert_eq!(
            infer("제주 여행 1", &first, &no_roster(), &today()),
            [text("제주 여행 "), Block::Seq { start: 1, width: 1 }]
        );
        let out = apply(
            &["IMG_2041.jpg", "IMG_2055.jpg"],
            "제주 여행 01",
            &no_roster(),
        );
        assert_eq!(out[1].as_deref(), Ok("제주 여행 02.jpg"));
        // 연도 모양 숫자는 순서 번호로 보지 않는다
        let out = apply(&["a.hwp", "b.hwp"], "2025 자료", &no_roster());
        assert_eq!(out[1].as_deref(), Ok("2025 자료.hwp"));
    }

    #[test]
    fn deleting_or_replacing_part_of_name() {
        let names = [
            "회의록_최종.hwp",
            "예산안_최종_수정.hwp",
            "참석자 명단.xlsx",
        ];
        let blocks = infer("회의록", &info(names[0]), &no_roster(), &today());
        assert_eq!(
            blocks,
            [Block::Name {
                from: Some("_최종".into()),
                to: None
            }]
        );
        let out = apply(&names, "회의록", &no_roster());
        assert_eq!(out[1].as_deref(), Ok("예산안_수정.hwp"));
        assert_eq!(out[2].as_deref(), Ok("참석자 명단.xlsx")); // 그 글자가 없으면 그대로
                                                               // 대부분 남기고 일부만 바꿈(대소문자 무시)
        let out = apply(
            &["scan_001.pdf", "SCAN_002.pdf"],
            "통신문_001",
            &no_roster(),
        );
        assert_eq!(out[1].as_deref(), Ok("통신문_002.pdf"));
    }

    #[test]
    fn deleting_a_piece_that_differs_per_file() {
        // 학과처럼 파일마다 다른 조각을 지우면 그 조각 자리를 지운다(글자 ‘_신소재공학’만 지우는 게 아니라)
        let names = [
            "면접자료_김태우_신소재공학.pdf",
            "면접자료_민정우_화학공학.pdf",
            "면접자료_안현진_바이오메디컬화학공학.pdf",
        ];
        let out = apply(&names, "면접자료_김태우", &no_roster());
        assert_eq!(out[0].as_deref(), Ok("면접자료_김태우.pdf"));
        assert_eq!(out[1].as_deref(), Ok("면접자료_민정우.pdf"));
        assert_eq!(out[2].as_deref(), Ok("면접자료_안현진.pdf"));
        // 가운데 조각(이름)을 지워도
        let out = apply(&names, "면접자료_신소재공학", &no_roster());
        assert_eq!(out[1].as_deref(), Ok("면접자료_화학공학.pdf"));
    }

    #[test]
    fn reorders_pieces_and_dates() {
        // 조각 순서 바꾸기
        let out = apply(
            &["독후감_3-2_05.hwp", "독후감_3-2_06.hwp", "메모.hwp"],
            "05_독후감",
            &no_roster(),
        );
        assert_eq!(out[1].as_deref(), Ok("06_독후감.hwp"));
        assert_eq!(out[2], Err("이름에 번호가 없어요".into()));
        // 오늘 날짜·파일 날짜
        let first = info("보고서.hwp");
        assert_eq!(
            infer("보고서_2026-10-16", &first, &no_roster(), &today())[2],
            Block::Today
        );
        assert_eq!(
            infer("2026-09-17 보고서", &first, &no_roster(), &today())[0],
            Block::FileDate
        );
        // 그대로 적으면 바꾸지 않음
        assert!(infer("보고서", &first, &no_roster(), &today()).is_empty());
    }

    #[test]
    fn parses_pasted_roster() {
        let r = parse_roster("번호\t성명\n1\t김민지\n2\t이서준\n\n3, 박하은\n30204 최도윤\n정우진 5\n6\n2\t다른이름\n");
        assert_eq!(r.names.get(&1).map(String::as_str), Some("김민지"));
        assert_eq!(r.names.get(&3).map(String::as_str), Some("박하은"));
        assert_eq!(r.names.get(&4).map(String::as_str), Some("최도윤"));
        assert_eq!(r.names.get(&5).map(String::as_str), Some("정우진"));
        assert_eq!(r.names.get(&2).map(String::as_str), Some("다른이름"));
        assert_eq!(r.info.count, 5);
        assert_eq!(r.info.duplicates, [2]);
        assert_eq!(r.info.ignored, 1); // "6"(이름 없음)
    }

    #[test]
    fn replaces_ignoring_case_and_tidies() {
        assert_eq!(replace_ci("Scan_SCAN_x", "scan", "가"), "가_가_x");
        assert_eq!(replace_ci("abc", "", "x"), "abc");
        assert_eq!(tidy("  a__b   c_ "), "a_b c");
        assert_eq!(tidy("_최종_"), "최종");
    }

    fn req(files: &[PathBuf], example: &str) -> RenameRequest {
        RenameRequest {
            files: files
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            example: example.into(),
            ..RenameRequest::default()
        }
    }

    fn new_names(p: &Preview) -> Vec<String> {
        p.rows.iter().map(|r| r.new_name.clone()).collect()
    }

    fn files_in(dir: &Path, names: &[&str]) -> Vec<PathBuf> {
        names
            .iter()
            .map(|n| {
                let p = dir.join(n);
                fs::write(&p, n).unwrap();
                p
            })
            .collect()
    }

    #[test]
    fn preview_hints_and_blocks_from_screen() {
        let d = tempfile::tempdir().unwrap();
        let files = files_in(d.path(), &["01.hwp", "02.hwp"]);
        // 학생 이름처럼 보이는 글자 + 번호 → 명렬표를 붙여 넣으라고
        let p = plan(&req(&files, "김민지(1)"), &today());
        assert!(p.open_roster);
        assert!(p.hint.as_deref().unwrap().contains("명렬표"));
        assert_eq!(p.views[1].label, "이름의 번호");
        // 화면에서 고친 블록을 그대로 쓴다(학생 이름 블록 + 명렬표)
        let mut r = req(&files, "");
        r.blocks = Some(vec![
            Block::Student,
            text("("),
            Block::Num { plain: true },
            text(")"),
        ]);
        r.roster = "1\t김민지\n2\t이서준".into();
        r.list_word = "번호·이름 목록".into();
        let p = plan(&r, &today());
        assert_eq!(new_names(&p), ["김민지(1).hwp", "이서준(2).hwp"]);
        assert_eq!(p.views[0].label, "번호·이름 목록의 이름");
        assert!(!p.open_roster);
        // 명렬표 없이 학생 이름 블록이면 안내 + 건너뜀
        r.roster.clear();
        let p = plan(&r, &today());
        assert!(p.open_roster);
        assert_eq!(p.rows[0].status, Status::Skip);
        assert_eq!(p.changes, 0);
        // 그대로 적으면 모두 그대로
        let p = plan(&req(&files, "01"), &today());
        assert!(p.rows.iter().all(|r| r.status == Status::Same));
    }

    #[test]
    fn conflicts_get_numbers_and_bad_names_are_skipped() {
        let d = tempfile::tempdir().unwrap();
        let files = files_in(d.path(), &["a.hwp", "b.hwp"]);
        fs::write(d.path().join("과제.hwp"), b"x").unwrap(); // 바꾸지 않는 파일
                                                             // 모든 파일을 같은 이름으로 → (2) (3)
        let mut r = req(&files, "");
        r.blocks = Some(vec![text("과제")]);
        let p = plan(&r, &today());
        assert_eq!(new_names(&p), ["과제 (2).hwp", "과제 (3).hwp"]);
        assert!(p.rows.iter().all(|r| r.status == Status::Conflict));
        // 쓸 수 없는 글자
        r.blocks = Some(vec![text("a:b")]);
        assert_eq!(plan(&r, &today()).rows[0].status, Status::Invalid);
        // 비켜 가는 파일의 이름은 쓸 수 있다(a→b, b→c)
        r.blocks = Some(vec![Block::Name {
            from: Some("b".into()),
            to: Some("c".into()),
        }]);
        let p = plan(&req(&files[1..], ""), &today());
        assert!(p.rows.iter().all(|r| r.status == Status::Same));
        let mut r2 = req(&files, "");
        r2.blocks = Some(vec![Block::Seq { start: 0, width: 1 }]);
        // a→0, b→1: 서로 겹치지 않음
        assert_eq!(new_names(&plan(&r2, &today())), ["0.hwp", "1.hwp"]);
    }

    #[test]
    fn swaps_names_safely_and_undoes() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a.hwp");
        let b = d.path().join("b.hwp");
        fs::write(&a, "A").unwrap();
        fs::write(&b, "B").unwrap();
        let (done, errors) = execute(&[(a.clone(), "b.hwp".into()), (b.clone(), "a.hwp".into())]);
        assert!(errors.is_empty());
        assert_eq!(done.len(), 2);
        assert_eq!(fs::read_to_string(&a).unwrap(), "B");
        assert_eq!(fs::read_to_string(&b).unwrap(), "A");
        // 임시 파일이 남지 않는다
        assert_eq!(fs::read_dir(d.path()).unwrap().count(), 2);
        let back: Vec<(PathBuf, String)> = done
            .iter()
            .map(|x| {
                (
                    x.to.clone(),
                    x.from.file_name().unwrap().to_string_lossy().into_owned(),
                )
            })
            .collect();
        let (undone, errors) = execute(&back);
        assert!(errors.is_empty() && undone.len() == 2);
        assert_eq!(fs::read_to_string(&a).unwrap(), "A");
        assert_eq!(fs::read_to_string(&b).unwrap(), "B");
    }

    #[test]
    fn thirty_numbered_files_to_student_names_and_back() {
        // SPEC 8단계 완료 기준: 번호로 된 과제 30개 → 이름(번호) → 한 번에 되돌리기
        let d = tempfile::tempdir().unwrap();
        let mut roster = String::from("번호\t이름\n");
        let files: Vec<PathBuf> = (1..=30)
            .map(|i| {
                roster.push_str(&format!(
                    "{i}\t학생{}\n",
                    (b'A' + (i as u8 - 1) % 26) as char
                ));
                let p = d.path().join(format!("{i:02}.hwp"));
                fs::write(&p, format!("{i}")).unwrap();
                p
            })
            .collect();
        let mut r = req(&files, "학생A(1)");
        r.roster = roster;
        let p = plan(&r, &today());
        assert_eq!(p.changes, 30);
        let plans: Vec<(PathBuf, String)> = p
            .rows
            .iter()
            .map(|r| (PathBuf::from(&r.path), r.new_name.clone()))
            .collect();
        let (done, errors) = execute(&plans);
        assert!(errors.is_empty() && done.len() == 30);
        assert_eq!(
            fs::read_to_string(d.path().join("학생E(5).hwp")).unwrap(),
            "5"
        );
        let back: Vec<(PathBuf, String)> = done
            .iter()
            .map(|x| {
                (
                    x.to.clone(),
                    x.from.file_name().unwrap().to_string_lossy().into_owned(),
                )
            })
            .collect();
        let (undone, _) = execute(&back);
        assert_eq!(undone.len(), 30);
        assert_eq!(fs::read_to_string(d.path().join("05.hwp")).unwrap(), "5");
    }

    #[test]
    fn lists_files_without_temp_and_optionally_subfolders() {
        // tempfile 기본 이름(.tmp…)은 숨김 폴더로 보여 막히므로 다른 이름으로
        let d = tempfile::Builder::new().prefix("rn").tempdir().unwrap();
        let top = d.path().join("과제");
        fs::create_dir_all(top.join("3-2")).unwrap();
        fs::write(top.join("01.hwp"), b"x").unwrap();
        fs::write(top.join("~$01.hwp"), b"x").unwrap();
        fs::write(top.join("a.tmp"), b"x").unwrap();
        fs::write(top.join("3-2").join("02.hwp"), b"x").unwrap();
        let names = |sub: bool| {
            let mut v: Vec<String> = list_files(&top, sub)
                .unwrap()
                .files
                .into_iter()
                .map(|f| format!("{}|{}", f.sub_dir, f.name))
                .collect();
            v.sort();
            v
        };
        assert_eq!(names(false), ["|01.hwp"]);
        assert_eq!(names(true), ["3-2|02.hwp", "|01.hwp"]);
        assert!(list_files(Path::new("/"), false).is_err());
        assert!(list_files(&d.path().join("없음"), false).is_err());
    }
}
