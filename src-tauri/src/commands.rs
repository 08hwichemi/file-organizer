//! 화면에서 `invoke`로 부르는 명령 모음.

use crate::folders::{self, FolderEntry};
use crate::settings::{self, OrganizeMode, Settings};
use crate::setup::{self, SetupRequest};
use rusqlite::Connection;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{Manager, State};

pub struct AppState {
    pub db: Mutex<Connection>,
}

impl AppState {
    fn with_db<T>(&self, f: impl FnOnce(&Connection) -> Result<T, String>) -> Result<T, String> {
        let conn = self
            .db
            .lock()
            .map_err(|_| "저장소를 열 수 없어요".to_string())?;
        f(&conn)
    }
}

/// 프로그램 이름·버전·운영체제.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub os: &'static str,
}

pub fn current_app_info() -> AppInfo {
    AppInfo {
        name: "파일 정리 도우미",
        version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
    }
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    current_app_info()
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
    state.with_db(|conn| {
        let mut s = settings::load(conn)?;
        setup::validate_watch_folders(&folders, s.root_folder.as_deref().unwrap_or_default())?;
        s.watch_folders = folders;
        settings::save(conn, &s)?;
        Ok(s)
    })
}

#[tauri::command]
pub fn list_folders(path: String) -> Result<Vec<FolderEntry>, String> {
    folders::list_subfolders(Path::new(&path)).map_err(|e| format!("폴더를 읽지 못했어요: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_uses_package_version() {
        let info = current_app_info();
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.name, "파일 정리 도우미");
        assert!(!info.os.is_empty());
    }
}
