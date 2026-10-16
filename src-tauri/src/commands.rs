//! 화면에서 `invoke`로 부르는 명령 모음.

use crate::folders::{self, FolderEntry, FolderMatch};
use crate::inbox::{self, InboxItem};
use crate::settings::{self, OrganizeMode, Settings};
use crate::setup::{self, SetupRequest};
use crate::watcher::{self, NewFile, WatchService};
use crate::{history, mover, popup};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

pub struct AppState {
    pub db: Mutex<Connection>,
    pub watcher: WatchService,
    /// 팝업에서 물어볼 차례를 기다리는 새 파일들(맨 앞이 지금 보이는 파일).
    pub pending: Mutex<VecDeque<PendingFile>>,
}

impl AppState {
    fn with_db<T>(&self, f: impl FnOnce(&Connection) -> Result<T, String>) -> Result<T, String> {
        let conn = self
            .db
            .lock()
            .map_err(|_| "저장소를 열 수 없어요".to_string())?;
        f(&conn)
    }

    fn with_pending<T>(&self, f: impl FnOnce(&mut VecDeque<PendingFile>) -> T) -> T {
        let mut queue = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut queue)
    }

    /// 감시를 지금 설정의 지켜볼 폴더로 맞춘다.
    pub fn apply_watch_folders(&self, s: &Settings) {
        let folders = if s.setup_done {
            s.watch_folders.iter().map(PathBuf::from).collect()
        } else {
            Vec::new()
        };
        self.watcher.set_folders(folders);
    }
}

fn db_err(e: rusqlite::Error) -> String {
    format!("기록을 저장하지 못했어요: {e}")
}

/// 화면에 알리는 이벤트 이름
pub const EVENT_POPUP_CHANGED: &str = "popup-changed";
pub const EVENT_INBOX_CHANGED: &str = "inbox-changed";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingFile {
    pub path: String,
    pub name: String,
    pub source_dir: String,
    pub size: u64,
}

impl PendingFile {
    fn new(path: &Path, source_dir: &Path) -> Self {
        PendingFile {
            path: path.to_string_lossy().into_owned(),
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            source_dir: source_dir.to_string_lossy().into_owned(),
            size: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        }
    }
}

/// 감시가 저장이 끝난 새 파일을 찾았을 때. 알림 방식에 따라 팝업 또는 정리함으로.
pub fn handle_new_file(app: &AppHandle, file: NewFile) {
    let state = app.state::<AppState>();
    let mode = state
        .with_db(settings::load)
        .map(|s| s.organize_mode)
        .unwrap_or_default();
    match mode {
        OrganizeMode::Manual => {
            let _ = state.with_db(|conn| {
                inbox::add(conn, &file.path, &file.source_dir, inbox::Reason::Collected)
                    .map_err(db_err)
            });
            let _ = app.emit(EVENT_INBOX_CHANGED, ());
        }
        OrganizeMode::Ask => {
            let item = PendingFile::new(&file.path, &file.source_dir);
            state.with_pending(|q| {
                if !q.iter().any(|p| p.path == item.path) {
                    q.push_back(item);
                }
            });
            let _ = app.emit(EVENT_POPUP_CHANGED, ());
            popup::show(app);
        }
    }
}

/// 프로그램 이름·버전·운영체제.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: String,
    pub os: &'static str,
}

/// 버전은 tauri.conf.json(= package.json)의 값. 배포할 때 자동으로 올라간다.
#[tauri::command]
pub fn app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        name: "파일 정리 도우미",
        version: app.package_info().version.to_string(),
        os: std::env::consts::OS,
    }
}

/// 처음 실행 화면에 필요한 정보. 기본 위치는 Windows에서 Known Folder 기준이라
/// OneDrive로 옮긴 바탕화면·문서도 실제 위치가 나온다.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupInfo {
    pub documents: Option<String>,
    pub desktop: Option<String>,
    pub downloads: Option<String>,
    pub root_folder_name: &'static str,
    pub separator: String,
    pub teacher_example: Vec<ExampleFolder>,
}

#[derive(Debug, Serialize)]
pub struct ExampleFolder {
    pub name: &'static str,
    pub children: &'static [&'static str],
}

#[tauri::command]
pub fn setup_info(app: tauri::AppHandle) -> SetupInfo {
    let path = app.path();
    let s = |p: tauri::Result<PathBuf>| p.ok().map(|p| p.to_string_lossy().into_owned());
    SetupInfo {
        documents: s(path.document_dir()),
        desktop: s(path.desktop_dir()),
        downloads: s(path.download_dir()),
        root_folder_name: setup::ROOT_FOLDER_NAME,
        separator: std::path::MAIN_SEPARATOR.to_string(),
        teacher_example: setup::TEACHER_EXAMPLE
            .iter()
            .map(|(name, children)| ExampleFolder { name, children })
            .collect(),
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    state.with_db(settings::load)
}

#[tauri::command]
pub fn complete_setup(
    state: State<'_, AppState>,
    request: SetupRequest,
) -> Result<Settings, String> {
    let new_settings = setup::run(&request)?;
    state.with_db(|conn| settings::save(conn, &new_settings))?;
    Ok(new_settings)
}

#[tauri::command]
pub fn set_organize_mode(
    state: State<'_, AppState>,
    mode: OrganizeMode,
) -> Result<Settings, String> {
    state.with_db(|conn| {
        let mut s = settings::load(conn)?;
        s.organize_mode = mode;
        settings::save(conn, &s)?;
        Ok(s)
    })
}

#[tauri::command]
pub fn set_watch_folders(
    state: State<'_, AppState>,
    folders: Vec<String>,
) -> Result<Settings, String> {
    state
        .with_db(|conn| {
            let mut s = settings::load(conn)?;
            setup::validate_watch_folders(&folders, s.root_folder.as_deref().unwrap_or_default())?;
            s.watch_folders = folders;
            settings::save(conn, &s)?;
            Ok(s)
        })
        .inspect(|s| state.apply_watch_folders(s))
}

#[tauri::command]
pub fn list_folders(path: String) -> Result<Vec<FolderEntry>, String> {
    folders::list_subfolders(Path::new(&path)).map_err(|e| format!("폴더를 읽지 못했어요: {e}"))
}

#[tauri::command]
pub fn create_folder(parent: String, name: String) -> Result<FolderEntry, String> {
    folders::create_folder(Path::new(&parent), &name)
}

#[tauri::command]
pub fn search_folders(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<FolderMatch>, String> {
    let root = state
        .with_db(settings::load)?
        .root_folder
        .ok_or("정리 폴더가 아직 없어요.")?;
    Ok(folders::search(Path::new(&root), &query, 50))
}

/// 최근에 넣은 폴더 3개(지금도 있는 것만).
#[tauri::command]
pub fn recent_destinations(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let all = state.with_db(|conn| history::recent_destinations(conn, 10).map_err(db_err))?;
    Ok(all
        .into_iter()
        .filter(|d| Path::new(d).is_dir())
        .take(3)
        .collect())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PopupState {
    pub current: Option<PendingFile>,
    /// 지금 파일 말고 기다리는 파일 수
    pub remaining: usize,
}

#[tauri::command]
pub fn popup_state(state: State<'_, AppState>) -> PopupState {
    state.with_pending(|q| PopupState {
        current: q.front().cloned(),
        remaining: q.len().saturating_sub(1),
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PutResult {
    pub move_id: i64,
    pub moved_path: String,
    pub dest_dir: String,
}

/// 파일을 폴더에 넣는다. `new_name`을 주면 그 이름으로 바꿔 넣는다. 이동은 기록에 남는다.
#[tauri::command]
pub fn put_file(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    dest_dir: String,
    new_name: Option<String>,
) -> Result<PutResult, String> {
    let src = PathBuf::from(&path);
    let moved = mover::move_into(&src, Path::new(&dest_dir), new_name.as_deref())?;
    let move_id = state.with_db(|conn| {
        inbox::remove(conn, &src).map_err(db_err)?;
        history::record(conn, &src, &moved).map_err(db_err)
    })?;
    state.with_pending(|q| q.retain(|p| p.path != path));
    let _ = app.emit(EVENT_INBOX_CHANGED, ());
    Ok(PutResult {
        move_id,
        moved_path: moved.to_string_lossy().into_owned(),
        dest_dir,
    })
}

/// 이동 기록 하나를 되돌린다: 원래 폴더·원래 이름으로. 돌아온 파일과 그 폴더를 돌려준다.
fn undo_one(state: &AppState, move_id: i64) -> Result<(PathBuf, PathBuf), String> {
    let record = state
        .with_db(|conn| history::get(conn, move_id).map_err(db_err))?
        .ok_or("되돌릴 기록을 찾을 수 없어요.")?;
    if record.undone {
        return Err("이미 되돌렸어요.".into());
    }
    let original = PathBuf::from(&record.original_path);
    let restored = mover::move_back(Path::new(&record.moved_path), &original)?;
    state.with_db(|conn| history::mark_undone(conn, move_id).map_err(db_err))?;
    // 돌아온 파일을 감시가 새 파일로 다시 잡지 않게
    state.watcher.mark_known(restored.clone());
    let source_dir = original.parent().unwrap_or(Path::new("")).to_path_buf();
    Ok((restored, source_dir))
}

/// 되돌리기(팝업): 원래 위치로 돌려놓는다. `requeue`면 팝업에서 다시 물어본다.
#[tauri::command]
pub fn undo_move(
    app: AppHandle,
    state: State<'_, AppState>,
    move_id: i64,
    requeue: bool,
) -> Result<PendingFile, String> {
    let (restored, source_dir) = undo_one(&state, move_id)?;
    let item = PendingFile::new(&restored, &source_dir);
    if requeue {
        state.with_pending(|q| q.push_front(item.clone()));
    }
    let _ = app.emit(EVENT_INBOX_CHANGED, ());
    Ok(item)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoManyResult {
    pub restored: usize,
    pub errors: Vec<String>,
}

/// 되돌리기(정리함): 여러 이동을 되돌리고, 돌아온 파일은 다시 정리함에 넣는다.
#[tauri::command]
pub fn undo_moves(
    app: AppHandle,
    state: State<'_, AppState>,
    move_ids: Vec<i64>,
) -> UndoManyResult {
    let mut result = UndoManyResult {
        restored: 0,
        errors: Vec::new(),
    };
    for id in move_ids {
        match undo_one(&state, id) {
            Ok((restored, source_dir)) => {
                let _ = state.with_db(|conn| {
                    inbox::add(conn, &restored, &source_dir, inbox::Reason::Later).map_err(db_err)
                });
                result.restored += 1;
            }
            Err(e) => result.errors.push(e),
        }
    }
    let _ = app.emit(EVENT_INBOX_CHANGED, ());
    result
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PutOutcome {
    pub path: String,
    pub move_id: Option<i64>,
    pub error: Option<String>,
}

/// 여러 파일을 한 폴더에 넣는다(정리함). 파일마다 결과를 돌려준다.
#[tauri::command]
pub fn put_files(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
    dest_dir: String,
) -> Vec<PutOutcome> {
    let dest = PathBuf::from(&dest_dir);
    let outcomes = paths
        .into_iter()
        .map(|path| {
            let src = PathBuf::from(&path);
            let result = mover::move_into(&src, &dest, None).and_then(|moved| {
                state.with_db(|conn| {
                    inbox::remove(conn, &src).map_err(db_err)?;
                    history::record(conn, &src, &moved).map_err(db_err)
                })
            });
            state.with_pending(|q| q.retain(|p| p.path != path));
            match result {
                Ok(id) => PutOutcome {
                    path,
                    move_id: Some(id),
                    error: None,
                },
                Err(e) => PutOutcome {
                    path,
                    move_id: None,
                    error: Some(e),
                },
            }
        })
        .collect();
    let _ = app.emit(EVENT_INBOX_CHANGED, ());
    outcomes
}

/// 지금 지켜볼 폴더에 있는 파일을 정리함으로 가져온다(시작할 때 있던 파일 등). 새로 들어온 수를 돌려준다.
#[tauri::command]
pub fn import_existing(app: AppHandle, state: State<'_, AppState>) -> Result<usize, String> {
    let s = state.with_db(settings::load)?;
    let mut added = 0;
    for dir in s.watch_folders.iter().map(PathBuf::from) {
        for (path, stat) in watcher::list_files(&dir) {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !watcher::is_candidate_name(&name)
                || stat.size == 0
                || watcher::has_partial_partner(&path)
            {
                continue;
            }
            let new = state.with_db(|conn| {
                inbox::add_if_absent(conn, &path, &dir, inbox::Reason::Collected).map_err(db_err)
            })?;
            if new {
                added += 1;
            }
        }
    }
    let _ = app.emit(EVENT_INBOX_CHANGED, ());
    Ok(added)
}

/// 자주 쓰는 폴더(숫자 키 1~6): 자주 넣은 폴더부터, 모자라면 정리 폴더 바로 아래 폴더로 채운다.
#[tauri::command]
pub fn favorite_folders(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    const COUNT: usize = 6;
    let s = state.with_db(settings::load)?;
    let mut out: Vec<String> = state
        .with_db(|conn| history::frequent_destinations(conn, 20).map_err(db_err))?
        .into_iter()
        .filter(|d| Path::new(d).is_dir())
        .take(COUNT)
        .collect();
    if let Some(root) = s.root_folder {
        if let Ok(top) = folders::list_subfolders(Path::new(&root)) {
            for f in top {
                if out.len() >= COUNT {
                    break;
                }
                if !out.contains(&f.path) {
                    out.push(f.path);
                }
            }
        }
    }
    Ok(out)
}

/// "나중에 정리": 정리함으로 보내고 팝업 차례에서 뺀다.
#[tauri::command]
pub fn defer_file(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<(), String> {
    let source_dir = state
        .with_pending(|q| {
            q.iter()
                .find(|p| p.path == path)
                .map(|p| p.source_dir.clone())
        })
        .map(PathBuf::from)
        .or_else(|| Path::new(&path).parent().map(Path::to_path_buf))
        .unwrap_or_default();
    state.with_db(|conn| {
        inbox::add(conn, Path::new(&path), &source_dir, inbox::Reason::Later).map_err(db_err)
    })?;
    state.with_pending(|q| q.retain(|p| p.path != path));
    let _ = app.emit(EVENT_INBOX_CHANGED, ());
    Ok(())
}

/// 팝업 창 숨기기(물어볼 파일이 더 없을 때).
#[tauri::command]
pub fn hide_popup(app: AppHandle) {
    popup::hide(&app);
}

#[tauri::command]
pub fn list_inbox(state: State<'_, AppState>) -> Result<Vec<InboxItem>, String> {
    state.with_db(|conn| inbox::list(conn).map_err(db_err))
}
