//! 파일 정리 도우미 — Rust 쪽 진입점.
//!
//! 화면(React)은 파일을 직접 만지지 않고 여기 등록한 명령을 `invoke`로 부른다(SPEC 8장).
//! 기능 모듈은 SPEC 9장 개발 단계에 맞춰 하나씩 추가한다.

mod commands;
mod db;
mod folders;
mod history;
mod inbox;
mod mover;
mod popup;
mod recycle;
mod settings;
mod setup;
mod watcher;

use std::collections::VecDeque;
use std::sync::Mutex;
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // Windows: %APPDATA%\com.hwichemi.file-organizer\data.db
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let conn = db::open(&data_dir.join("data.db"))?;
            let current = settings::load(&conn)?;

            let handle = app.handle().clone();
            let watcher = watcher::WatchService::start(move |file| {
                commands::handle_new_file(&handle, file);
            });
            let state = commands::AppState {
                db: Mutex::new(conn),
                watcher,
                pending: Mutex::new(VecDeque::new()),
            };
            state.apply_watch_folders(&current);
            app.manage(state);

            // 창은 저장소(state)를 준비한 뒤에 만든다. 창이 먼저 뜨면 화면이 설정을 묻는 순간
            // "state not managed" 오류가 날 수 있다(tauri.conf.json의 창은 모두 create: false).
            for config in &app.config().app.windows {
                tauri::WebviewWindowBuilder::from_config(app.handle(), config)?.build()?;
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // 트레이(6단계) 전까지는 메인 창을 닫으면 프로그램을 끝낸다.
            // (숨어 있는 팝업 창 때문에 프로그램이 보이지 않게 남아 있지 않도록)
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { .. } = event {
                    window.app_handle().exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::setup_info,
            commands::get_settings,
            commands::complete_setup,
            commands::set_organize_mode,
            commands::set_watch_folders,
            commands::list_folders,
            commands::create_folder,
            commands::search_folders,
            commands::recent_destinations,
            commands::popup_state,
            commands::put_file,
            commands::undo_move,
            commands::undo_moves,
            commands::put_files,
            commands::import_existing,
            commands::favorite_folders,
            commands::rename_folder,
            commands::move_folder,
            commands::trash_folder,
            commands::trash_files,
            commands::restore_deleted,
            commands::defer_file,
            commands::hide_popup,
            commands::list_inbox,
        ])
        .run(tauri::generate_context!())
        .expect("파일 정리 도우미를 시작하지 못했습니다");
}

#[cfg(test)]
mod tests {
    /// 창을 Tauri가 먼저 만들면 저장소 준비 전에 화면이 명령을 불러 오류가 난다(0.1.4에서 실제로 발생).
    #[test]
    fn windows_are_created_after_state_is_managed() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let windows = conf["app"]["windows"].as_array().unwrap();
        assert!(!windows.is_empty());
        for w in windows {
            assert_eq!(
                w["create"], false,
                "창 {}은 create: false여야 해요",
                w["label"]
            );
        }
    }
}
