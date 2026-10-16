//! 화면에서 `invoke`로 부르는 명령 모음.

use crate::folders::{self, FolderEntry, FolderMatch};
use crate::inbox::{self, InboxItem};
use crate::naming::{self, DestSegment};
use crate::rules::{self, Rule, RuleDraft, RuleInput, Target};
use crate::settings::{self, OrganizeMode, PlaceMode, Settings};
use crate::setup::{self, SetupRequest};
use crate::watcher::{self, NewFile, WatchService};
use crate::{history, mover, popup, recycle};
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
    /// "바로 정리"로 팝업 없이 옮긴 파일(팝업에서 알리고 되돌리기를 보여 준다)
    pub auto_moved: Mutex<Vec<AutoMoved>>,
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

    fn with_auto<T>(&self, f: impl FnOnce(&mut Vec<AutoMoved>) -> T) -> T {
        let mut list = self.auto_moved.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut list)
    }

    /// 이 파일에 맞는 규칙과 넣을 폴더(SPEC 4장). 규칙이 없거나 저장 위치가 없어졌으면 None.
    fn suggest(&self, path: &Path) -> Option<(Rule, Target)> {
        let name = path.file_name()?.to_string_lossy().into_owned();
        // 지켜볼 폴더는 바로 아래 파일만 보므로 출처 = 파일이 있는 폴더
        let source = path.parent()?;
        self.with_db(|conn| {
            let s = settings::load(conn)?;
            let Some(root) = s.root_folder.as_deref() else {
                return Ok(None);
            };
            let list = rules::list(conn).map_err(db_err)?;
            Ok(rules::find_match(&list, &name, source).and_then(|rule| {
                rules::resolve(
                    rule,
                    Path::new(root),
                    &name,
                    naming::today(),
                    s.term_months(),
                )
                .map(|t| (rule.clone(), t))
            }))
        })
        .ok()
        .flatten()
    }

    fn root(&self) -> Result<PathBuf, String> {
        self.with_db(settings::load)?
            .root_folder
            .map(PathBuf::from)
            .ok_or_else(|| "정리 폴더가 아직 없어요.".to_string())
    }

    /// 추천 위치(규칙)대로 넣는다. 연도·학기 폴더가 없으면 만든다.
    fn put_by_rule(&self, src: &Path) -> Result<(i64, PathBuf, Rule), String> {
        let (rule, target) = self.suggest(src).ok_or("이 파일에 맞는 규칙이 없어요.")?;
        rules::ensure_target(&target, &self.root()?)?;
        let moved = mover::move_into(src, &target.dir, None)?;
        let id = self.with_db(|conn| {
            inbox::remove(conn, src).map_err(db_err)?;
            let _ = rules::reset_misses(conn, rule.id);
            history::record_with_rule(conn, src, &moved, Some(rule.id)).map_err(db_err)
        })?;
        let p = src.to_string_lossy();
        self.with_pending(|q| q.retain(|x| x.path != p));
        Ok((id, moved, rule))
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

/// 추천 위치(팝업·정리함)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub rule: Rule,
    pub dest_dir: String,
    /// 이미 있는 폴더인지(없으면 넣을 때 만든다)
    pub exists: bool,
    /// 팝업에서 보이게 열 폴더(지금 있는 가장 깊은 폴더)
    pub reveal: String,
}

impl Suggestion {
    fn new((rule, target): (Rule, Target)) -> Self {
        Suggestion {
            rule,
            dest_dir: target.dir.to_string_lossy().into_owned(),
            exists: target.exists,
            reveal: target.existing.to_string_lossy().into_owned(),
        }
    }
}

/// "바로 정리"로 옮긴 파일
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoMoved {
    pub move_id: i64,
    pub name: String,
    pub dest_dir: String,
    pub rule: Rule,
}

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

/// 감시가 저장이 끝난 새 파일을 찾았을 때(SPEC 2장).
/// 모아 두기 → 정리함 / 바로 정리 + 규칙에 맞음 → 옮기고 알림 / 그 외 → 팝업.
pub fn handle_new_file(app: &AppHandle, file: NewFile) {
    let state = app.state::<AppState>();
    let s = state.with_db(settings::load).unwrap_or_default();
    if s.organize_mode == OrganizeMode::Ask && s.place_mode == PlaceMode::Auto {
        // 옮기지 못하면(파일이 열려 있는 등) 팝업으로 물어본다.
        if let Ok((move_id, moved, rule)) = state.put_by_rule(&file.path) {
            state.with_auto(|list| {
                list.push(AutoMoved {
                    move_id,
                    name: moved
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    dest_dir: moved
                        .parent()
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    rule,
                })
            });
            let _ = app.emit(EVENT_INBOX_CHANGED, ());
            let _ = app.emit(EVENT_POPUP_CHANGED, ());
            popup::show(app);
            return;
        }
    }
    match s.organize_mode {
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
pub fn set_place_mode(state: State<'_, AppState>, mode: PlaceMode) -> Result<Settings, String> {
    state.with_db(|conn| {
        let mut s = settings::load(conn)?;
        s.place_mode = mode;
        settings::save(conn, &s)?;
        Ok(s)
    })
}

/// 1학기·2학기가 시작하는 달
#[tauri::command]
pub fn set_term_months(
    app: AppHandle,
    state: State<'_, AppState>,
    first: u32,
    second: u32,
) -> Result<Settings, String> {
    if !settings::valid_term_months(first, second) {
        return Err("1학기가 2학기보다 먼저 시작해야 해요.".into());
    }
    let s = state.with_db(|conn| {
        let mut s = settings::load(conn)?;
        s.first_term_month = first;
        s.second_term_month = second;
        settings::save(conn, &s)?;
        Ok(s)
    })?;
    let _ = app.emit(EVENT_INBOX_CHANGED, ());
    Ok(s)
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

/// 정리 폴더 안쪽 폴더인지 확인(정리 폴더 자체나 바깥은 고치지 않는다).
fn ensure_inside_root(state: &AppState, path: &Path) -> Result<(), String> {
    let root = state
        .with_db(settings::load)?
        .root_folder
        .ok_or("정리 폴더가 아직 없어요.")?;
    if folders::is_inside(Path::new(&root), path) {
        Ok(())
    } else {
        Err("정리 폴더 안의 폴더만 바꿀 수 있어요.".into())
    }
}

fn entry_for(path: &Path) -> FolderEntry {
    FolderEntry {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: path.to_string_lossy().into_owned(),
        file_count: 0,
        folder_count: 0,
    }
}

/// 폴더 이름을 바꾸거나 옮긴 뒤 기록·규칙의 경로를 따라 바꾼다.
fn rewrite_paths(conn: &Connection, old: &Path, new: &Path) -> Result<(), String> {
    history::rewrite_folder_prefix(conn, old, new).map_err(db_err)?;
    rules::rewrite_folder_prefix(conn, old, new).map_err(db_err)?;
    Ok(())
}

#[tauri::command]
pub fn rename_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    new_name: String,
) -> Result<FolderEntry, String> {
    let old = PathBuf::from(&path);
    ensure_inside_root(&state, &old)?;
    let new = folders::rename_folder(&old, &new_name)?;
    let _ = state.with_db(|conn| rewrite_paths(conn, &old, &new));
    let _ = app.emit(EVENT_INBOX_CHANGED, ());
    Ok(entry_for(&new))
}

#[tauri::command]
pub fn move_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    dest_parent: String,
) -> Result<FolderEntry, String> {
    let old = PathBuf::from(&path);
    ensure_inside_root(&state, &old)?;
    let root = state
        .with_db(settings::load)?
        .root_folder
        .unwrap_or_default();
    let dest = PathBuf::from(&dest_parent);
    if !(folders::is_inside(Path::new(&root), &dest)
        || dest.canonicalize().ok() == Path::new(&root).canonicalize().ok())
    {
        return Err("정리 폴더 안으로만 옮길 수 있어요.".into());
    }
    let new = folders::move_folder(&old, &dest)?;
    let _ = state.with_db(|conn| rewrite_paths(conn, &old, &new));
    let _ = app.emit(EVENT_INBOX_CHANGED, ());
    Ok(entry_for(&new))
}

/// 폴더를 휴지통으로(정리 폴더 안쪽만). 되돌리기용 기록 번호를 돌려준다.
#[tauri::command]
pub fn trash_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<i64, String> {
    let p = PathBuf::from(&path);
    ensure_inside_root(&state, &p)?;
    let id = state.with_db(|conn| recycle::send_to_trash(conn, &p))?;
    let _ = app.emit(EVENT_INBOX_CHANGED, ());
    Ok(id)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashResult {
    pub deletion_ids: Vec<i64>,
    pub errors: Vec<String>,
}

/// 정리함의 파일들을 휴지통으로.
#[tauri::command]
pub fn trash_files(app: AppHandle, state: State<'_, AppState>, paths: Vec<String>) -> TrashResult {
    let mut result = TrashResult {
        deletion_ids: Vec::new(),
        errors: Vec::new(),
    };
    for path in paths {
        let p = PathBuf::from(&path);
        if !p.is_file() {
            result.errors.push("파일을 찾을 수 없어요.".into());
            continue;
        }
        match state.with_db(|conn| {
            let id = recycle::send_to_trash(conn, &p)?;
            inbox::remove(conn, &p).map_err(db_err)?;
            Ok(id)
        }) {
            Ok(id) => result.deletion_ids.push(id),
            Err(e) => result.errors.push(e),
        }
        state.with_pending(|q| q.retain(|x| x.path != path));
    }
    let _ = app.emit(EVENT_INBOX_CHANGED, ());
    result
}

/// 휴지통으로 보낸 것을 원래 자리로. 파일이면 정리함에도 다시 넣는다.
#[tauri::command]
pub fn restore_deleted(
    app: AppHandle,
    state: State<'_, AppState>,
    deletion_ids: Vec<i64>,
) -> UndoManyResult {
    let mut result = UndoManyResult {
        restored: 0,
        errors: Vec::new(),
    };
    for id in deletion_ids {
        match state.with_db(|conn| recycle::restore(conn, id)) {
            Ok(record) => {
                let path = PathBuf::from(&record.original_path);
                if !record.is_folder {
                    state.watcher.mark_known(path.clone());
                    let source = path.parent().unwrap_or(Path::new("")).to_path_buf();
                    let _ = state.with_db(|conn| {
                        inbox::add(conn, &path, &source, inbox::Reason::Later).map_err(db_err)
                    });
                }
                result.restored += 1;
            }
            Err(e) => result.errors.push(e),
        }
    }
    let _ = app.emit(EVENT_INBOX_CHANGED, ());
    result
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
    /// 지금 파일의 추천 위치
    pub suggestion: Option<Suggestion>,
    /// "바로 정리"로 옮겨서 알릴 파일들
    pub auto_moved: Vec<AutoMoved>,
}

#[tauri::command]
pub fn popup_state(state: State<'_, AppState>) -> PopupState {
    let (current, remaining) =
        state.with_pending(|q| (q.front().cloned(), q.len().saturating_sub(1)));
    let suggestion = current
        .as_ref()
        .and_then(|c| state.suggest(Path::new(&c.path)))
        .map(Suggestion::new);
    PopupState {
        current,
        remaining,
        suggestion,
        auto_moved: state.with_auto(|l| l.clone()),
    }
}

/// "바로 정리" 알림을 닫는다.
#[tauri::command]
pub fn dismiss_auto(state: State<'_, AppState>, move_ids: Vec<i64>) {
    state.with_auto(|l| l.retain(|a| !move_ids.contains(&a.move_id)));
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PutResult {
    pub move_id: i64,
    pub moved_path: String,
    pub dest_dir: String,
    /// 추천(규칙)대로 넣었으면 그 규칙
    pub rule: Option<Rule>,
    /// 같은 규칙에서 같은 다른 폴더를 세 번 연속 골랐으면: "규칙을 바꿀까요?"
    pub change: Option<RuleChange>,
    /// 그 밖에는 "다음에도 여기에?" 규칙 만들기 카드
    pub draft: Option<RuleDraft>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleChange {
    pub rule: Rule,
    /// 지금 규칙이 넣는 폴더(이번 파일 기준)
    pub old_dir: String,
    pub new_dir: String,
}

/// 파일을 폴더에 넣는다(팝업). `new_name`을 주면 그 이름으로 바꿔 넣는다. 이동은 기록에 남는다.
/// 추천 위치에 넣으면 규칙 번호도 남기고, 연도·학기 폴더가 없으면 만든다.
#[tauri::command]
pub fn put_file(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    dest_dir: String,
    new_name: Option<String>,
) -> Result<PutResult, String> {
    let src = PathBuf::from(&path);
    let dest = PathBuf::from(&dest_dir);
    let original_name = src
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let s = state.with_db(settings::load)?;

    let mut used: Option<Rule> = None;
    let mut change: Option<RuleChange> = None;
    if let Some((rule, target)) = state.suggest(&src) {
        if rules::same_path(&dest, &target.dir) {
            rules::ensure_target(&target, &state.root()?)?;
            used = Some(rule);
        } else if state
            .with_db(|conn| rules::note_choice(conn, rule.id, &dest, &target.dir).map_err(db_err))
            .unwrap_or(false)
        {
            change = Some(RuleChange {
                rule,
                old_dir: target.dir.to_string_lossy().into_owned(),
                new_dir: dest_dir.clone(),
            });
        }
    }

    let moved = mover::move_into(&src, &dest, new_name.as_deref())?;
    let move_id = state.with_db(|conn| {
        inbox::remove(conn, &src).map_err(db_err)?;
        if let Some(r) = &used {
            let _ = rules::reset_misses(conn, r.id);
        }
        history::record_with_rule(conn, &src, &moved, used.as_ref().map(|r| r.id)).map_err(db_err)
    })?;
    state.with_pending(|q| q.retain(|p| p.path != path));
    let _ = app.emit(EVENT_INBOX_CHANGED, ());

    let draft = match (&used, &change, &s.root_folder) {
        (None, None, Some(root)) => Some(rules::draft(
            Path::new(root),
            &original_name,
            &dest,
            naming::today(),
            s.term_months(),
        )),
        _ => None,
    };
    Ok(PutResult {
        move_id,
        moved_path: moved.to_string_lossy().into_owned(),
        dest_dir,
        rule: used,
        change,
        draft,
    })
}

/// 추천 위치에 넣는다(정리함의 [넣기]·[추천대로 모두]). 파일마다 결과를 돌려준다.
#[tauri::command]
pub fn put_suggested(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Vec<PutOutcome> {
    let outcomes = paths
        .into_iter()
        .map(|path| match state.put_by_rule(Path::new(&path)) {
            Ok((id, _, _)) => PutOutcome {
                path,
                move_id: Some(id),
                error: None,
            },
            Err(e) => PutOutcome {
                path,
                move_id: None,
                error: Some(e),
            },
        })
        .collect();
    let _ = app.emit(EVENT_INBOX_CHANGED, ());
    outcomes
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
    state.with_auto(|l| l.retain(|a| a.move_id != move_id));
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxEntry {
    #[serde(flatten)]
    pub item: InboxItem,
    pub suggestion: Option<Suggestion>,
}

#[tauri::command]
pub fn list_inbox(state: State<'_, AppState>) -> Result<Vec<InboxEntry>, String> {
    let items = state.with_db(|conn| inbox::list(conn).map_err(db_err))?;
    Ok(items
        .into_iter()
        .map(|item| InboxEntry {
            suggestion: state.suggest(Path::new(&item.path)).map(Suggestion::new),
            item,
        })
        .collect())
}

// ---------- 정리 규칙(SPEC 4장) ----------

/// 규칙 목록 화면용: 저장 위치를 단계별로, 폴더가 없어졌는지
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleView {
    #[serde(flatten)]
    pub rule: Rule,
    pub segments: Vec<DestSegment>,
    pub missing: bool,
}

#[tauri::command]
pub fn list_rules(state: State<'_, AppState>) -> Result<Vec<RuleView>, String> {
    let root = state.root()?;
    let list = state.with_db(|conn| rules::list(conn).map_err(db_err))?;
    Ok(list
        .into_iter()
        .map(|rule| {
            let dest = PathBuf::from(&rule.dest_dir);
            // 나누는 규칙은 앞부분만 있으면 된다(없는 연도·학기 폴더는 넣을 때 만든다)
            let missing = rules::resolve(&rule, &root, "", (2000, 3), Default::default()).is_none();
            RuleView {
                segments: rules::describe_dest(&root, &dest),
                missing,
                rule,
            }
        })
        .collect())
}

/// 규칙 편집 창에서 고른 폴더를 단계별로(나눌 수 있는 폴더 표시용)
#[tauri::command]
pub fn describe_dest(
    state: State<'_, AppState>,
    dest_dir: String,
) -> Result<Vec<DestSegment>, String> {
    Ok(rules::describe_dest(&state.root()?, Path::new(&dest_dir)))
}

fn checked_input(input: RuleInput) -> Result<RuleInput, String> {
    let input = rules::normalize(input)?;
    if !Path::new(&input.dest_dir).is_dir() {
        return Err("넣을 폴더를 찾을 수 없어요.".into());
    }
    Ok(input)
}

#[tauri::command]
pub fn create_rule(
    app: AppHandle,
    state: State<'_, AppState>,
    input: RuleInput,
) -> Result<Rule, String> {
    let input = checked_input(input)?;
    let rule = state.with_db(|conn| {
        let id = rules::create(conn, &input).map_err(db_err)?;
        rules::get(conn, id)
            .map_err(db_err)?
            .ok_or("규칙을 저장하지 못했어요.".into())
    })?;
    let _ = app.emit(EVENT_INBOX_CHANGED, ());
    Ok(rule)
}

#[tauri::command]
pub fn update_rule(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    input: RuleInput,
) -> Result<Rule, String> {
    let input = checked_input(input)?;
    let rule = state.with_db(|conn| {
        rules::update(conn, id, &input).map_err(db_err)?;
        rules::get(conn, id)
            .map_err(db_err)?
            .ok_or("규칙을 찾을 수 없어요.".into())
    })?;
    let _ = app.emit(EVENT_INBOX_CHANGED, ());
    Ok(rule)
}

/// 규칙 지우기(되돌리기: restore_rule)
#[tauri::command]
pub fn delete_rule(app: AppHandle, state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state.with_db(|conn| rules::delete(conn, id).map_err(db_err))?;
    let _ = app.emit(EVENT_INBOX_CHANGED, ());
    Ok(())
}

#[tauri::command]
pub fn restore_rule(app: AppHandle, state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state.with_db(|conn| rules::restore(conn, id).map_err(db_err))?;
    let _ = app.emit(EVENT_INBOX_CHANGED, ());
    Ok(())
}

/// 규칙 순서 바꾸기(위에서부터 먼저 본다)
#[tauri::command]
pub fn move_rule(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    up: bool,
) -> Result<bool, String> {
    let moved = state.with_db(|conn| rules::move_by_one(conn, id, up).map_err(db_err))?;
    let _ = app.emit(EVENT_INBOX_CHANGED, ());
    Ok(moved)
}

/// "규칙을 바꿀까요?" → 바꾸기(`dest_dir` = 새 저장 위치) 또는 그대로 두기(`dest_dir` 없음)
#[tauri::command]
pub fn answer_rule_change(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    dest_dir: Option<String>,
) -> Result<(), String> {
    state.with_db(|conn| match &dest_dir {
        Some(d) if Path::new(d).is_dir() => {
            rules::change_dest(conn, id, Path::new(d)).map_err(db_err)
        }
        Some(_) => Err("새 저장 위치 폴더를 찾을 수 없어요.".into()),
        None => rules::reset_misses(conn, id).map_err(db_err),
    })?;
    let _ = app.emit(EVENT_INBOX_CHANGED, ());
    Ok(())
}
