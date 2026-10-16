//! 파일 검색(SPEC 6장): Everything(PC 전체)을 쓸 수 없을 때의 자체 색인.
//!
//! 정리 폴더(하위 폴더까지)와 지켜볼 폴더(바로 아래 파일)를 읽어 메모리에 목록을 둔다.
//! 한글은 단어 중간도 찾아야 해서(예: ‘평가’로 ‘수행평가’) 글자 포함으로 찾는다.
//! 결과마다 연도·학기·학급(이름 → 없으면 폴더 이름)을 붙여 화면에서 거를 수 있게 한다.

use crate::naming;
use serde::Serialize;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Instant, UNIX_EPOCH};

/// 정리 폴더를 읽을 깊이·개수(아주 큰 폴더에서도 오래 걸리지 않게)
const MAX_DEPTH: usize = 12;
const MAX_FILES: usize = 200_000;

#[derive(Debug, Clone)]
pub struct IndexedFile {
    pub path: PathBuf,
    /// 전체 경로(소문자). 폴더 이름으로도 찾을 수 있게 경로 전체에서 찾는다.
    lower: String,
    /// 파일 이름(소문자). 이름에 들어 있으면 위로.
    name_lower: String,
    pub size: u64,
    /// 고친 시각(1970년부터 밀리초)
    pub modified: i64,
}

impl IndexedFile {
    pub fn new(path: PathBuf, size: u64, modified: i64) -> Self {
        let name_lower = path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        IndexedFile {
            lower: path.to_string_lossy().to_lowercase(),
            name_lower,
            path,
            size,
            modified,
        }
    }
}

/// 자체 색인(메모리)
#[derive(Default)]
pub struct Index {
    pub files: Vec<IndexedFile>,
    pub built_at: Option<Instant>,
}

fn modified_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn hidden_name(name: &str) -> bool {
    name.starts_with('.') || name.starts_with("~$")
}

/// 정리 폴더(하위 폴더까지)와 지켜볼 폴더(바로 아래 파일만)의 파일 목록.
pub fn build(root: Option<&Path>, watch: &[PathBuf]) -> Vec<IndexedFile> {
    let mut out: Vec<IndexedFile> = Vec::new();
    if let Some(root) = root {
        let mut queue = VecDeque::from([(root.to_path_buf(), 0usize)]);
        while let Some((dir, depth)) = queue.pop_front() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if hidden_name(&name) {
                    continue;
                }
                let Ok(meta) = entry.metadata() else {
                    continue;
                };
                if meta.is_dir() {
                    if depth + 1 < MAX_DEPTH {
                        queue.push_back((entry.path(), depth + 1));
                    }
                } else if meta.is_file() {
                    out.push(IndexedFile::new(
                        entry.path(),
                        meta.len(),
                        modified_ms(&meta),
                    ));
                    if out.len() >= MAX_FILES {
                        return out;
                    }
                }
            }
        }
    }
    for dir in watch {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_file() && !hidden_name(&name) && !out.iter().any(|f| f.path == entry.path())
            {
                out.push(IndexedFile::new(
                    entry.path(),
                    meta.len(),
                    modified_ms(&meta),
                ));
            }
        }
    }
    out
}

/// 띄어 쓴 낱말이 모두 경로에 들어 있는 파일. 이름에 모두 들어 있는 파일이 먼저, 그다음 최근에 고친 순.
pub fn find<'a>(files: &'a [IndexedFile], query: &str, limit: usize) -> Vec<&'a IndexedFile> {
    let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if terms.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<(bool, &IndexedFile)> = files
        .iter()
        .filter(|f| terms.iter().all(|t| f.lower.contains(t)))
        .map(|f| (terms.iter().all(|t| f.name_lower.contains(t)), f))
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.modified.cmp(&a.1.modified)));
    hits.into_iter().take(limit).map(|(_, f)| f).collect()
}

/// 거르기에 쓰는 연도·학기·학급: 파일 이름에서, 없으면 폴더 이름에서(가까운 폴더부터).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Facets {
    pub year: Option<i32>,
    pub term: Option<u8>,
    pub class: Option<String>,
}

pub fn facets(path: &Path) -> Facets {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // 폴더 이름(가까운 것부터). 폴더 이름의 점은 확장자가 아니므로 끝에 "."을 붙여 그대로 읽게 한다.
    let folders: Vec<String> = path
        .parent()
        .map(|p| {
            p.components()
                .rev()
                .map(|c| format!("{}.", c.as_os_str().to_string_lossy()))
                .collect()
        })
        .unwrap_or_default();
    Facets {
        year: naming::find_year(&name)
            .or_else(|| folders.iter().find_map(|f| naming::find_year(f))),
        term: naming::find_term(&name)
            .or_else(|| folders.iter().find_map(|f| naming::find_term(f))),
        class: naming::find_class(&name, false)
            .or_else(|| folders.iter().find_map(|f| naming::find_class(f, false)))
            .map(|c| c.folder_name()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn file(p: &str, modified: i64) -> IndexedFile {
        IndexedFile::new(PathBuf::from(p), 1, modified)
    }

    #[test]
    fn finds_by_all_terms_name_first_then_recent() {
        let files = vec![
            file("/r/평가/수행평가/2027/1학기/채점기준.hwp", 10),
            file("/r/평가/수행평가/안내.hwp", 30),
            file("/r/업무/수행평가_계획.hwp", 20),
            file("/r/업무/회의록.hwp", 40),
        ];
        let names = |q: &str| {
            find(&files, q, 10)
                .iter()
                .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        // 이름에 ‘수행평가’가 든 파일이 먼저, 나머지(폴더 이름으로 맞음)는 최근 순
        assert_eq!(
            names("수행평가"),
            ["수행평가_계획.hwp", "안내.hwp", "채점기준.hwp"]
        );
        // 단어 중간(‘평가’)도 찾는다
        assert_eq!(names("평가 2027"), ["채점기준.hwp"]);
        // 대소문자 무시, 빈 검색은 결과 없음
        assert_eq!(names("HWP 회의").len(), 1);
        assert!(names("   ").is_empty());
        assert_eq!(find(&files, "hwp", 2).len(), 2);
    }

    #[test]
    fn facets_from_name_then_folders() {
        let f = facets(Path::new("/r/평가/수행평가/2027/1학기/3-2_채점기준.hwp"));
        assert_eq!(
            f,
            Facets {
                year: Some(2027),
                term: Some(1),
                class: Some("3-2".into())
            }
        );
        // 이름이 먼저
        let f = facets(Path::new("/r/평가/2027/2028_2학기_안내.hwp"));
        assert_eq!((f.year, f.term), (Some(2028), Some(2)));
        assert_eq!(facets(Path::new("/r/업무/회의록.hwp")), Facets::default());
    }

    #[test]
    fn build_reads_root_deeply_and_watch_folders_shallowly() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("정리 폴더");
        fs::create_dir_all(root.join("평가").join("2027")).unwrap();
        fs::write(root.join("평가").join("2027").join("a.hwp"), b"a").unwrap();
        fs::write(root.join("~$a.hwp"), b"x").unwrap(); // 한글·엑셀 임시 파일은 뺀다
        let down = d.path().join("Downloads");
        fs::create_dir_all(down.join("안쪽")).unwrap();
        fs::write(down.join("b.pdf"), b"b").unwrap();
        fs::write(down.join("안쪽").join("c.pdf"), b"c").unwrap(); // 지켜볼 폴더는 바로 아래만
        let files = build(Some(&root), &[down]);
        let mut names: Vec<String> = files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["a.hwp", "b.pdf"]);
    }
}
