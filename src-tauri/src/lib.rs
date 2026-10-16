//! 파일 정리 도우미 — Rust 쪽 진입점.
//!
//! 화면(React)은 파일을 직접 만지지 않고 여기 등록한 명령을 `invoke`로 부른다(SPEC 8장).
//! 기능 모듈은 SPEC 9장 개발 단계에 맞춰 하나씩 추가한다.

mod classify;
mod commands;
mod db;
mod everything;
mod finder;
mod folders;
mod history;
mod inbox;
mod mover;
mod naming;
mod popup;
mod recycle;
mod rules;
mod search;
mod settings;
mod setup;
mod tray;
mod watcher;

use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use tauri::Manager;

/// 컴퓨터를 켤 때 자동 실행되면 붙는 인자(이때는 창 없이 트레이로만 시작)
const AUTOSTART_ARG: &str = "--autostart";

pub fn run() {
    tauri::Builder::default()
        // 한 개만 실행: 이미 켜져 있으면 새로 켜지 않고 켜진 프로그램의 창을 앞으로(가장 먼저 등록해야 한다)
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app, None);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![AUTOSTART_ARG]),
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        finder::show(app);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
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
                auto_moved: Mutex::new(Vec::new()),
                paused: AtomicBool::new(false),
                index: Mutex::new(search::Index::default()),
                indexing: AtomicBool::new(false),
                live: finder::Live::default(),
                shortcut_error: Mutex::new(None),
            };
            state.apply_watch_folders(&current);
            app.manage(state);

            // 창은 저장소(state)를 준비한 뒤에 만든다. 창이 먼저 뜨면 화면이 설정을 묻는 순간
            // "state not managed" 오류가 날 수 있다(tauri.conf.json의 창은 모두 create: false).
            for config in &app.config().app.windows {
                tauri::WebviewWindowBuilder::from_config(app.handle(), config)?.build()?;
            }
            tray::create(app.handle())?;

            // 파일 검색: 단축키 등록(실패하면 설정 화면에 알림), 자체 색인 미리 읽기(뒤에서)
            if let Err(e) = finder::apply_shortcut(app.handle(), &current.search_shortcut) {
                if let Ok(mut err) = app.state::<commands::AppState>().shortcut_error.lock() {
                    *err = Some(e);
                }
            }
            finder::start_live(app.handle());
            if current.setup_done {
                finder::refresh_index(app.handle(), true);
            }

            // 메인 창은 숨긴 채 만들어 두고, 컴퓨터를 켤 때 자동 실행된 경우(설정을 마쳤으면)에는
            // 트레이로만 시작한다(SPEC 7장).
            let autostarted = std::env::args().any(|a| a == AUTOSTART_ARG);
            if !(autostarted && current.setup_done) {
                tray::show_main(app.handle(), None);
            }
            // 자동 실행 설정을 실제 등록과 맞춘다(예전 버전에서 업데이트한 경우 기본 켬).
            if current.setup_done {
                let _ = commands::apply_autostart(app.handle(), current.autostart);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // 메인 창 X = 끄지 않고 숨기기(트레이에서 감시 계속). 끄기는 트레이 메뉴의 "끄기".
            if let ("main", tauri::WindowEvent::CloseRequested { api, .. }) =
                (window.label(), event)
            {
                api.prevent_close();
                let _ = window.hide();
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
            commands::set_place_mode,
            commands::set_term_months,
            commands::dismiss_auto,
            commands::put_suggested,
            commands::classify_preview,
            commands::classify_files,
            commands::get_auto_words,
            commands::set_auto_words,
            commands::get_status,
            commands::set_paused,
            commands::set_autostart,
            commands::set_wait_seconds,
            commands::set_pinned_favorites,
            commands::set_root_folder,
            commands::list_rules,
            commands::describe_dest,
            commands::create_rule,
            commands::update_rule,
            commands::delete_rule,
            commands::restore_rule,
            commands::move_rule,
            commands::answer_rule_change,
            finder::search_files,
            finder::prepare_search,
            finder::set_search_whole_pc,
            finder::open_path,
            finder::reveal_path,
            finder::search_info,
            finder::set_search_shortcut,
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
