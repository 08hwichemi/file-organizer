//! 화면에서 `invoke`로 부르는 명령 모음.

use crate::classify::{self, Classifier, Reason, WordGroup};
use crate::folders::{self, FileEntry, FolderEntry, FolderMatch};
use crate::inbox::{self, InboxItem};
use crate::naming::{self, DestSegment};
use crate::rules::{self, Rule, RuleDraft, RuleInput, Target};
use crate::settings::{self, NewFileScope, OrganizeMode, PlaceMode, Settings, UserKind};
use crate::setup::{self, SetupRequest};
use crate::watcher::{self, NewFile, WatchService};
use crate::{history, mover, popup, recycle, resort, tray};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

pub struct AppState {
    pub db: Mutex<Connection>,
    pub watcher: WatchService,
    /// 팝업에서 물어볼 차례를 기다리는 새 파일들(맨 앞이 지금 보이는 파일).
    pub pending: Mutex<VecDeque<PendingFile>>,
    /// "바로 정리"로 팝업 없이 옮긴 파일(팝업에서 알리고 되돌리기를 보여 준다)
    pub auto_moved: Mutex<Vec<AutoMoved>>,
    /// 일시 정지(트레이·메인 창). 정지 중 들어온 파일은 팝업 없이 정리함으로(SPEC 5장).
    /// 프로그램을 다시 켜면 풀린다.
    pub paused: AtomicBool,
    /// 파일 검색 자체 색인(Everything을 쓸 수 없을 때)
    pub index: Mutex<crate::search::Index>,
    /// 자체 색인을 읽는 중
    pub indexing: AtomicBool,
    /// 자체 색인을 최신으로 맞추는 감시
    pub live: crate::finder::Live,
    /// 검색 단축키를 등록하지 못했을 때 그 까닭(설정 화면에 보여 줌)
    pub shortcut_error: Mutex<Option<String>>,
    /// 내 PC 어디든: 하위 폴더까지 볼 곳(사용자 폴더·다른 고정 디스크, 켤 때 정함)
    pub wide_roots: Vec<PathBuf>,
    /// 한꺼번에 생겨서 정리함에 모아 둔 새 파일(팝업에서 한 번만 알린다)
    pub bulk: Mutex<Vec<PendingFile>>,
    /// 업데이트 뒤 다시 켜졌으면 그 버전(창을 열 때 한 번 알리고 비운다)
    pub updated_to: Mutex<Option<String>>,
    /// 자동 업데이트로 받아 둔 새 버전(autoupdate.rs)
    pub ready_update: crate::autoupdate::Ready,
}

impl AppState {
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

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

    /// 지금 설정(저장소를 못 열면 Err)
    pub fn with_settings(&self) -> Result<Settings, String> {
        self.with_db(settings::load)
    }

    /// 물어볼 파일·"바로 정리" 알림·한꺼번에 알림이 남아 있는지(자동 업데이트는 이때 기다린다)
    pub fn has_waiting_files(&self) -> bool {
        !(self.with_pending(|q| q.is_empty())
            && self.with_auto(|l| l.is_empty())
            && self.with_bulk(|l| l.is_empty()))
    }

    fn with_bulk<T>(&self, f: impl FnOnce(&mut Vec<PendingFile>) -> T) -> T {
        let mut list = self.bulk.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut list)
    }

    /// 지금 설정·규칙·자동 분류 단어·폴더로 분류기를 만든다(정리 폴더가 없으면 None).
    /// 파일이 여러 개면 한 번 만들어 같이 쓴다.
    fn classifier(&self) -> Option<Classifier> {
        self.with_db(|conn| {
            let s = settings::load(conn)?;
            let Some(root) = s.root_folder.as_deref() else {
                return Ok(None);
            };
            let list = rules::list(conn).map_err(db_err)?;
            Ok(Some(Classifier::new(
                Path::new(root),
                list,
                classify::load_words(conn),
                naming::today(),
                s.term_months(),
                s.user_kind,
            )))
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

    /// 정해진 폴더(추천·자동 분류)에 넣는다. 없는 폴더는 만든다. 규칙이면 규칙 번호도 기록.
    fn put_to(
        &self,
        src: &Path,
        reason: &Reason,
        target: &Target,
    ) -> Result<(i64, PathBuf), String> {
        rules::ensure_target(target, &self.root()?)?;
        let moved = mover::move_into(src, &target.dir, None)?;
        // 넣은 곳이 지켜볼 폴더(바탕화면 등)여도 새 파일로 다시 묻지 않게
        self.watcher.mark_known(moved.clone());
        let rule_id = match reason {
            Reason::Rule { rule } => Some(rule.id),
            _ => None,
        };
        let id = self.with_db(|conn| {
            inbox::remove(conn, src).map_err(db_err)?;
            if let Some(r) = rule_id {
                let _ = rules::reset_misses(conn, r);
            }
            history::record_with_rule(conn, src, &moved, rule_id).map_err(db_err)
        })?;
        let p = src.to_string_lossy();
        self.with_pending(|q| q.retain(|x| x.path != p));
        Ok((id, moved))
    }

    /// 감시를 지금 설정(새 파일을 찾는 곳·지켜볼 폴더·묻지 않는 폴더)에 맞춘다.
    pub fn apply_watch_folders(&self, s: &Settings) {
        self.watcher.set_scope(watch_scope(s, &self.wide_roots));
    }
}

/// 설정으로 감시할 곳을 정한다(SPEC 5장). 정리 폴더와 묻지 않는 폴더는 넓은 감시에서 빼고,
/// 묻지 않는 폴더는 지켜볼 폴더에서도 뺀다.
pub fn watch_scope(s: &Settings, wide_roots: &[PathBuf]) -> watcher::Scope {
    if !s.setup_done {
        return watcher::Scope::default();
    }
    let quiet: Vec<PathBuf> = s.quiet_folders.iter().map(PathBuf::from).collect();
    let folders = s
        .watch_folders
        .iter()
        .map(PathBuf::from)
        .filter(|w| {
            !quiet
                .iter()
                .any(|q| folders::same_dir(q, w) || folders::is_inside(q, w))
        })
        .collect();
    let wide = match s.new_file_scope {
        NewFileScope::Anywhere => wide_roots.to_vec(),
        NewFileScope::Watched => Vec::new(),
    };
    let mut skip = quiet;
    skip.extend(s.root_folder.as_deref().map(PathBuf::from));
    watcher::Scope {
        folders,
        wide,
        skip,
    }
}

fn db_err(e: rusqlite::Error) -> String {
    format!("기록을 저장하지 못했어요: {e}")
}

/// 화면에 알리는 이벤트 이름
pub const EVENT_POPUP_CHANGED: &str = "popup-changed";
pub const EVENT_INBOX_CHANGED: &str = "inbox-changed";
/// 일시 정지 등 상태가 바뀜
pub const EVENT_STATUS_CHANGED: &str = "status-changed";
/// 설정이 다른 창(팝업)에서 바뀜
pub const EVENT_SETTINGS_CHANGED: &str = "settings-changed";

/// 정리함이 바뀌었음을 화면과 트레이(정리함 개수)에 알린다.
pub fn notify_inbox(app: &AppHandle) {
    let _ = app.emit(EVENT_INBOX_CHANGED, ());
    tray::refresh(app);
}

/// 추천 위치(팝업·정리함)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    /// 왜 이 폴더인지(규칙 · 폴더 이름 · 자동 분류 단어 · 파일 형식)
    pub reason: Reason,
    pub dest_dir: String,
    /// 이미 있는 폴더인지(없으면 넣을 때 만든다)
    pub exists: bool,
    /// 팝업에서 보이게 열 폴더(지금 있는 가장 깊은 폴더)
    pub reveal: String,
}

impl Suggestion {
    fn new((reason, target): (Reason, Target)) -> Self {
        Suggestion {
            reason,
            dest_dir: target.dir.to_string_lossy().into_owned(),
            exists: target.exists,
            reveal: target.existing.to_string_lossy().into_owned(),
        }
    }
}

/// "바로 정리"·"자동 분류"로 옮긴 파일
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoMoved {
    pub move_id: i64,
    pub name: String,
    pub dest_dir: String,
    pub reason: Reason,
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

/// 한꺼번에 찾은 새 파일이 이보다 많으면 하나씩 묻지 않고 정리함에 모아 한 번만 알린다(압축 풀기·폴더 복사).
pub const BULK_LIMIT: usize = 5;
/// 지켜볼 폴더 밖에서 한꺼번에 이보다 많이 생기면 프로그램 설치·큰 폴더 복사로 보고 정리함에도 넣지 않는다.
pub const WIDE_IGNORE_OVER: usize = 300;

/// 새 파일 하나를 어떻게 했는지
enum Placed {
    /// 바로 정리·자동 분류로 옮김
    Moved,
    /// 정리함에 모음
    Collected,
    /// 팝업에서 물어볼 파일
    Ask(PendingFile),
}

/// 감시가 저장이 끝난 새 파일들을 찾았을 때(SPEC 2장, 한꺼번에 찾은 것은 한 묶음).
/// 자동 분류 → 나눌 수 있으면 옮기고 알림, 모르면 정리함 /
/// 모아 두기 → 정리함 / 바로 정리 + 규칙에 맞음 → 옮기고 알림 / 그 외 → 팝업.
/// 물어볼 파일이 `BULK_LIMIT`보다 많으면 정리함에 모으고 팝업에서 한 번만 알린다.
pub fn handle_new_files(app: &AppHandle, files: Vec<NewFile>) {
    let state = app.state::<AppState>();
    let elsewhere = files.iter().filter(|f| f.elsewhere).count();
    let files: Vec<NewFile> = if elsewhere > WIDE_IGNORE_OVER {
        files.into_iter().filter(|f| !f.elsewhere).collect()
    } else {
        files
    };
    // 이미 정리했던 파일을 사용자가 옮겨 오거나 고쳐 저장한 것, 정리함에 있는 파일을 고쳐 저장한 것은
    // 다시 묻지 않는다(사용자 보고)
    let files: Vec<NewFile> = state
        .with_db(|conn| {
            Ok(files
                .into_iter()
                .filter(|f| !inbox::contains(conn, &f.path).unwrap_or(false))
                .filter(|f| {
                    let created = std::fs::metadata(&f.path)
                        .and_then(|m| m.created())
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as i64);
                    !history::already_organized(conn, &f.path, created, |p| p.exists())
                })
                .collect())
        })
        .unwrap_or_default();
    if files.is_empty() {
        return;
    }
    if state.is_paused() {
        let _ = state.with_db(|conn| {
            for f in &files {
                inbox::add(conn, &f.path, &f.source_dir, inbox::Reason::Collected)
                    .map_err(db_err)?;
            }
            Ok(())
        });
        notify_inbox(app);
        return;
    }
    let s = state.with_db(settings::load).unwrap_or_default();
    let classifier = if s.place_mode != PlaceMode::Suggest && files.iter().any(|f| !f.elsewhere) {
        state.classifier()
    } else {
        None
    };
    let (mut moved, mut collected) = (false, false);
    let mut ask = Vec::new();
    for file in files {
        match place_new_file(&state, &s, classifier.as_ref(), file) {
            Placed::Moved => moved = true,
            Placed::Collected => collected = true,
            Placed::Ask(item) => ask.push(item),
        }
    }
    let mut show = moved;
    if ask.len() > BULK_LIMIT {
        let _ = state.with_db(|conn| {
            for f in &ask {
                inbox::add(
                    conn,
                    Path::new(&f.path),
                    Path::new(&f.source_dir),
                    inbox::Reason::Collected,
                )
                .map_err(db_err)?;
            }
            Ok(())
        });
        state.with_bulk(|list| {
            for f in ask {
                if !list.iter().any(|x| x.path == f.path) {
                    list.push(f);
                }
            }
        });
        collected = true;
        show = true;
    } else if !ask.is_empty() {
        state.with_pending(|q| {
            for item in ask {
                if !q.iter().any(|p| p.path == item.path) {
                    q.push_back(item);
                }
            }
        });
        show = true;
    }
    if moved || collected {
        notify_inbox(app);
    }
    if show {
        let _ = app.emit(EVENT_POPUP_CHANGED, ());
        popup::show(app);
    }
}

/// 새 파일 하나: 옮기거나(바로 정리·자동 분류), 정리함에 모으거나, 물어볼 차례로.
/// 지켜볼 폴더 밖(넓은 감시)에서 찾은 파일은 사용자가 직접 저장한 문서일 수 있어 묻지 않고 옮기지 않는다.
fn place_new_file(
    state: &AppState,
    s: &Settings,
    classifier: Option<&Classifier>,
    file: NewFile,
) -> Placed {
    let found = match (file.elsewhere, s.place_mode, s.organize_mode) {
        (true, _, _) => None,
        (false, PlaceMode::Classify, _) => classifier.and_then(|c| c.classify(&file.path)),
        (false, PlaceMode::Auto, OrganizeMode::Ask) => classifier
            .and_then(|c| c.by_rule(&file.path))
            .map(|(rule, t)| (Reason::Rule { rule }, t)),
        _ => None,
    };
    // 옮기지 못하면(파일이 열려 있는 등) 아래 원래 방식대로 묻거나 모은다.
    if let Some((reason, target)) = found {
        if let Ok((move_id, moved)) = state.put_to(&file.path, &reason, &target) {
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
                    reason,
                })
            });
            return Placed::Moved;
        }
    }
    // 자동 분류 중에는 나누지 못한 파일을 묻지 않고 정리함에 모은다(지켜볼 폴더 밖 파일은 물어본다).
    let organize = if s.place_mode == PlaceMode::Classify && !file.elsewhere {
        OrganizeMode::Manual
    } else {
        s.organize_mode
    };
    match organize {
        OrganizeMode::Manual => {
            let _ = state.with_db(|conn| {
                inbox::add(conn, &file.path, &file.source_dir, inbox::Reason::Collected)
                    .map_err(db_err)
            });
            Placed::Collected
        }
        OrganizeMode::Ask => Placed::Ask(PendingFile::new(&file.path, &file.source_dir)),
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
        name: "AutoFolder",
        version: app.package_info().version.to_string(),
        os: std::env::consts::OS,
    }
}

/// 업데이트를 설치하기 직전에 부른다(relaunch.rs): [지금 업데이트]면 다시 켜질 때 메인 창을 보이고,
/// 자동 업데이트(`quiet`)면 트레이로만 다시 켜진다.
#[tauri::command]
pub fn prepare_update(app: AppHandle, version: String, quiet: bool) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    crate::relaunch::mark(&dir, &version, quiet).map_err(|e| e.to_string())
}

/// 자동 업데이트로 받아 둔 새 버전(없으면 None). 화면의 "받아 두었어요" 띠.
#[tauri::command]
pub fn ready_update(state: State<'_, AppState>) -> Option<String> {
    state.ready_update.version()
}

/// [지금 업데이트]: 받아 둔 새 버전을 바로 설치한다(다시 켜질 때 창을 보인다).
#[tauri::command]
pub fn install_ready_update(app: AppHandle) -> Result<(), String> {
    crate::autoupdate::install(&app, true)
}

/// 업데이트 뒤 다시 켜졌으면 그 버전을 한 번만 돌려준다("0.1.35로 업데이트했어요").
#[tauri::command]
pub fn take_update_notice(state: State<'_, AppState>) -> Option<String> {
    state
        .updated_to
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
}

/// 새 버전은 자동으로 설치(기본 켬)
#[tauri::command]
pub fn set_auto_update(state: State<'_, AppState>, enabled: bool) -> Result<Settings, String> {
    state.with_db(|conn| {
        let mut s = settings::load(conn)?;
        s.auto_update = enabled;
        settings::save(conn, &s)?;
        Ok(s)
    })
}

/// 처음 실행 화면에 필요한 정보. 기본 위치는 Windows에서 Known Folder 기준이라
/// OneDrive로 옮긴 바탕화면·문서도 실제 위치가 나온다.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupInfo {
    pub documents: Option<String>,
    pub desktop: Option<String>,
    pub downloads: Option<String>,
    /// 사진·동영상·음악(새 파일이 저장된 곳을 "사진 › Screenshots"처럼 보여 줄 때)
    pub pictures: Option<String>,
    pub videos: Option<String>,
    pub music: Option<String>,
    pub root_folder_name: &'static str,
    pub separator: String,
    pub teacher_example: Vec<ExampleFolder>,
    /// 집·개인 예시 폴더(파일 종류)
    pub general_example: Vec<ExampleFolder>,
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
        pictures: s(path.picture_dir()),
        videos: s(path.video_dir()),
        music: s(path.audio_dir()),
        root_folder_name: setup::ROOT_FOLDER_NAME,
        separator: std::path::MAIN_SEPARATOR.to_string(),
        teacher_example: setup::TEACHER_EXAMPLE
            .iter()
            .map(|(name, children)| ExampleFolder { name, children })
            .collect(),
        general_example: setup::GENERAL_EXAMPLE
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
    app: AppHandle,
    state: State<'_, AppState>,
    request: SetupRequest,
) -> Result<Settings, String> {
    // 처음 설정과 "처음 설정 다시 하기" 모두: 지금 설정 위에 고른 것만 바꾼다(규칙·단어·기록은 그대로)
    let new_settings = state.with_db(|conn| {
        let s = setup::run_over(&request, settings::load(conn)?)?;
        settings::save(conn, &s)?;
        Ok(s)
    })?;
    // 자동 실행은 기본 켬(SPEC 7장). 실패해도 설정은 끝낸다(설정 화면에서 다시 켤 수 있음).
    let _ = apply_autostart(&app, new_settings.autostart);
    // 바뀐 지켜볼 폴더·정리 폴더로 바로 감시·색인(다시 켜지 않아도)
    state.apply_watch_folders(&new_settings);
    crate::finder::refresh_index(&app, false);
    notify_inbox(&app);
    Ok(new_settings)
}

/// 쓰는 곳 바꾸기(학교/일반): 학기 시작 달을 그에 맞추고, 고친 적 없는 자동 분류 단어는 그쪽 기본 단어가 된다.
/// 이미 만든 폴더는 그대로 둔다.
#[tauri::command]
pub fn set_user_kind(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: settings::UserKind,
) -> Result<Settings, String> {
    let s = state.with_db(|conn| {
        let mut s = settings::load(conn)?;
        if s.user_kind != kind {
            s.user_kind = kind;
            let m = kind.term_months();
            s.first_term_month = m.first;
            s.second_term_month = m.second;
            settings::save(conn, &s)?;
        }
        Ok(s)
    })?;
    notify_inbox(&app);
    Ok(s)
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
    notify_inbox(&app);
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

/// 새 파일을 찾는 곳: 내 PC 어디든 / 지켜볼 폴더만
#[tauri::command]
pub fn set_new_file_scope(
    state: State<'_, AppState>,
    scope: NewFileScope,
) -> Result<Settings, String> {
    state
        .with_db(|conn| {
            let mut s = settings::load(conn)?;
            s.new_file_scope = scope;
            settings::save(conn, &s)?;
            Ok(s)
        })
        .inspect(|s| state.apply_watch_folders(s))
}

/// 묻지 않는 폴더(설정 화면에서 더하기·빼기)
#[tauri::command]
pub fn set_quiet_folders(
    state: State<'_, AppState>,
    folders: Vec<String>,
) -> Result<Settings, String> {
    state
        .with_db(|conn| {
            let mut s = settings::load(conn)?;
            s.quiet_folders = folders;
            settings::save(conn, &s)?;
            Ok(s)
        })
        .inspect(|s| state.apply_watch_folders(s))
}

/// 팝업 ⋯ "이 폴더는 묻지 않기": 이 폴더(하위 폴더까지)에 새 파일이 생겨도 묻지 않는다.
/// 이 폴더에서 와서 기다리던 파일도 더 묻지 않는다(파일은 그 자리에 그대로).
#[tauri::command]
pub fn quiet_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    dir: String,
) -> Result<Settings, String> {
    let d = PathBuf::from(&dir);
    let s = state.with_db(|conn| {
        let mut s = settings::load(conn)?;
        if !s
            .quiet_folders
            .iter()
            .any(|q| folders::same_dir(Path::new(q), &d))
        {
            s.quiet_folders.push(dir.clone());
            settings::save(conn, &s)?;
        }
        Ok(s)
    })?;
    state.apply_watch_folders(&s);
    state.with_pending(|q| {
        q.retain(|p| {
            let src = Path::new(&p.source_dir);
            !(folders::same_dir(src, &d) || folders::is_inside(&d, src))
        })
    });
    let _ = app.emit(EVENT_POPUP_CHANGED, ());
    let _ = app.emit(EVENT_SETTINGS_CHANGED, ());
    Ok(s)
}

/// 한꺼번에 생긴 파일 알림을 닫으며 할 일
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BulkAction {
    /// 정리함을 연다(파일은 정리함에)
    Open,
    /// 닫기만(파일은 정리함에)
    Keep,
    /// 정리하지 않기: 정리함에서 뺀다(파일은 그 자리에 그대로)
    Drop,
}

/// 한꺼번에 생긴 파일 알림 닫기
#[tauri::command]
pub fn close_bulk(
    app: AppHandle,
    state: State<'_, AppState>,
    action: BulkAction,
) -> Result<(), String> {
    let list = state.with_bulk(std::mem::take);
    match action {
        BulkAction::Open => tray::show_main(&app, Some("inbox")),
        BulkAction::Keep => {}
        BulkAction::Drop => {
            state.with_db(|conn| {
                for f in &list {
                    inbox::remove(conn, Path::new(&f.path)).map_err(db_err)?;
                }
                Ok(())
            })?;
        }
    }
    notify_inbox(&app);
    let _ = app.emit(EVENT_POPUP_CHANGED, ());
    Ok(())
}

#[tauri::command]
pub fn list_folders(path: String) -> Result<Vec<FolderEntry>, String> {
    folders::list_subfolders(Path::new(&path)).map_err(|e| format!("폴더를 읽지 못했어요: {e}"))
}

/// 폴더 안 파일 목록(정리함 오른쪽 폴더 칸). 정리 폴더와 그 안쪽만 읽는다.
#[tauri::command]
pub fn list_files(state: State<'_, AppState>, path: String) -> Result<Vec<FileEntry>, String> {
    let root = state
        .with_db(settings::load)?
        .root_folder
        .ok_or("정리 폴더가 아직 없어요.")?;
    if !folders::is_root_or_inside(Path::new(&root), Path::new(&path)) {
        return Err("정리 폴더 안만 볼 수 있어요.".into());
    }
    folders::list_files(Path::new(&path)).map_err(|e| format!("폴더를 읽지 못했어요: {e}"))
}

/// 최근 정리한 파일 한 줄
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentMove {
    pub id: i64,
    pub name: String,
    pub moved_path: String,
    pub dest_dir: String,
    pub original_dir: String,
    pub moved_at: i64,
}

/// 최근 정리한 파일(정리함의 '최근 정리' 탭): 지금 그 자리에 있는 것만, 새것부터 100개.
#[tauri::command]
pub fn recent_moves(state: State<'_, AppState>) -> Result<Vec<RecentMove>, String> {
    let records = state.with_db(|conn| history::recent_puts(conn, 100).map_err(db_err))?;
    let parent = |p: &str| {
        Path::new(p)
            .parent()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    Ok(records
        .into_iter()
        .map(|r| RecentMove {
            id: r.id,
            name: Path::new(&r.moved_path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            dest_dir: parent(&r.moved_path),
            original_dir: parent(&r.original_path),
            moved_path: r.moved_path,
            moved_at: r.moved_at,
        })
        .collect())
}

/// 정리 기록 화면(SPEC 3-2): 넣기·옮기기·이름 바꾸기·휴지통 기록, 새것부터.
#[tauri::command]
pub fn move_log(state: State<'_, AppState>) -> Result<Vec<crate::movelog::LogEntry>, String> {
    state.with_db(|conn| crate::movelog::entries(conn, crate::movelog::MAX_ENTRIES).map_err(db_err))
}

/// 정리 기록 [엑셀 파일로 저장]: 사용자가 저장 창에서 고른 .xlsx 파일에 서식 있는 엑셀로 쓴다.
#[tauri::command]
pub fn save_log_xlsx(path: String, rows: Vec<crate::movelog::XlsxRow>) -> Result<(), String> {
    crate::movelog::save_xlsx(Path::new(&path), &rows)
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
    notify_inbox(&app);
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
    notify_inbox(&app);
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
    notify_inbox(&app);
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
    notify_inbox(&app);
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
    notify_inbox(&app);
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
    /// 한꺼번에 생겨 정리함에 모아 둔 파일(한 번만 알린다)
    pub bulk: Option<BulkNotice>,
}

/// 한꺼번에 생긴 새 파일 알림
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkNotice {
    pub count: usize,
    /// 앞의 몇 개(이름·위치를 보여 준다)
    pub shown: Vec<PendingFile>,
}

#[tauri::command]
pub fn popup_state(state: State<'_, AppState>) -> PopupState {
    let (current, remaining) =
        state.with_pending(|q| (q.front().cloned(), q.len().saturating_sub(1)));
    let suggestion = current
        .as_ref()
        .and_then(|c| state.classifier()?.suggest(Path::new(&c.path)))
        .map(Suggestion::new);
    PopupState {
        current,
        remaining,
        suggestion,
        auto_moved: state.with_auto(|l| l.clone()),
        bulk: state.with_bulk(|l| {
            (!l.is_empty()).then(|| BulkNotice {
                count: l.len(),
                shown: l.iter().take(3).cloned().collect(),
            })
        }),
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
    /// 추천 위치에 넣었으면 그 까닭(규칙이면 규칙 번호도 기록)
    pub followed: Option<Reason>,
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
/// 추천 위치에 넣으면 그 폴더가 없을 때 만들고, 규칙이면 규칙 번호도 남긴다.
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

    let mut used: Option<Reason> = None;
    let mut change: Option<RuleChange> = None;
    if let Some((reason, target)) = state.classifier().and_then(|c| c.suggest(&src)) {
        if rules::same_path(&dest, &target.dir) {
            rules::ensure_target(&target, &state.root()?)?;
            used = Some(reason);
        } else if let Reason::Rule { rule } = reason {
            let ask = state
                .with_db(|conn| {
                    rules::note_choice(conn, rule.id, &dest, &target.dir).map_err(db_err)
                })
                .unwrap_or(false);
            if ask {
                change = Some(RuleChange {
                    rule,
                    old_dir: target.dir.to_string_lossy().into_owned(),
                    new_dir: dest_dir.clone(),
                });
            }
        }
    }
    let used_rule = match &used {
        Some(Reason::Rule { rule }) => Some(rule.id),
        _ => None,
    };

    let moved = mover::move_into(&src, &dest, new_name.as_deref())?;
    // '다른 위치…'로 바탕화면 등 지켜볼 폴더에 넣어도 새 파일로 다시 묻지 않게
    state.watcher.mark_known(moved.clone());
    let move_id = state.with_db(|conn| {
        inbox::remove(conn, &src).map_err(db_err)?;
        if let Some(r) = used_rule {
            let _ = rules::reset_misses(conn, r);
        }
        history::record_with_rule(conn, &src, &moved, used_rule).map_err(db_err)
    })?;
    state.with_pending(|q| q.retain(|p| p.path != path));
    notify_inbox(&app);

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
        followed: used,
        change,
        draft,
    })
}

/// 정리함 파일 여러 개를 분류기로 정한 폴더에 넣는다(`by_type`이면 파일 형식까지).
fn put_classified(state: &AppState, paths: Vec<String>, by_type: bool) -> Vec<PutOutcome> {
    let classifier = state.classifier();
    paths
        .into_iter()
        .map(|path| {
            let src = PathBuf::from(&path);
            let found = classifier.as_ref().and_then(|c| {
                if by_type {
                    c.classify(&src)
                } else {
                    c.suggest(&src)
                }
            });
            let result = match found {
                Some((reason, target)) => state.put_to(&src, &reason, &target).map(|(id, _)| id),
                None => Err("이름이나 형식으로 넣을 곳을 정하지 못했어요.".into()),
            };
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
        .collect()
}

/// 추천 위치에 넣는다(정리함 줄의 [넣기]). 파일마다 결과를 돌려준다.
#[tauri::command]
pub fn put_suggested(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Vec<PutOutcome> {
    let outcomes = put_classified(&state, paths, false);
    notify_inbox(&app);
    outcomes
}

/// 자동 분류 미리보기: 파일마다 넣을 곳(모르면 None → 정리함에 남음).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifyPlan {
    pub path: String,
    pub name: String,
    pub target: Option<Suggestion>,
}

#[tauri::command]
pub fn classify_preview(state: State<'_, AppState>, paths: Vec<String>) -> Vec<ClassifyPlan> {
    let classifier = state.classifier();
    paths
        .into_iter()
        .map(|path| {
            let p = PathBuf::from(&path);
            ClassifyPlan {
                name: p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                target: classifier
                    .as_ref()
                    .and_then(|c| c.classify(&p))
                    .map(Suggestion::new),
                path,
            }
        })
        .collect()
}

/// 기존 폴더 다시 정리하기 미리보기: 고른 폴더의 파일을 읽어 넣을 곳을 정한다.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderPreview {
    pub plans: Vec<ClassifyPlan>,
    /// 파일이 너무 많아 앞의 일부만 읽음
    pub truncated: bool,
}

/// - 정리 폴더 밖 폴더('다른 폴더 정리하기'): 자동 분류와 같게(이름 → 형식). 안쪽까지 읽어도 정리 폴더는 뺀다.
/// - 정리 폴더 안 폴더('이 폴더 다시 나누기'): 이름으로 정해지는 파일만 옮기고, 형식만으로는 옮기지 않는다
///   (개인 폴더의 PDF가 '형식별'로 빠져나가지 않게).
/// - 지금 있는 폴더가 곧 넣을 곳이면 그대로 둔다.
/// - 남은 파일은 이름에 같은 낱말이 여럿 있으면 새 폴더를 제안한다: 정리 폴더 안이면 그 폴더 안에,
///   밖이면 정리 폴더 바로 아래(집·개인은 파일 종류가 같으면 그 종류 폴더 안, 예: 문서 › 계약서).
#[tauri::command]
pub fn folder_classify_preview(
    state: State<'_, AppState>,
    dir: String,
    include_sub: bool,
) -> Result<FolderPreview, String> {
    let dir = PathBuf::from(&dir);
    resort::check_dir(&dir)?;
    let s = state.with_db(settings::load)?;
    let root = PathBuf::from(s.root_folder.as_deref().ok_or("정리 폴더가 아직 없어요.")?);
    let inside = folders::is_inside(&root, &dir);
    let skip = (!folders::is_root_or_inside(&root, &dir)).then_some(root.as_path());
    let (files, truncated) = resort::list(&dir, include_sub, skip, resort::MAX_FILES);
    let classifier = state.classifier();
    let mut plans: Vec<ClassifyPlan> = files
        .iter()
        .map(|p| {
            let stays = |t: &Target| {
                p.parent()
                    .is_some_and(|parent| folders::same_dir(parent, &t.dir))
            };
            let found = classifier
                .as_ref()
                .and_then(|c| c.classify(p))
                .filter(|(reason, _)| !(inside && matches!(reason, Reason::Type { .. })))
                .filter(|(_, t)| !stays(t));
            // 파일 이름으로 모르면(형식으로만 정해졌으면) 원래 있던 폴더 이름으로: 그 파일의 폴더, 고른 폴더 순
            let found = match found {
                Some((r, t)) if !matches!(r, Reason::Type { .. }) => Some((r, t)),
                other => {
                    let names = [p.parent(), Some(dir.as_path())]
                        .into_iter()
                        .flatten()
                        .filter_map(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()));
                    names
                        .filter_map(|n| classifier.as_ref()?.by_source_folder(&n))
                        .find(|(_, t)| !stays(t))
                        .or(other)
                }
            };
            ClassifyPlan {
                path: p.to_string_lossy().into_owned(),
                name: p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                target: found.map(Suggestion::new),
            }
        })
        .collect();

    // 새 폴더 제안: 넣을 곳이 없거나 형식으로만 정해진 파일
    let open: Vec<usize> = (0..plans.len())
        .filter(|&i| {
            plans[i]
                .target
                .as_ref()
                .is_none_or(|t| matches!(t.reason, Reason::Type { .. }))
        })
        .collect();
    let names: Vec<String> = open.iter().map(|&i| plans[i].name.clone()).collect();
    let own_name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    for (word, members) in resort::suggest(&names, &[own_name]) {
        let kinds: HashSet<Option<&str>> = members
            .iter()
            .map(|&m| classify::kind_folder(&naming::extension(&names[m])))
            .collect();
        let base = if inside {
            dir.clone()
        } else {
            match (
                s.user_kind,
                kinds.into_iter().collect::<Vec<_>>().as_slice(),
            ) {
                (UserKind::General, [Some(k)]) => root.join(k),
                _ => root.clone(),
            }
        };
        let target = classify::target_for(base.join(&word));
        let count = members.len();
        for m in members {
            let i = open[m];
            let parent_is_target = Path::new(&plans[i].path)
                .parent()
                .is_some_and(|p| folders::same_dir(p, &target.dir));
            if !parent_is_target {
                plans[i].target = Some(Suggestion::new((
                    Reason::Suggest {
                        word: word.clone(),
                        count,
                    },
                    target.clone(),
                )));
            }
        }
    }
    Ok(FolderPreview { plans, truncated })
}

/// 자동 분류 실행(정리함 [자동 분류] → 미리보기 → 확인). 나누지 못한 파일은 정리함에 남는다.
#[tauri::command]
pub fn classify_files(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Vec<PutOutcome> {
    let outcomes = put_classified(&state, paths, true);
    notify_inbox(&app);
    outcomes
}

/// 자동 분류 단어(정리 규칙 화면)
#[tauri::command]
pub fn get_auto_words(state: State<'_, AppState>) -> Result<Vec<WordGroup>, String> {
    state.with_db(|conn| Ok(classify::load_words(conn)))
}

/// 자동 분류 단어 저장. `groups`가 없으면 기본 단어로 되돌린다.
#[tauri::command]
pub fn set_auto_words(
    app: AppHandle,
    state: State<'_, AppState>,
    groups: Option<Vec<WordGroup>>,
) -> Result<Vec<WordGroup>, String> {
    let groups = match groups {
        Some(g) => classify::normalize_words(g)?,
        None => classify::default_words(state.with_db(settings::load)?.user_kind),
    };
    state.with_db(|conn| classify::save_words(conn, &groups))?;
    notify_inbox(&app);
    Ok(groups)
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
    notify_inbox(&app);
    Ok(item)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoManyResult {
    pub restored: usize,
    pub errors: Vec<String>,
}

/// 되돌리기(정리함): 여러 이동을 되돌리고, 돌아온 파일은 다시 정리함에 넣는다.
/// 정리 폴더 안에서 폴더끼리 옮긴 것을 되돌리면 원래 폴더로만 돌아간다(정리함에 넣지 않음).
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
    // 지켜볼 폴더(다운로드·바탕화면 등)에서 넣은 파일만 정리함으로 돌린다. 정리 폴더 안에서 옮긴 것,
    // '다른 폴더 정리하기'로 옮긴 것은 원래 폴더로만(정리 폴더가 지켜볼 폴더 자체여도 같은 기준).
    let watch: Vec<PathBuf> = state
        .with_db(settings::load)
        .map(|s| s.watch_folders.iter().map(PathBuf::from).collect())
        .unwrap_or_default();
    for id in move_ids {
        match undo_one(&state, id) {
            Ok((restored, source_dir)) => {
                if watch.iter().any(|w| folders::same_dir(w, &source_dir)) {
                    let _ = state.with_db(|conn| {
                        inbox::add(conn, &restored, &source_dir, inbox::Reason::Later)
                            .map_err(db_err)
                    });
                }
                result.restored += 1;
            }
            Err(e) => result.errors.push(e),
        }
    }
    notify_inbox(&app);
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
/// `create`면 없는 폴더를 만든다(정리 폴더 안만 — 다시 나누기의 새 폴더 제안).
#[tauri::command]
pub fn put_files(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
    dest_dir: String,
    create: Option<bool>,
) -> Vec<PutOutcome> {
    let dest = PathBuf::from(&dest_dir);
    if create.unwrap_or(false) && !dest.is_dir() {
        let made = state
            .root()
            .and_then(|root| rules::ensure_target(&classify::target_for(dest.clone()), &root));
        if let Err(e) = made {
            return paths
                .into_iter()
                .map(|path| PutOutcome {
                    path,
                    move_id: None,
                    error: Some(e.clone()),
                })
                .collect();
        }
    }
    let outcomes = paths
        .into_iter()
        .map(|path| {
            let src = PathBuf::from(&path);
            // 이미 그 폴더에 있는 파일을 같은 폴더에 넣으면 '이름 (2)'가 되지 않게 그대로 둔다
            let same_dir = src
                .parent()
                .zip(dest.canonicalize().ok())
                .is_some_and(|(p, d)| p.canonicalize().is_ok_and(|p| p == d));
            let result = if same_dir {
                Err("이미 이 폴더에 있어요.".to_string())
            } else {
                mover::move_into(&src, &dest, None)
            }
            .and_then(|moved| {
                state.watcher.mark_known(moved.clone());
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
    notify_inbox(&app);
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
                || watcher::is_decoy(&path)
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
    notify_inbox(&app);
    Ok(added)
}

/// 자주 쓰는 폴더(숫자 키 1~6): 직접 고른 칸은 그 폴더, 나머지는 자주 넣은 폴더부터,
/// 모자라면 정리 폴더 바로 아래 폴더로 채운다.
#[tauri::command]
pub fn favorite_folders(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let s = state.with_db(settings::load)?;
    let mut auto: Vec<String> =
        state.with_db(|conn| history::frequent_destinations(conn, 20).map_err(db_err))?;
    if let Some(root) = &s.root_folder {
        if let Ok(top) = folders::list_subfolders(Path::new(root)) {
            auto.extend(top.into_iter().map(|f| f.path));
        }
    }
    Ok(settings::merge_favorites(&s.pinned_favorites, auto, |p| {
        Path::new(p).is_dir()
    }))
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
    notify_inbox(&app);
    Ok(())
}

/// 팝업에서 이름 바꾸기: 넣기 전에 바로 그 자리(다운로드 등)에서 이름을 바꾼다(사용자 보고: 넣지 않고 이름만
/// 바꾸면 실제 파일은 그대로였다). 같은 이름이 있으면 번호를 붙이고, 기록(이름 바꾸기)에 남아 되돌릴 수 있다.
#[tauri::command]
pub fn rename_pending(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    new_name: String,
) -> Result<PendingFile, String> {
    let src = PathBuf::from(&path);
    let dir = src
        .parent()
        .ok_or("파일이 있는 폴더를 알 수 없어요.")?
        .to_path_buf();
    let renamed = mover::move_into(&src, &dir, Some(&new_name))?;
    // 바뀐 이름을 감시가 새 파일로 다시 잡지 않게
    state.watcher.mark_known(renamed.clone());
    state.with_db(|conn| {
        let batch = history::next_batch(conn).map_err(db_err)?;
        history::record_in_batch(conn, &src, &renamed, batch).map_err(db_err)?;
        history::rewrite_file(conn, &src, &renamed, batch).map_err(db_err)?;
        inbox::rename_path(conn, &src, &renamed).map_err(db_err)
    })?;
    let source_dir = state
        .with_pending(|q| {
            q.iter()
                .find(|p| p.path == path)
                .map(|p| p.source_dir.clone())
        })
        .map(PathBuf::from)
        .unwrap_or(dir);
    let item = PendingFile::new(&renamed, &source_dir);
    state.with_pending(|q| {
        if let Some(p) = q.iter_mut().find(|p| p.path == path) {
            *p = item.clone();
        }
    });
    notify_inbox(&app);
    Ok(item)
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
    // 예전 버전이 정리함에 넣어 둔 백신 미끼 파일은 뺀다(파일은 그 자리에 그대로)
    let items = state.with_db(|conn| {
        let mut items = inbox::list(conn).map_err(db_err)?;
        for item in items
            .iter()
            .filter(|i| watcher::is_decoy(Path::new(&i.path)))
        {
            inbox::remove(conn, Path::new(&item.path)).map_err(db_err)?;
        }
        items.retain(|i| !watcher::is_decoy(Path::new(&i.path)));
        Ok(items)
    })?;
    let classifier = state.classifier();
    Ok(items
        .into_iter()
        .map(|item| InboxEntry {
            suggestion: classifier
                .as_ref()
                .and_then(|c| c.suggest(Path::new(&item.path)))
                .map(Suggestion::new),
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
    notify_inbox(&app);
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
    notify_inbox(&app);
    Ok(rule)
}

/// 규칙 지우기(되돌리기: restore_rule)
#[tauri::command]
pub fn delete_rule(app: AppHandle, state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state.with_db(|conn| rules::delete(conn, id).map_err(db_err))?;
    notify_inbox(&app);
    Ok(())
}

#[tauri::command]
pub fn restore_rule(app: AppHandle, state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state.with_db(|conn| rules::restore(conn, id).map_err(db_err))?;
    notify_inbox(&app);
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
    notify_inbox(&app);
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
    notify_inbox(&app);
    Ok(())
}

// ---------- 트레이·자동 실행·설정(6단계) ----------

/// 컴퓨터를 켤 때 자동 실행 켜고 끄기. 개발용(디버그) 실행 파일은 등록하지 않는다.
pub fn apply_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Ok(());
    }
    use tauri_plugin_autostart::ManagerExt;
    let launcher = app.autolaunch();
    let now = launcher.is_enabled().unwrap_or(false);
    let result = match (enabled, now) {
        (true, false) => launcher.enable(),
        (false, true) => launcher.disable(),
        _ => Ok(()),
    };
    result.map_err(|e| format!("자동 실행을 바꾸지 못했어요: {e}"))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub paused: bool,
}

#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> Status {
    Status {
        paused: state.is_paused(),
    }
}

/// 일시 정지 / 다시 시작
#[tauri::command]
pub fn set_paused(app: AppHandle, state: State<'_, AppState>, paused: bool) -> Status {
    state.set_paused(paused);
    tray::status_changed(&app);
    Status { paused }
}

fn update_settings(
    state: &AppState,
    change: impl FnOnce(&mut Settings) -> Result<(), String>,
) -> Result<Settings, String> {
    state.with_db(|conn| {
        let mut s = settings::load(conn)?;
        change(&mut s)?;
        settings::save(conn, &s)?;
        Ok(s)
    })
}

#[tauri::command]
pub fn set_autostart(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<Settings, String> {
    apply_autostart(&app, enabled)?;
    update_settings(&state, |s| {
        s.autostart = enabled;
        Ok(())
    })
}

/// 추천만일 때 응답이 없으면 n초 뒤 추천 위치에 넣기(0 = 끔)
#[tauri::command]
pub fn set_wait_seconds(state: State<'_, AppState>, seconds: u32) -> Result<Settings, String> {
    if seconds > settings::MAX_WAIT_SECONDS {
        return Err(format!(
            "{}초까지만 고를 수 있어요.",
            settings::MAX_WAIT_SECONDS
        ));
    }
    update_settings(&state, |s| {
        s.wait_seconds = seconds;
        Ok(())
    })
}

/// 자주 쓰는 폴더 직접 고르기(칸마다 폴더 또는 null = 자동)
#[tauri::command]
pub fn set_pinned_favorites(
    app: AppHandle,
    state: State<'_, AppState>,
    pins: Vec<Option<String>>,
) -> Result<Settings, String> {
    if pins.len() > settings::FAVORITE_SLOTS {
        return Err("자주 쓰는 폴더는 6개까지예요.".into());
    }
    let root = state.root()?;
    for p in pins.iter().flatten() {
        let path = Path::new(p);
        let at_root = path.canonicalize().ok() == root.canonicalize().ok();
        if !(path.is_dir() && (at_root || folders::is_inside(&root, path))) {
            return Err("정리 폴더 안의 폴더만 고를 수 있어요.".into());
        }
    }
    let s = update_settings(&state, |s| {
        s.pinned_favorites = pins;
        Ok(())
    })?;
    notify_inbox(&app);
    Ok(s)
}

/// 정리 폴더를 다른 폴더로 바꾼다. 원래 폴더와 그 안의 파일은 그대로 둔다.
#[tauri::command]
pub fn set_root_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<Settings, String> {
    let path = path.trim().to_string();
    if path.is_empty() || !Path::new(&path).is_dir() {
        return Err("고른 폴더를 찾을 수 없어요.".into());
    }
    let s = update_settings(&state, |s| {
        setup::validate_watch_folders(&s.watch_folders, &path)?;
        s.root_folder = Some(path);
        // 직접 고른 자주 쓰는 폴더는 예전 정리 폴더 기준이라 비운다
        s.pinned_favorites.clear();
        Ok(())
    })?;
    notify_inbox(&app);
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_scope_follows_new_file_scope_and_quiet_folders() {
        let p = PathBuf::from;
        let wide = [p("C:\\Users\\t\\Documents"), p("D:\\")];
        let mut s = Settings {
            setup_done: true,
            root_folder: Some("C:\\Users\\t\\Documents\\정리 폴더".into()),
            watch_folders: vec![
                "C:\\Users\\t\\Downloads".into(),
                "C:\\Users\\t\\Desktop".into(),
            ],
            quiet_folders: vec!["c:\\users\\t\\desktop".into()],
            ..Settings::default()
        };
        // 예전 설정도 기본은 내 PC 어디든
        let scope = watch_scope(&s, &wide);
        assert_eq!(scope.wide, wide);
        // 묻지 않는 폴더는 지켜볼 폴더에서도 빠지고, 정리 폴더와 함께 넓은 감시에서 뺀다
        assert_eq!(scope.folders, [p("C:\\Users\\t\\Downloads")]);
        assert_eq!(
            scope.skip,
            [
                p("c:\\users\\t\\desktop"),
                p("C:\\Users\\t\\Documents\\정리 폴더")
            ]
        );
        s.new_file_scope = NewFileScope::Watched;
        assert!(watch_scope(&s, &wide).wide.is_empty());
        s.setup_done = false;
        assert_eq!(watch_scope(&s, &wide), watcher::Scope::default());
    }
}
