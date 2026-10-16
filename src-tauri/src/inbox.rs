//! 정리함(SPEC 3-2): 미룬 파일, 팝업을 그냥 닫은 파일, "모아 두고 나중에 정리"로 모은 파일.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Reason {
    /// 팝업에서 "나중에 정리"를 누름(또는 Esc·닫기).
    Later,
    /// "모아 두고 나중에 정리" 방식이라 팝업 없이 모음.
    Collected,
}

impl Reason {
    fn as_str(self) -> &'static str {
        match self {
            Reason::Later => "later",
            Reason::Collected => "collected",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxItem {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub source_dir: String,
    pub added_at: i64,
    pub size: u64,
}

pub fn add(
    conn: &Connection,
    path: &Path,
    source_dir: &Path,
    reason: Reason,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO inbox (path, source_dir, added_at, reason) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(path) DO UPDATE SET added_at = excluded.added_at, reason = excluded.reason",
        params![
            path.to_string_lossy(),
            source_dir.to_string_lossy(),
            crate::db::now_ms(),
            reason.as_str()
        ],
    )?;
    Ok(())
}

/// 이미 정리함에 있으면 그대로 둔다(지금 있는 파일 가져오기용). 새로 넣었으면 true.
pub fn add_if_absent(
    conn: &Connection,
    path: &Path,
    source_dir: &Path,
    reason: Reason,
) -> rusqlite::Result<bool> {
    let changed = conn.execute(
        "INSERT OR IGNORE INTO inbox (path, source_dir, added_at, reason) VALUES (?1, ?2, ?3, ?4)",
        params![
            path.to_string_lossy(),
            source_dir.to_string_lossy(),
            crate::db::now_ms(),
            reason.as_str()
        ],
    )?;
    Ok(changed > 0)
}

pub fn remove(conn: &Connection, path: &Path) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM inbox WHERE path = ?1",
        params![path.to_string_lossy()],
    )?;
    Ok(())
}

/// 정리함 목록(새것부터). 그사이 사용자가 직접 옮기거나 지운 파일은 목록에서만 뺀다.
pub fn list(conn: &Connection) -> rusqlite::Result<Vec<InboxItem>> {
    let mut stmt = conn.prepare(
        "SELECT id, path, source_dir, added_at FROM inbox ORDER BY added_at DESC, id DESC",
    )?;
    let rows: Vec<(i64, String, String, i64)> = stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<Result<_, _>>()?;
    let mut items = Vec::new();
    for (id, path, source_dir, added_at) in rows {
        match std::fs::metadata(&path) {
            Ok(meta) if meta.is_file() => items.push(InboxItem {
                id,
                name: Path::new(&path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                path,
                source_dir,
                added_at,
                size: meta.len(),
            }),
            _ => {
                conn.execute("DELETE FROM inbox WHERE id = ?1", params![id])?;
            }
        }
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use std::fs;

    #[test]
    fn add_list_remove_and_prune_missing() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.pdf");
        let b = dir.path().join("b.hwp");
        fs::write(&a, b"aaa").unwrap();
        fs::write(&b, b"b").unwrap();
        let conn = db::open_in_memory().unwrap();
        add(&conn, &a, dir.path(), Reason::Later).unwrap();
        add(&conn, &b, dir.path(), Reason::Collected).unwrap();
        add(&conn, &b, dir.path(), Reason::Later).unwrap(); // 같은 파일은 한 줄

        let items = list(&conn).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items.iter().find(|i| i.name == "a.pdf").unwrap().size, 3);

        // 가져오기는 이미 있는 줄을 건드리지 않는다
        assert!(!add_if_absent(&conn, &a, dir.path(), Reason::Collected).unwrap());

        remove(&conn, &a).unwrap();
        assert!(add_if_absent(&conn, &a, dir.path(), Reason::Collected).unwrap());
        remove(&conn, &a).unwrap();
        fs::remove_file(&b).unwrap(); // 사용자가 직접 지운 경우(테스트용 임시 파일)
        assert!(list(&conn).unwrap().is_empty());
    }
}
