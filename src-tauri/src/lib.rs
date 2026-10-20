//! AutoFolder — Rust 쪽 진입점.
//!
//! 화면(React)은 파일을 직접 만지지 않고 여기 등록한 명령을 `invoke`로 부른다(SPEC 8장).
//! 기능 모듈은 SPEC 9장 개발 단계에 맞춰 하나씩 추가한다.

mod autoupdate;
mod classify;
mod commands;
mod db;
mod everything;
mod finder;
mod folders;
mod history;
mod inbox;
mod movelog;
mod mover;
mod naming;
mod popup;
mod recycle;
mod relaunch;
mod renamer;
mod resort;
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

/// 메인 창 첫 크기(논리 픽셀, 사용자 요청: 켤 때 더 크게): 화면 작업 영역의 85%, 1600×1000을 넘지 않고
/// 1100×720(tauri.conf.json의 기본)보다 작지 않게. 화면이 그보다 작으면 화면 안에 들어가게.
fn main_window_size(work_w: f64, work_h: f64) -> (f64, f64) {
    let fit = |avail: f64, min: f64, max: f64| (avail * 0.85).min(max).max(min).min(avail);
    (fit(work_w, 1100.0, 1600.0), fit(work_h, 720.0, 1000.0))
}

/// 메인 창을 지금 화면에 맞는 크기로 키우고 가운데에 둔다(크기를 못 읽으면 기본 크기 그대로).
fn fit_main_window(window: &tauri::WebviewWindow) {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(m) = monitor else { return };
    let scale = m.scale_factor();
    let area = m.work_area().size;
    let (w, h) = main_window_size(area.width as f64 / scale, area.height as f64 / scale);
    let _ = window.set_size(tauri::LogicalSize::new(w, h));
    let _ = window.center();
}

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
            let mut current = settings::load(&conn)?;
            // 화면 캡처·녹화가 저장되는 곳은 처음부터 묻지 않는다(한 번만, 설정에서 [다시 묻기]로 뺄 수 있음)
            let p = app.path();
            let default_quiet: Vec<std::path::PathBuf> = [
                p.picture_dir().map(|d| d.join("Screenshots")),
                p.video_dir().map(|d| d.join("Captures")),
            ]
            .into_iter()
            .filter_map(Result::ok)
            .collect();
            if settings::add_default_quiet(&mut current, &default_quiet) {
                settings::save(&conn, &current)?;
            }

            // 업데이트 뒤 다시 켜졌는지(표시는 읽으면서 지운다)
            let relaunched = relaunch::take(&data_dir, &app.package_info().version.to_string());

            let handle = app.handle().clone();
            let watcher = watcher::WatchService::start(move |files| {
                commands::handle_new_files(&handle, files);
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
                wide_roots: wide_roots(app.handle()),
                bulk: Mutex::new(Vec::new()),
                // 다음에 창을 열 때 "새 버전으로 업데이트했어요"를 한 번 알린다
                ready_update: autoupdate::Ready::default(),
                updated_to: Mutex::new(relaunched.map(|_| app.package_info().version.to_string())),
            };
            state.apply_watch_folders(&current);
            app.manage(state);

            // 창은 저장소(state)를 준비한 뒤에 만든다. 창이 먼저 뜨면 화면이 설정을 묻는 순간
            // "state not managed" 오류가 날 수 있다(tauri.conf.json의 창은 모두 create: false).
            for config in &app.config().app.windows {
                tauri::WebviewWindowBuilder::from_config(app.handle(), config)?.build()?;
            }
            if let Some(main) = app.get_webview_window("main") {
                fit_main_window(&main);
            }
            tray::create(app.handle())?;

            // 파일 검색: 단축키 등록(실패하면 설정 화면에 알림), 자체 색인 미리 읽기(뒤에서)
            if let Err(e) = finder::apply_shortcut(app.handle(), &current.search_shortcut) {
                if let Ok(mut err) = app.state::<commands::AppState>().shortcut_error.lock() {
                    *err = Some(e);
                }
            }
            finder::start_live(app.handle());
            // 자동 업데이트: 켤 때 바로 확인, 그 뒤 4시간마다(autoupdate.rs)
            autoupdate::start(app.handle().clone());
            if current.setup_done {
                finder::refresh_index(app.handle(), true);
            }

            // 메인 창은 숨긴 채 만들어 두고, 컴퓨터를 켤 때 자동 실행된 경우(설정을 마쳤으면)에는
            // 트레이로만 시작한다(SPEC 7장).
            // [지금 업데이트]로 다시 켜진 경우에는 자동 실행 인자가 붙어 있어도 창을 보인다(relaunch.rs).
            // 자동 업데이트(창을 쓰지 않을 때 뒤에서 설치)면 트레이로만 조용히 다시 켜진다.
            let autostarted = std::env::args().any(|a| a == AUTOSTART_ARG);
            let after_update = relaunched == Some(relaunch::Relaunched::Shown);
            let quiet_update = relaunched == Some(relaunch::Relaunched::Quiet);
            if after_update || !((autostarted && current.setup_done) || quiet_update) {
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
            commands::prepare_update,
            commands::ready_update,
            commands::install_ready_update,
            commands::take_update_notice,
            commands::set_auto_update,
            commands::setup_info,
            commands::get_settings,
            commands::complete_setup,
            commands::set_organize_mode,
            commands::set_user_kind,
            commands::set_watch_folders,
            commands::set_new_file_scope,
            commands::set_quiet_folders,
            commands::quiet_folder,
            commands::close_bulk,
            commands::list_folders,
            commands::list_files,
            commands::recent_moves,
            commands::move_log,
            commands::save_log_xlsx,
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
            commands::folder_classify_preview,
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
            renamer::rename_list,
            renamer::rename_preview,
            renamer::rename_apply,
            renamer::rename_undo,
        ])
        .run(tauri::generate_context!())
        .expect("AutoFolder를 시작하지 못했습니다");
}

/// 내 PC 어디든(SPEC 5장): 사용자 폴더(바탕화면·문서·다운로드·사진·동영상·음악, OneDrive로 옮긴 것 포함)와
/// Windows가 깔린 드라이브가 아닌 고정 디스크(D: 등)는 하위 폴더까지 본다.
fn wide_roots(app: &tauri::AppHandle) -> Vec<std::path::PathBuf> {
    let p = app.path();
    let known: Vec<std::path::PathBuf> = [
        p.desktop_dir(),
        p.document_dir(),
        p.download_dir(),
        p.picture_dir(),
        p.video_dir(),
        p.audio_dir(),
    ]
    .into_iter()
    .filter_map(Result::ok)
    .filter(|d| d.is_dir())
    .collect();
    watcher::wide_roots(&known, &other_drives())
}

/// Windows가 깔리지 않은 고정 디스크(C 드라이브 전체는 프로그램 폴더가 많아 사용자 폴더만 본다)
fn other_drives() -> Vec<std::path::PathBuf> {
    #[cfg(windows)]
    {
        let system = std::env::var("SystemDrive")
            .unwrap_or_else(|_| "C:".into())
            .to_ascii_uppercase();
        search::pc_roots()
            .into_iter()
            .filter(|d| {
                !d.to_string_lossy()
                    .to_ascii_uppercase()
                    .starts_with(&system)
            })
            .collect()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn main_window_fits_the_screen() {
        // 1920×1080(작업 표시줄 빼고 1040) 100%: 85%인데 가로는 1600까지
        assert_eq!(super::main_window_size(1920.0, 1040.0), (1600.0, 884.0));
        // 같은 화면 125%(학교 PC에 흔함): 1536×832 → 1305.6×720(세로는 최소 720)
        let (w, h) = super::main_window_size(1536.0, 832.0);
        assert!((w - 1305.6).abs() < 0.01 && h == 720.0);
        // 작은 화면: 화면 안에 들어가게
        assert_eq!(super::main_window_size(1024.0, 700.0), (1024.0, 700.0));
        // 아주 큰 화면: 1600×1000까지만
        assert_eq!(super::main_window_size(3840.0, 2100.0), (1600.0, 1000.0));
    }

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
