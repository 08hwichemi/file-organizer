//! 새 파일 팝업 창(SPEC 3-1): 테두리 없는 작은 창, 항상 위, 화면 오른쪽 아래.
//! 창은 tauri.conf.json에 숨긴 채 만들어 두고, 새 파일이 오면 보여 준다.

use tauri::{AppHandle, Manager, PhysicalPosition};

pub const LABEL: &str = "popup";
/// 화면 가장자리에서 떨어뜨릴 거리(논리 픽셀)
const MARGIN: f64 = 16.0;

pub fn show(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    if !window.is_visible().unwrap_or(false) {
        place_bottom_right(app, &window);
    }
    let _ = window.show();
    let _ = window.set_always_on_top(true);
    let _ = window.set_focus();
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}

fn place_bottom_right(app: &AppHandle, window: &tauri::WebviewWindow) {
    let monitor = app.primary_monitor().ok().flatten();
    let (Some(monitor), Ok(size)) = (monitor, window.outer_size()) else {
        return;
    };
    let area = monitor.work_area();
    let margin = (MARGIN * monitor.scale_factor()) as i32;
    let x = area.position.x + area.size.width as i32 - size.width as i32 - margin;
    let y = area.position.y + area.size.height as i32 - size.height as i32 - margin;
    let _ = window.set_position(PhysicalPosition::new(x.max(0), y.max(0)));
}
