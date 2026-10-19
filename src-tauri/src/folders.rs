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

/// 폴더 안 파일 하나(정리함 오른쪽 폴더 칸에서 보여 줌)
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub size: u64,
    /// 고친 시각(1970년부터 밀리초, 모르면 0)
    pub modified: i64,
}

/// `dir` 바로 안의 파일(숨김 제외), 최근에 고친 것부터. 하위 폴더 안은 읽지 않는다.
/// 지금 열어 본 폴더 하나만 읽으므로 가볍다(폴더 칸의 "파일 n개"를 셀 때도 같은 목록을 읽는다).
pub fn list_files(dir: &Path) -> std::io::Result<Vec<FileEntry>> {
    let mut files: Vec<FileEntry> = visible_entries(dir)?
        .into_iter()
        .filter(|(_, is_dir)| !*is_dir)
        // 백신 미끼 파일은 보여 주지 않는다(끌어서 옮기지 않게)
        .filter(|(path, _)| !crate::watcher::is_decoy(path))
        .filter_map(|(path, _)| {
            let meta = fs::metadata(&path).ok()?;
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            Some(FileEntry {
                name: file_name(&path),
                path: path.to_string_lossy().into_owned(),
                size: meta.len(),
                modified,
            })
        })
        .collect();
    files.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(files)
}

/// 정리 폴더 자체이거나 그 안쪽인지(읽기만 하는 곳에 쓴다)
pub fn is_root_or_inside(root: &Path, path: &Path) -> bool {
    same_file(root, path) || is_inside(root, path)
}

/// 파일·폴더 이름으로 쓸 수 있는지(Windows 기준). 문제가 있으면 사용자에게 보여 줄 문장.
pub fn validate_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("이름을 입력해 주세요.".into());
    }
    if let Some(c) = name.chars().find(|c| {
        matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control()
    }) {
        return Err(format!("이름에 ‘{c}’ 글자는 쓸 수 없어요."));
    }
    if name.ends_with('.') || name == ".." {
        return Err("이름 끝에 마침표를 쓸 수 없어요.".into());
    }
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.contains(&stem.as_str()) {
        return Err(format!("‘{name}’은 Windows에서 쓸 수 없는 이름이에요."));
    }
    Ok(())
}

/// `parent` 안에 새 폴더를 만든다. 같은 이름이 이미 있으면 만들지 않고 알려 준다.
pub fn create_folder(parent: &Path, name: &str) -> Result<FolderEntry, String> {
    validate_name(name)?;
    let name = name.trim();
    if !parent.is_dir() {
        return Err("폴더를 만들 위치를 찾을 수 없어요.".into());
    }
    let path = parent.join(name);
    match fs::create_dir(&path) {
        Ok(()) => Ok(FolderEntry {
            name: name.to_string(),
            path: path.to_string_lossy().into_owned(),
            file_count: 0,
            folder_count: 0,
        }),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(format!("‘{name}’ 폴더가 이미 있어요."))
        }
        Err(e) => Err(format!("폴더를 만들지 못했어요: {e}")),
    }
}

/// 폴더 이름 바꾸기. 같은 이름이 있으면 바꾸지 않는다. 새 경로를 돌려준다.
pub fn rename_folder(path: &Path, new_name: &str) -> Result<PathBuf, String> {
    validate_name(new_name)?;
    let new_name = new_name.trim();
    if !path.is_dir() {
        return Err("폴더를 찾을 수 없어요.".into());
    }
    let parent = path.parent().ok_or("이 폴더는 이름을 바꿀 수 없어요.")?;
    let target = parent.join(new_name);
    // 대소문자만 바꾸는 경우(Windows)에는 같은 폴더로 보이므로 그대로 진행한다.
    let same_folder = target.exists() && same_file(&target, path);
    if target.exists() && !same_folder {
        return Err(format!("‘{new_name}’ 폴더가 이미 있어요."));
    }
    fs::rename(path, &target).map_err(|e| describe_io(&e, "이름을 바꾸지 못했어요"))?;
    Ok(target)
}

/// 폴더를 다른 폴더 안으로 옮기기. 같은 이름이 있거나 자기 안으로 옮기려 하면 막는다. 새 경로를 돌려준다.
pub fn move_folder(path: &Path, dest_parent: &Path) -> Result<PathBuf, String> {
    if !path.is_dir() {
        return Err("옮길 폴더를 찾을 수 없어요.".into());
    }
    if !dest_parent.is_dir() {
        return Err("옮겨 넣을 폴더를 찾을 수 없어요.".into());
    }
    let (Ok(src), Ok(dest)) = (path.canonicalize(), dest_parent.canonicalize()) else {
        return Err("폴더 위치를 확인하지 못했어요.".into());
    };
    if dest.starts_with(&src) {
        return Err("폴더를 자기 안으로 옮길 수는 없어요.".into());
    }
    if path.parent().and_then(|p| p.canonicalize().ok()) == Some(dest.clone()) {
        return Err("이미 그 폴더 안에 있어요.".into());
    }
    let name = path.file_name().ok_or("폴더 이름을 읽을 수 없어요.")?;
    let target = dest_parent.join(name);
    if target.exists() {
        return Err(format!(
            "옮길 곳에 ‘{}’ 폴더가 이미 있어요.",
            name.to_string_lossy()
        ));
    }
    fs::rename(path, &target).map_err(|e| describe_io(&e, "옮기지 못했어요"))?;
    Ok(target)
}

/// `path`가 `root` 안쪽(root 자신은 아님)에 있는지.
pub fn is_inside(root: &Path, path: &Path) -> bool {
    match (root.canonicalize(), path.canonicalize()) {
        (Ok(r), Ok(p)) => p != r && p.starts_with(&r),
        _ => false,
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}

/// 같은 폴더인지. 둘 다 있으면 실제 위치로, 하나라도 없으면 글자로(대소문자·끝의 구분자 무시) 비교한다.
pub fn same_dir(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        p.to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .replace('/', "\\")
            .to_lowercase()
    };
    same_file(a, b) || norm(a) == norm(b)
}

fn describe_io(e: &std::io::Error, what: &str) -> String {
    // Windows ERROR_SHARING_VIOLATION = 32, ERROR_ACCESS_DENIED = 5
    if cfg!(windows) && matches!(e.raw_os_error(), Some(32) | Some(5)) {
        return format!("{what}. 폴더 안 파일이 다른 프로그램에서 열려 있는지 확인해 주세요.");
    }
    format!("{what}: {e}")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderMatch {
    pub name: String,
    pub path: String,
    /// 정리 폴더부터의 경로 이름들(예: ["평가", "수행평가"])
    pub trail: Vec<String>,
}

/// 정리 폴더 아래에서 이름에 `query`가 들어간 폴더를 찾는다(대소문자 무시, 얕은 것부터).
pub fn search(root: &Path, query: &str, limit: usize) -> Vec<FolderMatch> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut queue: std::collections::VecDeque<(PathBuf, Vec<String>)> =
        std::collections::VecDeque::from([(root.to_path_buf(), Vec::new())]);
    let mut visited = 0;
    while let Some((dir, trail)) = queue.pop_front() {
        visited += 1;
        if visited > 5_000 || trail.len() > 10 {
            break;
        }
        let Ok(children) = list_subfolders(&dir) else {
            continue;
        };
        for child in children {
            let mut child_trail = trail.clone();
            child_trail.push(child.name.clone());
            if child.name.to_lowercase().contains(&query) {
                out.push(FolderMatch {
                    name: child.name.clone(),
                    path: child.path.clone(),
                    trail: child_trail.clone(),
                });
                if out.len() >= limit {
                    return out;
                }
            }
            queue.push_back((PathBuf::from(child.path), child_trail));
        }
    }
    out
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
    fn list_files_skips_folders_and_hidden_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("하위")).unwrap();
        fs::write(dir.path().join("하위").join("안쪽.hwp"), "x").unwrap();
        fs::write(dir.path().join("~$임시.hwp"), "x").unwrap();
        fs::write(dir.path().join("옛날.pdf"), "abc").unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        fs::File::options()
            .write(true)
            .open(dir.path().join("옛날.pdf"))
            .unwrap()
            .set_modified(old)
            .unwrap();
        fs::write(dir.path().join("새것.hwp"), "x").unwrap();
        let files = list_files(dir.path()).unwrap();
        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["새것.hwp", "옛날.pdf"]);
        assert_eq!(files[1].size, 3);
        assert!(files[0].modified > files[1].modified);
    }

    #[test]
    fn root_or_inside() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("업무");
        fs::create_dir(&inner).unwrap();
        assert!(is_root_or_inside(dir.path(), dir.path()));
        assert!(is_root_or_inside(dir.path(), &inner));
        let other = tempfile::tempdir().unwrap();
        assert!(!is_root_or_inside(dir.path(), other.path()));
    }

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
    fn validates_names() {
        assert!(validate_name("수행평가").is_ok());
        assert!(validate_name("2027 1학기").is_ok());
        assert!(validate_name("  ").is_err());
        assert!(validate_name("a/b").is_err());
        assert!(validate_name("a:b").is_err());
        assert!(validate_name("끝.").is_err());
        assert!(validate_name("con").is_err());
        assert!(validate_name("CON.txt").is_err());
        assert!(validate_name("console").is_ok());
    }

    #[test]
    fn creates_folder_once() {
        let dir = tempfile::tempdir().unwrap();
        let made = create_folder(dir.path(), " 상담 ").unwrap();
        assert_eq!(made.name, "상담");
        assert!(dir.path().join("상담").is_dir());
        assert!(create_folder(dir.path(), "상담").is_err());
        assert!(create_folder(&dir.path().join("없음"), "x").is_err());
    }

    #[test]
    fn search_finds_nested_folders_shallow_first() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("평가/수행평가/2027")).unwrap();
        fs::create_dir_all(root.join("수행평가 자료")).unwrap();
        fs::create_dir_all(root.join("업무")).unwrap();
        let found = search(root, "수행", 10);
        let trails: Vec<_> = found.iter().map(|m| m.trail.join("/")).collect();
        assert_eq!(trails, ["수행평가 자료", "평가/수행평가"]);
        assert!(search(root, " ", 10).is_empty());
        assert_eq!(search(root, "수행", 1).len(), 1);
    }

    #[test]
    fn rename_folder_keeps_contents_and_refuses_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("수업/1학기")).unwrap();
        fs::create_dir_all(root.join("평가")).unwrap();
        fs::write(root.join("수업/자료.hwp"), b"x").unwrap();

        let renamed = rename_folder(&root.join("수업"), "수업자료").unwrap();
        assert_eq!(renamed, root.join("수업자료"));
        assert_eq!(fs::read(root.join("수업자료/자료.hwp")).unwrap(), b"x");
        assert!(root.join("수업자료/1학기").is_dir());

        assert!(rename_folder(&root.join("수업자료"), "평가").is_err());
        assert!(rename_folder(&root.join("수업자료"), "나쁜/이름").is_err());
        assert!(root.join("수업자료").is_dir());
    }

    #[test]
    fn move_folder_rules() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("업무/공문")).unwrap();
        fs::create_dir_all(root.join("평가")).unwrap();
        fs::write(root.join("업무/공문/a.pdf"), b"a").unwrap();

        let moved = move_folder(&root.join("업무/공문"), &root.join("평가")).unwrap();
        assert_eq!(moved, root.join("평가/공문"));
        assert_eq!(fs::read(root.join("평가/공문/a.pdf")).unwrap(), b"a");

        // 자기 안으로, 이미 있는 곳으로, 같은 이름이 있는 곳으로는 못 옮김
        assert!(move_folder(&root.join("평가"), &root.join("평가/공문")).is_err());
        assert!(move_folder(&root.join("평가/공문"), &root.join("평가")).is_err());
        fs::create_dir_all(root.join("업무/공문")).unwrap();
        assert!(move_folder(&root.join("평가/공문"), &root.join("업무")).is_err());
        assert!(root.join("평가/공문/a.pdf").exists());
    }

    #[test]
    fn inside_check_excludes_root_itself() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("a/b")).unwrap();
        assert!(is_inside(root, &root.join("a")));
        assert!(is_inside(root, &root.join("a/b")));
        assert!(!is_inside(root, root));
        assert!(!is_inside(&root.join("a"), root));
    }

    #[test]
    fn missing_folder_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_subfolders(&dir.path().join("없음")).is_err());
    }
}
