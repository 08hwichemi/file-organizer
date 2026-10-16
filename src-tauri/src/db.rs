//! SQLite 저장소(SPEC 8장). 설정·규칙·이동 기록·정리함을 `data.db` 하나에 둔다.
//! 표 구조가 바뀔 때마다 `migrate`에 단계를 추가하고 `user_version`을 올린다.

use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    migrate(&conn)?;
    Ok(conn)
}

#[cfg(test)]
pub fn open_in_memory() -> rusqlite::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version < 1 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS settings (
                 key   TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             PRAGMA user_version = 1;",
        )?;
    }
    if version < 2 {
        // 이동 기록(되돌리기용)과 정리함(미룬 파일·모아 둔 파일).
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS moves (
                 id            INTEGER PRIMARY KEY,
                 original_path TEXT    NOT NULL,
                 moved_path    TEXT    NOT NULL,
                 moved_at      INTEGER NOT NULL,
                 rule          TEXT,
                 undone_at     INTEGER
             );
             CREATE TABLE IF NOT EXISTS inbox (
                 id         INTEGER PRIMARY KEY,
                 path       TEXT    NOT NULL UNIQUE,
                 source_dir TEXT    NOT NULL,
                 added_at   INTEGER NOT NULL,
                 reason     TEXT    NOT NULL
             );
             PRAGMA user_version = 2;",
        )?;
    }
    if version < 3 {
        // 휴지통으로 보낸 기록(되돌리기·나중에 '최근 정리한 파일'용).
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS deletions (
                 id            INTEGER PRIMARY KEY,
                 original_path TEXT    NOT NULL,
                 is_folder     INTEGER NOT NULL,
                 deleted_at    INTEGER NOT NULL,
                 restored_at   INTEGER
             );
             PRAGMA user_version = 3;",
        )?;
    }
    Ok(())
}

pub fn get_value(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
}

pub fn set_value(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// 지금 시각(1970년부터 밀리초). 기록에 쓴다.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_then_get_overwrites_value() {
        let conn = open_in_memory().unwrap();
        assert_eq!(get_value(&conn, "a").unwrap(), None);
        set_value(&conn, "a", "1").unwrap();
        set_value(&conn, "a", "2").unwrap();
        assert_eq!(get_value(&conn, "a").unwrap().as_deref(), Some("2"));
    }

    #[test]
    fn reopening_file_keeps_data_and_migration_is_repeatable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.db");
        set_value(&open(&path).unwrap(), "k", "v").unwrap();
        let conn = open(&path).unwrap();
        assert_eq!(get_value(&conn, "k").unwrap().as_deref(), Some("v"));
    }
}
