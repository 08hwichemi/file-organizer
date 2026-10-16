//! 지우기 = Windows 휴지통으로 보내기(CLAUDE.md "반드시 지킬 것").
//! 영구 삭제는 하지 않는다. 보낸 것은 기록하고, 프로그램의 "되돌리기"나 휴지통에서 복원할 수 있다.

use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deletion {
    pub id: i64,
    pub original_path: String,
    pub is_folder: bool,
    /// 보낸 시각(1970년부터 밀리초)
    pub deleted_at: i64,
    pub restored: bool,
}

/// 파일이나 폴더를 휴지통으로 보내고 기록한다. 기록 번호를 돌려준다.
pub fn send_to_trash(conn: &Connection, path: &Path) -> Result<i64, String> {
    let is_folder = path.is_dir();
    if !is_folder && !path.is_file() {
        return Err("지울 파일이나 폴더를 찾을 수 없어요.".into());
    }
    // 휴지통이 없는 드라이브(USB·네트워크)에서는 Windows가 휴지통을 거치지 않고 바로 지울 수 있다.
    if !has_recycle_bin(path) {
        return Err(
            "이 위치(USB·네트워크 드라이브 등)에는 휴지통이 없어서 지울 수 없어요. 꼭 지워야 하면 탐색기에서 직접 지워 주세요."
                .into(),
        );
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
        "SELECT id, original_path, is_folder, deleted_at, restored_at FROM deletions WHERE id = ?1",
        params![id],
        |row| {
            Ok(Deletion {
                id: row.get(0)?,
                original_path: row.get(1)?,
                is_folder: row.get(2)?,
                deleted_at: row.get(3)?,
                restored: row.get::<_, Option<i64>>(4)?.is_some(),
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
    restore_from_trash(&original, record.deleted_at / 1000)?;
    conn.execute(
        "UPDATE deletions SET restored_at = ?2 WHERE id = ?1",
        params![id, crate::db::now_ms()],
    )
    .map_err(|e| e.to_string())?;
    Ok(record)
}

/// 휴지통 목록에서 되돌릴 항목을 고른다.
///
/// Windows가 알려 주는 원래 위치는 대소문자·`\\?\` 앞부분이 다를 수 있고, 이름은 "알려진 확장자 숨기기"
/// 설정이면 확장자가 빠져 있을 수 있다. 그래서 원래 폴더를 느슨하게 비교하고, 이름은 전체 또는 확장자 뺀 이름으로 맞춘다.
/// 원래 폴더가 맞는 것이 없으면, 보낸 시각 무렵(앞뒤 10분)에 같은 이름으로 들어온 것을 고른다.
/// 여럿이면 가장 최근 것. 고른 항목의 이름은 원래 이름(확장자 포함)으로 바꿔 돌려준다.
#[cfg_attr(
    not(any(windows, all(unix, not(target_os = "macos")))),
    allow(dead_code)
)]
fn pick_trash_item(
    items: Vec<trash::TrashItem>,
    original: &Path,
    deleted_at_secs: i64,
) -> Option<trash::TrashItem> {
    let name = original.file_name()?.to_string_lossy().into_owned();
    let stem = original
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.clone());
    let parent = normalize(original.parent()?);
    let name_matches = |item: &trash::TrashItem| {
        let n = item.name.to_string_lossy();
        n.eq_ignore_ascii_case(&name) || n.eq_ignore_ascii_case(&stem)
    };
    let candidates: Vec<trash::TrashItem> = items.into_iter().filter(name_matches).collect();
    let same_parent: Vec<trash::TrashItem> = candidates
        .iter()
        .filter(|item| normalize(&item.original_parent) == parent)
        .cloned()
        .collect();
    let pool = if same_parent.is_empty() {
        candidates
            .into_iter()
            .filter(|item| (item.time_deleted - deleted_at_secs).abs() <= 600)
            .collect()
    } else {
        same_parent
    };
    let mut item = pool.into_iter().max_by_key(|item| item.time_deleted)?;
    item.name = name.into();
    Some(item)
}

/// 경로 비교용: `\\?\` 앞부분과 끝의 구분자를 빼고, 구분자를 하나로, 대소문자를 무시한다(Windows).
fn normalize(path: &Path) -> String {
    let s = path.to_string_lossy();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    let s = s.replace('/', "\\");
    s.trim_end_matches('\\').to_lowercase()
}

#[cfg(any(windows, all(unix, not(target_os = "macos"))))]
fn restore_from_trash(original: &Path, deleted_at_secs: i64) -> Result<(), String> {
    use trash::os_limited;
    let items = os_limited::list().map_err(|e| describe(&e, "휴지통을 읽지 못했어요"))?;
    let item = pick_trash_item(items, original, deleted_at_secs)
        .ok_or("휴지통에서 찾을 수 없어요. 이미 비웠거나 직접 복원했을 수 있어요.")?;
    os_limited::restore_all([item]).map_err(|e| describe(&e, "되돌리지 못했어요"))?;
    if original.exists() {
        Ok(())
    } else {
        Err("휴지통에서 꺼냈지만 원래 자리에서 찾을 수 없어요. 휴지통을 확인해 주세요.".into())
    }
}

#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
fn restore_from_trash(_original: &Path, _deleted_at_secs: i64) -> Result<(), String> {
    Err("이 운영체제에서는 휴지통에서 직접 복원해 주세요.".into())
}

/// 이 경로가 있는 드라이브에 휴지통이 있는지(Windows: 고정 디스크만). 다른 운영체제는 true.
fn has_recycle_bin(_path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use std::path::{Component, Prefix};
        let Ok(full) = _path.canonicalize() else {
            return false;
        };
        let root = match full.components().next() {
            Some(Component::Prefix(p)) => match p.kind() {
                Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                    format!("{}:\\", letter as char)
                }
                // UNC(네트워크) 경로 등은 휴지통이 없다.
                _ => return false,
            },
            _ => return false,
        };
        let wide: Vec<u16> = std::ffi::OsStr::new(&root)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        const DRIVE_FIXED: u32 = 3;
        // SAFETY: 끝이 0인 UTF-16 문자열을 넘긴다.
        let kind = unsafe { windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(wide.as_ptr()) };
        return kind == DRIVE_FIXED;
    }
    #[allow(unreachable_code)]
    true
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

    fn item(name: &str, parent: &str, time: i64) -> trash::TrashItem {
        trash::TrashItem {
            id: format!("{parent}/{name}/{time}").into(),
            name: name.into(),
            original_parent: PathBuf::from(parent),
            time_deleted: time,
        }
    }

    /// 운영체제 구분자로 경로 만들기(테스트가 Windows·리눅스 둘 다에서 돌도록)
    fn p(parts: &[&str]) -> PathBuf {
        parts.iter().collect()
    }

    #[test]
    fn picks_trash_item_with_loose_path_and_hidden_extension() {
        let folder = p(&["C:", "Users", "선생님", "Documents", "정리 폴더"]);
        let original = folder.join("보고서.hwp");
        let sep = std::path::MAIN_SEPARATOR;
        // Windows가 앞에 \\?\를 붙이거나, 소문자로, 끝에 구분자를 붙여 알려 주는 경우
        let loose = format!(r"\\?\{}{sep}", folder.to_string_lossy().to_lowercase());
        let items = vec![
            item("보고서", &loose, 100),
            item("보고서", &p(&["C:", "다른 곳"]).to_string_lossy(), 500),
            item("다른 파일.hwp", &folder.to_string_lossy(), 600),
        ];
        let picked = pick_trash_item(items, &original, 100).unwrap();
        assert_eq!(picked.time_deleted, 100);
        // 확장자가 숨겨져 있어도 원래 이름(확장자 포함)으로 되돌린다
        assert_eq!(picked.name.to_string_lossy(), "보고서.hwp");
    }

    #[test]
    fn picks_latest_and_falls_back_to_time_when_parent_differs() {
        let folder = p(&["C:", "정리 폴더"]);
        let original = folder.join("연수");
        let f = folder.to_string_lossy().into_owned();
        let items = vec![item("연수", &f, 100), item("연수", &f, 200)];
        assert_eq!(
            pick_trash_item(items, &original, 0).unwrap().time_deleted,
            200
        );

        // 원래 폴더 표기가 아주 다르면 보낸 시각 무렵(앞뒤 10분)의 같은 이름으로
        let other = p(&["D:", "엉뚱"]).to_string_lossy().into_owned();
        let items = vec![item("연수", &other, 1_000), item("연수", &other, 5_000)];
        assert_eq!(
            pick_trash_item(items.clone(), &original, 1_100)
                .unwrap()
                .time_deleted,
            1_000
        );
        assert!(pick_trash_item(items, &original, 50_000).is_none());
        assert!(pick_trash_item(vec![], &original, 0).is_none());
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
