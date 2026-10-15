//! 새 파일 감시(SPEC 5장).
//!
//! - 지켜볼 폴더 바로 아래 파일만 본다(하위 폴더 X).
//! - 파일 시스템 알림(`notify`)이 오면 바로, 그 밖에는 주기적으로 폴더를 다시 읽는다.
//! - 저장이 끝났다고 보는 기준: 크기·수정 시각이 `STABLE_FOR` 동안 그대로 + 크기 > 0
//!   + 같은 이름의 `.crdownload` `.part` `.download`가 없음.
//! - 시작할 때 이미 있던 파일은 새 파일로 보지 않는다.
//! - 이 모듈은 파일을 읽기만 한다. 옮기거나 지우지 않는다.

use notify::{RecursiveMode, Watcher as _};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

/// 이 시간 동안 크기·수정 시각이 그대로면 저장이 끝난 것으로 본다.
pub const STABLE_FOR: Duration = Duration::from_secs(2);
/// 알림이 없어도 이 간격으로 다시 읽는다(알림이 빠지는 경우 대비).
const POLL_EVERY: Duration = Duration::from_millis(1500);
/// 저장 중인 파일이 있을 때는 더 자주 확인한다.
const POLL_WHILE_PENDING: Duration = Duration::from_millis(500);

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

/// 같은 이름의 다운로드 중 파일(`a.pdf.crdownload` 등)이 옆에 있는지.
fn has_partial_partner(path: &Path) -> bool {
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
    /// 이 파일이 들어온 지켜볼 폴더
    pub source_dir: PathBuf,
}

enum Command {
    Changed,
    SetFolders(Vec<PathBuf>),
    MarkKnown(PathBuf),
}

/// 백그라운드 감시. 만들 때 넘긴 함수가 새 파일마다 불린다.
#[derive(Clone)]
pub struct WatchService {
    tx: Sender<Command>,
}

impl WatchService {
    pub fn start(on_new_file: impl Fn(NewFile) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<Command>();
        let notify_tx = tx.clone();
        thread::Builder::new()
            .name("watcher".into())
            .spawn(move || {
                let mut tracker = Tracker::default();
                let mut folders: Vec<PathBuf> = Vec::new();
                let mut fs_watcher =
                    notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                        if res.is_ok() {
                            let _ = notify_tx.send(Command::Changed);
                        }
                    })
                    .ok();
                loop {
                    let wait = if tracker.has_pending() {
                        POLL_WHILE_PENDING
                    } else {
                        POLL_EVERY
                    };
                    match rx.recv_timeout(wait) {
                        Ok(Command::SetFolders(new_folders)) => {
                            if let Some(w) = fs_watcher.as_mut() {
                                for old in &folders {
                                    let _ = w.unwatch(old);
                                }
                                for dir in &new_folders {
                                    let _ = w.watch(dir, RecursiveMode::NonRecursive);
                                }
                            }
                            // 새로 지켜보는 폴더에 이미 있던 파일은 새 파일이 아니다.
                            for dir in new_folders.iter().filter(|d| !folders.contains(d)) {
                                tracker.baseline(list_files(dir).into_iter().map(|(p, _)| p));
                            }
                            folders = new_folders;
                        }
                        Ok(Command::MarkKnown(path)) => tracker.mark_known(path),
                        Ok(Command::Changed) | Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                    let mut files = Vec::new();
                    let mut source_of: HashMap<PathBuf, PathBuf> = HashMap::new();
                    for dir in &folders {
                        for (path, stat) in list_files(dir) {
                            source_of.insert(path.clone(), dir.clone());
                            files.push((path, stat));
                        }
                    }
                    for path in tracker.observe(Instant::now(), &files, has_partial_partner) {
                        let source_dir = source_of.get(&path).cloned().unwrap_or_default();
                        on_new_file(NewFile { path, source_dir });
                    }
                }
            })
            .expect("감시 스레드를 시작하지 못했습니다");
        WatchService { tx }
    }

    pub fn set_folders(&self, folders: Vec<PathBuf>) {
        let _ = self.tx.send(Command::SetFolders(folders));
    }

    pub fn mark_known(&self, path: PathBuf) {
        let _ = self.tx.send(Command::MarkKnown(path));
    }
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
        let service = WatchService::start(move |f| {
            let _ = tx.send(f);
        });
        service.set_folders(vec![dir.path().to_path_buf()]);
        thread::sleep(Duration::from_millis(300));
        let new_file = dir.path().join("새 파일.hwp");
        fs::write(&new_file, b"new").unwrap();

        let got = rx
            .recv_timeout(STABLE_FOR + Duration::from_secs(3))
            .unwrap();
        assert_eq!(got.path, new_file);
        assert_eq!(got.source_dir, dir.path());
        assert!(rx.recv_timeout(Duration::from_secs(1)).is_err());
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
