//! 파일 검색(SPEC 3-3·6장): 메인 창 오른쪽의 "파일 검색" 화면. 단축키(기본 Alt+Space)·트레이로도 연다.
//! Everything이 켜져 있으면 Everything으로, 아니면 이 프로그램이 직접 읽은 목록(자체 색인)에서 찾는다.
//! 자체 색인은 설정에 따라 PC 전체 또는 정리 폴더·지켜볼 폴더만 읽고, 그 뒤에는 바뀐 폴더만 다시 읽는다.
//! 결과에는 연도·학기·학급과, 규칙으로 넣은 파일이면 그 규칙을 붙인다.

use crate::commands::AppState;
use crate::everything::{self, Availability};
use crate::search::{self, Facets, Index, Root};
use crate::settings::{self, Settings};
use notify::{EventKind, RecursiveMode, Watcher as _};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

/// 검색 화면을 열었음(화면이 그 화면으로 가서 입력 칸에 초점을 준다)
const EVENT_OPENED: &str = "search-opened";
/// 자체 색인을 다 읽었음(화면이 지금 검색어로 다시 찾는다)
const EVENT_INDEXED: &str = "search-indexed";
/// 고를 수 있는 단축키(빈 문자열 = 쓰지 않음)
pub const SHORTCUTS: &[&str] = &["Alt+Space", "Ctrl+Alt+F", "Ctrl+Shift+Space"];
/// 정리 폴더·지켜볼 폴더 색인은 검색 화면을 열 때 이보다 오래됐으면 다시 읽는다(알림이 빠지는 경우 대비).
const FOLDERS_MAX_AGE: Duration = Duration::from_secs(120);
/// PC 전체 색인은 바뀐 폴더만 따라 읽고, 이 간격으로 처음부터 다시 읽는다.
const PC_MAX_AGE: Duration = Duration::from_secs(6 * 60 * 60);
/// 알림이 넘쳐 놓친 것이 있을 때 다시 읽는 최소 간격
const RESCAN_MIN_GAP: Duration = Duration::from_secs(10 * 60);
/// 바뀐 폴더를 모아 두었다가 다시 읽는 간격
const FLUSH_EVERY: Duration = Duration::from_secs(2);
const MAX_RESULTS: usize = 200;

/// 자체 색인을 최신으로 맞추는 감시(PC 전체·정리 폴더). 새 파일 팝업용 감시(watcher.rs)와는 따로다.
#[derive(Default)]
pub struct Live {
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
    tx: Mutex<Option<Sender<Change>>>,
    /// 지금 읽는 중인 색인에서 읽은 파일 수
    pub progress: AtomicUsize,
}

enum Change {
    Path(PathBuf),
    /// 알림이 넘쳐 놓쳤을 수 있음
    Rescan,
}

/// 검색 화면 열기(단축키·트레이): 메인 창을 앞으로 + 검색 화면으로
pub fn show(app: &AppHandle) {
    crate::tray::show_main(app, None);
    let _ = app.emit(EVENT_OPENED, ());
    refresh_index(app, false);
}

/// 이 컴퓨터를 덜 바쁘게: 색인을 읽는 스레드는 낮은 우선순위(디스크도 뒤로 양보)
fn background_priority() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN,
        };
        // SAFETY: 지금 스레드의 우선순위만 바꾼다.
        unsafe {
            SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
        }
    }
}

/// 설정에 맞는 색인 범위. PC 전체를 골랐어도 Everything이 켜져 있으면 Everything으로 찾으므로
/// 메모리를 아끼려고 정리 폴더·지켜볼 폴더만 읽는다.
fn wanted(app: &AppHandle, s: &Settings) -> (bool, Vec<Root>) {
    let whole_pc = s.search_whole_pc && availability(app) != Availability::Ready;
    let watch: Vec<PathBuf> = s.watch_folders.iter().map(PathBuf::from).collect();
    let roots = search::roots_for(whole_pc, s.root_folder.as_deref().map(Path::new), &watch);
    (whole_pc, roots)
}

/// 자체 색인을 뒤에서 새로 읽는다(`force`가 아니면 범위가 바뀌었거나 오래됐을 때만).
pub fn refresh_index(app: &AppHandle, force: bool) {
    let state = app.state::<AppState>();
    let s = state
        .db
        .lock()
        .ok()
        .and_then(|c| settings::load(&c).ok())
        .unwrap_or_default();
    if !s.setup_done {
        return;
    }
    let (whole_pc, roots) = wanted(app, &s);
    let max_age = if whole_pc {
        PC_MAX_AGE
    } else {
        FOLDERS_MAX_AGE
    };
    let fresh = state.index.lock().ok().is_some_and(|i| {
        i.whole_pc == whole_pc
            && i.roots() == roots.as_slice()
            && i.built_at.is_some_and(|t| t.elapsed() < max_age)
    });
    if (fresh && !force) || state.indexing.swap(true, Ordering::SeqCst) {
        return;
    }
    state.live.progress.store(0, Ordering::Relaxed);
    let app = app.clone();
    std::thread::spawn(move || {
        background_priority();
        let state = app.state::<AppState>();
        // 읽는 동안 바뀐 것도 놓치지 않게 감시를 먼저 건다(다 읽은 뒤 모아 둔 것을 반영).
        watch(&state, &roots);
        // PC 전체를 처음 읽으면 몇 분 걸릴 수 있다 → 그동안 찾을 수 있게 정리 폴더·지켜볼 폴더부터 빨리 읽는다.
        let empty = state.index.lock().map(|i| i.is_empty()).unwrap_or(true);
        if whole_pc && empty {
            let watch: Vec<PathBuf> = s.watch_folders.iter().map(PathBuf::from).collect();
            let quick = search::roots_for(false, s.root_folder.as_deref().map(Path::new), &watch);
            let index = Index::build(&quick, &AtomicUsize::new(0));
            if let Ok(mut i) = state.index.lock() {
                *i = index;
            }
            let _ = app.emit(EVENT_INDEXED, ());
        }
        let mut index = Index::build(&roots, &state.live.progress);
        index.whole_pc = whole_pc;
        if let Ok(mut i) = state.index.lock() {
            *i = index;
        }
        state.indexing.store(false, Ordering::SeqCst);
        let _ = app.emit(EVENT_INDEXED, ());
        // 읽는 동안 설정(범위·정리 폴더 등)이 바뀌었으면 그에 맞춰 다시
        refresh_index(&app, false);
    });
}

/// 색인한 곳의 변화를 듣는다(하위 폴더까지 읽는 곳은 하위 폴더까지).
fn watch(state: &AppState, roots: &[Root]) {
    let Some(tx) = state.live.tx.lock().ok().and_then(|t| t.clone()) else {
        return;
    };
    let watcher =
        notify::recommended_watcher(move |res: notify::Result<notify::Event>| match res {
            Ok(event) => {
                if event.need_rescan() {
                    let _ = tx.send(Change::Rescan);
                }
                if matches!(event.kind, EventKind::Access(_)) {
                    return; // 파일을 열어 보기만 한 것
                }
                for p in event.paths {
                    if !search::ignored_path(&p) {
                        let _ = tx.send(Change::Path(p));
                    }
                }
            }
            Err(_) => {
                let _ = tx.send(Change::Rescan);
            }
        });
    let Ok(mut watcher) = watcher else {
        return;
    };
    for r in roots {
        let mode = if r.deep {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        let _ = watcher.watch(&r.path, mode);
    }
    if let Ok(mut w) = state.live.watcher.lock() {
        *w = Some(watcher); // 예전 감시는 여기서 멈춘다
    }
}

/// 바뀐 폴더를 모았다가 조금씩 다시 읽는 스레드를 시작한다(프로그램을 켤 때 한 번).
pub fn start_live(app: &AppHandle) {
    let (tx, rx) = mpsc::channel::<Change>();
    if let Ok(mut t) = app.state::<AppState>().live.tx.lock() {
        *t = Some(tx);
    }
    let app = app.clone();
    std::thread::spawn(move || {
        background_priority();
        let mut dirty: HashSet<PathBuf> = HashSet::new();
        let mut rescan = false;
        let mut last = Instant::now();
        loop {
            match rx.recv_timeout(FLUSH_EVERY) {
                Ok(Change::Path(p)) => {
                    dirty.insert(p);
                    // 한꺼번에 너무 많이 바뀌면(큰 폴더 복사 등) 처음부터 다시 읽는 편이 빠르다
                    if dirty.len() > 20_000 {
                        dirty.clear();
                        rescan = true;
                    }
                }
                Ok(Change::Rescan) => rescan = true,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            if last.elapsed() < FLUSH_EVERY || (dirty.is_empty() && !rescan) {
                continue;
            }
            last = Instant::now();
            let state = app.state::<AppState>();
            if state.indexing.load(Ordering::SeqCst) {
                continue; // 다 읽은 뒤에 반영한다
            }
            let mut rebuild = false;
            if let Ok(mut index) = state.index.lock() {
                let old = index.built_at.is_none_or(|t| t.elapsed() > RESCAN_MIN_GAP);
                if rescan && old {
                    rebuild = true;
                    dirty.clear();
                } else {
                    index.refresh_dirs(dirty.drain());
                    rebuild = index.wasteful();
                }
            }
            rescan = false;
            if rebuild {
                refresh_index(&app, true);
            }
        }
    });
}

/// 단축키를 바꿔 등록한다. 다른 프로그램이 쓰고 있으면 오류.
pub fn apply_shortcut(app: &AppHandle, shortcut: &str) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    if shortcut.is_empty() {
        return Ok(());
    }
    gs.register(shortcut).map_err(|_| {
        format!("‘{shortcut}’ 단축키를 쓸 수 없어요. 다른 프로그램이 쓰고 있을 수 있어요.")
    })
}

/// 설치 폴더에 함께 넣은 Everything SDK DLL
fn dll_path(app: &AppHandle) -> PathBuf {
    app.path()
        .resource_dir()
        .map(|d| d.join("resources").join("Everything64.dll"))
        .unwrap_or_default()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub path: String,
    pub name: String,
    pub dir: String,
    pub size: u64,
    pub modified: i64,
    #[serde(flatten)]
    pub facets: Facets,
    /// 규칙으로 넣은 파일이면 그 규칙의 단어(예: "수행평가")
    pub rule: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    /// 어디서 찾았는지: "everything" = Everything(PC 전체), "pc" = 자체 색인(PC 전체),
    /// "folders" = 자체 색인(정리 폴더·지켜볼 폴더)
    pub source: &'static str,
    pub everything: Availability,
    /// 설정에서 PC 전체를 골랐는지
    pub want_whole_pc: bool,
    /// 자체 색인을 읽는 중인지
    pub indexing: bool,
    /// 자체 색인의 파일 수(읽는 중이면 지금까지 읽은 수)
    pub indexed: usize,
    pub hits: Vec<SearchHit>,
}

/// 규칙으로 넣은 파일(되돌린 것 제외) → 규칙 이름
fn rule_labels(conn: &Connection) -> HashMap<String, String> {
    let mut names: HashMap<i64, String> = HashMap::new();
    for r in crate::rules::list(conn).unwrap_or_default() {
        let label = if r.words.is_empty() {
            r.extensions
                .iter()
                .map(|e| format!(".{e}"))
                .collect::<Vec<_>>()
                .join("·")
        } else {
            r.words.join("·")
        };
        names.insert(r.id, label);
    }
    let mut out = HashMap::new();
    if let Ok(mut stmt) = conn
        .prepare("SELECT moved_path, rule FROM moves WHERE rule IS NOT NULL AND undone_at IS NULL")
    {
        if let Ok(rows) =
            stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        {
            for (path, rule) in rows.flatten() {
                if let Some(label) = rule.parse::<i64>().ok().and_then(|id| names.get(&id)) {
                    out.insert(path, label.clone());
                }
            }
        }
    }
    out
}

fn hit(path: PathBuf, size: u64, modified: i64, rules: &HashMap<String, String>) -> SearchHit {
    let p = path.to_string_lossy().into_owned();
    SearchHit {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        dir: path
            .parent()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default(),
        facets: search::facets(&path),
        rule: rules.get(&p).cloned(),
        path: p,
        size,
        modified,
    }
}

/// 찾기. 오래 걸릴 수 있어(PC 전체 수십만 개) 화면이 멈추지 않게 뒤에서 돈다.
#[tauri::command]
pub async fn search_files(app: AppHandle, query: String) -> Result<SearchResponse, String> {
    let state = app.state::<AppState>();
    let query = query.trim();
    let want_whole_pc = state
        .db
        .lock()
        .ok()
        .and_then(|c| settings::load(&c).ok())
        .is_some_and(|s| s.search_whole_pc);
    let indexing = state.indexing.load(Ordering::SeqCst);
    let (index_pc, index_len) = state
        .index
        .lock()
        .map(|i| (i.whole_pc, i.len()))
        .unwrap_or((false, 0));
    let indexed = if indexing {
        state.live.progress.load(Ordering::Relaxed)
    } else {
        index_len
    };
    let from_index = if index_pc { "pc" } else { "folders" };
    let response = |source, everything, hits| SearchResponse {
        source,
        everything,
        want_whole_pc,
        indexing,
        indexed,
        hits,
    };
    // 입력 전: 어디서 찾게 될지만 알린다
    if query.is_empty() {
        let everything = availability(&app);
        let source = if everything == Availability::Ready {
            "everything"
        } else {
            from_index
        };
        return Ok(response(source, everything, Vec::new()));
    }
    let rules = state.db.lock().map(|c| rule_labels(&c)).unwrap_or_default();
    let everything = match everything::search(&dll_path(&app), query, MAX_RESULTS as u32) {
        Ok(found) => {
            let hits = found
                .into_iter()
                .map(|h| hit(h.path, h.size, h.modified, &rules))
                .collect();
            return Ok(response("everything", Availability::Ready, hits));
        }
        Err(a) => a,
    };
    // Everything이 꺼졌는데 색인이 정리 폴더만이면(켜져 있을 때 읽은 것) PC 전체로 다시 읽는다
    if want_whole_pc && !index_pc && !indexing {
        refresh_index(&app, false);
    }
    let hits = state
        .index
        .lock()
        .map(|index| {
            index
                .find(query, MAX_RESULTS)
                .into_iter()
                .map(|f| hit(f.path, f.size, f.modified, &rules))
                .collect()
        })
        .unwrap_or_default();
    Ok(response(from_index, everything, hits))
}

/// Everything을 지금 쓸 수 있는지(결과 하나만 물어본다)
fn availability(app: &AppHandle) -> Availability {
    match everything::search(&dll_path(app), "", 1) {
        Ok(_) => Availability::Ready,
        Err(a) => a,
    }
}

/// 검색 화면을 열 때: 색인이 없거나 오래됐으면 뒤에서 읽기 시작
#[tauri::command]
pub fn prepare_search(app: AppHandle) {
    refresh_index(&app, false);
}

/// 파일 열기(연결된 프로그램으로)
#[tauri::command]
pub fn open_path(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    if !Path::new(&path).exists() {
        return Err("파일을 찾을 수 없어요. 옮겨졌거나 지워졌을 수 있어요.".into());
    }
    app.opener()
        .open_path(&path, None::<&str>)
        .map_err(|e| format!("열지 못했어요: {e}"))
}

/// 파일이 있는 폴더를 열고 그 파일을 골라 둔다(탐색기)
#[tauri::command]
pub fn reveal_path(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    if !Path::new(&path).exists() {
        return Err("파일을 찾을 수 없어요. 옮겨졌거나 지워졌을 수 있어요.".into());
    }
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| format!("위치를 열지 못했어요: {e}"))
}

/// 설정 화면용: Everything을 쓸 수 있는지, 자체 색인 상태, 단축키 오류
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchInfo {
    pub everything: Availability,
    /// 자체 색인이 PC 전체인지
    pub whole_pc: bool,
    pub indexing: bool,
    pub indexed: usize,
    pub shortcut_error: Option<String>,
}

#[tauri::command]
pub fn search_info(app: AppHandle, state: State<'_, AppState>) -> SearchInfo {
    let indexing = state.indexing.load(Ordering::SeqCst);
    let (whole_pc, len) = state
        .index
        .lock()
        .map(|i| (i.whole_pc, i.len()))
        .unwrap_or((false, 0));
    SearchInfo {
        everything: availability(&app),
        whole_pc,
        indexing,
        indexed: if indexing {
            state.live.progress.load(Ordering::Relaxed)
        } else {
            len
        },
        shortcut_error: state.shortcut_error.lock().ok().and_then(|e| e.clone()),
    }
}

/// 찾는 범위: PC 전체(true) / 정리 폴더와 지켜볼 폴더만(false). 바꾸면 그 범위로 다시 읽는다.
#[tauri::command]
pub fn set_search_whole_pc(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<Settings, String> {
    let s = {
        let conn = state
            .db
            .lock()
            .map_err(|_| "저장소를 열 수 없어요".to_string())?;
        let mut s = settings::load(&conn)?;
        s.search_whole_pc = enabled;
        settings::save(&conn, &s)?;
        s
    };
    if !enabled {
        // 메모리를 바로 돌려준다(작은 범위는 금방 다시 읽는다)
        if let Ok(mut i) = state.index.lock() {
            *i = Index::default();
        }
    }
    refresh_index(&app, false);
    Ok(s)
}

/// 검색 단축키 바꾸기("" = 쓰지 않음). 등록하지 못하면 원래 단축키로 돌리고 오류.
#[tauri::command]
pub fn set_search_shortcut(
    app: AppHandle,
    state: State<'_, AppState>,
    shortcut: String,
) -> Result<Settings, String> {
    if !(shortcut.is_empty() || SHORTCUTS.contains(&shortcut.as_str())) {
        return Err("고를 수 없는 단축키예요.".into());
    }
    let conn = state
        .db
        .lock()
        .map_err(|_| "저장소를 열 수 없어요".to_string())?;
    let mut s = settings::load(&conn)?;
    if let Err(e) = apply_shortcut(&app, &shortcut) {
        let _ = apply_shortcut(&app, &s.search_shortcut);
        return Err(e);
    }
    s.search_shortcut = shortcut;
    settings::save(&conn, &s)?;
    if let Ok(mut err) = state.shortcut_error.lock() {
        *err = None;
    }
    Ok(s)
}
