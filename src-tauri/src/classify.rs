//! 자동 분류(SPEC 4-1): 규칙이 없어도 파일 이름·형식을 보고 넣을 폴더를 정한다.
//!
//! 순서: 정리 규칙 → 이름 단어(정리 폴더의 폴더 이름 · 자동 분류 단어) → 파일 형식
//! (학교: `형식별\한글 문서` 등, 집·개인: 정리 폴더 바로 아래 `문서`·`사진`… 종류 폴더).
//! - 이름 단어는 파일 이름에 든 가장 긴 단어로 고른다(같으면 폴더 이름이 먼저).
//! - 자동 분류 단어의 폴더가 없으면, 같은 이름의 폴더가 정리 폴더 어딘가에 있으면 거기로, 없으면 만든다.
//! - 형식으로도 모르는 파일(설치 파일 등)은 None → 정리함에 남긴다.
//! - 파일 이름은 이 PC 안에서만 본다(밖으로 보내지 않는다).

use crate::naming::{self, TermMonths};
use crate::rules::{self, Rule, Target};
use crate::settings::UserKind;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// 형식별 폴더를 모아 두는 폴더(정리 폴더 바로 아래)
pub const TYPE_PARENT: &str = "형식별";
/// 설정 표에 자동 분류 단어를 저장하는 키
const WORDS_KEY: &str = "auto_words";
/// 폴더 이름을 읽을 깊이·개수(정리 폴더가 아주 커도 느려지지 않게)
const MAX_DEPTH: usize = 4;
const MAX_FOLDERS: usize = 3000;

/// 자동 분류 단어 묶음: 이 단어 중 하나가 이름에 있으면 `folder`(정리 폴더 아래 경로)로.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordGroup {
    pub folder: Vec<String>,
    pub words: Vec<String>,
}

/// 교사용 기본 단어(SPEC 4-1). 폴더는 처음 실행의 교사용 예시 구조에 맞췄다.
const TEACHER_WORDS: &[(&[&str], &[&str])] = &[
    (&["업무", "공문"], &["공문", "시행문", "협조", "회신"]),
    (&["업무", "회의록"], &["회의록", "협의회", "회의"]),
    (&["업무", "계획서"], &["계획서", "계획"]),
    (&["업무", "가정통신문"], &["가정통신문", "통신문"]),
    (&["수업"], &["수업", "학습지", "활동지", "교안", "지도안"]),
    (&["평가", "수행평가"], &["수행평가", "채점기준", "루브릭"]),
    (
        &["평가", "지필고사"],
        &[
            "지필",
            "중간고사",
            "기말고사",
            "문항정보표",
            "이원목적",
            "시험지",
            "감독",
        ],
    ),
    (&["학급", "상담"], &["상담"]),
    (&["학급", "출결"], &["출결", "결석", "지각", "조퇴"]),
    (
        &["생활기록부"],
        &[
            "생활기록부",
            "생기부",
            "세특",
            "세부능력",
            "행동특성",
            "창체",
            "진로활동",
            "동아리",
        ],
    ),
    (&["연수"], &["연수", "이수증"]),
];

/// 집·개인 기본 단어: 없다. 예시 폴더가 파일 종류(문서·사진…)라 이름 단어로 안쪽 폴더를 미리 정하지 않는다.
/// 사용자가 `문서 › 계약서`처럼 폴더를 만들면 폴더 이름으로 저절로 찾아 넣는다(사용자 결정).
const GENERAL_WORDS: &[(&[&str], &[&str])] = &[];

/// 쓰는 곳에 맞는 기본 단어(학교: 교사용, 집·개인: 없음)
pub fn default_words(kind: UserKind) -> Vec<WordGroup> {
    let table = match kind {
        UserKind::School => TEACHER_WORDS,
        UserKind::General => GENERAL_WORDS,
    };
    table
        .iter()
        .map(|(folder, words)| WordGroup {
            folder: folder.iter().map(|s| s.to_string()).collect(),
            words: words.iter().map(|s| s.to_string()).collect(),
        })
        .collect()
}

/// 저장한 자동 분류 단어. 아직 고친 적이 없으면 쓰는 곳에 맞는 기본 단어.
pub fn load_words(conn: &Connection) -> Vec<WordGroup> {
    crate::db::get_value(conn, WORDS_KEY)
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_else(|| {
            let kind = crate::settings::load(conn)
                .map(|s| s.user_kind)
                .unwrap_or_default();
            default_words(kind)
        })
}

/// 빈 칸·겹치는 단어를 정리하고, 폴더 이름이 바른지 확인한다.
pub fn normalize_words(groups: Vec<WordGroup>) -> Result<Vec<WordGroup>, String> {
    let mut out = Vec::new();
    for g in groups {
        let folder: Vec<String> = g
            .folder
            .iter()
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .collect();
        if folder.is_empty() {
            return Err("넣을 폴더 이름을 적어 주세요.".into());
        }
        for name in &folder {
            crate::folders::validate_name(name)?;
        }
        let mut words: Vec<String> = Vec::new();
        for w in g.words.iter().map(|w| w.trim()).filter(|w| !w.is_empty()) {
            if !words.iter().any(|x| x.to_lowercase() == w.to_lowercase()) {
                words.push(w.to_string());
            }
        }
        if words.is_empty() {
            return Err(format!(
                "‘{}’ 폴더에 단어를 하나 이상 적어 주세요.",
                folder.join(" › ")
            ));
        }
        out.push(WordGroup { folder, words });
    }
    Ok(out)
}

pub fn save_words(conn: &Connection, groups: &[WordGroup]) -> Result<(), String> {
    let json = serde_json::to_string(groups).map_err(|e| e.to_string())?;
    crate::db::set_value(conn, WORDS_KEY, &json).map_err(|e| format!("저장하지 못했어요: {e}"))
}

/// 형식별 폴더 이름. 모르는 형식(설치 파일 등)은 None.
pub fn type_folder(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "hwp" | "hwpx" | "hwt" => "한글 문서",
        "doc" | "docx" | "odt" | "rtf" => "워드 문서",
        "pdf" => "PDF",
        "xls" | "xlsx" | "xlsm" | "csv" | "cell" | "ods" => "엑셀",
        "ppt" | "pptx" | "show" | "odp" | "key" => "발표 자료",
        "jpg" | "jpeg" | "png" | "gif" | "heic" | "webp" | "bmp" | "tif" | "tiff" => "사진",
        "mp4" | "mov" | "avi" | "mkv" | "wmv" => "동영상",
        "mp3" | "wav" | "m4a" => "소리",
        "zip" | "7z" | "rar" | "egg" | "alz" => "압축 파일",
        "txt" | "md" => "텍스트",
        _ => return None,
    })
}

/// 집·개인의 종류 폴더 이름(정리 폴더 바로 아래, 처음 설정의 예시 폴더와 같음). 모르는 형식은 None.
pub fn kind_folder(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "hwp" | "hwpx" | "hwt" | "doc" | "docx" | "odt" | "rtf" | "pdf" | "xls" | "xlsx"
        | "xlsm" | "csv" | "cell" | "ods" | "ppt" | "pptx" | "show" | "odp" | "key" | "txt"
        | "md" => "문서",
        "jpg" | "jpeg" | "png" | "gif" | "heic" | "webp" | "bmp" | "tif" | "tiff" => "사진",
        "mp4" | "mov" | "avi" | "mkv" | "wmv" => "동영상",
        "mp3" | "wav" | "m4a" | "flac" | "aac" | "ogg" => "음악",
        "zip" | "7z" | "rar" | "egg" | "alz" | "exe" | "msi" => "압축·설치",
        _ => return None,
    })
}

/// 왜 그 폴더인지
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Reason {
    /// 사용자가 만든 정리 규칙
    Rule { rule: Rule },
    /// 이름에 정리 폴더의 폴더 이름이 들어 있음
    Folder { word: String },
    /// 이름에 자동 분류 단어가 들어 있음
    Word { word: String },
    /// 이름으로는 몰라서 파일 형식으로
    Type { type_name: String },
}

struct FolderName {
    lower: String,
    path: PathBuf,
}

/// 한 번 만들어 여러 파일에 쓴다(정리함 목록처럼 파일이 많을 때 폴더를 한 번만 읽게).
pub struct Classifier {
    root: PathBuf,
    rules: Vec<Rule>,
    words: Vec<WordGroup>,
    folders: Vec<FolderName>,
    today: (i32, u32),
    months: TermMonths,
    kind: UserKind,
}

/// 폴더 이름이 분류 기준이 될 만한지: 두 글자 이상, 숫자만은 아님, 연도·학기·학급 폴더는 아님.
fn usable_folder_name(name: &str) -> bool {
    name.chars().count() >= 2
        && !name.chars().all(|c| c.is_ascii_digit())
        && naming::folder_template(name).is_none()
        && !name.starts_with('.')
}

/// 정리 폴더 아래 폴더들(얕은 것부터). 형식별 폴더 안은 뺀다.
fn scan_folders(root: &Path) -> Vec<FolderName> {
    let mut out = Vec::new();
    let mut queue = VecDeque::from([(root.to_path_buf(), 0usize)]);
    while let Some((dir, depth)) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut children: Vec<PathBuf> = entries
            .flatten()
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .map(|e| e.path())
            .collect();
        children.sort();
        for child in children {
            let name = child
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if depth == 0 && name == TYPE_PARENT {
                continue;
            }
            if usable_folder_name(&name) {
                out.push(FolderName {
                    lower: name.to_lowercase(),
                    path: child.clone(),
                });
            }
            if out.len() >= MAX_FOLDERS {
                return out;
            }
            if depth + 1 < MAX_DEPTH {
                queue.push_back((child, depth + 1));
            }
        }
    }
    out
}

fn target_for(dir: PathBuf) -> Target {
    let mut existing = dir.clone();
    while !existing.is_dir() && existing.pop() {}
    Target {
        exists: dir.is_dir(),
        dir,
        existing,
    }
}

impl Classifier {
    pub fn new(
        root: &Path,
        rules: Vec<Rule>,
        words: Vec<WordGroup>,
        today: (i32, u32),
        months: TermMonths,
        kind: UserKind,
    ) -> Self {
        Classifier {
            root: root.to_path_buf(),
            folders: scan_folders(root),
            rules,
            words,
            today,
            months,
            kind,
        }
    }

    /// 정리 규칙만(바로 정리 모드)
    pub fn by_rule(&self, path: &Path) -> Option<(Rule, Target)> {
        let name = path.file_name()?.to_string_lossy().into_owned();
        let source = path.parent()?;
        let rule = rules::find_match(&self.rules, &name, source)?;
        let target = rules::resolve(rule, &self.root, &name, self.today, self.months)?;
        Some((rule.clone(), target))
    }

    /// 추천(팝업·정리함 줄): 규칙 → 이름 단어
    pub fn suggest(&self, path: &Path) -> Option<(Reason, Target)> {
        if let Some((rule, target)) = self.by_rule(path) {
            return Some((Reason::Rule { rule }, target));
        }
        let name = path.file_name()?.to_string_lossy();
        self.by_name(&naming::stem(&name).to_lowercase())
    }

    /// 자동 분류: 추천 → 파일 형식. 그래도 모르면 None(정리함에 남김).
    pub fn classify(&self, path: &Path) -> Option<(Reason, Target)> {
        if let Some(found) = self.suggest(path) {
            return Some(found);
        }
        let name = path.file_name()?.to_string_lossy();
        let ext = naming::extension(&name);
        let (type_name, dir) = match self.kind {
            UserKind::School => {
                let t = type_folder(&ext)?;
                (t, self.root.join(TYPE_PARENT).join(t))
            }
            UserKind::General => {
                let t = kind_folder(&ext)?;
                (t, self.root.join(t))
            }
        };
        Some((
            Reason::Type {
                type_name: type_name.to_string(),
            },
            target_for(dir),
        ))
    }

    /// 이름에 든 가장 긴 단어로: 폴더 이름 또는 자동 분류 단어(같은 길이면 폴더 이름).
    fn by_name(&self, stem: &str) -> Option<(Reason, Target)> {
        let mut best: Option<(usize, Reason, Target)> = None;
        let mut consider = |len: usize, make: &dyn Fn() -> Option<(Reason, Target)>| {
            if best.as_ref().is_none_or(|(l, _, _)| len > *l) {
                if let Some((reason, target)) = make() {
                    best = Some((len, reason, target));
                }
            }
        };
        for f in &self.folders {
            if stem.contains(&f.lower) {
                let word = f
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                consider(f.lower.chars().count(), &|| {
                    Some((
                        Reason::Folder { word: word.clone() },
                        target_for(f.path.clone()),
                    ))
                });
            }
        }
        for g in &self.words {
            for w in &g.words {
                if stem.contains(&w.to_lowercase()) {
                    consider(w.chars().count(), &|| {
                        Some((Reason::Word { word: w.clone() }, self.group_target(g)?))
                    });
                }
            }
        }
        best.map(|(_, reason, target)| (reason, target))
    }

    /// 단어 묶음의 폴더: 같은 이름의 폴더가 있으면(얕은 것부터) 거기, 없으면 정해 둔 자리(넣을 때 만든다).
    fn group_target(&self, g: &WordGroup) -> Option<Target> {
        let leaf = g.folder.last()?.to_lowercase();
        if let Some(f) = self.folders.iter().find(|f| f.lower == leaf) {
            return Some(target_for(f.path.clone()));
        }
        let mut dir = self.root.clone();
        for n in &g.folder {
            dir.push(n);
        }
        Some(target_for(dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree() -> (tempfile::TempDir, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("정리 폴더");
        for p in [
            "업무/공문",
            "업무/회의록",
            "평가/수행평가/2027/1학기",
            "평가/지필고사",
            "학급/사진",
            "학급/상담",
            "수업",
            "개인",
            "형식별/상담",
        ] {
            fs::create_dir_all(root.join(p)).unwrap();
        }
        (d, root)
    }

    fn classifier(root: &Path, rules: Vec<Rule>) -> Classifier {
        Classifier::new(
            root,
            rules,
            default_words(UserKind::School),
            (2027, 4),
            TermMonths::default(),
            UserKind::School,
        )
    }

    fn reason_and_dir(c: &Classifier, name: &str) -> Option<(Reason, PathBuf)> {
        c.classify(&PathBuf::from("/home/Downloads").join(name))
            .map(|(r, t)| (r, t.dir))
    }

    #[test]
    fn folder_names_in_file_name() {
        let (_d, root) = tree();
        let c = classifier(&root, vec![]);
        let (r, dir) = reason_and_dir(&c, "학업성적관리위원회_회의록_10월.hwp").unwrap();
        assert_eq!(
            r,
            Reason::Folder {
                word: "회의록".into()
            }
        );
        assert_eq!(dir, root.join("업무").join("회의록"));
        // 형식별 폴더 안의 폴더 이름은 기준이 아니다
        let (_, dir) = reason_and_dir(&c, "3-2_학부모상담_일정.xlsx").unwrap();
        assert_eq!(dir, root.join("학급").join("상담"));
        // 연도 폴더 이름(2027)은 기준이 아니다
        let (r, _) = reason_and_dir(&c, "2027_안내.pdf").unwrap();
        assert!(matches!(r, Reason::Type { .. }));
    }

    #[test]
    fn longest_word_wins_and_missing_word_folder_is_created_later() {
        let (_d, root) = tree();
        let c = classifier(&root, vec![]);
        // ‘시행문’(3글자, 단어) → 공문 폴더(이미 있음)
        let (r, dir) = reason_and_dir(&c, "2027_시행문_학교폭력.pdf").unwrap();
        assert_eq!(
            r,
            Reason::Word {
                word: "시행문".into()
            }
        );
        assert_eq!(dir, root.join("업무").join("공문"));
        // ‘가정통신문’ → 업무/가정통신문(아직 없음 → 넣을 때 만든다)
        let t = c
            .classify(Path::new("/d/10월_가정통신문.hwp"))
            .map(|(_, t)| t)
            .unwrap();
        assert_eq!(t.dir, root.join("업무").join("가정통신문"));
        assert!(!t.exists);
        assert_eq!(t.existing, root.join("업무"));
        // ‘수행평가’(폴더 이름, 4글자)가 ‘채점기준’(단어, 4글자)과 같으면 폴더 이름이 먼저
        let (r, _) = reason_and_dir(&c, "수행평가_채점기준.hwp").unwrap();
        assert_eq!(
            r,
            Reason::Folder {
                word: "수행평가".into()
            }
        );
        // ‘중간고사’(단어 4글자) > ‘수업’(폴더 2글자)
        let (r, dir) = reason_and_dir(&c, "수업_중간고사_문제.hwp").unwrap();
        assert_eq!(
            r,
            Reason::Word {
                word: "중간고사".into()
            }
        );
        assert_eq!(dir, root.join("평가").join("지필고사"));
    }

    #[test]
    fn word_folder_found_elsewhere_by_same_name() {
        let (_d, root) = tree();
        fs::create_dir_all(root.join("연수 자료").join("연수")).unwrap();
        let c = classifier(&root, vec![]);
        let (_, dir) = reason_and_dir(&c, "이수증_2027.pdf").unwrap();
        assert_eq!(dir, root.join("연수 자료").join("연수"));
    }

    #[test]
    fn type_fallback_and_unknown_stays() {
        let (_d, root) = tree();
        let c = classifier(&root, vec![]);
        let (r, dir) = reason_and_dir(&c, "scan_20261013.PDF").unwrap();
        assert_eq!(
            r,
            Reason::Type {
                type_name: "PDF".into()
            }
        );
        assert_eq!(dir, root.join(TYPE_PARENT).join("PDF"));
        assert!(reason_and_dir(&c, "setup_v2.exe").is_none());
        assert!(reason_and_dir(&c, "README").is_none());
        // 추천에는 형식이 들어가지 않는다
        assert!(c.suggest(Path::new("/d/scan_001.pdf")).is_none());
    }

    #[test]
    fn rules_come_first() {
        let (_d, root) = tree();
        let rule = Rule {
            id: 7,
            words: vec!["회의록".into()],
            extensions: vec![],
            source_dir: None,
            dest_dir: root.join("개인").to_string_lossy().into_owned(),
            split: false,
            used: 0,
            any_word: false,
        };
        let c = classifier(&root, vec![rule]);
        let (r, dir) = reason_and_dir(&c, "회의록.hwp").unwrap();
        assert!(matches!(r, Reason::Rule { rule } if rule.id == 7));
        assert_eq!(dir, root.join("개인"));
    }

    #[test]
    fn words_normalize_and_roundtrip() {
        let conn = crate::db::open_in_memory().unwrap();
        assert_eq!(load_words(&conn), default_words(UserKind::School));
        // 일반 사용자는 일반 기본 단어(고친 적이 없을 때)
        let s = crate::settings::Settings {
            user_kind: UserKind::General,
            ..crate::settings::Settings::default()
        };
        crate::settings::save(&conn, &s).unwrap();
        assert!(load_words(&conn).is_empty());
        crate::settings::save(&conn, &crate::settings::Settings::default()).unwrap();
        let groups = normalize_words(vec![WordGroup {
            folder: vec![" 업무 ".into(), "".into(), "출장".into()],
            words: vec![" 출장 ".into(), "출장".into(), "".into(), "여비".into()],
        }])
        .unwrap();
        assert_eq!(groups[0].folder, ["업무", "출장"]);
        assert_eq!(groups[0].words, ["출장", "여비"]);
        save_words(&conn, &groups).unwrap();
        assert_eq!(load_words(&conn), groups);
        assert!(normalize_words(vec![WordGroup {
            folder: vec!["나쁜:이름".into()],
            words: vec!["a".into()]
        }])
        .is_err());
        assert!(normalize_words(vec![WordGroup {
            folder: vec!["출장".into()],
            words: vec![" ".into()]
        }])
        .is_err());
    }

    #[test]
    fn general_users_sort_by_kind_into_top_folders() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("정리 폴더");
        fs::create_dir_all(root.join("문서").join("계약서")).unwrap();
        let c = Classifier::new(
            &root,
            vec![],
            default_words(UserKind::General),
            (2027, 4),
            TermMonths::default(),
            UserKind::General,
        );
        let dir = |n: &str| c.classify(&PathBuf::from("/d").join(n)).map(|(_, t)| t.dir);
        assert_eq!(dir("scan_001.pdf"), Some(root.join("문서")));
        assert_eq!(dir("IMG_2031.HEIC"), Some(root.join("사진")));
        assert_eq!(dir("setup_v2.exe"), Some(root.join("압축·설치")));
        assert_eq!(dir("노래.flac"), Some(root.join("음악")));
        // 사용자가 만든 안쪽 폴더는 이름으로 찾는다
        assert_eq!(
            dir("전세_계약서_2027.pdf"),
            Some(root.join("문서").join("계약서"))
        );
        assert_eq!(dir("README"), None);
    }

    #[test]
    fn type_folders() {
        assert_eq!(type_folder("hwpx"), Some("한글 문서"));
        assert_eq!(type_folder("jpeg"), Some("사진"));
        assert_eq!(type_folder("exe"), None);
        assert_eq!(type_folder(""), None);
    }
}
