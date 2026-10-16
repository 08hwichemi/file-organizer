//! Everything(voidtools)으로 PC 전체 검색(SPEC 6장).
//!
//! 공식 SDK의 `Everything64.dll`(MIT 라이선스, 배포 때 함께 넣음)을 불러 SDK 함수만 쓴다. 역분석하지 않는다.
//! Everything이 꺼져 있거나 DLL이 없으면 `Unavailable`을 돌려주고, 부르는 쪽은 자체 색인으로 찾는다.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// Everything을 쓸 수 있는지(화면에 알리는 값). Windows와 그 밖에서 쓰는 값이 달라 일부는 한쪽에서만 쓴다.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Availability {
    /// 쓸 수 있음(PC 전체 검색)
    Ready,
    /// Everything이 설치·실행되어 있지 않음
    NotRunning,
    /// SDK DLL이 없음(설치 파일에 빠짐)
    NoDll,
    /// Windows가 아님(개발 환경)
    Unsupported,
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub path: PathBuf,
    pub size: u64,
    /// 고친 시각(1970년부터 밀리초)
    pub modified: i64,
}

/// Windows FILETIME(1601년부터 100나노초) → 1970년부터 밀리초
#[cfg_attr(not(windows), allow(dead_code))]
fn filetime_to_ms(ft: u64) -> i64 {
    const EPOCH_DIFF: u64 = 116_444_736_000_000_000;
    if ft < EPOCH_DIFF {
        return 0;
    }
    ((ft - EPOCH_DIFF) / 10_000) as i64
}

/// 휴지통·임시 폴더처럼 찾아 줄 필요 없는 곳
#[cfg_attr(not(windows), allow(dead_code))]
fn skip_path(lower: &str) -> bool {
    lower.contains("\\$recycle.bin\\") || lower.contains("\\appdata\\local\\temp\\")
}

#[cfg(windows)]
mod imp {
    use super::*;
    use libloading::Library;
    use std::sync::{Mutex, OnceLock};

    // SDK(Everything.h)의 값
    const REQUEST_FULL_PATH_AND_FILE_NAME: u32 = 0x0000_0004;
    const REQUEST_SIZE: u32 = 0x0000_0010;
    const REQUEST_DATE_MODIFIED: u32 = 0x0000_0040;
    const SORT_DATE_MODIFIED_DESCENDING: u32 = 14;
    const ERROR_IPC: u32 = 2;

    type SetSearchW = unsafe extern "system" fn(*const u16);
    type SetU32 = unsafe extern "system" fn(u32);
    type QueryW = unsafe extern "system" fn(i32) -> i32;
    type GetU32 = unsafe extern "system" fn() -> u32;
    type GetFullPath = unsafe extern "system" fn(u32, *mut u16, u32) -> u32;
    type GetU64 = unsafe extern "system" fn(u32, *mut u64) -> i32;
    type IsFile = unsafe extern "system" fn(u32) -> i32;

    /// SDK는 한 번에 하나의 검색만 다룬다(전역 상태) → 잠금으로 차례대로.
    static LIB: OnceLock<Mutex<Option<Library>>> = OnceLock::new();

    pub fn search(dll: &Path, query: &str, max: u32) -> Result<Vec<Hit>, Availability> {
        let lock = LIB.get_or_init(|| Mutex::new(None));
        let mut guard = lock.lock().map_err(|_| Availability::NotRunning)?;
        if guard.is_none() {
            if !dll.is_file() {
                return Err(Availability::NoDll);
            }
            // SAFETY: 설치 폴더에 함께 넣은 공식 SDK DLL을 불러온다.
            *guard = Some(unsafe { Library::new(dll) }.map_err(|_| Availability::NoDll)?);
        }
        let lib = guard.as_ref().ok_or(Availability::NoDll)?;
        // SAFETY: 아래 이름·모양은 SDK의 Everything.h에 적힌 그대로다.
        unsafe {
            let set_search = lib
                .get::<SetSearchW>(b"Everything_SetSearchW\0")
                .map_err(|_| Availability::NoDll)?;
            let set_flags = lib
                .get::<SetU32>(b"Everything_SetRequestFlags\0")
                .map_err(|_| Availability::NoDll)?;
            let set_max = lib
                .get::<SetU32>(b"Everything_SetMax\0")
                .map_err(|_| Availability::NoDll)?;
            let set_sort = lib.get::<SetU32>(b"Everything_SetSort\0").ok();
            let query_w = lib
                .get::<QueryW>(b"Everything_QueryW\0")
                .map_err(|_| Availability::NoDll)?;
            let last_error = lib
                .get::<GetU32>(b"Everything_GetLastError\0")
                .map_err(|_| Availability::NoDll)?;
            let num = lib
                .get::<GetU32>(b"Everything_GetNumResults\0")
                .map_err(|_| Availability::NoDll)?;
            let full_path = lib
                .get::<GetFullPath>(b"Everything_GetResultFullPathNameW\0")
                .map_err(|_| Availability::NoDll)?;
            let size = lib.get::<GetU64>(b"Everything_GetResultSize\0").ok();
            let date = lib
                .get::<GetU64>(b"Everything_GetResultDateModified\0")
                .ok();
            let is_file = lib.get::<IsFile>(b"Everything_IsFileResult\0").ok();

            // 파일만(폴더 빼고). 띄어 쓴 낱말은 Everything에서도 "모두 포함".
            let text: Vec<u16> = format!("file: {query}")
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            set_search(text.as_ptr());
            set_flags(REQUEST_FULL_PATH_AND_FILE_NAME | REQUEST_SIZE | REQUEST_DATE_MODIFIED);
            set_max(max);
            if let Some(sort) = &set_sort {
                sort(SORT_DATE_MODIFIED_DESCENDING);
            }
            if query_w(1) == 0 {
                // 대부분 ERROR_IPC(Everything이 꺼져 있음). 그 밖의 오류도 자체 색인으로 넘긴다.
                let _ = last_error() == ERROR_IPC;
                return Err(Availability::NotRunning);
            }
            let mut out = Vec::new();
            let mut buf = vec![0u16; 32_768];
            for i in 0..num() {
                if let Some(f) = &is_file {
                    if f(i) == 0 {
                        continue;
                    }
                }
                let len = full_path(i, buf.as_mut_ptr(), buf.len() as u32) as usize;
                if len == 0 {
                    continue;
                }
                let path = String::from_utf16_lossy(&buf[..len.min(buf.len())]);
                if skip_path(&path.to_lowercase()) {
                    continue;
                }
                let mut s: u64 = 0;
                let mut d: u64 = 0;
                if let Some(g) = &size {
                    g(i, &mut s);
                }
                if let Some(g) = &date {
                    g(i, &mut d);
                }
                out.push(Hit {
                    path: PathBuf::from(path),
                    size: s,
                    modified: filetime_to_ms(d),
                });
            }
            Ok(out)
        }
    }
}

/// Everything으로 찾는다. 쓸 수 없으면 그 까닭.
pub fn search(dll: &Path, query: &str, max: u32) -> Result<Vec<Hit>, Availability> {
    #[cfg(windows)]
    {
        imp::search(dll, query, max)
    }
    #[cfg(not(windows))]
    {
        let _ = (dll, query, max);
        Err(Availability::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filetime_converts_to_unix_ms() {
        // 1970-01-01 00:00:00 UTC
        assert_eq!(filetime_to_ms(116_444_736_000_000_000), 0);
        // 그 1초 뒤
        assert_eq!(filetime_to_ms(116_444_736_010_000_000), 1000);
        assert_eq!(filetime_to_ms(0), 0);
    }

    #[test]
    fn skips_recycle_bin_and_temp() {
        assert!(skip_path("c:\\$recycle.bin\\s-1-5\\a.pdf"));
        assert!(skip_path("c:\\users\\t\\appdata\\local\\temp\\x.tmp"));
        assert!(!skip_path("c:\\users\\t\\documents\\a.pdf"));
    }

    #[cfg(not(windows))]
    #[test]
    fn unsupported_outside_windows() {
        assert_eq!(
            search(Path::new("Everything64.dll"), "a", 10).unwrap_err(),
            Availability::Unsupported
        );
    }
}
