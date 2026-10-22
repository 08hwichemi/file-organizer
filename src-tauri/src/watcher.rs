//! 새 파일 감시(SPEC 5장).
//!
//! - 지켜볼 폴더: 바로 아래 파일을 본다. 알림(`notify`)이 오면 바로, 그 밖에는 주기적으로 폴더를 다시 읽는다.
//! - 내 PC 어디든(넓은 감시): 바탕화면·문서·다운로드 등과 그 하위 폴더, 다른 고정 디스크에 새로 생긴 파일도 본다
//!   (사용자 요청: 인터넷·카카오톡·메신저 등 어디로 받든). 폴더 전체를 다시 읽을 수는 없어서 알림으로 온 파일만 본다.
//!   - 사람이 받는 형식(문서·사진·영상·소리·압축)만. 정리 폴더·묻지 않는 폴더·프로그램 폴더(AppData 등)는 뺀다.
//!   - 만든 지 `FRESH_FOR`가 넘은 파일은 옛 파일을 옮긴 것으로 보고 넘긴다. 이미 알린 파일을 옮긴 것도
//!     (크기·만든 시각이 같음) 넘긴다.
//! - 한꺼번에 찾은 파일은 묶어서 넘긴다(압축 풀기·폴더 복사를 한 번에 알리려고).
//! - 저장이 끝났다고 보는 기준: 크기·수정 시각이 `STABLE_FOR` 동안 그대로 + 크기 > 0
//!   + 같은 이름의 `.crdownload` `.part` `.download`가 없음.
//! - 시작할 때 이미 있던 파일은 새 파일로 보지 않는다.
//! - 이 모듈은 파일을 읽기만 한다. 옮기거나 지우지 않는다.

use notify::{EventKind, RecursiveMode, Watcher as _};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

/// 이 시간 동안 크기·수정 시각이 그대로면 저장이 끝난 것으로 본다.
pub const STABLE_FOR: Duration = Duration::from_secs(2);
/// 알림이 없어도 이 간격으로 다시 읽는다(알림이 빠지는 경우 대비).
const POLL_EVERY: Duration = Duration::from_millis(1500);
/// 저장 중인 파일이 있을 때는 더 자주 확인한다.
const POLL_WHILE_PENDING: Duration = Duration::from_millis(500);

/// 넓은 감시: 만든 지 이보다 오래된 파일은 새 파일이 아니다(옛 파일을 옮긴 것).
pub const FRESH_FOR: Duration = Duration::from_secs(60 * 60);
/// 찾은 새 파일을 묶으려고 붙잡아 두는 최대 시간(그사이 저장 중인 파일이 끝나기를 기다린다).
const HOLD_MAX: Duration = Duration::from_secs(10);
/// 넓은 감시: 이만큼 그대로인데도 저장이 끝나지 않은 파일(빈 파일 등)은 그만 본다.
const WIDE_GIVE_UP: Duration = Duration::from_secs(10 * 60);
/// 넓은 감시가 기억하는 파일 수(넘으면 비운다. 메모리를 아끼려고)
const MEMORY_LIMIT: usize = 20_000;
/// 넓은 감시에서 더 빼는 폴더 이름(게임·앱 설치 폴더: 파일이 한꺼번에 많이 생긴다)
const WIDE_SKIP_DIRS: &[&str] = &[
    "program files",
    "program files (x86)",
    "windowsapps",
    "steamlibrary",
    "steamapps",
    "xboxgames",
    "epic games",
    "riot games",
    "battle.net",
];

const SKIP_EXTENSIONS: &[&str] = &["crdownload", "part", "tmp", "download", "lnk", "url", "ini"];
const PARTIAL_SUFFIXES: &[&str] = &["crdownload", "part", "download"];

/// 이름만 보고 정리 대상이 될 수 있는 파일인지.
pub fn is_candidate_name(name: &str) -> bool {
    if name.starts_with('~') || name.starts_with('.') {
        return false;
    }
    let Some((_, ext)) = name.rsplit_once('.') else {
        return false; // 확장자 없음(엑셀 임시 파일 등)
    };
    if ext.is_empty() {
        return false;
    }
    !SKIP_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
}

/// 넓은 감시는 사람이 받는 파일 형식만 본다(문서·사진·영상·소리·압축). 프로그램이 만드는 .db·.log 등과
/// 설치 파일은 뺀다.
pub fn wide_ext_ok(name: &str) -> bool {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    !matches!(ext.as_str(), "exe" | "msi") && crate::classify::kind_folder(&ext).is_some()
}

/// 미끼 파일을 알아볼 때 읽는 앞부분 크기(사진 정보 칸이 이 안에 있다)
const DECOY_HEAD: usize = 64 * 1024;
/// 이보다 큰 파일은 미끼 파일로 보지 않는다(V3 미끼는 200KB 남짓)
const DECOY_MAX_SIZE: u64 = 2 * 1024 * 1024;

/// 백신이 랜섬웨어를 알아채려고 몰래 만들어 두는 미끼(디코이) 파일인지(사용자 보고: 바탕화면에 자꾸 생겨 창이 뜸).
/// 안랩 V3: 이름은 무작위(`(gQfE6h.jpg` 등), 사진 속 글 정보(XMP)에
/// "This is normal decoy file which is created by AhnLab for detecting ransomware."가 들어 있다.
/// 사용자 파일이 아니고, 옮기거나 이름을 바꾸면 백신이 다시 만들거나 의심할 수 있어 건드리지 않는다.
/// 사진 형식·작은 파일만 앞부분을 읽어 본다(이 PC 안에서만 읽고 밖으로 보내지 않는다).
pub fn is_decoy(path: &Path) -> bool {
    use std::io::Read;
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "gif" | "bmp") {
        return false;
    }
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    if file.metadata().map_or(true, |m| m.len() > DECOY_MAX_SIZE) {
        return false;
    }
    let mut head = Vec::with_capacity(DECOY_HEAD);
    if file.take(DECOY_HEAD as u64).read_to_end(&mut head).is_err() {
        return false;
    }
    let has = |needle: &[u8]| head.windows(needle.len()).any(|w| w == needle);
    has(b"decoy file") && has(b"AhnLab")
}

/// 같은 이름의 다운로드 중 파일(`a.pdf.crdownload` 등)이 옆에 있는지.
pub fn has_partial_partner(path: &Path) -> bool {
    PARTIAL_SUFFIXES.iter().any(|suffix| {
        let mut partner = path.as_os_str().to_owned();
        partner.push(".");
        partner.push(suffix);
        Path::new(&partner).exists()
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStat {
    pub size: u64,
    pub modified: Option<SystemTime>,
}

#[derive(Debug)]
struct Candidate {
    stat: FileStat,
    stable_since: Instant,
}

/// 폴더 목록을 반복해서 받아 "저장이 끝난 새 파일"을 골라낸다. 디스크를 직접 읽지 않아 테스트하기 쉽다.
#[derive(Debug, Default)]
pub struct Tracker {
    /// 이미 알고 있는 파일(시작 때 있던 것, 이미 알린 것, 되돌리기로 돌아온 것).
    known: HashSet<PathBuf>,
    candidates: HashMap<PathBuf, Candidate>,
}

impl Tracker {
    /// 시작할 때 이미 있던 파일은 새 파일이 아니다.
    pub fn baseline(&mut self, files: impl IntoIterator<Item = PathBuf>) {
        self.known.extend(files);
    }

    /// 되돌리기 등으로 돌아온 파일을 다시 새 파일로 잡지 않게 한다.
    pub fn mark_known(&mut self, path: PathBuf) {
        self.candidates.remove(&path);
        self.known.insert(path);
    }

    pub fn has_pending(&self) -> bool {
        !self.candidates.is_empty()
    }

    /// 아직 쓰는 중인 파일이 있는지(최근에 크기·시각이 바뀜). 빈 파일처럼 그대로 머물러 있는 것은 빼고 본다.
    pub fn is_writing(&self, now: Instant) -> bool {
        self.candidates
            .values()
            .any(|c| now.duration_since(c.stable_since) < STABLE_FOR * 2)
    }

    /// 지켜볼 폴더들의 현재 파일 목록을 넘기면, 이번에 저장이 끝난 새 파일을 돌려준다.
    /// `partner_exists`는 `a.pdf.crdownload` 같은 다운로드 중 파일이 옆에 있는지 확인한다.
    pub fn observe(
        &mut self,
        now: Instant,
        files: &[(PathBuf, FileStat)],
        partner_exists: impl Fn(&Path) -> bool,
    ) -> Vec<PathBuf> {
        let present: HashSet<&PathBuf> = files.iter().map(|(p, _)| p).collect();
        // 사라진 파일은 잊는다(같은 이름으로 다시 받으면 새 파일).
        self.known.retain(|p| present.contains(p));
        self.candidates.retain(|p, _| present.contains(p));

        let mut ready = Vec::new();
        for (path, stat) in files {
            if self.known.contains(path) {
                continue;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default();
            if !is_candidate_name(&name) {
                continue;
            }
            let entry = self.candidates.entry(path.clone()).or_insert(Candidate {
                stat: *stat,
                stable_since: now,
            });
            if entry.stat != *stat {
                entry.stat = *stat;
                entry.stable_since = now;
                continue;
            }
            if now.duration_since(entry.stable_since) >= STABLE_FOR
                && stat.size > 0
                && !partner_exists(path)
            {
                ready.push(path.clone());
            }
        }
        for path in &ready {
            self.candidates.remove(path);
            self.known.insert(path.clone());
        }
        ready
    }
}

/// 폴더 바로 아래 파일 목록(숨김·시스템 파일, 폴더, 바로 가기 링크 제외).
pub fn list_files(dir: &Path) -> Vec<(PathBuf, FileStat)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if is_hidden_or_system(&meta) {
            continue;
        }
        out.push((
            entry.path(),
            FileStat {
                size: meta.len(),
                modified: meta.modified().ok(),
            },
        ));
    }
    out
}

fn is_hidden_or_system(_meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
        return _meta.file_attributes() & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0;
    }
    #[allow(unreachable_code)]
    false
}

/// 저장이 끝난 새 파일 하나.
#[derive(Debug, Clone)]
pub struct NewFile {
    pub path: PathBuf,
    /// 이 파일이 들어온 폴더(지켜볼 폴더, 넓은 감시면 파일이 있는 폴더)
    pub source_dir: PathBuf,
    /// 지켜볼 폴더 밖(넓은 감시)에서 찾았는지. 이런 파일은 묻지 않고 옮기지 않는다(SPEC 5장).
    pub elsewhere: bool,
}

/// 어디를 볼지(SPEC 5장).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scope {
    /// 지켜볼 폴더: 바로 아래 파일만
    pub folders: Vec<PathBuf>,
    /// 내 PC 어디든: 이 아래 하위 폴더까지(비면 넓은 감시를 하지 않는다)
    pub wide: Vec<PathBuf>,
    /// 넓은 감시에서 빼는 곳(정리 폴더·묻지 않는 폴더, 하위 폴더까지)
    pub skip: Vec<PathBuf>,
}

/// 넓은 감시로 볼 곳: 사용자 폴더(바탕화면·문서 등)와 다른 고정 디스크. 다른 곳 안에 들어 있는 곳은 뺀다.
pub fn wide_roots(known: &[PathBuf], drives: &[PathBuf]) -> Vec<PathBuf> {
    let all: Vec<&PathBuf> = drives.iter().chain(known).collect();
    let mut out: Vec<PathBuf> = Vec::new();
    for (i, p) in all.iter().enumerate() {
        let n = norm(p);
        let inside_other = all
            .iter()
            .enumerate()
            .any(|(j, q)| j != i && under(&n, &norm(q)));
        let dup = out.iter().any(|o| norm(o) == n);
        if !inside_other && !dup {
            out.push((*p).clone());
        }
    }
    out
}

/// 경로 비교용 글자: 구분자는 \, 영문은 소문자(Windows 경로는 대소문자를 가리지 않는다), 끝의 구분자 없음.
fn norm(p: &Path) -> String {
    let mut s = p.to_string_lossy().replace('/', "\\").to_lowercase();
    while s.ends_with('\\') {
        s.pop();
    }
    s
}

/// `path`가 `dir` 안쪽인지(둘 다 `norm`한 글자)
fn under(path: &str, dir: &str) -> bool {
    path.len() > dir.len() && path.starts_with(dir) && path.as_bytes()[dir.len()] == b'\\'
}

/// 넓은 감시 알림을 빨리 거른다(알림 스레드에서 이름·위치만 본다. 디스크는 읽지 않는다).
#[derive(Debug, Clone, Default)]
struct WideFilter {
    roots: Vec<String>,
    skip: Vec<String>,
    /// 지켜볼 폴더(바로 아래 파일은 원래 감시가 본다)
    direct: Vec<String>,
}

impl WideFilter {
    fn new(scope: &Scope) -> Self {
        let n = |v: &[PathBuf]| v.iter().map(|p| norm(p)).collect();
        WideFilter {
            roots: n(&scope.wide),
            skip: n(&scope.skip),
            direct: n(&scope.folders),
        }
    }

    fn accepts(&self, path: &Path) -> bool {
        let p = norm(path);
        let name = p.rsplit('\\').next().unwrap_or_default();
        if !is_candidate_name(name) || !wide_ext_ok(name) {
            return false;
        }
        if !self.roots.iter().any(|r| under(&p, r)) {
            return false;
        }
        if self.skip.iter().any(|s| under(&p, s)) {
            return false;
        }
        let parent = p.rsplit_once('\\').map_or("", |(dir, _)| dir);
        if self.direct.iter().any(|d| d == parent) {
            return false;
        }
        // 파일 위 폴더들: 파일 검색이 빼는 곳(AppData·숨김 폴더 등)과 게임·앱 설치 폴더는 뺀다.
        // 글자로 나눠 보아 어느 운영체제에서나 같게 동작한다(첫 조각은 드라이브 "c:").
        let parts: Vec<&str> = p.split('\\').collect();
        let drive = parts.first().is_some_and(|d| d.ends_with(':'));
        !parts[..parts.len() - 1]
            .iter()
            .enumerate()
            .any(|(i, part)| {
                WIDE_SKIP_DIRS.contains(part) || crate::search::skip_dir(part, drive && i == 1)
            })
    }
}

/// 넓은 감시에서 파일 하나를 읽은 것
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WideStat {
    pub stat: FileStat,
    /// 만든 시각(모르면 None: 새 파일로 본다)
    pub created: Option<SystemTime>,
}

/// 파일이면(숨김·시스템 제외) 크기·시각을 읽는다.
pub fn read_wide(path: &Path) -> Option<WideStat> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || is_hidden_or_system(&meta) {
        return None;
    }
    Some(WideStat {
        stat: FileStat {
            size: meta.len(),
            modified: meta.modified().ok(),
        },
        created: meta.created().ok(),
    })
}

/// 넓은 감시: 알림으로 온 파일 가운데 "저장이 끝난 새 파일"을 골라낸다. 디스크는 `read`로만 읽어 테스트하기 쉽다.
#[derive(Debug, Default)]
pub struct WideTracker {
    /// 알림이 온 파일(None = 아직 읽지 않음)
    candidates: HashMap<PathBuf, Option<Candidate>>,
    /// 이미 알린(또는 프로그램이 옮긴) 파일과 그때의 만든 시각. 같은 자리에서 고쳐 저장해도
    /// 만든 시각은 그대로라 다시 묻지 않는다. 만든 시각이 다르면 같은 이름으로 새로 받은 파일이다.
    known: HashMap<PathBuf, Option<SystemTime>>,
    /// 이미 알린 파일의 (크기, 만든 시각): 탐색기로 옮기거나 이름만 바꿔도 그대로라 옮긴 것을 알아본다.
    seen: HashSet<(u64, SystemTime)>,
}

impl WideTracker {
    /// 알림이 온 파일을 살펴볼 목록에 넣는다.
    pub fn touch(&mut self, path: PathBuf) {
        self.candidates.entry(path).or_insert(None);
    }

    pub fn has_pending(&self) -> bool {
        !self.candidates.is_empty()
    }

    /// 아직 쓰는 중인 파일이 있는지(읽기 전이거나 최근에 바뀜)
    pub fn is_writing(&self, now: Instant) -> bool {
        self.candidates.values().any(|c| {
            c.as_ref()
                .is_none_or(|c| now.duration_since(c.stable_since) < STABLE_FOR * 2)
        })
    }

    /// 다시 새 파일로 보지 않을 파일(이미 알렸거나, 되돌리기·이름 바꾸기로 프로그램이 옮긴 것).
    pub fn remember(&mut self, path: PathBuf, stat: Option<WideStat>) {
        self.candidates.remove(&path);
        if self.known.len() >= MEMORY_LIMIT {
            self.known.clear();
        }
        if self.seen.len() >= MEMORY_LIMIT {
            self.seen.clear();
        }
        let created = stat.and_then(|w| w.created);
        if let Some(c) = created {
            self.seen.insert((stat.map_or(0, |w| w.stat.size), c));
        }
        self.known.insert(path, created);
    }

    /// 이미 알린 파일과 같은 파일인지(크기·만든 시각이 같음): 탐색기로 옮기거나 이름만 바꾼 것.
    pub fn was_seen(&self, stat: Option<WideStat>) -> bool {
        stat.is_some_and(|w| {
            w.created
                .is_some_and(|c| self.seen.contains(&(w.stat.size, c)))
        })
    }

    /// 지금 저장이 끝난 새 파일을 돌려준다. `wall`은 지금 시각(만든 지 얼마나 됐는지 볼 때).
    pub fn observe(
        &mut self,
        now: Instant,
        wall: SystemTime,
        read: impl Fn(&Path) -> Option<WideStat>,
        partner_exists: impl Fn(&Path) -> bool,
    ) -> Vec<PathBuf> {
        let mut ready = Vec::new();
        let paths: Vec<PathBuf> = self.candidates.keys().cloned().collect();
        for path in paths {
            let Some(w) = read(&path) else {
                self.candidates.remove(&path); // 사라짐(이름이 바뀐 앞 이름 등)
                continue;
            };
            let known = self
                .known
                .get(&path)
                .is_some_and(|c| c.is_none() || *c == w.created);
            let old = w
                .created
                .is_some_and(|c| wall.duration_since(c).is_ok_and(|age| age > FRESH_FOR));
            let moved = self.was_seen(Some(w));
            if known || old || moved {
                self.candidates.remove(&path);
                continue;
            }
            let slot = self.candidates.entry(path.clone()).or_insert(None);
            match slot {
                Some(c) if c.stat == w.stat => {
                    let still = now.duration_since(c.stable_since);
                    if still >= STABLE_FOR && w.stat.size > 0 && !partner_exists(&path) {
                        ready.push((path, w));
                    } else if still >= WIDE_GIVE_UP {
                        // 빈 파일 등 오래 그대로인 것은 그만 본다(다시 바뀌면 알림이 와서 다시 본다)
                        self.candidates.remove(&path);
                    }
                }
                _ => {
                    *slot = Some(Candidate {
                        stat: w.stat,
                        stable_since: now,
                    })
                }
            }
        }
        for (path, w) in &ready {
            self.remember(path.clone(), Some(*w));
        }
        ready.into_iter().map(|(p, _)| p).collect()
    }
}

enum Command {
    Changed,
    SetScope(Scope),
    MarkKnown(PathBuf),
    /// 넓은 감시 알림이 온 파일들
    Touched(Vec<PathBuf>),
}

/// 백그라운드 감시. 만들 때 넘긴 함수가 새 파일 묶음마다 불린다(한꺼번에 찾은 파일은 한 묶음).
#[derive(Clone)]
pub struct WatchService {
    tx: Sender<Command>,
}

impl WatchService {
    pub fn start(on_new_files: impl Fn(Vec<NewFile>) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<Command>();
        let notify_tx = tx.clone();
        let wide_tx = tx.clone();
        let filter = Arc::new(Mutex::new(WideFilter::default()));
        thread::Builder::new()
            .name("watcher".into())
            .spawn(move || {
                let mut tracker = Tracker::default();
                let mut wide = WideTracker::default();
                let mut scope = Scope::default();
                let mut fs_watcher =
                    notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                        if res.is_ok() {
                            let _ = notify_tx.send(Command::Changed);
                        }
                    })
                    .ok();
                // 넓은 감시(하위 폴더까지). 알림 스레드에서 이름·위치로 먼저 걸러 볼 만한 파일만 보낸다.
                let mut _wide_watcher: Option<notify::RecommendedWatcher> = None;
                let mut outbox: Vec<NewFile> = Vec::new();
                let mut held_since: Option<Instant> = None;
                loop {
                    let busy = tracker.has_pending() || wide.has_pending() || !outbox.is_empty();
                    let wait = if busy { POLL_WHILE_PENDING } else { POLL_EVERY };
                    let mut commands = Vec::new();
                    match rx.recv_timeout(wait) {
                        Ok(c) => commands.push(c),
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                    // 한꺼번에 온 알림은 한 번에 처리한다
                    commands.extend(rx.try_iter());
                    for command in commands {
                        match command {
                            Command::SetScope(new) => {
                                if let Some(w) = fs_watcher.as_mut() {
                                    for old in &scope.folders {
                                        let _ = w.unwatch(old);
                                    }
                                    for dir in &new.folders {
                                        let _ = w.watch(dir, RecursiveMode::NonRecursive);
                                    }
                                }
                                // 새로 지켜보는 폴더에 이미 있던 파일은 새 파일이 아니다.
                                for dir in new.folders.iter().filter(|d| !scope.folders.contains(d))
                                {
                                    tracker.baseline(list_files(dir).into_iter().map(|(p, _)| p));
                                }
                                if let Ok(mut f) = filter.lock() {
                                    *f = WideFilter::new(&new);
                                }
                                if new.wide != scope.wide {
                                    drop(_wide_watcher.take()); // 예전 넓은 감시는 여기서 멈춘다
                                    if !new.wide.is_empty() {
                                        _wide_watcher =
                                            start_wide(&new.wide, filter.clone(), wide_tx.clone());
                                    }
                                }
                                let f = WideFilter::new(&new);
                                wide.candidates.retain(|p, _| f.accepts(p));
                                scope = new;
                            }
                            Command::MarkKnown(path) => {
                                tracker.mark_known(path.clone());
                                wide.remember(path.clone(), read_wide(&path));
                                outbox.retain(|f| f.path != path);
                            }
                            Command::Touched(paths) => {
                                paths.into_iter().for_each(|p| wide.touch(p))
                            }
                            Command::Changed => {}
                        }
                    }
                    let mut files = Vec::new();
                    let mut source_of: HashMap<PathBuf, PathBuf> = HashMap::new();
                    for dir in &scope.folders {
                        for (path, stat) in list_files(dir) {
                            source_of.insert(path.clone(), dir.clone());
                            files.push((path, stat));
                        }
                    }
                    let now = Instant::now();
                    for path in tracker.observe(now, &files, has_partial_partner) {
                        let stat = read_wide(&path);
                        // 이미 알린 파일을 탐색기에서 이름만 바꾼 것이면 다시 묻지 않는다(사용자 보고)
                        let renamed = wide.was_seen(stat);
                        // 나중에 탐색기로 옮겨도 넓은 감시가 다시 묻지 않게
                        wide.remember(path.clone(), stat);
                        if renamed || is_decoy(&path) {
                            continue; // 백신 미끼 파일은 묻지 않는다(알린 것으로 쳐서 다시 보지 않음)
                        }
                        let source_dir = source_of.get(&path).cloned().unwrap_or_default();
                        outbox.push(NewFile {
                            path,
                            source_dir,
                            elsewhere: false,
                        });
                    }
                    if wide.has_pending() {
                        for path in
                            wide.observe(now, SystemTime::now(), read_wide, has_partial_partner)
                        {
                            let source_dir =
                                path.parent().map(Path::to_path_buf).unwrap_or_default();
                            outbox.push(NewFile {
                                path,
                                source_dir,
                                elsewhere: true,
                            });
                        }
                    }
                    // 저장 중인 파일이 남아 있으면 조금 기다렸다가 같이 넘긴다(압축 풀기·폴더 복사를 한 묶음으로).
                    if !outbox.is_empty() {
                        let since = *held_since.get_or_insert(now);
                        // 빈 파일·다운로드 중 파일처럼 오래 머무는 것은 기다리지 않는다
                        let waiting = tracker.is_writing(now) || wide.is_writing(now);
                        if !waiting || now.duration_since(since) >= HOLD_MAX {
                            on_new_files(std::mem::take(&mut outbox));
                            held_since = None;
                        }
                    }
                }
            })
            .expect("감시 스레드를 시작하지 못했습니다");
        WatchService { tx }
    }

    pub fn set_scope(&self, scope: Scope) {
        let _ = self.tx.send(Command::SetScope(scope));
    }

    pub fn mark_known(&self, path: PathBuf) {
        let _ = self.tx.send(Command::MarkKnown(path));
    }
}

/// 넓은 감시를 시작한다: 파일이 생기거나 바뀐 알림 가운데 볼 만한 파일만 감시 스레드로 보낸다.
fn start_wide(
    roots: &[PathBuf],
    filter: Arc<Mutex<WideFilter>>,
    tx: Sender<Command>,
) -> Option<notify::RecommendedWatcher> {
    let mut w = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
            return;
        }
        let Ok(f) = filter.lock() else { return };
        let paths: Vec<PathBuf> = event.paths.into_iter().filter(|p| f.accepts(p)).collect();
        if !paths.is_empty() {
            let _ = tx.send(Command::Touched(paths));
        }
    })
    .ok()?;
    for root in roots {
        let _ = w.watch(root, RecursiveMode::Recursive);
    }
    Some(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stat(size: u64, secs: u64) -> FileStat {
        FileStat {
            size,
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs)),
        }
    }

    const NO_PARTNER: fn(&Path) -> bool = |_| false;

    #[test]
    fn candidate_names_follow_spec() {
        for ok in [
            "a.pdf",
            "2027_수행평가.hwp",
            "사진.JPG",
            "보고서.final.docx",
        ] {
            assert!(is_candidate_name(ok), "{ok}");
        }
        for skip in [
            "a.pdf.crdownload",
            "a.pdf.part",
            "x.tmp",
            "x.download",
            "바로가기.lnk",
            "site.url",
            "desktop.ini",
            "~$보고서.docx",
            "~WRL0001.tmp",
            ".hidden.txt",
            "엑셀임시",
            "끝점.",
        ] {
            assert!(!is_candidate_name(skip), "{skip}");
        }
    }

    #[test]
    fn reports_file_once_after_it_stays_unchanged() {
        let t0 = Instant::now();
        let mut tr = Tracker::default();
        let f = PathBuf::from("/dl/a.pdf");

        assert!(tr
            .observe(t0, &[(f.clone(), stat(10, 1))], NO_PARTNER)
            .is_empty());
        // 아직 쓰는 중(크기 변함) → 다시 기다림
        let t1 = t0 + Duration::from_secs(1);
        assert!(tr
            .observe(t1, &[(f.clone(), stat(20, 2))], NO_PARTNER)
            .is_empty());
        let t2 = t1 + STABLE_FOR - Duration::from_millis(1);
        assert!(tr
            .observe(t2, &[(f.clone(), stat(20, 2))], NO_PARTNER)
            .is_empty());
        let t3 = t1 + STABLE_FOR;
        assert_eq!(
            tr.observe(t3, &[(f.clone(), stat(20, 2))], NO_PARTNER),
            std::slice::from_ref(&f)
        );
        // 한 번만 알린다
        let t4 = t3 + Duration::from_secs(5);
        assert!(tr
            .observe(t4, &[(f.clone(), stat(20, 2))], NO_PARTNER)
            .is_empty());
    }

    #[test]
    fn ignores_existing_empty_and_partial_files() {
        let t0 = Instant::now();
        let later = t0 + STABLE_FOR * 2;
        let old = PathBuf::from("/dl/old.pdf");
        let empty = PathBuf::from("/dl/empty.pdf");
        let partial = PathBuf::from("/dl/b.pdf");
        let mut tr = Tracker::default();
        tr.baseline([old.clone()]);
        let files = [
            (old.clone(), stat(5, 1)),
            (empty.clone(), stat(0, 1)),
            (partial.clone(), stat(5, 1)),
        ];
        let has_partner = |p: &Path| p == partial.as_path();
        tr.observe(t0, &files, has_partner);
        assert!(tr.observe(later, &files, has_partner).is_empty());
        // 다운로드가 끝나 .crdownload가 사라지면 알린다
        assert_eq!(tr.observe(later, &files, NO_PARTNER), [partial]);
    }

    #[test]
    fn same_name_downloaded_again_is_new_but_marked_known_is_not() {
        let t0 = Instant::now();
        let f = PathBuf::from("/dl/a.pdf");
        let mut tr = Tracker::default();
        tr.baseline([f.clone()]);
        tr.observe(t0, &[(f.clone(), stat(5, 1))], NO_PARTNER);
        // 사용자가 옮겨서 사라짐 → 같은 이름으로 다시 받음
        tr.observe(t0, &[], NO_PARTNER);
        tr.observe(t0, &[(f.clone(), stat(7, 2))], NO_PARTNER);
        assert_eq!(
            tr.observe(t0 + STABLE_FOR, &[(f.clone(), stat(7, 2))], NO_PARTNER),
            std::slice::from_ref(&f)
        );

        // 되돌리기로 돌아온 파일은 새 파일로 보지 않는다
        let back = PathBuf::from("/dl/back.hwp");
        tr.mark_known(back.clone());
        tr.observe(t0, &[(back.clone(), stat(3, 1))], NO_PARTNER);
        assert!(tr
            .observe(t0 + STABLE_FOR, &[(back, stat(3, 1))], NO_PARTNER)
            .is_empty());
    }

    #[test]
    fn ahnlab_decoy_files_are_recognized() {
        let dir = tempfile::tempdir().unwrap();
        // V3 미끼 파일처럼: JPEG 머리 + 사진 정보(XMP)에 안랩 문구
        let mut decoy = vec![0xFF, 0xD8, 0xFF, 0xE1];
        decoy.extend_from_slice(&[0; 9000]);
        decoy.extend_from_slice(
            b"photoshop:LayerText=\"This is normal decoy file which is created by AhnLab for detecting ransomware.\"",
        );
        decoy.extend_from_slice(&[7; 150_000]);
        let d = dir.path().join("(gQfE6h.jpg");
        fs::write(&d, &decoy).unwrap();
        assert!(is_decoy(&d));
        // 보통 사진, 사진이 아닌 형식, 글이 앞부분 밖에 있는 큰 파일은 아니다
        let photo = dir.path().join("체육대회.jpg");
        fs::write(&photo, [0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3]).unwrap();
        assert!(!is_decoy(&photo));
        let doc = dir.path().join("안내.hwp");
        fs::write(&doc, &decoy).unwrap();
        assert!(!is_decoy(&doc));
        let mut late = vec![0; DECOY_HEAD + 10];
        late.extend_from_slice(b"decoy file AhnLab");
        let l = dir.path().join("late.jpg");
        fs::write(&l, &late).unwrap();
        assert!(!is_decoy(&l));
    }

    #[test]
    fn partner_check_looks_for_download_suffixes() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.pdf");
        fs::write(&f, b"x").unwrap();
        assert!(!has_partial_partner(&f));
        fs::write(dir.path().join("a.pdf.part"), b"x").unwrap();
        assert!(has_partial_partner(&f));
    }

    #[test]
    fn service_reports_new_file_but_not_existing_one() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("있던 파일.pdf"), b"old").unwrap();
        let (tx, rx) = mpsc::channel();
        let service = WatchService::start(move |files| {
            let _ = tx.send(files);
        });
        service.set_scope(Scope {
            folders: vec![dir.path().to_path_buf()],
            ..Scope::default()
        });
        thread::sleep(Duration::from_millis(300));
        // 그대로 머무는 빈 파일이 있어도 새 파일을 늦게 알리지 않는다
        fs::write(dir.path().join("빈 파일.txt"), b"").unwrap();
        thread::sleep(STABLE_FOR * 2 + Duration::from_millis(500));
        let new_file = dir.path().join("새 파일.hwp");
        fs::write(&new_file, b"new").unwrap();

        let got = rx
            .recv_timeout(STABLE_FOR + Duration::from_secs(3))
            .unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, new_file);
        assert_eq!(got[0].source_dir, dir.path());
        assert!(!got[0].elsewhere);
        assert!(rx.recv_timeout(Duration::from_secs(1)).is_err());
    }

    #[test]
    fn service_finds_files_anywhere_in_wide_roots_together() {
        // tempdir() 이름은 "."으로 시작해 숨김 폴더로 보고 거른다 → 다른 이름으로
        let home = tempfile::Builder::new().prefix("home").tempdir().unwrap();
        let docs = home.path().join("문서");
        let kakao = docs.join("카카오톡 받은 파일");
        let root = docs.join("정리 폴더");
        let quiet = home.path().join("사진").join("Screenshots");
        for d in [&kakao, &root, &quiet] {
            fs::create_dir_all(d).unwrap();
        }
        let (tx, rx) = mpsc::channel();
        let service = WatchService::start(move |files| {
            let _ = tx.send(files);
        });
        service.set_scope(Scope {
            folders: Vec::new(),
            wide: vec![home.path().to_path_buf()],
            skip: vec![root.clone(), quiet.clone()],
        });
        thread::sleep(Duration::from_millis(400));
        let a = kakao.join("상담 신청서.hwp");
        let b = kakao.join("안내문.pdf");
        fs::write(&a, b"a").unwrap();
        fs::write(&b, b"b").unwrap();
        // 정리 폴더·묻지 않는 폴더·프로그램이 만드는 형식은 보지 않는다
        fs::write(root.join("넣은 파일.hwp"), b"x").unwrap();
        fs::write(quiet.join("캡처.png"), b"x").unwrap();
        fs::write(kakao.join("chat.db"), b"x").unwrap();

        let got = rx
            .recv_timeout(STABLE_FOR + Duration::from_secs(4))
            .unwrap();
        let mut paths: Vec<_> = got.iter().map(|f| f.path.clone()).collect();
        paths.sort();
        let mut want = vec![a.clone(), b.clone()];
        want.sort();
        assert_eq!(paths, want);
        assert!(got.iter().all(|f| f.elsewhere && f.source_dir == kakao));
        assert!(rx.recv_timeout(Duration::from_millis(1500)).is_err());

        // 이미 알린 파일을 다른 폴더로 옮겨도 다시 알리지 않는다
        let moved = docs.join("상담 신청서.hwp");
        fs::rename(&a, &moved).unwrap();
        assert!(rx
            .recv_timeout(STABLE_FOR + Duration::from_secs(2))
            .is_err());
    }

    #[test]
    fn wide_only_watches_files_people_receive() {
        for ok in ["a.hwp", "b.PDF", "c.jpg", "d.mp4", "e.zip", "f.xlsx"] {
            assert!(wide_ext_ok(ok), "{ok}");
        }
        for skip in ["a.db", "b.log", "c.exe", "d.msi", "e.json", "확장자없음"] {
            assert!(!wide_ext_ok(skip), "{skip}");
        }
    }

    #[test]
    fn wide_roots_drop_folders_inside_other_roots() {
        let p = PathBuf::from;
        let known = [
            p("C:\\Users\\t\\Desktop"),
            p("D:\\OneDrive\\문서"),
            p("C:\\Users\\t\\Desktop"),
        ];
        let drives = [p("D:\\")];
        assert_eq!(
            wide_roots(&known, &drives),
            [p("D:\\"), p("C:\\Users\\t\\Desktop")]
        );
    }

    #[test]
    fn wide_filter_skips_root_quiet_direct_and_program_folders() {
        let f = WideFilter::new(&Scope {
            folders: vec![PathBuf::from("C:\\Users\\t\\Downloads")],
            wide: vec![
                PathBuf::from("C:\\Users\\t\\Documents"),
                PathBuf::from("C:\\Users\\t\\Downloads"),
                PathBuf::from("D:\\"),
            ],
            skip: vec![PathBuf::from("C:\\Users\\t\\Documents\\정리 폴더")],
        });
        let yes = |s: &str| assert!(f.accepts(Path::new(s)), "{s}");
        let no = |s: &str| assert!(!f.accepts(Path::new(s)), "{s}");
        yes("C:\\Users\\t\\Documents\\카카오톡 받은 파일\\안내.hwp");
        yes("c:\\users\\T\\documents\\a.pdf"); // 대소문자가 달라도
        yes("C:\\Users\\t\\Downloads\\수행평가\\a.pdf"); // 지켜볼 폴더의 하위 폴더
        yes("D:\\자료\\b.xlsx");
        no("C:\\Users\\t\\Downloads\\a.pdf"); // 바로 아래는 원래 감시가
        no("C:\\Users\\t\\Documents\\정리 폴더\\평가\\a.pdf");
        no("C:\\Users\\t\\Desktop\\a.pdf"); // 볼 곳 밖
        no("D:\\SteamLibrary\\steamapps\\x.png");
        no("C:\\Users\\t\\Documents\\.git\\a.txt");
        no("C:\\Users\\t\\Documents\\a.pdf.crdownload");
        no("C:\\Users\\t\\Documents\\~$보고서.docx");
    }

    fn wide_stat(size: u64, created: SystemTime) -> WideStat {
        WideStat {
            stat: stat(size, 1),
            created: Some(created),
        }
    }

    #[test]
    fn wide_tracker_reports_fresh_files_once_stable() {
        let t0 = Instant::now();
        let wall = SystemTime::now();
        let made = wall - Duration::from_secs(5);
        let f = PathBuf::from("/d/카톡/a.hwp");
        let mut tr = WideTracker::default();
        tr.touch(f.clone());
        let read = |_: &Path| Some(wide_stat(10, made));
        assert!(tr.observe(t0, wall, read, NO_PARTNER).is_empty());
        assert!(tr.has_pending());
        assert_eq!(
            tr.observe(t0 + STABLE_FOR, wall, read, NO_PARTNER),
            std::slice::from_ref(&f)
        );
        assert!(!tr.has_pending());
        // 고쳐서 같은 자리에 저장(만든 시각은 그대로): 다시 묻지 않는다
        tr.touch(f.clone());
        let edited = |_: &Path| Some(wide_stat(30, made));
        tr.observe(t0, wall, edited, NO_PARTNER);
        assert!(tr
            .observe(t0 + STABLE_FOR, wall, edited, NO_PARTNER)
            .is_empty());
        // 같은 이름으로 새로 받음(만든 시각이 다름): 새 파일
        tr.touch(f.clone());
        let again = |_: &Path| Some(wide_stat(12, wall));
        tr.observe(t0, wall, again, NO_PARTNER);
        assert_eq!(tr.observe(t0 + STABLE_FOR, wall, again, NO_PARTNER), [f]);
    }

    #[test]
    fn wide_tracker_skips_old_moved_and_vanished_files() {
        let t0 = Instant::now();
        let wall = SystemTime::now();
        let mut tr = WideTracker::default();
        // 옛 파일을 옮겨 옴
        let old = PathBuf::from("/d/old.pdf");
        tr.touch(old.clone());
        let read_old = |_: &Path| Some(wide_stat(5, wall - FRESH_FOR - Duration::from_secs(1)));
        tr.observe(t0, wall, read_old, NO_PARTNER);
        assert!(!tr.has_pending());
        // 이미 알린 파일을 다른 곳으로 옮김(크기·만든 시각 같음)
        let made = wall - Duration::from_secs(60);
        tr.remember(PathBuf::from("/dl/a.pdf"), Some(wide_stat(7, made)));
        tr.touch(PathBuf::from("/d/폴더/a.pdf"));
        tr.observe(t0, wall, |_| Some(wide_stat(7, made)), NO_PARTNER);
        assert!(!tr.has_pending());
        // 알림만 오고 사라진 파일(이름이 바뀐 앞 이름)
        tr.touch(PathBuf::from("/d/gone.pdf"));
        tr.observe(t0, wall, |_| None, NO_PARTNER);
        assert!(!tr.has_pending());
    }

    #[test]
    fn was_seen_matches_renamed_file_only() {
        let made = SystemTime::now() - Duration::from_secs(60);
        let mut tr = WideTracker::default();
        tr.remember(PathBuf::from("/dl/a.pdf"), Some(wide_stat(7, made)));
        // 탐색기에서 이름만 바꿈(크기·만든 시각 같음)
        assert!(tr.was_seen(Some(wide_stat(7, made))));
        // 새로 받은 파일(만든 시각이 다름)·만든 시각을 모름
        assert!(!tr.was_seen(Some(wide_stat(7, SystemTime::now()))));
        assert!(!tr.was_seen(Some(WideStat {
            stat: stat(7, 1),
            created: None
        })));
        assert!(!tr.was_seen(None));
    }

    #[test]
    fn list_files_skips_folders() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.pdf"), b"abc").unwrap();
        fs::create_dir(dir.path().join("폴더")).unwrap();
        let files = list_files(dir.path());
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].1.size, 3);
    }
}
