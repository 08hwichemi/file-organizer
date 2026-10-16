//! 트레이 아이콘(SPEC 3-3): 열기 · 정리함(개수) · 일시 정지/다시 시작 · 끄기.
//! 아이콘을 누르면 메인 창을 연다. 창 X는 끄지 않고 숨긴다(lib.rs) — 감시는 트레이에서 계속된다.
//! 검색(Alt+Space)은 7단계에서 메뉴에 더한다.

use crate::commands::{AppState, EVENT_STATUS_CHANGED};
use crate::inbox;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

const ID: &str = "main";
/// 화면에 "이 화면으로 가라"고 알리는 이벤트(값: "inbox" 등)
pub const EVENT_NAVIGATE: &str = "navigate";

/// 글자를 바꿔야 하는 메뉴 항목
pub struct TrayItems {
    inbox: MenuItem<Wry>,
    pause: MenuItem<Wry>,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "열기", true, None::<&str>)?;
    let inbox = MenuItem::with_id(app, "inbox", "정리함", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "일시 정지", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "끄기", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &inbox, &pause, &sep, &quit])?;

    let mut builder = TrayIconBuilder::with_id(ID)
        .tooltip("파일 정리 도우미")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app, None),
            "inbox" => show_main(app, Some("inbox")),
            "pause" => {
                let state = app.state::<AppState>();
                state.set_paused(!state.is_paused());
                status_changed(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle(), None);
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    app.manage(TrayItems { inbox, pause });
    refresh(app);
    Ok(())
}

/// 메뉴 글자를 지금 상태로: "정리함 (3)", "일시 정지"/"다시 시작". 아이콘에 마우스를 올리면 나오는 글자에도 일시 정지를 표시.
pub fn refresh(app: &AppHandle) {
    let (Some(items), Some(state)) = (app.try_state::<TrayItems>(), app.try_state::<AppState>())
    else {
        return;
    };
    let count = state
        .db
        .lock()
        .ok()
        .and_then(|conn| inbox::list(&conn).ok())
        .map(|l| l.len())
        .unwrap_or(0);
    let _ = items.inbox.set_text(if count > 0 {
        format!("정리함 ({count})")
    } else {
        "정리함".to_string()
    });
    let paused = state.is_paused();
    let _ = items.pause.set_text(if paused {
        "다시 시작"
    } else {
        "일시 정지"
    });
    if let Some(tray) = app.tray_by_id(ID) {
        let _ = tray.set_tooltip(Some(if paused {
            "파일 정리 도우미 · 일시 정지 중"
        } else {
            "파일 정리 도우미"
        }));
    }
}

/// 일시 정지 등 상태가 바뀌었음을 트레이와 화면에 알린다.
pub fn status_changed(app: &AppHandle) {
    refresh(app);
    let _ = app.emit(EVENT_STATUS_CHANGED, ());
}

/// 메인 창을 앞으로. `page`를 주면 그 화면으로.
pub fn show_main(app: &AppHandle, page: Option<&str>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    if let Some(p) = page {
        let _ = app.emit(EVENT_NAVIGATE, p);
    }
}
