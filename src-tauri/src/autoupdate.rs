//! 자동 업데이트(SPEC 7장): 확인·받기·설치를 Rust 쪽에서 한다.
//!
//! 0.1.35에서는 화면(웹뷰) 쪽이 "창이 닫혀 있을 때만" 설치했다. 그런데 프로그램을 켜면 창이 떠서
//! 몇 번을 껐다 켜도 설치할 때가 오지 않았다(사용자 보고: 결국 [지금 업데이트]를 눌러야 함).
//! 그래서 이제는
//! - **켜고 3분 안에** 새 버전을 받으면 창이 떠 있어도 바로 설치한다(켤 때 업데이트). 창이 떠 있었으면
//!   다시 켜질 때도 창을 보인다.
//! - 그 뒤에 받은 것은 예전처럼 쓰지 않을 때(창이 닫혀 있을 때) 설치한다.
//! - 새 파일 창이 떠 있거나 물어볼 파일이 있으면 언제든 기다린다.
//!
//! 웹뷰는 창을 닫으면 쉬므로, 기다리고 확인하는 일은 따로 돈 스레드가 한다.
//! 이 확인은 프로그램 버전만 묻고, 파일 이름·경로 같은 사용자 정보는 보내지 않는다.

use crate::commands::AppState;
use crate::relaunch;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// 새 버전을 받아 두었음(화면에 "받아 두었어요" 띠). 값 = 버전
pub const EVENT_UPDATE_READY: &str = "update-ready";
/// 새 버전을 확인하는 간격
const CHECK_EVERY: Duration = Duration::from_secs(4 * 60 * 60);
/// 받아 둔 업데이트를 설치해도 되는지 보는 간격
const IDLE_EVERY: Duration = Duration::from_secs(60);
/// 확인에 실패했을 때(인터넷이 아직 안 됨 등) 다시 해 보는 간격
const RETRY_AFTER: Duration = Duration::from_secs(10 * 60);
/// 켠 뒤 이 시간 안이면 창이 떠 있어도 설치한다(켤 때 업데이트)
const START_WINDOW: Duration = Duration::from_secs(3 * 60);

/// 받아 둔 새 버전(설치 파일까지)
#[derive(Default)]
pub struct Ready(Mutex<Option<(Update, Vec<u8>)>>);

impl Ready {
    pub fn version(&self) -> Option<String> {
        self.0
            .lock()
            .ok()
            .and_then(|r| r.as_ref().map(|(u, _)| u.version.clone()))
    }
}

/// 언제 설치해도 되는지(순수 판단, 테스트용)
pub fn may_install(fresh: bool, main_visible: bool, busy: bool) -> bool {
    !busy && (fresh || !main_visible)
}

fn visible(app: &AppHandle, label: &str) -> bool {
    app.get_webview_window(label)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

/// 새 파일 창이 떠 있거나 물어볼·알릴 파일이 있으면 쓰는 중
fn busy(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    visible(app, crate::popup::LABEL) || state.has_waiting_files()
}

/// 새 버전이 있으면 받아 둔다. Ok(None) = 최신, Err = 확인·받기 실패(곧 다시)
fn fetch(app: &AppHandle) -> Result<Option<(Update, Vec<u8>)>, ()> {
    tauri::async_runtime::block_on(async {
        let Some(update) = app
            .updater()
            .map_err(|_| ())?
            .check()
            .await
            .map_err(|_| ())?
        else {
            return Ok(None);
        };
        let bytes = update.download(|_, _| {}, || {}).await.map_err(|_| ())?;
        Ok(Some((update, bytes)))
    })
}

/// 받아 둔 것을 설치한다. Windows에서는 설치 프로그램을 띄우고 이 프로그램은 끝난다(설치 뒤 다시 켜짐).
/// `show`면 다시 켜질 때 창을 보인다.
pub fn install(app: &AppHandle, show: bool) -> Result<(), String> {
    let ready = app
        .state::<AppState>()
        .ready_update
        .0
        .lock()
        .ok()
        .and_then(|r| r.clone());
    let Some((update, bytes)) = ready else {
        return Err("받아 둔 새 버전이 없어요.".into());
    };
    if let Ok(dir) = app.path().app_data_dir() {
        let _ = relaunch::mark(&dir, &update.version, !show);
    }
    update
        .install(bytes)
        .map_err(|e| format!("업데이트를 설치하지 못했어요: {e}"))?;
    app.restart();
}

/// 프로그램을 켤 때 한 번 부른다.
pub fn start(app: AppHandle) {
    let _ = std::thread::Builder::new()
        .name("auto-update".into())
        .spawn(move || {
            let started = Instant::now();
            // 다음에 확인할 때(처음엔 바로)
            let mut next_check = Instant::now();
            loop {
                let s = app.state::<AppState>().with_settings().unwrap_or_default();
                if s.setup_done && s.auto_update {
                    let state = app.state::<AppState>();
                    if state.ready_update.version().is_none() && Instant::now() >= next_check {
                        match fetch(&app) {
                            Ok(Some((update, bytes))) => {
                                let version = update.version.clone();
                                if let Ok(mut r) = state.ready_update.0.lock() {
                                    *r = Some((update, bytes));
                                }
                                let _ = app.emit(EVENT_UPDATE_READY, version);
                                next_check = Instant::now() + CHECK_EVERY;
                            }
                            Ok(None) => next_check = Instant::now() + CHECK_EVERY,
                            Err(()) => next_check = Instant::now() + RETRY_AFTER,
                        }
                    }
                    if state.ready_update.version().is_some() {
                        let fresh = started.elapsed() < START_WINDOW;
                        let main_visible = visible(&app, "main");
                        if may_install(fresh, main_visible, busy(&app)) {
                            // 창이 떠 있었으면 다시 켜질 때도 창을 보인다. 설치에 실패하면(서명이 안 맞는 등)
                            // 받아 둔 것을 버리고 다음 확인 때 다시 받는다(1분마다 같은 실패를 되풀이하지 않게).
                            if install(&app, main_visible).is_err() {
                                if let Ok(mut r) = state.ready_update.0.lock() {
                                    *r = None;
                                }
                                next_check = Instant::now() + CHECK_EVERY;
                            }
                        }
                    }
                }
                std::thread::sleep(IDLE_EVERY);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_at_start_or_when_window_is_closed_but_never_while_busy() {
        // 켠 직후: 창이 떠 있어도 설치
        assert!(may_install(true, true, false));
        // 그 뒤: 창이 닫혀 있을 때만
        assert!(!may_install(false, true, false));
        assert!(may_install(false, false, false));
        // 새 파일 창이 떠 있거나 물어볼 파일이 있으면 언제든 기다린다
        assert!(!may_install(true, false, true));
        assert!(!may_install(false, false, true));
    }
}
