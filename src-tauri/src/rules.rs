//! 정리 규칙(SPEC 4장): "조건 → 저장 위치". 위에서부터 먼저 맞는 규칙 하나로 추천한다.
//!
//! - 조건: 파일 이름 단어(모두 포함) · 확장자 · 출처(지켜볼 폴더). 단어 규칙을 먼저 보고,
//!   단어 없이 확장자만 있는 규칙은 마지막 기준으로 본다.
//! - 저장 위치: 고른 폴더 그대로. `split`이면 그 경로 중 `2027`·`1학기`·`3-2` 같은 폴더를
//!   파일마다 [연도]·[학기]·[학급]으로 바꿔 넣는다(없으면 만든다).
//! - 같은 규칙에서 사용자가 같은 다른 폴더를 세 번 연속 고르면 "규칙을 바꿀까요?"를 묻는다.

use crate::naming::{self, DestSegment, SlotValues, TermMonths};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// "규칙을 바꿀까요?"를 묻기까지 연속으로 다른 폴더를 고른 횟수
const MISSES_BEFORE_ASKING: i64 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: i64,
    /// 파일 이름에 들어 있어야 하는 단어(대소문자 무시)
    pub words: Vec<String>,
    /// 단어가 여럿일 때 하나라도 있으면 맞음(false면 모두 있어야 맞음)
    pub any_word: bool,
    /// 확장자(소문자, 점 없이). 비어 있으면 아무 확장자.
    pub extensions: Vec<String>,
    /// 이 폴더에서 온 파일만. 없으면 어디서 왔든.
    pub source_dir: Option<String>,
    pub dest_dir: String,
    /// 연도·학기·학급 폴더를 파일마다 나눌지
    pub split: bool,
    /// 이 규칙으로 넣은 횟수(되돌린 것 제외)
    pub used: i64,
}

impl Rule {
    /// 이름(소문자, 확장자 뺀)이 단어 조건에 맞는지. 단어가 없으면 맞음.
    pub fn words_match(&self, name: &str) -> bool {
        let has = |w: &String| name.contains(&w.to_lowercase());
        if self.any_word {
            self.words.is_empty() || self.words.iter().any(has)
        } else {
            self.words.iter().all(has)
        }
    }
}

/// 화면에서 만들거나 고칠 때 보내는 값
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleInput {
    pub words: Vec<String>,
    #[serde(default)]
    pub any_word: bool,
    pub extensions: Vec<String>,
    pub source_dir: Option<String>,
    pub dest_dir: String,
    pub split: bool,
}

/// 빈 칸·겹치는 값을 정리하고, 조건이 하나도 없으면 막는다.
pub fn normalize(input: RuleInput) -> Result<RuleInput, String> {
    let mut words: Vec<String> = Vec::new();
    for w in input
        .words
        .iter()
        .map(|w| w.trim())
        .filter(|w| !w.is_empty())
    {
        if !words.iter().any(|x| x.to_lowercase() == w.to_lowercase()) {
            words.push(w.to_string());
        }
    }
    let mut extensions: Vec<String> = Vec::new();
    for e in input.extensions.iter() {
        let e = e.trim().trim_start_matches('.').to_lowercase();
        if e.is_empty() {
            continue;
        }
        if e.chars()
            .any(|c| c.is_whitespace() || "\\/:*?\"<>|.".contains(c))
        {
            return Err(format!(
                "확장자 ‘{e}’를 알아볼 수 없어요. hwp, pdf처럼 적어 주세요."
            ));
        }
        if !extensions.contains(&e) {
            extensions.push(e);
        }
    }
    if words.is_empty() && extensions.is_empty() {
        return Err("조건을 하나 이상 정해 주세요(파일 이름의 단어나 확장자).".into());
    }
    let dest_dir = input.dest_dir.trim().to_string();
    if dest_dir.is_empty() {
        return Err("넣을 폴더를 골라 주세요.".into());
    }
    Ok(RuleInput {
        any_word: input.any_word && words.len() > 1,
        words,
        extensions,
        source_dir: input
            .source_dir
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        dest_dir,
        split: input.split,
    })
}

fn to_json(list: &[String]) -> String {
    serde_json::to_string(list).unwrap_or_else(|_| "[]".into())
}

fn from_json(s: &str) -> Vec<String> {
    serde_json::from_str(s).unwrap_or_default()
}

const SELECT: &str = "SELECT id, words, extensions, source_dir, dest_dir, split,
       (SELECT COUNT(*) FROM moves m WHERE m.rule = CAST(r.id AS TEXT) AND m.undone_at IS NULL),
       any_word
     FROM rules r";

fn row_to_rule(row: &rusqlite::Row) -> rusqlite::Result<Rule> {
    Ok(Rule {
        id: row.get(0)?,
        words: from_json(&row.get::<_, String>(1)?),
        extensions: from_json(&row.get::<_, String>(2)?),
        source_dir: row.get(3)?,
        dest_dir: row.get(4)?,
        split: row.get::<_, i64>(5)? != 0,
        used: row.get(6)?,
        any_word: row.get::<_, i64>(7)? != 0,
    })
}

/// 새 규칙은 맨 위에 둔다(방금 만든 규칙이 예전 규칙에 가려지지 않게).
pub fn create(conn: &Connection, input: &RuleInput) -> rusqlite::Result<i64> {
    let top: i64 = conn.query_row("SELECT COALESCE(MIN(position), 1) FROM rules", [], |r| {
        r.get(0)
    })?;
    conn.execute(
        "INSERT INTO rules (position, words, extensions, source_dir, dest_dir, split, created_at, any_word)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            top - 1,
            to_json(&input.words),
            to_json(&input.extensions),
            input.source_dir,
            input.dest_dir,
            input.split,
            crate::db::now_ms(),
            input.any_word
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update(conn: &Connection, id: i64, input: &RuleInput) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE rules SET words = ?2, extensions = ?3, source_dir = ?4, dest_dir = ?5, split = ?6,
                          any_word = ?7, miss_streak = 0, miss_dir = NULL
         WHERE id = ?1",
        params![
            id,
            to_json(&input.words),
            to_json(&input.extensions),
            input.source_dir,
            input.dest_dir,
            input.split,
            input.any_word
        ],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, id: i64) -> rusqlite::Result<Option<Rule>> {
    conn.query_row(
        &format!("{SELECT} WHERE r.id = ?1 AND r.deleted_at IS NULL"),
        params![id],
        row_to_rule,
    )
    .optional()
}

/// 지운 것 빼고, 위에서부터(먼저 보는 순서).
pub fn list(conn: &Connection) -> rusqlite::Result<Vec<Rule>> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE r.deleted_at IS NULL ORDER BY r.position, r.id"
    ))?;
    let rows = stmt.query_map([], row_to_rule)?;
    rows.collect()
}

/// 지우기(되돌릴 수 있게 표시만 한다).
pub fn delete(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE rules SET deleted_at = ?2 WHERE id = ?1",
        params![id, crate::db::now_ms()],
    )?;
    Ok(())
}

pub fn restore(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE rules SET deleted_at = NULL WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

/// 한 칸 위(`up`) 또는 아래로. 움직였으면 true.
pub fn move_by_one(conn: &Connection, id: i64, up: bool) -> rusqlite::Result<bool> {
    let mut stmt = conn
        .prepare("SELECT id, position FROM rules WHERE deleted_at IS NULL ORDER BY position, id")?;
    let order: Vec<(i64, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let Some(i) = order.iter().position(|(rid, _)| *rid == id) else {
        return Ok(false);
    };
    let j = if up { i.checked_sub(1) } else { Some(i + 1) };
    let Some(j) = j.filter(|j| *j < order.len()) else {
        return Ok(false);
    };
    // 자리 번호를 순서대로 다시 매긴 뒤 둘을 바꾼다(같은 번호가 있어도 확실히 바뀌게).
    let mut ids: Vec<i64> = order.iter().map(|(rid, _)| *rid).collect();
    ids.swap(i, j);
    for (pos, rid) in ids.iter().enumerate() {
        conn.execute(
            "UPDATE rules SET position = ?2 WHERE id = ?1",
            params![rid, pos as i64],
        )?;
    }
    Ok(true)
}

/// 폴더 이름을 바꾸거나 옮긴 뒤 규칙의 저장 위치도 따라 바꾼다.
pub fn rewrite_folder_prefix(conn: &Connection, old: &Path, new: &Path) -> rusqlite::Result<usize> {
    let mut stmt = conn.prepare("SELECT id, dest_dir FROM rules")?;
    let rows: Vec<(i64, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut changed = 0;
    for (id, dest) in rows {
        let dest_path = Path::new(&dest);
        let Ok(rest) = dest_path.strip_prefix(old) else {
            continue;
        };
        let moved = if rest.as_os_str().is_empty() {
            new.to_path_buf()
        } else {
            new.join(rest)
        };
        conn.execute(
            "UPDATE rules SET dest_dir = ?2 WHERE id = ?1",
            params![id, moved.to_string_lossy()],
        )?;
        changed += 1;
    }
    Ok(changed)
}

/// 같은 폴더인지(끝의 구분자 무시, Windows는 대소문자도 무시).
pub fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        let s = p.to_string_lossy();
        let s = s.trim_end_matches(['\\', '/']);
        if cfg!(windows) {
            s.replace('/', "\\").to_lowercase()
        } else {
            s.to_string()
        }
    };
    norm(a) == norm(b)
}

/// 파일에 맞는 규칙 하나: 단어 규칙을 위에서부터, 없으면 확장자만 있는 규칙을 위에서부터.
pub fn find_match<'a>(rules: &'a [Rule], file_name: &str, source_dir: &Path) -> Option<&'a Rule> {
    let name = naming::stem(file_name).to_lowercase();
    let ext = naming::extension(file_name);
    let fits = |r: &&Rule| {
        (r.extensions.is_empty() || r.extensions.contains(&ext))
            && r.source_dir
                .as_deref()
                .is_none_or(|s| same_path(Path::new(s), source_dir))
            && r.words_match(&name)
    };
    rules
        .iter()
        .filter(|r| !r.words.is_empty())
        .find(fits)
        .or_else(|| rules.iter().filter(|r| r.words.is_empty()).find(fits))
}

/// 규칙이 이 파일을 넣을 폴더
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub dir: PathBuf,
    /// 이미 있는 폴더인지(없으면 넣을 때 만든다)
    pub exists: bool,
    /// 지금 있는 가장 깊은 폴더(팝업에서 이 폴더가 보이게 연다)
    pub existing: PathBuf,
}

/// 폴더 경로를 정리 폴더 아래 이름들로. 정리 폴더 밖이면 None.
fn relative_names(root: &Path, dest: &Path) -> Option<Vec<String>> {
    let rel = dest.strip_prefix(root).ok()?;
    Some(
        rel.components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect(),
    )
}

/// 규칙이 이 파일을 넣을 폴더. 저장 위치(나누는 폴더 앞부분)가 없어졌으면 None.
pub fn resolve(
    rule: &Rule,
    root: &Path,
    file_name: &str,
    today: (i32, u32),
    months: TermMonths,
) -> Option<Target> {
    let dest = PathBuf::from(&rule.dest_dir);
    let fixed_only = || {
        dest.is_dir().then(|| Target {
            dir: dest.clone(),
            exists: true,
            existing: dest.clone(),
        })
    };
    if !rule.split {
        return fixed_only();
    }
    let Some(names) = relative_names(root, &dest) else {
        return fixed_only();
    };
    let Some(first) = names
        .iter()
        .position(|n| naming::folder_template(n).is_some())
    else {
        return fixed_only();
    };
    let mut dir = root.to_path_buf();
    for n in &names[..first] {
        dir.push(n);
    }
    if !dir.is_dir() {
        return None;
    }
    let values = SlotValues::for_file(file_name, today, months);
    for n in &names[first..] {
        match naming::folder_template(n) {
            Some(pieces) => {
                if let Some(filled) = naming::fill_template(&pieces, &values) {
                    dir.push(filled);
                }
            }
            None => dir.push(n),
        }
    }
    let mut existing = dir.clone();
    while !existing.is_dir() && existing.pop() {}
    Some(Target {
        exists: dir.is_dir(),
        dir,
        existing,
    })
}

/// 넣기 전에 규칙이 정한 폴더를 만든다(연도·학기 폴더 등). 정리 폴더 안쪽만.
pub fn ensure_target(target: &Target, root: &Path) -> Result<(), String> {
    if target.exists {
        return Ok(());
    }
    let root_ok = target
        .existing
        .canonicalize()
        .ok()
        .zip(root.canonicalize().ok())
        .is_some_and(|(e, r)| e.starts_with(r));
    if !root_ok {
        return Err("정리 폴더 밖에는 폴더를 만들지 않아요.".into());
    }
    std::fs::create_dir_all(&target.dir).map_err(|e| format!("폴더를 만들지 못했어요: {e}"))
}

/// 저장 위치를 정리 폴더 아래 단계별로(나눌 수 있는 폴더는 자리표시 모양과 함께).
pub fn describe_dest(root: &Path, dest: &Path) -> Vec<DestSegment> {
    match relative_names(root, dest) {
        Some(names) => names
            .into_iter()
            .map(|name| DestSegment {
                template: naming::folder_template(&name).map(|p| naming::template_label(&p)),
                name,
            })
            .collect(),
        None => vec![DestSegment {
            name: dest.to_string_lossy().into_owned(),
            template: None,
        }],
    }
}

/// 넣은 뒤 보여 줄 "다음에도 여기에?" 카드 내용
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleDraft {
    /// 파일 이름의 단어들(누르면 조건에 넣고 뺀다)
    pub words: Vec<String>,
    /// 처음부터 골라 둘 단어: 넣은 폴더 이름과 같은 단어(연도·학기·학급은 폴더 나누기로 하므로 빼고)
    pub preselected: Vec<String>,
    pub dest_dir: String,
    pub segments: Vec<DestSegment>,
    /// 나누기를 켰을 때 예: "2028년 2학기"
    pub example_label: String,
    /// 그 예의 폴더 이름들(정리 폴더 아래)
    pub example_segments: Vec<String>,
}

pub fn draft(
    root: &Path,
    file_name: &str,
    dest_dir: &Path,
    today: (i32, u32),
    months: TermMonths,
) -> RuleDraft {
    let words = naming::name_words(file_name);
    let segments = describe_dest(root, dest_dir);
    let preselected = words
        .iter()
        .filter(|w| naming::folder_template(w).is_none())
        .filter(|w| {
            segments
                .iter()
                .any(|s| s.name.to_lowercase() == w.to_lowercase())
        })
        .cloned()
        .collect();

    // 예는 이 파일과 다른 값으로: 다음 해, 다른 학기, 다른 반
    let now = SlotValues::for_file(file_name, today, months);
    let example = SlotValues {
        year: now.year + 1,
        term: if now.term == 2 { 1 } else { 2 },
        class: Some(naming::ClassId {
            grade: Some(1),
            class: if now.class.map(|c| c.class) == Some(1) {
                2
            } else {
                1
            },
        }),
    };
    let mut slots = (false, false, false);
    let example_segments = segments
        .iter()
        .filter_map(|s| match naming::folder_template(&s.name) {
            Some(pieces) => {
                for p in &pieces {
                    match p {
                        naming::Piece::Slot(naming::Slot::Year) => slots.0 = true,
                        naming::Piece::Slot(naming::Slot::Term) => slots.1 = true,
                        naming::Piece::Slot(naming::Slot::Class) => slots.2 = true,
                        naming::Piece::Text(_) => {}
                    }
                }
                naming::fill_template(&pieces, &example)
            }
            None => Some(s.name.clone()),
        })
        .collect();
    let class = example.class.expect("예시 학급");
    let example_label = [
        slots.0.then(|| format!("{}년", example.year)),
        slots.1.then(|| format!("{}학기", example.term)),
        slots
            .2
            .then(|| format!("{}학년 {}반", class.grade.unwrap_or(1), class.class)),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ");

    RuleDraft {
        words,
        preselected,
        dest_dir: dest_dir.to_string_lossy().into_owned(),
        segments,
        example_label,
        example_segments,
    }
}

/// 추천이 있던 파일을 어디에 넣었는지 기록한다. 추천대로 넣었으면 연속 횟수를 지운다.
/// 같은 다른 폴더를 연속으로 세 번 고르면 true("규칙을 바꿀까요?"를 물을 때).
pub fn note_choice(
    conn: &Connection,
    id: i64,
    chosen: &Path,
    suggested: &Path,
) -> rusqlite::Result<bool> {
    if same_path(chosen, suggested) {
        reset_misses(conn, id)?;
        return Ok(false);
    }
    let (streak, last): (i64, Option<String>) = conn.query_row(
        "SELECT miss_streak, miss_dir FROM rules WHERE id = ?1",
        params![id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let streak = match last {
        Some(l) if same_path(Path::new(&l), chosen) => streak + 1,
        _ => 1,
    };
    conn.execute(
        "UPDATE rules SET miss_streak = ?2, miss_dir = ?3 WHERE id = ?1",
        params![id, streak, chosen.to_string_lossy()],
    )?;
    Ok(streak >= MISSES_BEFORE_ASKING)
}

pub fn reset_misses(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE rules SET miss_streak = 0, miss_dir = NULL WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

/// "규칙을 바꿀까요?" → 바꾸기: 저장 위치만 새 폴더로.
pub fn change_dest(conn: &Connection, id: i64, dest: &Path) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE rules SET dest_dir = ?2, miss_streak = 0, miss_dir = NULL WHERE id = ?1",
        params![id, dest.to_string_lossy()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use std::fs;

    const M: TermMonths = TermMonths {
        first: 3,
        second: 9,
    };

    fn input(words: &[&str], exts: &[&str], dest: &str) -> RuleInput {
        RuleInput {
            words: words.iter().map(|s| s.to_string()).collect(),
            extensions: exts.iter().map(|s| s.to_string()).collect(),
            source_dir: None,
            dest_dir: dest.into(),
            split: false,
            any_word: false,
        }
    }

    #[test]
    fn normalize_cleans_and_requires_a_condition() {
        let n = normalize(input(
            &[" 수행평가 ", "", "수행평가"],
            &[".HWP", "hwp", " pdf"],
            " /r/평가 ",
        ))
        .unwrap();
        assert_eq!(n.words, ["수행평가"]);
        assert_eq!(n.extensions, ["hwp", "pdf"]);
        assert_eq!(n.dest_dir, "/r/평가");
        assert!(normalize(input(&[" "], &[], "/r")).is_err());
        assert!(normalize(input(&["a"], &[], " ")).is_err());
        assert!(normalize(input(&[], &["h w p"], "/r")).is_err());
    }

    #[test]
    fn new_rules_go_on_top_and_can_be_reordered() {
        let conn = db::open_in_memory().unwrap();
        let a = create(&conn, &input(&["a"], &[], "/r/a")).unwrap();
        let b = create(&conn, &input(&["b"], &[], "/r/b")).unwrap();
        let ids = |c: &Connection| list(c).unwrap().iter().map(|r| r.id).collect::<Vec<_>>();
        assert_eq!(ids(&conn), [b, a]);
        assert!(move_by_one(&conn, a, true).unwrap());
        assert_eq!(ids(&conn), [a, b]);
        assert!(!move_by_one(&conn, a, true).unwrap()); // 이미 맨 위
        assert!(move_by_one(&conn, a, false).unwrap());
        assert_eq!(ids(&conn), [b, a]);
    }

    #[test]
    fn delete_hides_and_restore_brings_back() {
        let conn = db::open_in_memory().unwrap();
        let a = create(&conn, &input(&["a"], &[], "/r/a")).unwrap();
        delete(&conn, a).unwrap();
        assert!(list(&conn).unwrap().is_empty());
        assert!(get(&conn, a).unwrap().is_none());
        restore(&conn, a).unwrap();
        assert_eq!(list(&conn).unwrap()[0].words, ["a"]);
    }

    #[test]
    fn used_counts_moves_with_rule() {
        let conn = db::open_in_memory().unwrap();
        let a = create(&conn, &input(&["a"], &[], "/r/a")).unwrap();
        crate::history::record_with_rule(&conn, Path::new("/d/1"), Path::new("/r/a/1"), Some(a))
            .unwrap();
        let undone = crate::history::record_with_rule(
            &conn,
            Path::new("/d/2"),
            Path::new("/r/a/2"),
            Some(a),
        )
        .unwrap();
        crate::history::mark_undone(&conn, undone).unwrap();
        crate::history::record(&conn, Path::new("/d/3"), Path::new("/r/a/3")).unwrap();
        assert_eq!(get(&conn, a).unwrap().unwrap().used, 1);
    }

    fn rule(id: i64, words: &[&str], exts: &[&str], source: Option<&str>) -> Rule {
        Rule {
            id,
            words: words.iter().map(|s| s.to_string()).collect(),
            extensions: exts.iter().map(|s| s.to_string()).collect(),
            source_dir: source.map(String::from),
            dest_dir: format!("/r/{id}"),
            split: false,
            used: 0,
            any_word: false,
        }
    }

    #[test]
    fn any_word_matches_one_of_the_words() {
        let mut r = rule(1, &["중간고사", "기말고사"], &[], None);
        let d = Path::new("/d");
        let found = |r: &Rule, name: &str| find_match(std::slice::from_ref(r), name, d).is_some();
        assert!(!found(&r, "2학기_기말고사_시험지.hwp")); // 모두: 둘 다 있어야
        assert!(found(&r, "중간고사_기말고사_일정.hwp"));
        r.any_word = true; // 하나라도
        assert!(found(&r, "2학기_기말고사_시험지.hwp"));
        assert!(found(&r, "1학기 중간고사.pdf"));
        assert!(!found(&r, "수행평가.hwp"));
    }

    #[test]
    fn any_word_is_saved_and_only_kept_with_two_or_more_words() {
        let conn = db::open_in_memory().unwrap();
        let mut i = input(&["중간고사", "기말고사"], &[], "/r/a");
        i.any_word = true;
        let id = create(&conn, &normalize(i).unwrap()).unwrap();
        assert!(get(&conn, id).unwrap().unwrap().any_word);
        let mut one = input(&["중간고사"], &[], "/r/a");
        one.any_word = true;
        assert!(!normalize(one).unwrap().any_word);
    }

    #[test]
    fn match_first_word_rule_then_extension_rule() {
        let rules = vec![
            rule(1, &[], &["hwp"], None),
            rule(2, &["2027", "수행평가"], &[], None),
            rule(3, &["수행평가"], &[], None),
            rule(4, &["사진"], &[], Some("/home/Desktop")),
        ];
        let d = Path::new("/home/Downloads");
        let id = |name: &str, src: &Path| find_match(&rules, name, src).map(|r| r.id);
        assert_eq!(id("2027_1학기_수행평가.hwp", d), Some(2)); // 두 단어 모두 → 위에 있는 2번
        assert_eq!(id("2028_수행평가.hwp", d), Some(3));
        assert_eq!(id("가정통신문.HWP", d), Some(1)); // 단어 규칙이 없으면 확장자
        assert_eq!(id("가정통신문.pdf", d), None);
        // 출처
        assert_eq!(id("체험학습_사진.jpg", d), None);
        assert_eq!(
            id("체험학습_사진.jpg", Path::new("/home/Desktop/")),
            Some(4)
        );
        // 대소문자 무시
        let rules = vec![rule(5, &["Scan"], &[], None)];
        assert_eq!(find_match(&rules, "SCAN_001.pdf", d).map(|r| r.id), Some(5));
    }

    #[test]
    fn extension_and_words_together() {
        let rules = vec![rule(1, &["회의록"], &["hwp", "hwpx"], None)];
        let d = Path::new("/d");
        assert!(find_match(&rules, "회의록_10월.hwpx", d).is_some());
        assert!(find_match(&rules, "회의록_10월.pdf", d).is_none());
    }

    /// 정리 폴더/평가/수행평가/2027/1학기 를 만든다
    fn tree() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("정리 폴더");
        fs::create_dir_all(
            root.join("평가")
                .join("수행평가")
                .join("2027")
                .join("1학기"),
        )
        .unwrap();
        (dir, root)
    }

    #[test]
    fn resolve_fixed_and_split_destinations() {
        let (_d, root) = tree();
        let year_term = root
            .join("평가")
            .join("수행평가")
            .join("2027")
            .join("1학기");
        let mut r = rule(1, &["수행평가"], &[], None);
        r.dest_dir = year_term.to_string_lossy().into_owned();

        // 나누지 않으면 그 폴더 그대로
        let t = resolve(&r, &root, "2028_2학기_수행평가.hwp", (2027, 4), M).unwrap();
        assert_eq!(t.dir, year_term);
        assert!(t.exists);

        // 나누면 파일 이름의 연도·학기로
        r.split = true;
        let t = resolve(&r, &root, "2028_2학기_수행평가.hwp", (2027, 4), M).unwrap();
        assert_eq!(
            t.dir,
            root.join("평가")
                .join("수행평가")
                .join("2028")
                .join("2학기")
        );
        assert!(!t.exists);
        assert_eq!(t.existing, root.join("평가").join("수행평가"));

        // 이름에 없으면 오늘 날짜: 2027년 10월 = 2027학년도 2학기
        let t = resolve(&r, &root, "수행평가_안내.hwp", (2027, 10), M).unwrap();
        assert_eq!(
            t.dir,
            root.join("평가")
                .join("수행평가")
                .join("2027")
                .join("2학기")
        );
        assert_eq!(t.existing, root.join("평가").join("수행평가").join("2027"));

        // 만들기
        ensure_target(&t, &root).unwrap();
        assert!(t.dir.is_dir());
    }

    #[test]
    fn resolve_fails_when_fixed_part_is_gone() {
        let (_d, root) = tree();
        let mut r = rule(1, &["수행평가"], &[], None);
        r.dest_dir = root.join("없는 폴더").to_string_lossy().into_owned();
        assert!(resolve(&r, &root, "수행평가.hwp", (2027, 4), M).is_none());
        r.split = true;
        r.dest_dir = root
            .join("없는 폴더")
            .join("2027")
            .to_string_lossy()
            .into_owned();
        assert!(resolve(&r, &root, "수행평가.hwp", (2027, 4), M).is_none());
    }

    #[test]
    fn ensure_target_refuses_outside_root() {
        let (d, root) = tree();
        let outside = d.path().join("바깥");
        let t = Target {
            dir: outside.join("2027"),
            exists: false,
            existing: d.path().to_path_buf(),
        };
        assert!(ensure_target(&t, &root).is_err());
        assert!(!outside.exists());
    }

    #[test]
    fn draft_preselects_folder_words_and_shows_example() {
        let (_d, root) = tree();
        let dest = root
            .join("평가")
            .join("수행평가")
            .join("2027")
            .join("1학기");
        let d = draft(
            &root,
            "2027_1학기_3-2_수행평가_채점기준.hwp",
            &dest,
            (2027, 4),
            M,
        );
        assert_eq!(d.words, ["2027", "1학기", "3-2", "수행평가", "채점기준"]);
        assert_eq!(d.preselected, ["수행평가"]);
        let templates: Vec<Option<&str>> =
            d.segments.iter().map(|s| s.template.as_deref()).collect();
        assert_eq!(templates, [None, None, Some("[연도]"), Some("[학기]")]);
        assert_eq!(d.example_label, "2028년 2학기");
        assert_eq!(d.example_segments, ["평가", "수행평가", "2028", "2학기"]);
    }

    #[test]
    fn three_misses_to_the_same_folder_ask_to_change() {
        let conn = db::open_in_memory().unwrap();
        let id = create(&conn, &input(&["a"], &[], "/r/a")).unwrap();
        let sug = Path::new("/r/a");
        let other = Path::new("/r/b");
        assert!(!note_choice(&conn, id, other, sug).unwrap());
        assert!(!note_choice(&conn, id, other, sug).unwrap());
        // 중간에 추천대로 넣으면 처음부터
        assert!(!note_choice(&conn, id, sug, sug).unwrap());
        assert!(!note_choice(&conn, id, other, sug).unwrap());
        assert!(!note_choice(&conn, id, Path::new("/r/c"), sug).unwrap()); // 다른 폴더도 처음부터
        assert!(!note_choice(&conn, id, other, sug).unwrap());
        assert!(!note_choice(&conn, id, other, sug).unwrap());
        assert!(note_choice(&conn, id, other, sug).unwrap());
        change_dest(&conn, id, other).unwrap();
        assert_eq!(get(&conn, id).unwrap().unwrap().dest_dir, "/r/b");
        assert!(!note_choice(&conn, id, Path::new("/r/b/"), other).unwrap());
    }

    #[test]
    fn rewrite_prefix_follows_folder_rename() {
        let conn = db::open_in_memory().unwrap();
        let sep = std::path::MAIN_SEPARATOR;
        let old = format!("{sep}r{sep}수업");
        let inside = create(&conn, &input(&["a"], &[], &format!("{old}{sep}1학기"))).unwrap();
        let same = create(&conn, &input(&["b"], &[], &old)).unwrap();
        let similar = create(&conn, &input(&["c"], &[], &format!("{old}자료"))).unwrap();
        let new = format!("{sep}r{sep}수업 자료");
        assert_eq!(
            rewrite_folder_prefix(&conn, Path::new(&old), Path::new(&new)).unwrap(),
            2
        );
        let dest = |id| get(&conn, id).unwrap().unwrap().dest_dir;
        assert_eq!(dest(inside), format!("{new}{sep}1학기"));
        assert_eq!(dest(same), new);
        assert_eq!(dest(similar), format!("{old}자료"));
    }
}
