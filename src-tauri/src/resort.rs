//! 기존 폴더 다시 정리하기(SPEC 3-2): '다른 폴더 정리하기'(정리 폴더 밖 폴더를 골라 정리 폴더로)와
//! '이 폴더 다시 나누기'(정리 폴더 안 폴더의 파일을 다시 나눔)에 쓰는 파일 읽기와 새 폴더 제안.
//!
//! - 파일을 옮기지는 않는다(미리보기용). 옮기기는 화면에서 확인한 뒤 put_files가 한다(기록·되돌리기 가능).
//! - 저장이 끝나지 않은 파일·바로 가기·숨긴 파일은 빼고 읽는다(watcher와 같은 기준).
//! - 새 폴더 제안: 이름에 같은 낱말이 든 파일이 여럿이면(MIN_GROUP개 이상) 그 낱말로 폴더를 제안한다.

use crate::folders;
use crate::naming;
use crate::watcher;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

/// 한 번에 읽는 파일 수 상한(아주 큰 폴더라도 창이 느려지지 않게)
pub const MAX_FILES: usize = 2000;
/// 안쪽 폴더를 읽을 깊이
const MAX_DEPTH: usize = 6;
/// 새 폴더를 제안하려면 같은 낱말이 든 파일이 몇 개 있어야 하는지
pub const MIN_GROUP: usize = 3;
/// 한 번에 제안하는 새 폴더 수
const MAX_GROUPS: usize = 12;

/// 폴더 이름으로 쓰기에 뜻이 없는 낱말(사본·최종·스캔·카카오톡 사진 이름 등)
const STOP_WORDS: &[&str] = &[
    "최종",
    "수정",
    "수정본",
    "최종본",
    "사본",
    "복사본",
    "원본",
    "파일",
    "자료",
    "문서",
    "사진",
    "이미지",
    "스캔",
    "제목",
    "없음",
    "새로운",
    "학기",
    "학년",
    "학년도",
    "학급",
    "copy",
    "final",
    "scan",
    "img",
    "image",
    "photo",
    "file",
    "doc",
    "document",
    "download",
    "screenshot",
    "스크린샷",
    "kakaotalk",
    "dsc",
    "untitled",
    "new",
    "the",
    "and",
    "pdf",
    "hwp",
    "jpg",
    "png",
];

/// 고른 폴더를 정리해도 되는지. Windows·프로그램 폴더는 건드리지 않는다.
pub fn check_dir(dir: &Path) -> Result<(), String> {
    if !dir.is_dir() {
        return Err("폴더를 찾을 수 없어요.".into());
    }
    #[cfg(windows)]
    for var in [
        "SystemRoot",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramData",
        "APPDATA",
        "LOCALAPPDATA",
    ] {
        if let Ok(v) = std::env::var(var) {
            let v = Path::new(&v);
            if folders::same_dir(v, dir) || folders::is_inside(v, dir) {
                return Err("Windows·프로그램 폴더는 정리하지 않아요.".into());
            }
        }
    }
    Ok(())
}

/// 폴더의 파일(정리할 수 있는 것만). `include_sub`면 안쪽 폴더까지, `skip` 폴더(정리 폴더 등)는 빼고.
/// 상한을 넘으면 거기까지만 읽고 `true`를 함께 돌려준다.
pub fn list(
    dir: &Path,
    include_sub: bool,
    skip: Option<&Path>,
    limit: usize,
) -> (Vec<PathBuf>, bool) {
    let mut out = Vec::new();
    let mut queue = VecDeque::from([(dir.to_path_buf(), 0usize)]);
    while let Some((d, depth)) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                let hidden = name.starts_with('.') || name.starts_with('$');
                let skipped = skip.is_some_and(|s| folders::same_dir(s, &path));
                if include_sub && depth + 1 < MAX_DEPTH && !hidden && !skipped {
                    queue.push_back((path, depth + 1));
                }
                continue;
            }
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            if !ft.is_file()
                || size == 0
                || !watcher::is_candidate_name(&name)
                || watcher::has_partial_partner(&path)
            {
                continue;
            }
            if out.len() >= limit {
                return (out, true);
            }
            out.push(path);
        }
    }
    (out, false)
}

/// 파일 이름(확장자 뺀 부분)의 낱말: 한글 두 글자 이상, 영어 세 글자 이상. 숫자·기호에서 끊는다.
pub fn words(file_name: &str) -> Vec<String> {
    #[derive(PartialEq, Clone, Copy)]
    enum Kind {
        Hangul,
        Latin,
        Other,
    }
    let kind = |c: char| match c {
        '가'..='힣' => Kind::Hangul,
        c if c.is_ascii_alphabetic() => Kind::Latin,
        _ => Kind::Other,
    };
    let mut out: Vec<String> = Vec::new();
    let mut push = |token: &str, k: Kind| {
        let n = token.chars().count();
        let ok = match k {
            Kind::Hangul => n >= 2,
            Kind::Latin => n >= 3,
            Kind::Other => false,
        };
        let lower = token.to_lowercase();
        if ok && !STOP_WORDS.contains(&lower.as_str()) && !out.contains(&lower) {
            out.push(lower);
        }
    };
    let stem = naming::stem(file_name);
    let mut token = String::new();
    let mut current = Kind::Other;
    for c in stem.chars() {
        let k = kind(c);
        if k != current {
            push(&token, current);
            token.clear();
            current = k;
        }
        token.push(c);
    }
    push(&token, current);
    out
}

/// 새 폴더 제안: 같은 낱말이 든 파일이 많은 낱말부터(같으면 긴 낱말) 묶는다. 한 파일은 한 묶음에만.
/// `names`의 순서 번호로 돌려준다. `exclude` 낱말(지금 폴더 이름 등)은 제안하지 않는다.
pub fn suggest(names: &[String], exclude: &[String]) -> Vec<(String, Vec<usize>)> {
    let file_words: Vec<HashSet<String>> = names
        .iter()
        .map(|n| {
            words(n)
                .into_iter()
                .filter(|w| !exclude.iter().any(|x| x.to_lowercase() == *w))
                .collect()
        })
        .collect();
    let mut taken = vec![false; names.len()];
    let mut groups = Vec::new();
    while groups.len() < MAX_GROUPS {
        let mut count: HashMap<&str, usize> = HashMap::new();
        for (i, ws) in file_words.iter().enumerate() {
            if taken[i] {
                continue;
            }
            for w in ws {
                *count.entry(w.as_str()).or_default() += 1;
            }
        }
        let best = count
            .into_iter()
            .filter(|(_, n)| *n >= MIN_GROUP)
            .max_by(|a, b| {
                (a.1, a.0.chars().count())
                    .cmp(&(b.1, b.0.chars().count()))
                    // 같으면 가나다 앞쪽(결과가 늘 같게)
                    .then_with(|| b.0.cmp(a.0))
            })
            .map(|(w, _)| w.to_string());
        let Some(word) = best else { break };
        let members: Vec<usize> = (0..names.len())
            .filter(|&i| !taken[i] && file_words[i].contains(&word))
            .collect();
        for &i in &members {
            taken[i] = true;
        }
        groups.push((word, members));
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn words_split_on_digits_and_marks() {
        assert_eq!(words("2026_전세계약서_최종(2).pdf"), ["전세계약서"]);
        assert_eq!(words("Invoice-2027-03 영수증.PDF"), ["invoice", "영수증"]);
        assert_eq!(words("IMG_2031.jpg"), Vec::<String>::new());
        assert_eq!(words("3-2_학부모상담_일정.xlsx"), ["학부모상담", "일정"]);
        // 한 글자·두 글자 영어는 뺀다
        assert_eq!(words("가_ab_정보.txt"), ["정보"]);
    }

    #[test]
    fn suggest_groups_by_most_common_word() {
        let names: Vec<String> = [
            "계약서_전세.pdf",
            "계약서_자동차.pdf",
            "보험_청구.pdf",
            "계약서_차량.pdf",
            "보험_갱신 안내.pdf",
            "보험료_납입.pdf",
            "여행_사진.jpg",
            "보험_해지.pdf",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let groups = suggest(&names, &[]);
        // 둘 다 3개면 긴 낱말(계약서)이 먼저. 보험료는 보험과 다른 낱말
        assert_eq!(
            groups,
            [
                ("계약서".to_string(), vec![0, 1, 3]),
                ("보험".to_string(), vec![2, 4, 7]),
            ]
        );
    }

    #[test]
    fn suggest_needs_enough_files_and_skips_excluded() {
        let names: Vec<String> = ["공문_a.pdf", "공문_b.pdf", "공문_c.pdf"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(suggest(&names, &[]).len(), 1);
        assert!(suggest(&names[..2], &[]).is_empty());
        assert!(suggest(&names, &["공문".to_string()]).is_empty());
    }

    #[test]
    fn list_reads_candidates_and_respects_sub_skip_and_limit() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        fs::create_dir_all(root.join("안쪽")).unwrap();
        fs::create_dir_all(root.join("정리 폴더")).unwrap();
        for p in [
            "a.pdf",
            "b.hwp",
            "바로가기.lnk",
            "받는중.pdf.crdownload",
            "안쪽/c.pdf",
            "정리 폴더/d.pdf",
        ] {
            fs::write(root.join(p), b"x").unwrap();
        }
        fs::write(root.join("빈.pdf"), b"").unwrap();
        let names = |v: Vec<PathBuf>| -> Vec<String> {
            v.iter()
                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
                .collect()
        };
        let (top, cut) = list(root, false, None, MAX_FILES);
        assert_eq!(names(top), ["a.pdf", "b.hwp"]);
        assert!(!cut);
        let (all, _) = list(root, true, Some(&root.join("정리 폴더")), MAX_FILES);
        assert_eq!(names(all), ["a.pdf", "b.hwp", "c.pdf"]);
        let (few, cut) = list(root, true, None, 2);
        assert_eq!(few.len(), 2);
        assert!(cut);
    }

    #[test]
    fn check_dir_needs_existing_folder() {
        let d = tempfile::tempdir().unwrap();
        assert!(check_dir(d.path()).is_ok());
        assert!(check_dir(&d.path().join("없음")).is_err());
    }
}
