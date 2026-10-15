//! 폴더 읽기. 폴더 구조는 DB에 저장하지 않고 매번 디스크를 직접 읽는다(SPEC 8장).

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderEntry {
    pub name: String,
    pub path: String,
    /// 바로 안에 있는 파일 수(숨김 제외).
    pub file_count: usize,
    /// 바로 안에 있는 하위 폴더 수(숨김 제외).
    pub folder_count: usize,
}

/// `dir` 바로 아래 폴더들을 이름순으로 돌려준다. 숨김·시스템 폴더와 바로 가기 링크는 뺀다.
pub fn list_subfolders(dir: &Path) -> std::io::Result<Vec<FolderEntry>> {
    let mut folders: Vec<FolderEntry> = visible_entries(dir)?
        .into_iter()
        .filter(|(_, is_dir)| *is_dir)
        .map(|(path, _)| {
            let (file_count, folder_count) = count_children(&path);
            FolderEntry {
                name: file_name(&path),
                path: path.to_string_lossy().into_owned(),
                file_count,
                folder_count,
            }
        })
        .collect();
    folders.sort_by_key(|f| f.name.to_lowercase());
    Ok(folders)
}

fn count_children(dir: &Path) -> (usize, usize) {
    match visible_entries(dir) {
        Ok(entries) => {
            let folders = entries.iter().filter(|(_, is_dir)| *is_dir).count();
            (entries.len() - folders, folders)
        }
        // 권한이 없는 폴더 등은 0으로 보여 준다.
        Err(_) => (0, 0),
    }
}

/// (경로, 폴더인지) 목록. 숨김·시스템 항목과 심볼릭 링크·정션은 뺀다.
fn visible_entries(dir: &Path) -> std::io::Result<Vec<(PathBuf, bool)>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else { continue };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if is_hidden(&path, &entry) {
            continue;
        }
        out.push((path, file_type.is_dir()));
    }
    Ok(out)
}

fn is_hidden(path: &Path, _entry: &fs::DirEntry) -> bool {
    let name = file_name(path);
    if name.starts_with('.') || name.starts_with('~') {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
        if let Ok(meta) = _entry.metadata() {
            if meta.file_attributes() & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0 {
                return true;
            }
        }
    }
    false
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_only_visible_folders_sorted_with_counts() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("평가/수행평가")).unwrap();
        fs::create_dir_all(root.join("업무")).unwrap();
        fs::create_dir_all(root.join(".숨김")).unwrap();
        fs::write(root.join("평가/채점기준.hwp"), b"x").unwrap();
        fs::write(root.join("평가/.임시"), b"x").unwrap();
        fs::write(root.join("루트파일.txt"), b"x").unwrap();

        let list = list_subfolders(root).unwrap();
        let names: Vec<_> = list.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["업무", "평가"]);
        assert_eq!((list[1].file_count, list[1].folder_count), (1, 1));
        assert_eq!((list[0].file_count, list[0].folder_count), (0, 0));
    }

    #[test]
    fn missing_folder_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_subfolders(&dir.path().join("없음")).is_err());
    }
}
