//! 프로그램 설정. DB `settings` 표에 JSON 한 덩어리로 저장한다.
//! 나중에 항목이 늘어도 예전 설정을 읽을 수 있게 모든 필드에 기본값을 둔다.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

const KEY: &str = "app";

/// 새 파일이 생겼을 때 어떻게 할지(SPEC 3-3 설정 "정리 방식").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrganizeMode {
    /// 새 파일마다 팝업을 띄워 넣을 곳을 묻는다.
    #[default]
    Ask,
    /// 팝업 없이 정리함에 모아 두고, 사용자가 원할 때 정리한다.
    Manual,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// 처음 실행 설정을 마쳤는지.
    pub setup_done: bool,
    /// 정리 폴더(파일을 넣어 둘 폴더들의 맨 위).
    pub root_folder: Option<String>,
    /// 새 파일을 지켜볼 폴더(바로 아래 파일만).
    pub watch_folders: Vec<String>,
    pub organize_mode: OrganizeMode,
}

pub fn load(conn: &Connection) -> Result<Settings, String> {
    let raw = crate::db::get_value(conn, KEY).map_err(|e| format!("설정을 읽지 못했어요: {e}"))?;
    match raw {
        // 망가진 값이면 처음 실행 화면부터 다시 시작한다(파일은 건드리지 않는다).
        Some(json) => Ok(serde_json::from_str(&json).unwrap_or_default()),
        None => Ok(Settings::default()),
    }
}

pub fn save(conn: &Connection, settings: &Settings) -> Result<(), String> {
    let json = serde_json::to_string(settings).map_err(|e| e.to_string())?;
    crate::db::set_value(conn, KEY, &json).map_err(|e| format!("설정을 저장하지 못했어요: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn defaults_before_first_run() {
        let conn = db::open_in_memory().unwrap();
        let s = load(&conn).unwrap();
        assert!(!s.setup_done);
        assert_eq!(s.organize_mode, OrganizeMode::Ask);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let conn = db::open_in_memory().unwrap();
        let s = Settings {
            setup_done: true,
            root_folder: Some("C:\\정리 폴더".into()),
            watch_folders: vec!["C:\\Downloads".into()],
            organize_mode: OrganizeMode::Manual,
        };
        save(&conn, &s).unwrap();
        assert_eq!(load(&conn).unwrap(), s);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let conn = db::open_in_memory().unwrap();
        db::set_value(&conn, KEY, r#"{"setupDone":true}"#).unwrap();
        let s = load(&conn).unwrap();
        assert!(s.setup_done);
        assert_eq!(s.organize_mode, OrganizeMode::Ask);
        assert!(s.watch_folders.is_empty());
    }

    #[test]
    fn mode_is_stored_as_lowercase_word() {
        assert_eq!(
            serde_json::to_string(&OrganizeMode::Manual).unwrap(),
            "\"manual\""
        );
    }
}
