//! 이동 기록(SPEC 5장): (원래 위치, 옮긴 위치, 시각, 규칙). 모든 이동은 여기에 남고 되돌릴 수 있다.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveRecord {
    pub id: i64,
    pub original_path: String,
    pub moved_path: String,
    pub moved_at: i64,
    pub undone: bool,
}

pub fn record(conn: &Connection, original: &Path, moved: &Path) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO moves (original_path, moved_path, moved_at) VALUES (?1, ?2, ?3)",
        params![
            original.to_string_lossy(),
            moved.to_string_lossy(),
            crate::db::now_ms()
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get(conn: &Connection, id: i64) -> rusqlite::Result<Option<MoveRecord>> {
    conn.query_row(
        "SELECT id, original_path, moved_path, moved_at, undone_at FROM moves WHERE id = ?1",
        params![id],
        |row| {
            Ok(MoveRecord {
                id: row.get(0)?,
                original_path: row.get(1)?,
                moved_path: row.get(2)?,
                moved_at: row.get(3)?,
                undone: row.get::<_, Option<i64>>(4)?.is_some(),
            })
        },
    )
    .optional()
}

pub fn mark_undone(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE moves SET undone_at = ?2 WHERE id = ?1",
        params![id, crate::db::now_ms()],
    )?;
    Ok(())
}

/// 최근에 파일을 넣은 폴더(되돌린 이동 제외), 새것부터, 중복 없이.
pub fn recent_destinations(conn: &Connection, limit: usize) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT moved_path FROM moves WHERE undone_at IS NULL ORDER BY moved_at DESC, id DESC LIMIT 200",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut out: Vec<String> = Vec::new();
    for moved in rows {
        let Some(dir) = Path::new(&moved?)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
        else {
            continue;
        };
        if !out.contains(&dir) {
            out.push(dir);
        }
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn record_get_and_undo() {
        let conn = db::open_in_memory().unwrap();
        let id = record(&conn, Path::new("/d/a.pdf"), Path::new("/r/평가/a.pdf")).unwrap();
        let rec = get(&conn, id).unwrap().unwrap();
        assert_eq!(rec.original_path, "/d/a.pdf");
        assert!(!rec.undone);
        mark_undone(&conn, id).unwrap();
        assert!(get(&conn, id).unwrap().unwrap().undone);
        assert!(get(&conn, id + 100).unwrap().is_none());
    }

    #[test]
    fn recent_destinations_are_unique_newest_first_without_undone() {
        let conn = db::open_in_memory().unwrap();
        record(&conn, Path::new("/d/1"), Path::new("/r/업무/1")).unwrap();
        record(&conn, Path::new("/d/2"), Path::new("/r/평가/2")).unwrap();
        record(&conn, Path::new("/d/3"), Path::new("/r/업무/3")).unwrap();
        let undone = record(&conn, Path::new("/d/4"), Path::new("/r/학급/4")).unwrap();
        mark_undone(&conn, undone).unwrap();
        assert_eq!(
            recent_destinations(&conn, 3).unwrap(),
            ["/r/업무", "/r/평가"]
        );
        assert_eq!(recent_destinations(&conn, 1).unwrap(), ["/r/업무"]);
    }
}
