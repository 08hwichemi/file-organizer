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

/// 엑셀 파일 한 줄(화면이 짧은 곳 이름·상태를 만들어 보낸다)
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XlsxRow {
    /// 시각(1970년부터 밀리초). 이 PC의 시간대로 날짜·시각 칸에 넣는다.
    pub at: i64,
    pub kind: String,
    pub name: String,
    pub from: String,
    pub to: String,
    pub status: String,
    pub from_path: String,
    pub to_path: String,
}

/// [엑셀 파일로 저장]: 서식이 있는 .xlsx로(사용자 요청: CSV는 칸 너비·색·필터가 없었다).
/// 파란 제목 줄(고정·필터), 칸 너비, 줄무늬, 상태 색, 진짜 날짜·시각 칸, 가로 인쇄.
/// .xlsx 파일에만 쓴다(다른 파일을 덮어쓰지 않게).
pub fn save_xlsx(path: &Path, rows: &[XlsxRow]) -> Result<(), String> {
    use chrono::{Datelike, TimeZone, Timelike};
    use rust_xlsxwriter::{Color, ExcelDateTime, Format, FormatAlign, FormatBorder, Workbook};

    let is_xlsx = path
        .extension()
        .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("xlsx"));
    if !is_xlsx {
        return Err("엑셀 파일(.xlsx)로만 저장할 수 있어요.".into());
    }
    let err = |e: rust_xlsxwriter::XlsxError| format!("엑셀 파일을 만들지 못했어요: {e}");

    // 색은 화면과 같게: 강조 #2B59C3, 글자 #17191C, 보조 #5F6670, 선 #E4E6EA, 배경 #F4F5F7
    let base = |stripe: bool| {
        let f = Format::new()
            .set_font_name("맑은 고딕")
            .set_font_size(10)
            .set_font_color(Color::RGB(0x17191C))
            .set_align(FormatAlign::VerticalCenter)
            .set_border_bottom(FormatBorder::Thin)
            .set_border_bottom_color(Color::RGB(0xE4E6EA));
        if stripe {
            f.set_background_color(Color::RGB(0xF4F5F7))
        } else {
            f
        }
    };
    let head = Format::new()
        .set_font_name("맑은 고딕")
        .set_font_size(10)
        .set_bold()
        .set_font_color(Color::White)
        .set_background_color(Color::RGB(0x2B59C3))
        .set_align(FormatAlign::VerticalCenter)
        .set_indent(1);

    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    sheet.set_name("정리 기록").map_err(err)?;
    const HEAD: [(&str, f64); 9] = [
        ("날짜", 12.0),
        ("시각", 9.0),
        ("종류", 11.0),
        ("파일 이름", 38.0),
        ("원래 있던 곳", 22.0),
        ("넣은 곳", 24.0),
        ("상태", 14.0),
        ("원래 경로", 40.0),
        ("넣은 경로", 46.0),
    ];
    for (c, (title, width)) in HEAD.iter().enumerate() {
        let c = c as u16;
        sheet
            .write_string_with_format(0, c, *title, &head)
            .map_err(err)?;
        sheet.set_column_width(c, *width).map_err(err)?;
    }
    sheet.set_row_height(0, 24).map_err(err)?;

    for (i, r) in rows.iter().enumerate() {
        let row = i as u32 + 1;
        let f = base(i % 2 == 1);
        let text = f.clone().set_indent(1);
        let center = f.clone().set_align(FormatAlign::Center);
        let when = chrono::Local
            .timestamp_millis_opt(r.at)
            .single()
            .ok_or("기록의 시각을 읽지 못했어요.")?;
        let date =
            ExcelDateTime::from_ymd(when.year() as u16, when.month() as u8, when.day() as u8)
                .map_err(err)?;
        let time =
            ExcelDateTime::from_hms(when.hour() as u16, when.minute() as u8, 0).map_err(err)?;
        sheet
            .write_datetime_with_format(row, 0, &date, &center.clone().set_num_format("yyyy-mm-dd"))
            .map_err(err)?;
        sheet
            .write_datetime_with_format(row, 1, &time, &center.set_num_format("hh:mm"))
            .map_err(err)?;
        sheet
            .write_string_with_format(row, 2, &r.kind, &text)
            .map_err(err)?;
        sheet
            .write_string_with_format(row, 3, &r.name, &text.clone().set_bold())
            .map_err(err)?;
        sheet
            .write_string_with_format(row, 4, &r.from, &text)
            .map_err(err)?;
        sheet
            .write_string_with_format(row, 5, &r.to, &text)
            .map_err(err)?;
        let status_color = if r.status == "휴지통에 있음" {
            0xB42318
        } else {
            0x5F6670
        };
        let status = text
            .clone()
            .set_bold()
            .set_font_color(Color::RGB(status_color));
        sheet
            .write_string_with_format(row, 6, &r.status, &status)
            .map_err(err)?;
        let path = text.clone().set_font_color(Color::RGB(0x5F6670));
        sheet
            .write_string_with_format(row, 7, &r.from_path, &path)
            .map_err(err)?;
        sheet
            .write_string_with_format(row, 8, &r.to_path, &path)
            .map_err(err)?;
        sheet.set_row_height(row, 20).map_err(err)?;
    }

    sheet.set_freeze_panes(1, 0).map_err(err)?;
    sheet.autofilter(0, 0, rows.len() as u32, 8).map_err(err)?;
    sheet.set_landscape();
    sheet.set_print_fit_to_pages(1, 0);
    book.save(path).map_err(err)
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
    fn saves_only_xlsx() {
        let d = tempfile::tempdir().unwrap();
        let row = XlsxRow {
            at: 1_760_690_460_000,
            kind: "넣기".into(),
            name: "2학기_지필고사_감독배정표.xlsx".into(),
            from: "다운로드".into(),
            to: "평가 › 지필고사".into(),
            status: "".into(),
            from_path: "C:\\Users\\선생님\\Downloads".into(),
            to_path: "C:\\정리 폴더\\평가\\지필고사".into(),
        };
        let ok = d.path().join("정리 기록.xlsx");
        save_xlsx(&ok, &[row.clone(), row.clone()]).unwrap();
        // 엑셀 파일은 zip(PK로 시작)
        assert_eq!(&fs::read(&ok).unwrap()[..2], b"PK");
        let not_xlsx = d.path().join("중요.hwp");
        fs::write(&not_xlsx, b"keep").unwrap();
        assert!(save_xlsx(&not_xlsx, &[row]).is_err());
        assert_eq!(fs::read(&not_xlsx).unwrap(), b"keep");
    }
}
