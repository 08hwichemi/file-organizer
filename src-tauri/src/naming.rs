//! 파일 이름 읽기(SPEC 4장).
//!
//! - 규칙 카드에 보여 줄 단어 나누기
//! - 이름에서 연도·학기·학급 찾기(`2027`, `27학년도` / `1학기` / `3-2`, `3학년 2반`, `302`, `3반`)
//! - 오늘 날짜로 학년도·학기 계산(3~8월 = 1학기, 9~12월 = 2학기, 1~2월 = 전년도 2학기)
//! - 폴더 이름을 자리표시로 바꾸기(`2027` → [연도], `1학기` → [학기], `3-2` → [학급])

use serde::Serialize;

/// 학기가 시작하는 달. 설정에서 바꿀 수 있다(기본 3월·9월).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermMonths {
    pub first: u32,
    pub second: u32,
}

impl Default for TermMonths {
    fn default() -> Self {
        TermMonths {
            first: 3,
            second: 9,
        }
    }
}

/// 날짜(연, 월)가 속한 학년도와 학기. 1학기 시작 달 전이면 전년도 2학기.
pub fn academic_term(year: i32, month: u32, months: TermMonths) -> (i32, u8) {
    if month >= months.second {
        (year, 2)
    } else if month >= months.first {
        (year, 1)
    } else {
        (year - 1, 2)
    }
}

/// 학급: 학년을 모르면(`3반`) 반만.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassId {
    pub grade: Option<u8>,
    pub class: u8,
}

impl ClassId {
    /// 폴더 이름: `3-2`, 학년을 모르면 `2반`
    pub fn folder_name(self) -> String {
        match self.grade {
            Some(g) => format!("{g}-{}", self.class),
            None => format!("{}반", self.class),
        }
    }
}

/// 확장자를 뺀 이름. "보고서.final.hwp" → "보고서.final"
pub fn stem(file_name: &str) -> &str {
    match file_name.rfind('.') {
        Some(i) if i > 0 => &file_name[..i],
        _ => file_name,
    }
}

/// 확장자(소문자, 점 없이). 없으면 빈 문자열.
pub fn extension(file_name: &str) -> String {
    match file_name.rfind('.') {
        Some(i) if i > 0 => file_name[i + 1..].to_lowercase(),
        _ => String::new(),
    }
}

fn is_separator(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '_' | '.'
                | ','
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '+'
                | '~'
                | '!'
                | '@'
                | '#'
                | '$'
                | '%'
                | '^'
                | '&'
                | '='
                | ';'
                | '\''
                | '"'
                | '·'
                | '「'
                | '」'
                | '『'
                | '』'
                | '《'
                | '》'
                | '<'
                | '>'
        )
}

/// 규칙 카드에 보여 줄 단어들(이름 순서대로, 겹치지 않게).
/// `-`는 `3-2`처럼 숫자 사이일 때만 붙여 두고, 그 밖에는 단어를 나눈다.
pub fn name_words(file_name: &str) -> Vec<String> {
    const LIMIT: usize = 12;
    let mut out: Vec<String> = Vec::new();
    for token in stem(file_name).split(is_separator) {
        let parts: Vec<&str> = if is_class_dash(token) {
            vec![token]
        } else {
            token.split('-').collect()
        };
        for part in parts {
            let part = part.trim();
            if part.is_empty() || out.iter().any(|w| w.to_lowercase() == part.to_lowercase()) {
                continue;
            }
            out.push(part.to_string());
            if out.len() >= LIMIT {
                return out;
            }
        }
    }
    out
}

/// `3-2` 모양(학년 한 자리 - 반 한두 자리)
pub(crate) fn is_class_dash(s: &str) -> bool {
    let Some((g, c)) = s.split_once('-') else {
        return false;
    };
    g.len() == 1
        && (1..=2).contains(&c.len())
        && g.chars().all(|ch| ch.is_ascii_digit())
        && c.chars().all(|ch| ch.is_ascii_digit())
        && (1..=6).contains(&g.parse::<u8>().unwrap_or(0))
        && (1..=20).contains(&c.parse::<u8>().unwrap_or(0))
}

/// 이어진 숫자 덩어리(시작 위치, 끝 위치(끝 다음), 값). 위치는 글자 단위.
struct DigitRun {
    start: usize,
    end: usize,
    text: String,
}

fn digit_runs(chars: &[char]) -> Vec<DigitRun> {
    let mut runs = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            runs.push(DigitRun {
                start,
                end: i,
                text: chars[start..i].iter().collect(),
            });
        } else {
            i += 1;
        }
    }
    runs
}

fn followed_by(chars: &[char], at: usize, word: &str) -> bool {
    let w: Vec<char> = word.chars().collect();
    chars.len() >= at + w.len() && chars[at..at + w.len()] == w[..]
}

/// 이름에서 연도: `2027`(2000~2099의 네 자리 숫자) 또는 `27학년도`. 앞에서부터 처음 것.
pub fn find_year(file_name: &str) -> Option<i32> {
    let chars: Vec<char> = stem(file_name).chars().collect();
    for run in digit_runs(&chars) {
        let Ok(n) = run.text.parse::<i32>() else {
            continue;
        };
        if run.text.len() == 4 && (2000..=2099).contains(&n) {
            return Some(n);
        }
        if run.text.len() == 2 && followed_by(&chars, run.end, "학년도") {
            return Some(2000 + n);
        }
    }
    None
}

/// 이름에서 학기: `1학기`, `2학기`만(`1-1`은 학급과 헷갈려서 학기로 보지 않는다).
pub fn find_term(file_name: &str) -> Option<u8> {
    let chars: Vec<char> = stem(file_name).chars().collect();
    digit_runs(&chars).into_iter().find_map(|run| {
        let ok = (run.text == "1" || run.text == "2") && followed_by(&chars, run.end, "학기");
        ok.then(|| run.text.parse().unwrap_or(1))
    })
}

/// 이름에서 학급: `3학년 2반` → `3-2` → (`allow_three_digits`면) `302` → `3반` 순서로 찾는다.
pub fn find_class(file_name: &str, allow_three_digits: bool) -> Option<ClassId> {
    let chars: Vec<char> = stem(file_name).chars().collect();
    let runs = digit_runs(&chars);
    let grade_of = |r: &DigitRun| -> Option<u8> {
        let g: u8 = r.text.parse().ok()?;
        (r.text.len() == 1 && (1..=6).contains(&g)).then_some(g)
    };
    let class_of = |r: &DigitRun| -> Option<u8> {
        let c: u8 = r.text.parse().ok()?;
        (r.text.len() <= 2 && (1..=20).contains(&c)).then_some(c)
    };

    // 3학년 2반, 3학년2반
    for (i, r) in runs.iter().enumerate() {
        let (Some(g), Some(next)) = (grade_of(r), runs.get(i + 1)) else {
            continue;
        };
        if !followed_by(&chars, r.end, "학년") {
            continue;
        }
        let between = &chars[r.end + 2..next.start];
        if between.iter().all(|c| c.is_whitespace()) && followed_by(&chars, next.end, "반") {
            if let Some(c) = class_of(next) {
                return Some(ClassId {
                    grade: Some(g),
                    class: c,
                });
            }
        }
    }
    // 3-2 (2027-10-13 같은 날짜는 빼려고 앞뒤에 다른 '-'가 없어야 한다)
    for (i, r) in runs.iter().enumerate() {
        let (Some(g), Some(next)) = (grade_of(r), runs.get(i + 1)) else {
            continue;
        };
        let dash_between = next.start == r.end + 1 && chars[r.end] == '-';
        let dash_before = r.start > 0 && chars[r.start - 1] == '-';
        let dash_after = chars.get(next.end) == Some(&'-');
        if dash_between && !dash_before && !dash_after {
            if let Some(c) = class_of(next) {
                return Some(ClassId {
                    grade: Some(g),
                    class: c,
                });
            }
        }
    }
    // 302 (학급 규칙일 때만: 번호 같은 다른 숫자와 헷갈리기 쉬워서)
    if allow_three_digits {
        for r in &runs {
            if r.text.len() != 3 {
                continue;
            }
            let g: u8 = r.text[..1].parse().unwrap_or(0);
            let c: u8 = r.text[1..].parse().unwrap_or(0);
            if (1..=6).contains(&g) && (1..=20).contains(&c) {
                return Some(ClassId {
                    grade: Some(g),
                    class: c,
                });
            }
        }
    }
    // 3반
    for r in &runs {
        if followed_by(&chars, r.end, "반") {
            if let Some(c) = class_of(r) {
                return Some(ClassId {
                    grade: None,
                    class: c,
                });
            }
        }
    }
    None
}

/// 폴더 이름 안의 자리표시
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Year,
    Term,
    Class,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    Slot(Slot),
}

/// 폴더 이름이 연도·학기·학급으로만 되어 있으면 자리표시 모양을 돌려준다.
/// `2027` → [연도], `2027학년도` → [연도]학년도, `2027 1학기` → [연도] [학기], `3-2` → [학급].
/// `수행평가`, `학급 3-2`처럼 다른 글자가 섞이면 None.
pub fn folder_template(name: &str) -> Option<Vec<Piece>> {
    let mut pieces = Vec::new();
    let mut has_slot = false;
    let mut token = String::new();
    let flush = |token: &mut String, pieces: &mut Vec<Piece>, has_slot: &mut bool| -> bool {
        if token.is_empty() {
            return true;
        }
        let t = std::mem::take(token);
        let digits = |s: &str| s.len() == 4 && s.chars().all(|c| c.is_ascii_digit());
        let year_ok = |s: &str| digits(s) && (2000..=2099).contains(&s.parse::<i32>().unwrap_or(0));
        if year_ok(&t) {
            pieces.push(Piece::Slot(Slot::Year));
        } else if t.strip_suffix("학년도").is_some_and(year_ok) {
            pieces.push(Piece::Slot(Slot::Year));
            pieces.push(Piece::Text("학년도".into()));
        } else if t == "1학기" || t == "2학기" {
            pieces.push(Piece::Slot(Slot::Term));
        } else if is_class_dash(&t) {
            pieces.push(Piece::Slot(Slot::Class));
        } else {
            return false;
        }
        *has_slot = true;
        true
    };
    for c in name.chars() {
        if c == ' ' || c == '_' {
            if !flush(&mut token, &mut pieces, &mut has_slot) {
                return None;
            }
            pieces.push(Piece::Text(c.to_string()));
        } else {
            token.push(c);
        }
    }
    if !flush(&mut token, &mut pieces, &mut has_slot) || !has_slot {
        return None;
    }
    Some(pieces)
}

/// 자리표시를 보여 줄 글자로: [연도] [학기]
pub fn template_label(pieces: &[Piece]) -> String {
    pieces
        .iter()
        .map(|p| match p {
            Piece::Text(t) => t.as_str(),
            Piece::Slot(Slot::Year) => "[연도]",
            Piece::Slot(Slot::Term) => "[학기]",
            Piece::Slot(Slot::Class) => "[학급]",
        })
        .collect()
}

/// 자리표시에 넣을 값
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotValues {
    pub year: i32,
    pub term: u8,
    pub class: Option<ClassId>,
}

impl SlotValues {
    /// 파일 이름 → 없으면 오늘 날짜(학년도·학기). 학급은 이름에서만 읽는다.
    pub fn for_file(file_name: &str, today: (i32, u32), months: TermMonths) -> Self {
        let (year_now, term_now) = academic_term(today.0, today.1, months);
        SlotValues {
            year: find_year(file_name).unwrap_or(year_now),
            term: find_term(file_name).unwrap_or(term_now),
            class: find_class(file_name, true),
        }
    }
}

/// 자리표시에 값을 넣어 폴더 이름을 만든다. 학급을 모르면 그 부분을 빼고, 다 빠지면 None(그 폴더 단계를 건너뜀).
pub fn fill_template(pieces: &[Piece], values: &SlotValues) -> Option<String> {
    let mut out = String::new();
    for p in pieces {
        match p {
            Piece::Text(t) => out.push_str(t),
            Piece::Slot(Slot::Year) => out.push_str(&values.year.to_string()),
            Piece::Slot(Slot::Term) => out.push_str(&format!("{}학기", values.term)),
            Piece::Slot(Slot::Class) => {
                if let Some(c) = values.class {
                    out.push_str(&c.folder_name());
                }
            }
        }
    }
    let trimmed = out.trim_matches(|c| c == ' ' || c == '_').to_string();
    (!trimmed.is_empty()).then_some(trimmed)
}

/// 오늘 날짜(이 컴퓨터 시간대 기준) (연, 월)
pub fn today() -> (i32, u32) {
    use chrono::Datelike;
    let now = chrono::Local::now();
    (now.year(), now.month())
}

/// 규칙 카드에 보여 줄 폴더 한 단계: 이름과, 나눌 수 있으면 자리표시 모양
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DestSegment {
    pub name: String,
    /// 예: "[연도]", "[연도] [학기]". 나눌 수 없는 폴더면 None.
    pub template: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: TermMonths = TermMonths {
        first: 3,
        second: 9,
    };

    #[test]
    fn academic_term_boundaries() {
        assert_eq!(academic_term(2027, 3, M), (2027, 1));
        assert_eq!(academic_term(2027, 8, M), (2027, 1));
        assert_eq!(academic_term(2027, 9, M), (2027, 2));
        assert_eq!(academic_term(2027, 12, M), (2027, 2));
        assert_eq!(academic_term(2028, 1, M), (2027, 2));
        assert_eq!(academic_term(2028, 2, M), (2027, 2));
        // 2학기를 8월부터로 바꾼 경우
        let aug = TermMonths {
            first: 3,
            second: 8,
        };
        assert_eq!(academic_term(2027, 8, aug), (2027, 2));
        assert_eq!(academic_term(2027, 7, aug), (2027, 1));
    }

    #[test]
    fn words_from_name() {
        assert_eq!(
            name_words("2027_1학기_3-2_수행평가_채점기준.hwp"),
            ["2027", "1학기", "3-2", "수행평가", "채점기준"]
        );
        assert_eq!(
            name_words("학업성적관리위원회 회의록(10월).hwp"),
            ["학업성적관리위원회", "회의록", "10월"]
        );
        // 숫자 사이가 아닌 '-'는 나누고, 겹치는 단어는 한 번만
        assert_eq!(name_words("가정-통신문_가정.pdf"), ["가정", "통신문"]);
        assert_eq!(
            name_words("2027-10-13 회의.pdf"),
            ["2027", "10", "13", "회의"]
        );
    }

    #[test]
    fn year_from_name() {
        assert_eq!(find_year("2027_1학기_수행평가.hwp"), Some(2027));
        assert_eq!(find_year("27학년도 교육과정.pdf"), Some(2027));
        assert_eq!(find_year("2027학년도 교육과정.pdf"), Some(2027));
        assert_eq!(find_year("scan_20261013.pdf"), None); // 여덟 자리는 연도로 보지 않는다
        assert_eq!(find_year("회의록 1999.hwp"), None);
        assert_eq!(find_year("2027.hwp"), Some(2027));
        assert_eq!(find_year("수행평가.hwp"), None);
    }

    #[test]
    fn term_from_name() {
        assert_eq!(find_term("2027_1학기_수행평가.hwp"), Some(1));
        assert_eq!(find_term("2학기 지필고사.xlsx"), Some(2));
        assert_eq!(find_term("12학기.hwp"), None);
        assert_eq!(find_term("1-1 수행평가.hwp"), None);
        assert_eq!(find_term("3학년 2반.hwp"), None);
    }

    #[test]
    fn class_from_name() {
        let gc = |g, c| {
            Some(ClassId {
                grade: Some(g),
                class: c,
            })
        };
        assert_eq!(find_class("2027_1학기_3-2_수행평가", false), gc(3, 2));
        assert_eq!(find_class("3학년 2반 상담.hwp", false), gc(3, 2));
        assert_eq!(find_class("3학년2반 상담.hwp", false), gc(3, 2));
        assert_eq!(find_class("1-10 출결.xlsx", false), gc(1, 10));
        // 302는 학급 규칙일 때만
        assert_eq!(find_class("302 출결.xlsx", false), None);
        assert_eq!(find_class("302 출결.xlsx", true), gc(3, 2));
        assert_eq!(find_class("999 출결.xlsx", true), None);
        // 날짜의 '-'는 학급이 아니다
        assert_eq!(find_class("2027-1-13 회의", false), None);
        assert_eq!(find_class("회의 2027-10-1", false), None);
        // 학년 없이 반만
        assert_eq!(
            find_class("3반 사진.jpg", false),
            Some(ClassId {
                grade: None,
                class: 3
            })
        );
        assert_eq!(find_class("수행평가.hwp", true), None);
    }

    #[test]
    fn folder_names_become_templates() {
        let label = |n: &str| folder_template(n).map(|p| template_label(&p));
        assert_eq!(label("2027").as_deref(), Some("[연도]"));
        assert_eq!(label("1학기").as_deref(), Some("[학기]"));
        assert_eq!(label("2027 1학기").as_deref(), Some("[연도] [학기]"));
        assert_eq!(label("2027학년도").as_deref(), Some("[연도]학년도"));
        assert_eq!(label("3-2").as_deref(), Some("[학급]"));
        assert_eq!(label("수행평가"), None);
        assert_eq!(label("학급 3-2"), None);
        assert_eq!(label("1999"), None);
        assert_eq!(label("3학기"), None);
    }

    #[test]
    fn fill_uses_name_then_today() {
        let pieces = folder_template("2027 1학기").unwrap();
        // 이름에 연도·학기가 있으면 그대로
        let v = SlotValues::for_file("2028_2학기_수행평가.hwp", (2026, 10), M);
        assert_eq!(fill_template(&pieces, &v).as_deref(), Some("2028 2학기"));
        // 없으면 오늘 날짜: 2027년 1월 = 2026학년도 2학기
        let v = SlotValues::for_file("수행평가.hwp", (2027, 1), M);
        assert_eq!(fill_template(&pieces, &v).as_deref(), Some("2026 2학기"));
        // 연도만 있으면 학기는 오늘 날짜
        let v = SlotValues::for_file("2028 수행평가.hwp", (2027, 4), M);
        assert_eq!(fill_template(&pieces, &v).as_deref(), Some("2028 1학기"));
    }

    #[test]
    fn missing_class_skips_folder() {
        let class = folder_template("3-2").unwrap();
        let v = SlotValues::for_file("수행평가.hwp", (2027, 4), M);
        assert_eq!(fill_template(&class, &v), None);
        let v = SlotValues::for_file("302_수행평가.hwp", (2027, 4), M);
        assert_eq!(fill_template(&class, &v).as_deref(), Some("3-2"));
        let v = SlotValues::for_file("5반_수행평가.hwp", (2027, 4), M);
        assert_eq!(fill_template(&class, &v).as_deref(), Some("5반"));
    }

    #[test]
    fn stem_and_extension() {
        assert_eq!(stem("보고서.final.hwp"), "보고서.final");
        assert_eq!(extension("보고서.final.HWP"), "hwp");
        assert_eq!(extension("README"), "");
        assert_eq!(extension(".env"), "");
    }
}
