//! 화면에서 `invoke`로 부르는 명령 모음.

use serde::Serialize;

/// 프로그램 이름·버전·운영체제. 1단계에서는 화면과 Rust 연결 확인용.
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
