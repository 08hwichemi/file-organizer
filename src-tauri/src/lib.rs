//! 파일 정리 도우미 — Rust 쪽 진입점.
//!
//! 화면(React)은 파일을 직접 만지지 않고 여기 등록한 명령을 `invoke`로 부른다(SPEC 8장).
//! 기능 모듈(watcher, mover, rules, search, everything, db, tray)은
//! SPEC 9장 개발 단계에 맞춰 하나씩 추가한다.

mod commands;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::app_info])
        .run(tauri::generate_context!())
        .expect("파일 정리 도우미를 시작하지 못했습니다");
}
