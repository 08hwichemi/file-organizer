//! 파일 정리 도우미 — Rust 쪽 진입점.
//!
//! 화면(React)은 파일을 직접 만지지 않고 여기 등록한 명령을 `invoke`로 부른다(SPEC 8장).
//! 기능 모듈은 SPEC 9장 개발 단계에 맞춰 하나씩 추가한다.

mod commands;
mod db;
mod folders;
mod settings;
mod setup;

use std::sync::Mutex;
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Windows: %APPDATA%\com.hwichemi.file-organizer\data.db
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let conn = db::open(&data_dir.join("data.db"))?;
            app.manage(commands::AppState {
                db: Mutex::new(conn),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::setup_info,
            commands::get_settings,
            commands::complete_setup,
            commands::set_organize_mode,
            commands::set_watch_folders,
            commands::list_folders,
        ])
        .run(tauri::generate_context!())
        .expect("파일 정리 도우미를 시작하지 못했습니다");
}
