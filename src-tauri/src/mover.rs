//! 파일 이동·되돌리기(SPEC 5장).
//!
//! - 덮어쓰지 않는다: 같은 이름이 있으면 `이름 (2).ext`처럼 번호를 붙인다.
//!   이름은 빈 자리표시 파일을 `create_new`로 먼저 만들어 확보하므로, 그사이 다른 파일이 생겨도 덮어쓰지 않는다.
//! - 다른 드라이브: 복사한 뒤 원본을 지운다. 원본을 못 지우면 방금 만든 복사본을 지우고 오류를 낸다.
//! - 사용자 파일을 지우는 다른 코드는 두지 않는다.

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

/// `dir` 안에 `file_name`을 둘 자리를 확보하고 그 경로를 돌려준다.
/// 같은 이름이 있으면 `이름 (2).ext`, `이름 (3).ext` … 순서로 고른다.
fn reserve_destination(dir: &Path, file_name: &str) -> io::Result<PathBuf> {
    let (stem, ext) = split_name(file_name);
    for n in 1..10_000 {
        let candidate = if n == 1 {
            dir.join(file_name)
        } else {
            dir.join(format!("{stem} ({n}){ext}"))
        };
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => return Ok(candidate),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other("같은 이름의 파일이 너무 많아요"))
}

/// "보고서.final.hwp" → ("보고서.final", ".hwp"), "README" → ("README", "")
fn split_name(file_name: &str) -> (&str, &str) {
    match file_name.rfind('.') {
        Some(i) if i > 0 => (&file_name[..i], &file_name[i..]),
        _ => (file_name, ""),
    }
}

/// `src`를 `dest_dir` 안으로 옮기고 최종 경로를 돌려준다. `new_name`을 주면 그 이름으로.
pub fn move_into(src: &Path, dest_dir: &Path, new_name: Option<&str>) -> Result<PathBuf, String> {
    if !src.is_file() {
        return Err("파일을 찾을 수 없어요. 이미 옮겨졌거나 지워졌을 수 있어요.".into());
    }
    if !dest_dir.is_dir() {
        return Err("넣을 폴더를 찾을 수 없어요.".into());
    }
    let name = match new_name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => {
            crate::folders::validate_name(n)?;
            n.to_string()
        }
        None => src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or("파일 이름을 읽을 수 없어요.")?,
    };
    let dest = reserve_destination(dest_dir, &name).map_err(|e| describe(&e))?;
    match move_over_placeholder(src, &dest) {
        Ok(()) => Ok(dest),
        Err(e) => {
            // 확보해 둔 빈 자리표시 파일만 치운다(사용자 파일 아님).
            let _ = fs::remove_file(&dest);
            Err(describe(&e))
        }
    }
}

/// 되돌리기: 옮긴 파일을 원래 폴더·원래 이름으로. 그 자리에 다른 파일이 생겼으면 번호를 붙인다.
pub fn move_back(moved: &Path, original: &Path) -> Result<PathBuf, String> {
    let dir = original.parent().ok_or("원래 폴더를 알 수 없어요.")?;
    let name = original
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or("원래 이름을 알 수 없어요.")?;
    if !dir.is_dir() {
        return Err("원래 폴더가 없어져서 되돌릴 수 없어요.".into());
    }
    move_into(moved, dir, Some(&name))
}

/// `dest`(우리가 만든 빈 자리표시 파일)를 `src`로 바꿔 놓는다.
fn move_over_placeholder(src: &Path, dest: &Path) -> io::Result<()> {
    match fs::rename(src, dest) {
        Ok(()) => Ok(()),
        Err(e) if is_cross_device(&e) => {
            fs::copy(src, dest)?;
            if let Err(remove_err) = fs::remove_file(src) {
                // 원본이 그대로 있으니, 방금 만든 복사본을 치운다.
                let _ = fs::remove_file(dest);
                return Err(remove_err);
            }
            Ok(())
        }
        Err(e) => Err(e),
    }
}

fn is_cross_device(e: &io::Error) -> bool {
    // Windows ERROR_NOT_SAME_DEVICE = 17, 리눅스 EXDEV = 18
    matches!(e.raw_os_error(), Some(17) if cfg!(windows))
        || matches!(e.raw_os_error(), Some(18) if cfg!(unix))
        || e.kind() == io::ErrorKind::CrossesDevices
}

fn describe(e: &io::Error) -> String {
    // Windows ERROR_SHARING_VIOLATION = 32, ERROR_LOCK_VIOLATION = 33
    if cfg!(windows) && matches!(e.raw_os_error(), Some(32) | Some(33)) {
        return "파일이 다른 프로그램에서 열려 있어요. 닫고 다시 눌러 주세요.".into();
    }
    match e.kind() {
        io::ErrorKind::PermissionDenied => "이 폴더에 넣을 권한이 없어요.".into(),
        io::ErrorKind::NotFound => "파일이나 폴더를 찾을 수 없어요.".into(),
        _ => format!("옮기지 못했어요: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let downloads = dir.path().join("Downloads");
        let target = dir.path().join("정리 폴더").join("평가");
        fs::create_dir_all(&downloads).unwrap();
        fs::create_dir_all(&target).unwrap();
        (dir, downloads, target)
    }

    #[test]
    fn moves_file_into_folder() {
        let (_d, downloads, target) = setup();
        let src = downloads.join("채점기준.hwp");
        fs::write(&src, b"hwp").unwrap();
        let dest = move_into(&src, &target, None).unwrap();
        assert_eq!(dest, target.join("채점기준.hwp"));
        assert!(!src.exists());
        assert_eq!(fs::read(&dest).unwrap(), b"hwp");
    }

    #[test]
    fn never_overwrites_and_numbers_instead() {
        let (_d, downloads, target) = setup();
        fs::write(target.join("a.pdf"), b"old").unwrap();
        fs::write(target.join("a (2).pdf"), b"old2").unwrap();
        let src = downloads.join("a.pdf");
        fs::write(&src, b"new").unwrap();
        let dest = move_into(&src, &target, None).unwrap();
        assert_eq!(dest, target.join("a (3).pdf"));
        assert_eq!(fs::read(target.join("a.pdf")).unwrap(), b"old");
        assert_eq!(fs::read(target.join("a (2).pdf")).unwrap(), b"old2");
        assert_eq!(fs::read(dest).unwrap(), b"new");
    }

    #[test]
    fn rename_while_moving() {
        let (_d, downloads, target) = setup();
        let src = downloads.join("scan_001.pdf");
        fs::write(&src, b"x").unwrap();
        let dest = move_into(&src, &target, Some("가정통신문.pdf")).unwrap();
        assert_eq!(dest, target.join("가정통신문.pdf"));
        assert!(move_into(&dest, &target, Some("나쁜/이름.pdf")).is_err());
        assert!(dest.exists());
    }

    #[test]
    fn move_back_restores_original_name_and_numbers_if_taken() {
        let (_d, downloads, target) = setup();
        let original = downloads.join("a.pdf");
        fs::write(&original, b"mine").unwrap();
        let moved = move_into(&original, &target, Some("바뀐 이름.pdf")).unwrap();
        // 그사이 같은 이름 파일이 다시 받아짐
        fs::write(&original, b"other").unwrap();
        let back = move_back(&moved, &original).unwrap();
        assert_eq!(back, downloads.join("a (2).pdf"));
        assert_eq!(fs::read(&back).unwrap(), b"mine");
        assert_eq!(fs::read(&original).unwrap(), b"other");
    }

    #[test]
    fn missing_source_or_folder_is_an_error_and_leaves_nothing_behind() {
        let (_d, downloads, target) = setup();
        assert!(move_into(&downloads.join("없음.pdf"), &target, None).is_err());
        let src = downloads.join("a.pdf");
        fs::write(&src, b"x").unwrap();
        assert!(move_into(&src, &target.join("없는 폴더"), None).is_err());
        assert!(src.exists());
        assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
    }

    #[test]
    fn split_name_keeps_last_extension() {
        assert_eq!(split_name("보고서.final.hwp"), ("보고서.final", ".hwp"));
        assert_eq!(split_name("README"), ("README", ""));
        assert_eq!(split_name(".env"), (".env", ""));
    }
}
