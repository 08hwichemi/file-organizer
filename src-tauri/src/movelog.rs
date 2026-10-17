//! 정리 기록(SPEC 3-2, 사용자 요청): 넣고 옮긴 기록(moves)과 휴지통 기록(deletions)을 한 목록으로, 새것부터.
//!
//! 기록은 이동·휴지통 보내기 때마다 이미 남기고 있어서(SPEC 5장) 모아 보여 주기만 한다.
//! 기록은 이 PC(사용자 자료 폴더의 data.db)에만 있고 밖으로 보내지 않는다.

use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;

/// 한 번에 보여 주는 기록 수(아주 오래 써도 화면이 느려지지 않게)
pub const MAX_ENTRIES: usize = 3000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    /// "move"(넣기·옮기기) | "rename"(이름 한꺼번에 바꾸기) | "trash"(휴지통)
    pub kind: &'static str,
    /// moves.id 또는 deletions.id(되돌리기·되살리기에 쓴다)
    pub id: i64,
    /// 파일·폴더 이름(넣기·이름 바꾸기는 바뀐 뒤 이름, 휴지통은 원래 이름)
    pub name: String,
    pub from_path: String,
    /// 옮긴 경로(휴지통은 없음)
    pub to_path: Option<String>,
    pub at: i64,
    /// 되돌린 시각(넣기·이름 바꾸기) 또는 되살린 시각(휴지통)
    pub reverted_at: Option<i64>,
    /// 지금도 옮긴 자리에 있는지(넣기·이름 바꾸기), 아직 휴지통에 있는지(휴지통): [위치]·[되돌리기]·[되살리기]를 보일지
    pub present: bool,
    pub is_folder: bool,
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// 넣기·옮기기·이름 바꾸기·휴지통 기록, 새것부터 `limit`개.
pub fn entries(conn: &Connection, limit: usize) -> rusqlite::Result<Vec<LogEntry>> {
    let mut out = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT id, original_path, moved_path, moved_at, undone_at, batch FROM moves
         ORDER BY moved_at DESC, id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, Option<i64>>(4)?,
            r.get::<_, Option<i64>>(5)?,
        ))
    })?;
    for row in rows {
        let (id, from, to, at, undone, batch) = row?;
        let target = Path::new(&to);
        let present = undone.is_none() && target.exists();
        out.push(LogEntry {
            kind: if batch.is_some() { "rename" } else { "move" },
            id,
            name: file_name(&to),
            is_folder: present && target.is_dir(),
            from_path: from,
            to_path: Some(to),
            at,
            reverted_at: undone,
            present,
        });
    }
    let mut stmt = conn.prepare(
        "SELECT id, original_path, is_folder, deleted_at, restored_at FROM deletions
         ORDER BY deleted_at DESC, id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, bool>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, Option<i64>>(4)?,
        ))
    })?;
    for row in rows {
        let (id, from, is_folder, at, restored) = row?;
        out.push(LogEntry {
            kind: "trash",
            id,
            name: file_name(&from),
            from_path: from,
            to_path: None,
            at,
            reverted_at: restored,
            present: restored.is_none(),
            is_folder,
        });
    }
    // 같은 시각이면 나중에 기록한 것(번호가 큰 것)이 위로
    out.sort_by(|a, b| b.at.cmp(&a.at).then(b.id.cmp(&a.id)));
    out.truncate(limit);
    Ok(out)
}

/// [엑셀 파일로 저장]: 화면이 만든 CSV를 사용자가 고른 곳에 쓴다.
/// 엑셀이 한글을 바로 읽게 UTF-8 BOM을 붙인다. .csv 파일만 쓴다(다른 파일을 덮어쓰지 않게).
pub fn save_csv(path: &Path, text: &str) -> Result<(), String> {
    let is_csv = path
        .extension()
        .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("csv"));
    if !is_csv {
        return Err("CSV 파일(.csv)로만 저장할 수 있어요.".into());
    }
    std::fs::write(path, format!("\u{FEFF}{text}")).map_err(|e| format!("저장하지 못했어요: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, history};
    use std::fs;

    #[test]
    fn merges_moves_renames_and_trash_newest_first() {
        let d = tempfile::tempdir().unwrap();
        let conn = db::open_in_memory().unwrap();
        let put = d.path().join("평가").join("a.pdf");
        fs::create_dir_all(put.parent().unwrap()).unwrap();
        fs::write(&put, b"x").unwrap();
        let moved = history::record(&conn, Path::new("/d/a.pdf"), &put).unwrap();
        let gone =
            history::record(&conn, Path::new("/d/b.pdf"), &d.path().join("없음.pdf")).unwrap();
        let undone =
            history::record(&conn, Path::new("/d/c.pdf"), &d.path().join("c.pdf")).unwrap();
        history::mark_undone(&conn, undone).unwrap();
        let batch = history::next_batch(&conn).unwrap();
        history::record_in_batch(
            &conn,
            Path::new("/r/옛 이름.hwp"),
            Path::new("/r/새 이름.hwp"),
            batch,
        )
        .unwrap();
        conn.execute(
            "INSERT INTO deletions (original_path, is_folder, deleted_at) VALUES ('/d/scan.pdf', 0, 9999999999999)",
            [],
        )
        .unwrap();

        let list = entries(&conn, 100).unwrap();
        assert_eq!(list.len(), 5);
        // 휴지통 기록(가장 늦은 시각)이 맨 위
        assert_eq!(
            (list[0].kind, list[0].name.as_str(), list[0].present),
            ("trash", "scan.pdf", true)
        );
        assert!(list[0].to_path.is_none());
        let by_id =
            |id: i64, kind: &str| list.iter().find(|e| e.id == id && e.kind == kind).unwrap();
        assert!(by_id(moved, "move").present);
        assert_eq!(by_id(moved, "move").name, "a.pdf");
        assert_eq!(by_id(moved, "move").from_path, "/d/a.pdf");
        // 그 뒤에 다시 옮겨져 그 자리에 없는 파일
        assert!(!by_id(gone, "move").present);
        // 되돌린 기록은 시각이 남고 [되돌리기]는 없다
        assert!(by_id(undone, "move").reverted_at.is_some());
        assert!(!by_id(undone, "move").present);
        assert!(list
            .iter()
            .any(|e| e.kind == "rename" && e.name == "새 이름.hwp"));
    }

    #[test]
    fn limit_keeps_the_newest() {
        let conn = db::open_in_memory().unwrap();
        for i in 0..5 {
            history::record(
                &conn,
                Path::new(&format!("/d/{i}")),
                Path::new(&format!("/r/{i}")),
            )
            .unwrap();
        }
        let list = entries(&conn, 3).unwrap();
        assert_eq!(
            list.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["4", "3", "2"]
        );
    }

    #[test]
    fn saves_only_csv_with_bom() {
        let d = tempfile::tempdir().unwrap();
        let ok = d.path().join("정리 기록.csv");
        save_csv(&ok, "날짜,파일\n").unwrap();
        assert_eq!(fs::read_to_string(&ok).unwrap(), "\u{FEFF}날짜,파일\n");
        let not_csv = d.path().join("중요.hwp");
        fs::write(&not_csv, b"keep").unwrap();
        assert!(save_csv(&not_csv, "x").is_err());
        assert_eq!(fs::read(&not_csv).unwrap(), b"keep");
    }
}
