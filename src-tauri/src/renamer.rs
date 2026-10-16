//! 이름 한꺼번에 바꾸기(SPEC 3-4).
//!
//! - 폴더를 골라 그 안 파일(하위 폴더는 고를 때만) 이름을 한꺼번에 바꾼다. 확장자는 바꾸지 않는다.
//! - 바꾸는 방법: 틀로 새로 짓기(`{학생 이름}({번호})` 등) → 글자 바꾸기(찾아 바꾸기·앞뒤에 붙이기) 차례로.
//! - 규칙 알아내기: 이름을 조각으로 나눠 자리별로 비교해 공통 조각·번호·학급·연도·학기를 찾고 틀을 제안한다.
//! - 명렬표: 붙여 넣은 글을 그때그때 읽기만 하고 저장하지 않는다.
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

// ───────────── 규칙 알아내기 ─────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PieceInfo {
    /// 1부터. 틀에서는 `{조각 n}`
    pub index: usize,
    pub kind: Kind,
    /// 모든 파일에서 같은지
    pub common: bool,
    /// 첫 파일의 글자(예: "독후감")
    pub sample: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedTemplate {
    pub label: String,
    pub template: String,
    /// 명렬표가 있어야 하는지
    pub needs_roster: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    /// 가장 많은 이름 모양의 조각들
    pub pieces: Vec<PieceInfo>,
    /// 그 모양인 파일 수 / 전체
    pub matching: usize,
    pub total: usize,
    /// 자리마다 바뀌는 번호 조각(1부터)
    pub number_piece: Option<usize>,
    pub suggestions: Vec<SuggestedTemplate>,
}

fn shape(t: &[Token]) -> Vec<Kind> {
    t.iter().map(|x| x.kind).collect()
}

/// 이름들을 조각으로 나눠 자리별로 비교한다. 모양이 서로 다르면 가장 많은 모양을 기준으로.
pub fn analyze(stems: &[&str]) -> Analysis {
    let all: Vec<Vec<Token>> = stems.iter().map(|s| tokens(s)).collect();
    // 가장 많은 모양(같으면 먼저 나온 것)
    let mut counts: Vec<(Vec<Kind>, usize)> = Vec::new();
    for t in &all {
        let s = shape(t);
        match counts.iter_mut().find(|(k, _)| *k == s) {
            Some((_, n)) => *n += 1,
            None => counts.push((s, 1)),
        }
    }
    let best = counts
        .iter()
        .enumerate()
        .max_by(|(ia, a), (ib, b)| a.1.cmp(&b.1).then(ib.cmp(ia)))
        .map(|(_, (k, n))| (k.clone(), *n));
    let Some((base, matching)) = best.filter(|(k, _)| !k.is_empty()) else {
        return Analysis {
            pieces: Vec::new(),
            matching: 0,
            total: stems.len(),
            number_piece: None,
            suggestions: Vec::new(),
        };
    };
    let group: Vec<&Vec<Token>> = all.iter().filter(|t| shape(t) == base).collect();
    let pieces: Vec<PieceInfo> = base
        .iter()
        .enumerate()
        .map(|(i, kind)| {
            let first = &group[0][i].text;
            PieceInfo {
                index: i + 1,
                kind: *kind,
                common: group
                    .iter()
                    .all(|t| t[i].text.to_lowercase() == first.to_lowercase()),
                sample: first.clone(),
            }
        })
        .collect();
    // 번호: 파일마다 바뀌는 번호 조각(여럿이면 마지막). 파일이 하나뿐이면 마지막 번호 조각.
    let numbers: Vec<&PieceInfo> = pieces.iter().filter(|p| p.kind == Kind::Number).collect();
    let number_piece = numbers
        .iter()
        .rev()
        .find(|p| !p.common || group.len() == 1)
        .map(|p| p.index);

    let mut suggestions = Vec::new();
    if let Some(n) = number_piece {
        let common_text = pieces
            .iter()
            .find(|p| p.kind == Kind::Text && p.common && p.index != n);
        suggestions.push(SuggestedTemplate {
            label: "번호로 된 이름이네요 → 학생 이름(번호)".into(),
            template: "{학생 이름}({번호})".into(),
            needs_roster: true,
        });
        if let Some(c) = common_text {
            let class = pieces.iter().find(|p| p.kind == Kind::Class);
            let sample: Vec<String> = pieces.iter().map(|p| p.sample.clone()).collect();
            suggestions.push(SuggestedTemplate {
                label: format!("‘{}’ 모양 → 번호_학생 이름_{}", sample.join("_"), c.sample),
                template: format!("{{번호}}_{{학생 이름}}_{{조각 {}}}", c.index),
                needs_roster: true,
            });
            if class.is_some() {
                suggestions.push(SuggestedTemplate {
                    label: format!("{}_학급_번호", c.sample),
                    template: format!("{{조각 {}}}_{{학급}}_{{번호}}", c.index),
                    needs_roster: false,
                });
            } else {
                suggestions.push(SuggestedTemplate {
                    label: format!("{}(번호)", c.sample),
                    template: format!("{{조각 {}}}({{번호}})", c.index),
                    needs_roster: false,
                });
            }
        }
    }
    Analysis {
        pieces,
        matching,
        total: stems.len(),
        number_piece,
        suggestions,
    }
}

/// 이름에서 번호 읽기: 알아낸 번호 조각 → 없으면 이름의 숫자 하나(학급·연도·학기는 빼고, `5번`도 5).
/// 숫자가 여럿이면 마지막 것.
pub fn read_number(toks: &[Token], number_piece: Option<usize>, matches_base: bool) -> Option<u32> {
    if let Some(i) = number_piece.filter(|_| matches_base) {
        if let Some(t) = toks.get(i - 1).filter(|t| t.kind == Kind::Number) {
            let digits = t.text.trim_end_matches('번').len();
            return t.value.map(|v| student_number(v, digits));
        }
    }
    if let Some(t) = toks.iter().rev().find(|t| t.kind == Kind::Number) {
        let digits = t.text.trim_end_matches('번').len();
        return t.value.map(|v| student_number(v, digits));
    }
    // 글자와 붙은 숫자(예: 과제05, 김민지5번)
    for t in toks.iter().rev().filter(|t| t.kind == Kind::Text) {
        let chars: Vec<char> = t.text.chars().collect();
        let mut end = chars.len();
        while end > 0 {
            if chars[end - 1].is_ascii_digit() {
                let mut start = end;
                while start > 0 && chars[start - 1].is_ascii_digit() {
                    start -= 1;
                }
                let run: String = chars[start..end].iter().collect();
                // 연도처럼 보이는 네 자리, 학기(1학기)는 번호가 아니다
                let after: String = chars[end..].iter().collect();
                let year_like = run.len() == 4 && (2000..=2099).contains(&run.parse().unwrap_or(0));
                if !year_like && !after.starts_with("학기") && !after.starts_with("학년") {
                    if let Ok(v) = run.parse::<u32>() {
                        return Some(student_number(v, run.len()));
                    }
                }
                end = start;
            } else {
                end -= 1;
            }
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

// ───────────── 틀 · 글자 바꾸기 ─────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NumberOptions {
    /// true = 이름에서 읽기, false = 순서대로 매기기
    pub from_name: bool,
    pub start: u32,
    /// 자릿수(0 = 자동: 이름에서 읽으면 그대로, 순서대로면 가장 큰 번호의 자릿수)
    pub digits: u8,
}

impl Default for NumberOptions {
    fn default() -> Self {
        NumberOptions {
            from_name: true,
            start: 1,
            digits: 0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Replacement {
    pub find: String,
    pub with: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RenameRequest {
    /// 바꿀 파일(체크한 것, 보이는 순서대로)
    pub files: Vec<String>,
    /// 틀(빈 문자열 = 원래 이름 그대로 두고 글자 바꾸기만)
    pub template: String,
    pub number: NumberOptions,
    pub replacements: Vec<Replacement>,
    pub prefix: String,
    pub suffix: String,
    /// 붙여 넣은 명렬표(저장하지 않음)
    pub roster: String,
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

fn uses_slot(template: &str, slot: &str) -> bool {
    template.contains(&format!("{{{slot}}}"))
}

/// 파일 하나를 틀에 맞춰 지을 때 쓰는 값
struct FileCtx<'a> {
    stem: &'a str,
    name: &'a str,
    toks: Vec<Token>,
    matches_base: bool,
    number: Option<String>,
    student: Option<&'a str>,
    modified: i64,
}

fn date_text(ms: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// 틀 채우기. 쓸 수 없으면 그 까닭.
fn render(template: &str, ctx: &FileCtx) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            rest = "";
            break;
        };
        let slot = after[..close].trim();
        let value: String = match slot {
            "원래 이름" => ctx.stem.to_string(),
            "번호" => ctx.number.clone().ok_or("이름에서 번호를 찾지 못했어요")?,
            "학생 이름" => match (&ctx.number, ctx.student) {
                (_, Some(s)) => s.to_string(),
                (None, None) => return Err("이름에서 번호를 찾지 못했어요".into()),
                (Some(n), None) => {
                    return Err(format!("명렬표에 {}번이 없어요", n.trim_start_matches('0')))
                }
            },
            "연도" => naming::find_year(ctx.name)
                .map(|y| y.to_string())
                .unwrap_or_default(),
            "학기" => naming::find_term(ctx.name)
                .map(|t| format!("{t}학기"))
                .unwrap_or_default(),
            "학급" => naming::find_class(ctx.name, false)
                .map(|c| c.folder_name())
                .unwrap_or_default(),
            "날짜" => date_text(ctx.modified),
            other => match other
                .strip_prefix("조각")
                .map(str::trim)
                .and_then(|n| n.parse::<usize>().ok())
            {
                Some(n) => {
                    if !ctx.matches_base {
                        return Err("이름 모양이 다른 파일이에요".into());
                    }
                    ctx.toks
                        .get(n.wrapping_sub(1))
                        .map(|t| t.text.clone())
                        .ok_or("이름 모양이 다른 파일이에요")?
                }
                // 모르는 칸은 글자 그대로
                None => format!("{{{}}}", &after[..close]),
            },
        };
        out.push_str(&value);
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

// ───────────── 미리보기 ─────────────

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub analysis: Analysis,
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

/// 바꿀 이름을 모두 정한다(디스크는 읽기만).
pub fn plan(req: &RenameRequest) -> Preview {
    let paths: Vec<PathBuf> = req
        .files
        .iter()
        .take(MAX_FILES)
        .map(PathBuf::from)
        .collect();
    let names: Vec<String> = paths
        .iter()
        .map(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
        .collect();
    let stems: Vec<&str> = names.iter().map(|n| naming::stem(n)).collect();
    let analysis = analyze(&stems);
    let base: Vec<Kind> = analysis.pieces.iter().map(|p| p.kind).collect();
    let roster = parse_roster(&req.roster);
    let template = req.template.trim();
    let wants_number = uses_slot(template, "번호") || uses_slot(template, "학생 이름");

    // 번호(순서대로면 체크한 순서)
    let count = paths.len() as u32;
    let last = req.number.start.saturating_add(count.saturating_sub(1));
    let width = match req.number.digits {
        0 if req.number.from_name => 1,
        0 => last.to_string().len(),
        d => d as usize,
    };

    // 1) 파일마다 새 이름(겹침은 아직 안 봄)
    let mut rows: Vec<PreviewRow> = Vec::new();
    for (i, path) in paths.iter().enumerate() {
        let name = &names[i];
        let (stem, ext) = split_ext(name);
        let toks = tokens(stem);
        let matches_base = !base.is_empty() && shape(&toks) == base;
        let value = if req.number.from_name {
            read_number(&toks, analysis.number_piece, matches_base)
        } else {
            Some(req.number.start + i as u32)
        };
        let ctx = FileCtx {
            stem,
            name,
            matches_base,
            number: value.map(|v| format!("{v:0width$}")),
            student: value.and_then(|v| roster.names.get(&v)).map(String::as_str),
            toks,
            modified: std::fs::metadata(path)
                .map(|m| modified_ms(&m))
                .unwrap_or(0),
        };
        let mut row = PreviewRow {
            path: path.to_string_lossy().into_owned(),
            old_name: name.clone(),
            new_name: name.clone(),
            status: Status::Same,
            note: None,
        };
        let built = if template.is_empty() {
            Ok(stem.to_string())
        } else if wants_number && value.is_none() {
            Err("이름에서 번호를 찾지 못했어요".to_string())
        } else {
            render(template, &ctx)
        };
        let new_stem = match built {
            Ok(s) => s,
            Err(note) => {
                row.status = Status::Skip;
                row.note = Some(note);
                rows.push(row);
                continue;
            }
        };
        let mut s = new_stem;
        for r in &req.replacements {
            s = replace_ci(&s, &r.find, &r.with);
        }
        let s = tidy(&format!("{}{s}{}", req.prefix, req.suffix));
        let new_name = format!("{s}{ext}");
        if s.is_empty() {
            row.status = Status::Invalid;
            row.note = Some("이름이 비어 있어요".into());
        } else if let Err(e) = folders::validate_name(&new_name) {
            row.status = Status::Invalid;
            row.note = Some(e);
        } else if new_name != *name {
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
        analysis,
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
    Ok(plan(&request))
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
    let preview = plan(&request);
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

    fn kinds(stem: &str) -> Vec<Kind> {
        shape(&tokens(stem))
    }

    fn req(files: &[PathBuf], template: &str) -> RenameRequest {
        RenameRequest {
            files: files
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            template: template.into(),
            ..RenameRequest::default()
        }
    }

    fn new_names(p: &Preview) -> Vec<String> {
        p.rows.iter().map(|r| r.new_name.clone()).collect()
    }

    #[test]
    fn splits_pieces_like_rule_cards() {
        use Kind::*;
        assert_eq!(kinds("독후감_3-2_05"), [Text, Class, Number]);
        assert_eq!(
            kinds("2027 1학기 수행평가(5번)"),
            [Year, Term, Text, Number]
        );
        assert_eq!(kinds("scan_001"), [Text, Number]);
        assert_eq!(kinds("27학년도-계획"), [Year, Text]);
        assert_eq!(kinds("과제05"), [Text]);
        assert_eq!(tokens("독후감_3-2_05")[2].value, Some(5));
    }

    #[test]
    fn finds_common_piece_and_number() {
        let stems = ["독후감_3-2_01", "독후감_3-2_02", "독후감_3-2_13", "메모"];
        let a = analyze(&stems);
        assert_eq!((a.matching, a.total), (3, 4));
        assert_eq!(a.number_piece, Some(3));
        assert!(a.pieces[0].common && a.pieces[0].sample == "독후감");
        assert_eq!(a.pieces[1].kind, Kind::Class);
        assert!(!a.pieces[2].common);
        let templates: Vec<&str> = a.suggestions.iter().map(|s| s.template.as_str()).collect();
        assert_eq!(
            templates,
            [
                "{학생 이름}({번호})",
                "{번호}_{학생 이름}_{조각 1}",
                "{조각 1}_{학급}_{번호}"
            ]
        );
        // 번호만 있는 이름
        let a = analyze(&["01", "02", "03"]);
        assert_eq!(a.number_piece, Some(1));
        assert_eq!(a.suggestions.len(), 1);
        // 번호가 없으면 제안도 없음
        assert!(analyze(&["회의록", "계획서"]).suggestions.is_empty());
        assert!(analyze(&[]).pieces.is_empty());
    }

    #[test]
    fn reads_number_from_name() {
        let n = |s: &str| read_number(&tokens(s), None, false);
        assert_eq!(n("05"), Some(5));
        assert_eq!(n("독후감_3-2_07"), Some(7)); // 학급은 번호가 아님
        assert_eq!(n("2027_과제"), None); // 연도도 아님
        assert_eq!(n("김민지5번"), Some(5));
        assert_eq!(n("과제05_최종"), Some(5));
        assert_eq!(n("1학기 과제"), None);
        assert_eq!(n("30205_독후감"), Some(5)); // 학년반번호
                                                // 알아낸 번호 조각이 먼저(뒤에 다른 숫자가 있어도)
        let t = tokens("12_독후감_2");
        assert_eq!(read_number(&t, Some(1), true), Some(12));
        assert_eq!(read_number(&t, Some(1), false), Some(2));
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

    #[test]
    fn numbered_homework_with_roster() {
        let d = tempfile::tempdir().unwrap();
        let files: Vec<PathBuf> = (1..=3)
            .map(|i| {
                let p = d.path().join(format!("{i:02}.hwp"));
                fs::write(&p, b"x").unwrap();
                p
            })
            .collect();
        let mut r = req(&files, "{학생 이름}({번호})");
        r.roster = "1\t김민지\n2\t이서준\n".into();
        let p = plan(&r);
        assert_eq!(new_names(&p), ["김민지(1).hwp", "이서준(2).hwp", "03.hwp"]);
        assert_eq!(p.rows[2].status, Status::Skip);
        assert_eq!(p.rows[2].note.as_deref(), Some("명렬표에 3번이 없어요"));
        assert_eq!(p.changes, 2);
        assert_eq!(p.roster.count, 2);

        // 순서대로 번호(자동 자릿수), 원래 이름 + 글자 바꾸기
        let mut r = req(&files, "가정통신문({번호})");
        r.number = NumberOptions {
            from_name: false,
            start: 9,
            digits: 0,
        };
        assert_eq!(
            new_names(&plan(&r)),
            [
                "가정통신문(09).hwp",
                "가정통신문(10).hwp",
                "가정통신문(11).hwp"
            ]
        );
        r.number.digits = 3;
        r.template = "{번호}".into();
        r.prefix = "과제_".into();
        assert_eq!(new_names(&plan(&r))[0], "과제_009.hwp");
    }

    #[test]
    fn replacements_without_template() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("SCAN_001 최종.pdf");
        fs::write(&a, b"x").unwrap();
        let mut r = req(std::slice::from_ref(&a), "");
        r.replacements = vec![
            Replacement {
                find: "scan".into(),
                with: "가정통신문".into(),
            },
            Replacement {
                find: "최종".into(),
                with: String::new(),
            },
        ];
        assert_eq!(new_names(&plan(&r)), ["가정통신문_001.pdf"]);
        // 아무것도 안 바꾸면 그대로
        let p = plan(&req(std::slice::from_ref(&a), ""));
        assert_eq!(p.rows[0].status, Status::Same);
        assert_eq!(p.changes, 0);
    }

    #[test]
    fn conflicts_get_numbers_and_bad_names_are_skipped() {
        let d = tempfile::tempdir().unwrap();
        let mk = |n: &str| {
            let p = d.path().join(n);
            fs::write(&p, n).unwrap();
            p
        };
        let a = mk("a.hwp");
        let b = mk("b.hwp");
        mk("과제.hwp"); // 바꾸지 않는 파일
        let p = plan(&req(&[a.clone(), b.clone()], "과제"));
        assert_eq!(new_names(&p), ["과제 (2).hwp", "과제 (3).hwp"]);
        assert!(p.rows.iter().all(|r| r.status == Status::Conflict));
        // 이름이 비거나 쓸 수 없는 글자
        let p = plan(&req(&[a.clone(), b.clone()], "{학급}"));
        assert_eq!(p.rows[0].status, Status::Invalid);
        let p = plan(&req(std::slice::from_ref(&a), "a:b"));
        assert_eq!(p.rows[0].status, Status::Invalid);
        // 비켜 가는 파일의 이름은 쓸 수 있다(a→b, b→c)
        let mut r = req(&[a.clone(), b.clone()], "");
        let rep = |f: &str, w: &str| Replacement {
            find: f.into(),
            with: w.into(),
        };
        r.replacements = vec![rep("b", "c"), rep("a", "b")];
        let p = plan(&r);
        assert_eq!(new_names(&p), ["b.hwp", "c.hwp"]);
        assert!(p.rows.iter().all(|r| r.status == Status::Rename));
    }

    #[test]
    fn pieces_skip_files_of_other_shapes() {
        let d = tempfile::tempdir().unwrap();
        let files: Vec<PathBuf> = ["독후감_3-2_01.hwp", "독후감_3-2_02.hwp", "메모.hwp"]
            .iter()
            .map(|n| {
                let p = d.path().join(n);
                fs::write(&p, b"x").unwrap();
                p
            })
            .collect();
        let p = plan(&req(&files, "{번호}_{조각 1}"));
        assert_eq!(new_names(&p)[..2], ["1_독후감.hwp", "2_독후감.hwp"]);
        assert_eq!(p.rows[2].status, Status::Skip);
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
        // 되돌리기 = 바뀐 경로 → 원래 이름
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
        let mut r = req(&files, "{학생 이름}({번호})");
        r.roster = roster;
        let p = plan(&r);
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
