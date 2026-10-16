//! 파일 검색 자체 색인(SPEC 6장).
//!
//! Everything이 없어도 PC 전체(고정 디스크. Windows·프로그램·AppData 같은 시스템 폴더와 숨김 폴더는 뺀다)를
//! 읽어 메모리에 목록을 둔다. 설정에서 "정리 폴더와 지켜볼 폴더만"을 고르면 그 범위만 읽는다.
//! - 폴더는 한 번만 저장하고 파일은 폴더 번호 + 이름만 들고 있어, 수십만 개도 메모리를 적게 쓴다.
//! - 한글은 단어 중간도 찾아야 해서(‘평가’로 ‘수행평가’) 글자 포함으로 찾는다.
//! - 다 읽은 뒤에는 바뀐 폴더만 다시 읽어(`refresh_dirs`) 목록을 최신으로 맞춘다.
//! - 이 모듈은 폴더를 읽기만 한다. 파일을 옮기거나 지우지 않는다.

use crate::naming;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Instant, UNIX_EPOCH};

/// 맨 위 폴더(부모 없음)
const NONE: u32 = u32::MAX;
/// 너무 깊은 폴더(잘못 만든 반복 구조 등)는 더 들어가지 않는다.
const MAX_DEPTH: usize = 40;
/// 이보다 많으면 더 읽지 않는다(메모리 약 100MB).
pub const MAX_FILES: usize = 1_500_000;

/// 어느 깊이에서든 들어가지 않는 폴더(소문자)
const SKIP_DIRS: &[&str] = &[
    "appdata",
    "node_modules",
    "__pycache__",
    "$recycle.bin",
    "system volume information",
];
/// 드라이브 바로 아래에서 들어가지 않는 폴더(소문자): Windows·프로그램·복구용
const SKIP_TOP_DIRS: &[&str] = &[
    "windows",
    "windows.old",
    "program files",
    "program files (x86)",
    "programdata",
    "recovery",
    "perflogs",
    "msocache",
    "config.msi",
    "onedrivetemp",
    "intel",
    "amd",
    "nvidia",
    "drivers",
    "esd",
    "inetpub",
    "xboxgames",
];
/// 찾아 줄 필요 없는 파일(저장 중·임시·바로 가기)
const SKIP_EXTENSIONS: &[&str] = &[
    "crdownload",
    "part",
    "download",
    "tmp",
    "temp",
    "lnk",
    "url",
    "ini",
];

/// 읽을 곳 하나
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Root {
    pub path: PathBuf,
    /// 하위 폴더까지 읽는지(지켜볼 폴더는 바로 아래 파일만)
    pub deep: bool,
}

struct Dir {
    parent: u32,
    /// 맨 위 폴더는 전체 경로
    name: Box<str>,
    deep: bool,
    alive: bool,
}

struct Entry {
    dir: u32,
    /// `names` 안의 위치
    off: u32,
    len: u32,
    size: u64,
    /// 고친 시각(1970년부터 밀리초)
    modified: i64,
    alive: bool,
}

/// 찾은 파일
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    pub size: u64,
    pub modified: i64,
}

/// 자체 색인(메모리)
#[derive(Default)]
pub struct Index {
    dirs: Vec<Dir>,
    files: Vec<Entry>,
    /// 파일 이름을 이어 붙인 것(파일마다 따로 할당하지 않아 메모리를 아낀다)
    names: String,
    /// 폴더 전체 경로 → 번호
    lookup: HashMap<PathBuf, u32>,
    roots: Vec<Root>,
    alive_files: usize,
    dead_files: usize,
    /// 다 읽은 시각(아직 안 읽었으면 None)
    pub built_at: Option<Instant>,
    /// 읽은 범위(true = PC 전체)
    pub whole_pc: bool,
}

fn modified_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Windows의 숨김·시스템 속성
#[cfg(windows)]
pub(crate) fn hidden_attr(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    meta.file_attributes() & (HIDDEN | SYSTEM) != 0
}

#[cfg(not(windows))]
pub(crate) fn hidden_attr(_meta: &std::fs::Metadata) -> bool {
    false
}

/// 들어가지 않을 폴더인지. `top` = 드라이브 바로 아래.
pub fn skip_dir(name: &str, top: bool) -> bool {
    if name.starts_with('.') || name.starts_with('$') || name.starts_with('~') {
        return true;
    }
    let lower = name.to_lowercase();
    SKIP_DIRS.contains(&lower.as_str()) || (top && SKIP_TOP_DIRS.contains(&lower.as_str()))
}

/// 목록에 넣지 않을 파일인지(숨김 이름, 한글·엑셀 임시 파일, 저장 중인 파일 등)
pub fn skip_file(name: &str) -> bool {
    if name.starts_with('.') || name.starts_with('~') {
        return true;
    }
    match name.rsplit_once('.') {
        Some((_, ext)) => SKIP_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()),
        None => false,
    }
}

/// 바뀐 곳이 색인에서 빼는 폴더 안인지(예: AppData). 알림이 많이 오는 곳을 빨리 걸러 낸다.
pub fn ignored_path(path: &Path) -> bool {
    let mut parts = path.components().peekable();
    // 드라이브 이름(C:)과 맨 위(\)를 지나면 그다음이 드라이브 바로 아래
    let mut top = false;
    while let Some(c) = parts.peek() {
        match c {
            std::path::Component::Prefix(_) | std::path::Component::RootDir => {
                top = true;
                parts.next();
            }
            _ => break,
        }
    }
    let names: Vec<String> = parts
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    // 마지막(바뀐 파일·폴더 자신)은 빼고 그 위 폴더들만 본다
    let dirs = names.len().saturating_sub(1);
    names
        .iter()
        .take(dirs)
        .enumerate()
        .any(|(i, n)| skip_dir(n, top && i == 0))
}

/// 드라이브 맨 위이거나 Windows·프로그램·AppData 같은 시스템 폴더(또는 그 안)인지(이름 바꾸기에서 막는다)
pub fn system_path(path: &Path) -> bool {
    if path.parent().is_none() {
        return true;
    }
    let mut top = false;
    let mut i = 0;
    for c in path.components() {
        match c {
            std::path::Component::Prefix(_) | std::path::Component::RootDir => top = true,
            other => {
                if skip_dir(&other.as_os_str().to_string_lossy(), top && i == 0) {
                    return true;
                }
                i += 1;
            }
        }
    }
    false
}

/// 대소문자 무시 포함(영문만 접어 비교: 이름마다 소문자 사본을 두지 않으려고). `needle`은 소문자.
fn contains_ci(hay: &str, needle: &str) -> bool {
    let (h, n) = (hay.as_bytes(), needle.as_bytes());
    if n.is_empty() {
        return true;
    }
    if n.len() > h.len() {
        return false;
    }
    h.windows(n.len())
        .any(|w| w.iter().zip(n).all(|(a, b)| a.to_ascii_lowercase() == *b))
}

/// 낱말마다 맞으면 그 자리 비트를 켠다(낱말은 최대 64개).
fn term_mask(text: &str, terms: &[String]) -> u64 {
    terms
        .iter()
        .enumerate()
        .filter(|(_, t)| contains_ci(text, t))
        .fold(0, |m, (i, _)| m | (1 << i))
}

impl Index {
    /// `roots`를 읽어 새 색인을 만든다. `progress`에 지금까지 읽은 파일 수를 적는다.
    pub fn build(roots: &[Root], progress: &AtomicUsize) -> Index {
        let mut index = Index {
            roots: roots.to_vec(),
            ..Index::default()
        };
        for root in roots {
            if !root.path.is_dir() || index.lookup.contains_key(&root.path) {
                continue;
            }
            let id = index.add_dir(NONE, root.path.to_string_lossy().into(), root.deep);
            index.lookup.insert(root.path.clone(), id);
            index.scan(id, root.path.clone(), 0, &mut HashMap::new(), progress);
        }
        index.built_at = Some(Instant::now());
        index
    }

    pub fn roots(&self) -> &[Root] {
        &self.roots
    }

    /// 들고 있는 파일 수
    pub fn len(&self) -> usize {
        self.alive_files
    }

    pub fn is_empty(&self) -> bool {
        self.alive_files == 0
    }

    /// 지운 칸이 많이 쌓였으면(다시 읽어 정리할 때)
    pub fn wasteful(&self) -> bool {
        self.dead_files > 100_000 && self.dead_files > self.alive_files
    }

    fn add_dir(&mut self, parent: u32, name: Box<str>, deep: bool) -> u32 {
        self.dirs.push(Dir {
            parent,
            name,
            deep,
            alive: true,
        });
        (self.dirs.len() - 1) as u32
    }

    fn dir_path(&self, id: u32) -> PathBuf {
        let mut chain = Vec::new();
        let mut cur = id;
        while cur != NONE {
            let d = &self.dirs[cur as usize];
            chain.push(&*d.name);
            cur = d.parent;
        }
        let mut path = PathBuf::new();
        for name in chain.iter().rev() {
            path.push(name);
        }
        path
    }

    /// 이 폴더와 그 위 폴더가 모두 살아 있는지(지운 폴더 안쪽은 찾지 않는다)
    fn chain_alive(&self, id: u32) -> bool {
        let mut cur = id;
        while cur != NONE {
            let d = &self.dirs[cur as usize];
            if !d.alive {
                return false;
            }
            cur = d.parent;
        }
        true
    }

    fn name_of(&self, f: &Entry) -> &str {
        &self.names[f.off as usize..(f.off + f.len) as usize]
    }

    fn push_file(&mut self, dir: u32, name: &str, meta: &std::fs::Metadata) {
        let off = self.names.len() as u32;
        self.names.push_str(name);
        self.files.push(Entry {
            dir,
            off,
            len: name.len() as u32,
            size: meta.len(),
            modified: modified_ms(meta),
            alive: true,
        });
        self.alive_files += 1;
    }

    /// `start` 폴더부터 읽는다(하위 폴더는 deep일 때만).
    /// `known`: 이미 색인에 있는 하위 폴더(다시 읽을 때 그대로 두고, 이번에 보이지 않은 것은 지운다).
    fn scan(
        &mut self,
        start: u32,
        start_path: PathBuf,
        start_depth: usize,
        known: &mut HashMap<String, u32>,
        progress: &AtomicUsize,
    ) {
        let mut stack = vec![(start, start_path, start_depth)];
        let mut first = true;
        while let Some((id, path, depth)) = stack.pop() {
            let mut known_here = if first {
                std::mem::take(known)
            } else {
                HashMap::new()
            };
            first = false;
            let Ok(entries) = std::fs::read_dir(&path) else {
                continue;
            };
            let deep = self.dirs[id as usize].deep;
            let top = self.dirs[id as usize].parent == NONE && path.parent().is_none();
            for entry in entries.flatten() {
                if self.alive_files >= MAX_FILES {
                    return;
                }
                let Ok(name) = entry.file_name().into_string() else {
                    continue; // 이름을 글자로 읽을 수 없는 파일(드묾)
                };
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                // 바로 가기 폴더(심볼릭 링크·연결 지점)는 따라가지 않는다(같은 곳을 두 번 읽거나 끝없이 돌지 않게)
                if kind.is_symlink() {
                    continue;
                }
                if kind.is_dir() {
                    if !deep || depth + 1 > MAX_DEPTH || skip_dir(&name, top) {
                        continue;
                    }
                    if known_here.remove(&name).is_some() {
                        continue; // 이미 색인에 있는 폴더
                    }
                    if entry.metadata().is_ok_and(|m| hidden_attr(&m)) {
                        continue;
                    }
                    let child = self.add_dir(id, name.into(), true);
                    let child_path = entry.path();
                    self.lookup.insert(child_path.clone(), child);
                    stack.push((child, child_path, depth + 1));
                } else if kind.is_file() {
                    if skip_file(&name) {
                        continue;
                    }
                    let Ok(meta) = entry.metadata() else {
                        continue;
                    };
                    if hidden_attr(&meta) {
                        continue;
                    }
                    self.push_file(id, &name, &meta);
                    progress.fetch_add(1, Ordering::Relaxed);
                }
            }
            // 다시 읽은 폴더에서 사라진 하위 폴더는 지운다
            for (_, gone) in known_here {
                self.forget_dir(gone);
            }
        }
    }

    /// 바뀐 파일·폴더 경로들을 받아, 그 위 폴더만 다시 읽는다.
    pub fn refresh_dirs(&mut self, changed: impl IntoIterator<Item = PathBuf>) {
        // 다시 읽을 폴더 = 바뀐 것의 위 폴더 중 색인에 있는 가장 가까운 것
        let mut targets: HashSet<u32> = HashSet::new();
        for p in changed {
            let mut cur = p.parent();
            while let Some(dir) = cur {
                if let Some(&id) = self.lookup.get(dir) {
                    if self.chain_alive(id) {
                        targets.insert(id);
                        break;
                    }
                }
                cur = dir.parent();
            }
        }
        if targets.is_empty() {
            return;
        }
        for f in self.files.iter_mut() {
            if f.alive && targets.contains(&f.dir) {
                f.alive = false;
                self.alive_files -= 1;
                self.dead_files += 1;
            }
        }
        let mut children: HashMap<u32, HashMap<String, u32>> = HashMap::new();
        for (i, d) in self.dirs.iter().enumerate() {
            if d.alive && targets.contains(&d.parent) {
                children
                    .entry(d.parent)
                    .or_default()
                    .insert(d.name.to_string(), i as u32);
            }
        }
        let progress = AtomicUsize::new(0);
        for id in targets {
            let path = self.dir_path(id);
            let mut known = children.remove(&id).unwrap_or_default();
            if !path.is_dir() {
                // 폴더가 없어졌다(위 폴더도 바뀐 것으로 알림이 와서 거기서 정리된다)
                self.forget_dir(id);
                continue;
            }
            self.scan(id, path, 0, &mut known, &progress);
        }
        self.sweep();
    }

    /// 폴더를 지운 것으로. 같은 경로에 새로 만든 폴더가 이미 있으면 그쪽 번호는 그대로 둔다.
    fn forget_dir(&mut self, id: u32) {
        self.dirs[id as usize].alive = false;
        let p = self.dir_path(id);
        if self.lookup.get(&p) == Some(&id) {
            self.lookup.remove(&p);
        }
    }

    /// 지운 폴더 안쪽의 폴더·파일도 지운 것으로(부모가 늘 앞에 있어 한 번에 계산)
    fn sweep(&mut self) {
        let mut ok = vec![false; self.dirs.len()];
        let mut gone: Vec<u32> = Vec::new();
        for i in 0..self.dirs.len() {
            let parent = self.dirs[i].parent;
            let parent_ok = parent == NONE || ok[parent as usize];
            if self.dirs[i].alive && !parent_ok {
                gone.push(i as u32);
            }
            ok[i] = parent_ok && self.dirs[i].alive;
        }
        for id in gone {
            self.forget_dir(id);
        }
        for f in self.files.iter_mut() {
            if f.alive && !ok[f.dir as usize] {
                f.alive = false;
                self.alive_files -= 1;
                self.dead_files += 1;
            }
        }
    }

    /// 띄어 쓴 낱말이 모두 경로(폴더 이름 포함)에 들어 있는 파일.
    /// 이름에 모두 들어 있는 파일이 먼저, 그다음 최근에 고친 순.
    pub fn find(&self, query: &str, limit: usize) -> Vec<Found> {
        let mut terms: Vec<String> = Vec::new();
        for t in query.split_whitespace().map(str::to_lowercase) {
            if !terms.contains(&t) && terms.len() < 64 {
                terms.push(t);
            }
        }
        if terms.is_empty() || limit == 0 {
            return Vec::new();
        }
        let full: u64 = if terms.len() == 64 {
            u64::MAX
        } else {
            (1 << terms.len()) - 1
        };
        // 폴더마다: 그 폴더와 위 폴더 이름에 든 낱말, 살아 있는지(부모가 늘 앞에 있어 한 번에 계산)
        let mut dir_mask = vec![0u64; self.dirs.len()];
        let mut dir_ok = vec![false; self.dirs.len()];
        for (i, d) in self.dirs.iter().enumerate() {
            let (pm, pok) = if d.parent == NONE {
                (0, true)
            } else {
                (dir_mask[d.parent as usize], dir_ok[d.parent as usize])
            };
            dir_ok[i] = pok && d.alive;
            dir_mask[i] = if dir_ok[i] {
                pm | term_mask(&d.name, &terms)
            } else {
                0
            };
        }
        let mut hits: Vec<(bool, i64, usize)> = Vec::new();
        for (i, f) in self.files.iter().enumerate() {
            if !f.alive || !dir_ok[f.dir as usize] {
                continue;
            }
            let name_mask = term_mask(self.name_of(f), &terms);
            if dir_mask[f.dir as usize] | name_mask == full {
                hits.push((name_mask == full, f.modified, i));
            }
        }
        let order =
            |a: &(bool, i64, usize), b: &(bool, i64, usize)| b.0.cmp(&a.0).then(b.1.cmp(&a.1));
        if hits.len() > limit {
            hits.select_nth_unstable_by(limit - 1, order);
            hits.truncate(limit);
        }
        hits.sort_by(order);
        hits.into_iter()
            .map(|(_, _, i)| {
                let f = &self.files[i];
                Found {
                    path: self.dir_path(f.dir).join(self.name_of(f)),
                    size: f.size,
                    modified: f.modified,
                }
            })
            .collect()
    }
}

/// PC 전체로 읽을 곳: 고정 디스크(C:\ D:\ …). Windows가 아니면(개발 환경) 홈 폴더.
pub fn pc_roots() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
        const DRIVE_FIXED: u32 = 3;
        // SAFETY: 인자 없는 Windows 함수. 드라이브마다 비트가 켜진 값을 돌려준다.
        let mask = unsafe { GetLogicalDrives() };
        (0..26u8)
            .filter(|i| mask & (1 << i) != 0)
            .map(|i| format!("{}:\\", (b'A' + i) as char))
            .filter(|root| {
                let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
                // SAFETY: 끝이 0인 UTF-16 문자열을 넘긴다.
                unsafe { GetDriveTypeW(wide.as_ptr()) == DRIVE_FIXED }
            })
            .map(PathBuf::from)
            .collect()
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .into_iter()
            .collect()
    }
}

/// 이미 하위 폴더까지 읽는 곳(`deep`) 안에 있어 따로 읽지 않아도 되는지
pub fn covered(path: &Path, deep: &[PathBuf]) -> bool {
    deep.iter().any(|root| {
        let Ok(rest) = path.strip_prefix(root) else {
            return false;
        };
        let top = root.parent().is_none();
        !rest
            .components()
            .enumerate()
            .any(|(i, c)| skip_dir(&c.as_os_str().to_string_lossy(), top && i == 0))
    })
}

/// 읽을 곳: PC 전체면 고정 디스크 전부(그 밖에 있는 정리 폴더·지켜볼 폴더는 따로),
/// 아니면 정리 폴더(하위 폴더까지) + 지켜볼 폴더(바로 아래 파일만).
pub fn roots_for(whole_pc: bool, root: Option<&Path>, watch: &[PathBuf]) -> Vec<Root> {
    let mut out: Vec<Root> = Vec::new();
    let mut deep: Vec<PathBuf> = Vec::new();
    if whole_pc {
        for d in pc_roots() {
            deep.push(d.clone());
            out.push(Root {
                path: d,
                deep: true,
            });
        }
    }
    if let Some(r) = root {
        if !covered(r, &deep) {
            deep.push(r.to_path_buf());
            out.push(Root {
                path: r.to_path_buf(),
                deep: true,
            });
        }
    }
    for w in watch {
        if !covered(w, &deep) && !out.iter().any(|r| &r.path == w) {
            out.push(Root {
                path: w.clone(),
                deep: false,
            });
        }
    }
    out
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
    use std::time::{Duration, SystemTime};

    fn write(path: &Path, modified_secs: u64) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"x").unwrap();
        let f = fs::File::options().write(true).open(path).unwrap();
        f.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(modified_secs))
            .unwrap();
    }

    fn names(found: &[Found]) -> Vec<String> {
        found
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    fn deep(path: &Path) -> Root {
        Root {
            path: path.to_path_buf(),
            deep: true,
        }
    }

    #[test]
    fn finds_by_all_terms_name_first_then_recent() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path().join("정리 폴더");
        write(&r.join("평가/수행평가/2027/1학기/채점기준.hwp"), 10);
        write(&r.join("평가/수행평가/안내.hwp"), 30);
        write(&r.join("업무/수행평가_계획.hwp"), 20);
        write(&r.join("업무/회의록.HWP"), 40);
        let index = Index::build(&[deep(&r)], &AtomicUsize::new(0));
        assert_eq!(index.len(), 4);
        let find = |q: &str| names(&index.find(q, 10));
        // 이름에 ‘수행평가’가 든 파일이 먼저, 나머지(폴더 이름으로 맞음)는 최근 순
        assert_eq!(
            find("수행평가"),
            ["수행평가_계획.hwp", "안내.hwp", "채점기준.hwp"]
        );
        // 단어 중간(‘평가’)도, 폴더 이름과 파일 이름에 나눠 들어 있어도 찾는다
        assert_eq!(find("평가 2027"), ["채점기준.hwp"]);
        assert_eq!(find("수행평가 안내"), ["안내.hwp"]);
        // 대소문자 무시, 빈 검색은 결과 없음
        assert_eq!(find("hwp 회의"), ["회의록.HWP"]);
        assert!(find("   ").is_empty());
        assert_eq!(index.find("hwp", 2).len(), 2);
        // 경로를 그대로 돌려준다
        assert_eq!(
            index.find("채점", 1)[0].path,
            r.join("평가/수행평가/2027/1학기/채점기준.hwp")
        );
    }

    #[test]
    fn skips_hidden_temp_and_system_folders() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path().join("사용자");
        write(&r.join("문서/a.hwp"), 1);
        write(&r.join("문서/~$a.hwp"), 1); // 한글·엑셀 임시 파일
        write(&r.join("문서/b.pdf.crdownload"), 1); // 받는 중
        write(&r.join("AppData/Local/c.hwp"), 1);
        write(&r.join(".git/d.hwp"), 1);
        write(&r.join("node_modules/x/e.hwp"), 1);
        let index = Index::build(&[deep(&r)], &AtomicUsize::new(0));
        assert_eq!(names(&index.find("a", 10)), ["a.hwp"]);
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn skip_rules() {
        assert!(skip_dir("Windows", true));
        assert!(skip_dir("Program Files (x86)", true));
        // 드라이브 바로 아래가 아니면 같은 이름이어도 들어간다(예: 문서\Windows 자료)
        assert!(!skip_dir("Windows", false));
        assert!(skip_dir("AppData", false));
        assert!(skip_dir("$Recycle.Bin", true));
        assert!(!skip_dir("수행평가", false));
        assert!(skip_file("~$보고서.hwp"));
        assert!(skip_file("a.TMP"));
        assert!(!skip_file("보고서.hwp"));
        assert!(!skip_file("README"));
    }

    #[test]
    fn system_folders() {
        assert!(system_path(Path::new("/")));
        assert!(system_path(Path::new("/Windows")));
        assert!(system_path(Path::new("/Program Files/한글")));
        assert!(system_path(Path::new("/home/t/AppData/Local")));
        assert!(!system_path(Path::new("/home/t/문서/과제")));
    }

    #[test]
    fn ignores_changes_inside_skipped_folders() {
        assert!(ignored_path(Path::new("/home/t/AppData/Local/Temp/x.tmp")));
        assert!(ignored_path(Path::new("/home/t/proj/node_modules/a/b.js")));
        assert!(!ignored_path(Path::new("/home/t/문서/AppData")));
        assert!(!ignored_path(Path::new("/home/t/문서/a.hwp")));
        // 드라이브 바로 아래 Windows 폴더 안
        assert!(ignored_path(Path::new("/Windows/System32/a.dll")));
        assert!(!ignored_path(Path::new("/home/Windows/a.hwp")));
    }

    #[test]
    fn watch_folders_are_read_shallowly_and_roots_not_twice() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("정리 폴더");
        write(&root.join("평가/2027/a.hwp"), 1);
        let down = d.path().join("Downloads");
        write(&down.join("b.pdf"), 1);
        write(&down.join("안쪽/c.pdf"), 1); // 지켜볼 폴더는 바로 아래만
        let roots = roots_for(false, Some(&root), std::slice::from_ref(&down));
        assert_eq!(
            roots,
            [
                deep(&root),
                Root {
                    path: down.clone(),
                    deep: false
                }
            ]
        );
        let index = Index::build(&roots, &AtomicUsize::new(0));
        let mut found = names(&index.find(".", 10));
        found.sort();
        assert_eq!(found, ["a.hwp", "b.pdf"]);

        // PC 전체 안에 있는 폴더는 따로 읽지 않는다. 빼는 폴더(AppData) 안이면 따로 읽는다.
        let pc = vec![d.path().to_path_buf()];
        assert!(covered(&root, &pc));
        assert!(!covered(&d.path().join("AppData/정리"), &pc));
        assert!(!covered(&root, &[]));
    }

    #[test]
    fn refresh_reads_only_changed_folders() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path().join("r");
        write(&r.join("업무/공문.hwp"), 1);
        write(&r.join("평가/채점.hwp"), 1);
        let mut index = Index::build(&[deep(&r)], &AtomicUsize::new(0));
        assert_eq!(index.len(), 2);

        // 새 파일, 새 폴더(안에 파일), 지운 파일, 지운 폴더
        write(&r.join("업무/회의록.hwp"), 5);
        write(&r.join("업무/2027/계획서.hwp"), 6);
        fs::remove_file(r.join("업무/공문.hwp")).unwrap();
        fs::remove_dir_all(r.join("평가")).unwrap();
        index.refresh_dirs([
            r.join("업무/회의록.hwp"),
            r.join("업무/2027"),
            r.join("업무/공문.hwp"),
            r.join("평가"),
        ]);
        let mut found = names(&index.find("hwp", 10));
        found.sort();
        assert_eq!(found, ["계획서.hwp", "회의록.hwp"]);
        assert_eq!(index.len(), 2);

        // 지운 폴더를 같은 이름으로 다시 만들면 다시 찾는다
        write(&r.join("평가/새 채점.hwp"), 7);
        index.refresh_dirs([r.join("평가/새 채점.hwp"), r.join("평가")]);
        assert_eq!(names(&index.find("채점", 10)), ["새 채점.hwp"]);
        // 색인 밖의 변화는 무시
        index.refresh_dirs([d.path().join("다른 곳/a.hwp")]);
        assert_eq!(index.len(), 3);
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
}
