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

/// 규칙에 맞는 파일을 어떻게 할지(SPEC 4장 "추천과 자동 이동").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaceMode {
    /// 추천 위치를 먼저 보여 주기만 한다.
    #[default]
    Suggest,
    /// 팝업 없이 추천 위치로 바로 옮기고 알린다(되돌리기 가능).
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// 처음 실행 설정을 마쳤는지.
    pub setup_done: bool,
    /// 정리 폴더(파일을 넣어 둘 폴더들의 맨 위).
    pub root_folder: Option<String>,
    /// 새 파일을 지켜볼 폴더(바로 아래 파일만).
    pub watch_folders: Vec<String>,
    pub organize_mode: OrganizeMode,
    pub place_mode: PlaceMode,
    /// 1학기·2학기가 시작하는 달(학년도·학기 계산, 기본 3월·9월)
    pub first_term_month: u32,
    pub second_term_month: u32,
}

impl Default for Settings {
    fn default() -> Self {
        let months = crate::naming::TermMonths::default();
        Settings {
            setup_done: false,
            root_folder: None,
            watch_folders: Vec::new(),
            organize_mode: OrganizeMode::default(),
            place_mode: PlaceMode::default(),
            first_term_month: months.first,
            second_term_month: months.second,
        }
    }
}

impl Settings {
    pub fn term_months(&self) -> crate::naming::TermMonths {
        let m = crate::naming::TermMonths {
            first: self.first_term_month,
            second: self.second_term_month,
        };
        if valid_term_months(m.first, m.second) {
            m
        } else {
            crate::naming::TermMonths::default()
        }
    }
}

/// 1학기가 2학기보다 먼저 시작해야 한다(1~12월).
pub fn valid_term_months(first: u32, second: u32) -> bool {
    (1..=12).contains(&first) && (1..=12).contains(&second) && first < second
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
            place_mode: PlaceMode::Auto,
            first_term_month: 3,
            second_term_month: 8,
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
        // 5단계 전에 저장한 설정: 추천만, 3월·9월
        assert_eq!(s.place_mode, PlaceMode::Suggest);
        assert_eq!((s.first_term_month, s.second_term_month), (3, 9));
    }

    #[test]
    fn broken_term_months_fall_back() {
        let s = Settings {
            first_term_month: 9,
            second_term_month: 3,
            ..Settings::default()
        };
        assert_eq!(s.term_months(), crate::naming::TermMonths::default());
        assert!(valid_term_months(3, 8));
        assert!(!valid_term_months(0, 9));
        assert!(!valid_term_months(3, 13));
    }

    #[test]
    fn mode_is_stored_as_lowercase_word() {
        assert_eq!(
            serde_json::to_string(&OrganizeMode::Manual).unwrap(),
            "\"manual\""
        );
    }
}
