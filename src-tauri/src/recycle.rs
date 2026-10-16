//! 지우기 = Windows 휴지통으로 보내기(CLAUDE.md "반드시 지킬 것").
//! 영구 삭제는 하지 않는다. 보낸 것은 기록하고, 프로그램의 "되돌리기"나 휴지통에서 복원할 수 있다.

use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deletion {
    pub id: i64,
    pub original_path: String,
    pub is_folder: bool,
    pub restored: bool,
}

/// 파일이나 폴더를 휴지통으로 보내고 기록한다. 기록 번호를 돌려준다.
pub fn send_to_trash(conn: &Connection, path: &Path) -> Result<i64, String> {
    let is_folder = path.is_dir();
    if !is_folder && !path.is_file() {
        return Err("지울 파일이나 폴더를 찾을 수 없어요.".into());
    }
    trash::delete(path).map_err(|e| describe(&e, "휴지통으로 보내지 못했어요"))?;
    conn.execute(
        "INSERT INTO deletions (original_path, is_folder, deleted_at) VALUES (?1, ?2, ?3)",
        params![path.to_string_lossy(), is_folder, crate::db::now_ms()],
    )
    .map_err(|e| format!("기록을 저장하지 못했어요: {e}"))?;
    Ok(conn.last_insert_rowid())
}

pub fn get(conn: &Connection, id: i64) -> rusqlite::Result<Option<Deletion>> {
    conn.query_row(
        "SELECT id, original_path, is_folder, restored_at FROM deletions WHERE id = ?1",
        params![id],
        |row| {
            Ok(Deletion {
                id: row.get(0)?,
                original_path: row.get(1)?,
                is_folder: row.get(2)?,
                restored: row.get::<_, Option<i64>>(3)?.is_some(),
            })
        },
    )
    .optional()
}

/// 휴지통에서 원래 자리로 되돌린다(같은 자리에서 지운 것이 여럿이면 가장 최근 것).
pub fn restore(conn: &Connection, id: i64) -> Result<Deletion, String> {
    let record = get(conn, id)
        .map_err(|e| e.to_string())?
        .ok_or("되돌릴 기록을 찾을 수 없어요.")?;
    if record.restored {
        return Err("이미 되돌렸어요.".into());
    }
    let original = PathBuf::from(&record.original_path);
    if original.exists() {
        return Err(format!(
            "‘{}’ 자리에 같은 이름이 이미 있어서 되돌릴 수 없어요. 휴지통에서 직접 복원해 주세요.",
            original.display()
        ));
    }
    restore_from_trash(&original)?;
    conn.execute(
        "UPDATE deletions SET restored_at = ?2 WHERE id = ?1",
        params![id, crate::db::now_ms()],
    )
    .map_err(|e| e.to_string())?;
    Ok(record)
}

#[cfg(any(windows, all(unix, not(target_os = "macos"))))]
fn restore_from_trash(original: &Path) -> Result<(), String> {
    use trash::os_limited;
    let items = os_limited::list().map_err(|e| describe(&e, "휴지통을 읽지 못했어요"))?;
    let item = items
        .into_iter()
        .filter(|item| item.original_path() == original)
        .max_by_key(|item| item.time_deleted)
        .ok_or("휴지통에서 찾을 수 없어요. 이미 비웠거나 직접 복원했을 수 있어요.")?;
    os_limited::restore_all([item]).map_err(|e| describe(&e, "되돌리지 못했어요"))
}

#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
fn restore_from_trash(_original: &Path) -> Result<(), String> {
    Err("이 운영체제에서는 휴지통에서 직접 복원해 주세요.".into())
}

fn describe(e: &trash::Error, what: &str) -> String {
    format!("{what}: {e}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn missing_path_is_an_error_and_not_recorded() {
        let conn = db::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        assert!(send_to_trash(&conn, &dir.path().join("없음")).is_err());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM deletions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn restore_refuses_when_original_place_is_taken() {
        let conn = db::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        std::fs::write(&path, b"x").unwrap();
        // 휴지통을 쓰지 않고 기록만 만들어, 자리가 차 있으면 되돌리지 않는지 본다.
        conn.execute(
            "INSERT INTO deletions (original_path, is_folder, deleted_at) VALUES (?1, 0, 0)",
            params![path.to_string_lossy()],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        assert!(restore(&conn, id).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"x");
        assert!(!get(&conn, id).unwrap().unwrap().restored);
        assert!(restore(&conn, id + 1).is_err());
    }

    /// 실제 휴지통을 쓰는 시험(직접 돌릴 때만: cargo test -- --ignored).
    #[test]
    #[ignore]
    fn real_trash_roundtrip() {
        let conn = db::open_in_memory().unwrap();
        let home = std::env::var("HOME").unwrap();
        let dir = tempfile::tempdir_in(home).unwrap();
        let folder = dir.path().join("지울 폴더");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("a.txt"), b"keep").unwrap();

        let id = send_to_trash(&conn, &folder).unwrap();
        assert!(!folder.exists());
        restore(&conn, id).unwrap();
        assert_eq!(std::fs::read(folder.join("a.txt")).unwrap(), b"keep");
        assert!(get(&conn, id).unwrap().unwrap().restored);
        assert!(restore(&conn, id).is_err());
    }
}
