//! 파일 검색창(SPEC 3-3·6장): 단축키(기본 Alt+Space)·트레이·메인 창에서 연다.
//! Everything이 켜져 있으면 PC 전체, 아니면 자체 색인(정리 폴더 + 지켜볼 폴더)에서 찾는다.
//! 결과에는 연도·학기·학급과, 규칙으로 넣은 파일이면 그 규칙을 붙인다.

use crate::commands::AppState;
use crate::everything::{self, Availability};
use crate::search::{self, Facets};
use crate::settings::{self, Settings};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

pub const LABEL: &str = "search";
/// 검색창이 열렸음(화면이 입력 칸을 비우고 초점을 준다)
const EVENT_OPENED: &str = "search-opened";
/// 고를 수 있는 단축키(빈 문자열 = 쓰지 않음)
pub const SHORTCUTS: &[&str] = &["Alt+Space", "Ctrl+Alt+F", "Ctrl+Shift+Space"];
/// 자체 색인을 새로 읽는 간격(검색창을 열 때 이보다 오래됐으면 다시 읽는다)
const INDEX_MAX_AGE: Duration = Duration::from_secs(120);
const MAX_RESULTS: usize = 200;

/// 검색창 열기(이미 열려 있으면 앞으로)
pub fn show(app: &AppHandle) {
    refresh_index(app, false);
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.center();
        let _ = w.show();
        let _ = w.set_focus();
    }
    let _ = app.emit(EVENT_OPENED, ());
}

pub fn hide(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.hide();
    }
}

/// 단축키: 검색창이 보이면 숨기고, 아니면 연다.
pub fn toggle(app: &AppHandle) {
    let visible = app
        .get_webview_window(LABEL)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    if visible {
        hide(app);
    } else {
        show(app);
    }
}

/// 자체 색인을 뒤에서 새로 읽는다(`force`가 아니면 오래됐을 때만).
pub fn refresh_index(app: &AppHandle, force: bool) {
    let state = app.state::<AppState>();
    let fresh = state
        .index
        .lock()
        .ok()
        .and_then(|i| i.built_at)
        .is_some_and(|t| t.elapsed() < INDEX_MAX_AGE);
    if (fresh && !force) || state.indexing.swap(true, Ordering::SeqCst) {
        return;
    }
    let s = state
        .db
        .lock()
        .ok()
        .and_then(|c| settings::load(&c).ok())
        .unwrap_or_default();
    let app = app.clone();
    std::thread::spawn(move || {
        let watch: Vec<PathBuf> = s.watch_folders.iter().map(PathBuf::from).collect();
        let files = search::build(s.root_folder.as_deref().map(Path::new), &watch);
        let state = app.state::<AppState>();
        if let Ok(mut index) = state.index.lock() {
            index.files = files;
            index.built_at = Some(Instant::now());
        }
        state.indexing.store(false, Ordering::SeqCst);
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
    /// "everything" = PC 전체, "index" = 정리 폴더·지켜볼 폴더
    pub source: &'static str,
    pub everything: Availability,
    /// 자체 색인을 아직 읽는 중인지
    pub indexing: bool,
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

#[tauri::command]
pub fn search_files(app: AppHandle, state: State<'_, AppState>, query: String) -> SearchResponse {
    let query = query.trim();
    let indexing = state.indexing.load(Ordering::SeqCst);
    // 입력 전: 어디서 찾게 될지만 알린다
    if query.is_empty() {
        let everything = availability(&app);
        return SearchResponse {
            source: if everything == Availability::Ready {
                "everything"
            } else {
                "index"
            },
            everything,
            indexing,
            hits: Vec::new(),
        };
    }
    let rules = state.db.lock().map(|c| rule_labels(&c)).unwrap_or_default();
    let everything = match everything::search(&dll_path(&app), query, MAX_RESULTS as u32) {
        Ok(found) => {
            return SearchResponse {
                source: "everything",
                everything: Availability::Ready,
                indexing: false,
                hits: found
                    .into_iter()
                    .map(|h| hit(h.path, h.size, h.modified, &rules))
                    .collect(),
            };
        }
        Err(a) => a,
    };
    let hits = state
        .index
        .lock()
        .map(|index| {
            search::find(&index.files, query, MAX_RESULTS)
                .into_iter()
                .map(|f| hit(f.path.clone(), f.size, f.modified, &rules))
                .collect()
        })
        .unwrap_or_default();
    SearchResponse {
        source: "index",
        everything,
        indexing,
        hits,
    }
}

/// Everything을 지금 쓸 수 있는지(결과 하나만 물어본다)
fn availability(app: &AppHandle) -> Availability {
    match everything::search(&dll_path(app), "", 1) {
        Ok(_) => Availability::Ready,
        Err(a) => a,
    }
}

#[tauri::command]
pub fn show_search(app: AppHandle) {
    show(&app);
}

#[tauri::command]
pub fn hide_search(app: AppHandle) {
    hide(&app);
}

/// 파일 열기(연결된 프로그램으로). 검색창은 닫는다.
#[tauri::command]
pub fn open_path(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    if !Path::new(&path).exists() {
        return Err("파일을 찾을 수 없어요. 옮겨졌거나 지워졌을 수 있어요.".into());
    }
    app.opener()
        .open_path(&path, None::<&str>)
        .map_err(|e| format!("열지 못했어요: {e}"))?;
    hide(&app);
    Ok(())
}

/// 파일이 있는 폴더를 열고 그 파일을 골라 둔다(탐색기). 검색창은 닫는다.
#[tauri::command]
pub fn reveal_path(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    if !Path::new(&path).exists() {
        return Err("파일을 찾을 수 없어요. 옮겨졌거나 지워졌을 수 있어요.".into());
    }
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| format!("위치를 열지 못했어요: {e}"))?;
    hide(&app);
    Ok(())
}

/// 설정 화면용: Everything을 쓸 수 있는지, 단축키 오류
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchInfo {
    pub everything: Availability,
    pub shortcut_error: Option<String>,
}

#[tauri::command]
pub fn search_info(app: AppHandle, state: State<'_, AppState>) -> SearchInfo {
    SearchInfo {
        everything: availability(&app),
        shortcut_error: state.shortcut_error.lock().ok().and_then(|e| e.clone()),
    }
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
