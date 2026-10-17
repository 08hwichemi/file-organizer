//! 업데이트 뒤 다시 켜질 때 메인 창 보이기(SPEC 7장).
//!
//! 설치 프로그램은 업데이트를 마치면 프로그램을 **처음 켜졌을 때와 같은 인자로** 다시 켠다.
//! 컴퓨터를 켤 때 자동 실행(`--autostart`)된 프로그램에서 업데이트하면 다시 켜질 때도 `--autostart`가 붙어
//! 트레이로만 시작했다(사용자 보고: 업데이트하면 창이 다시 안 켜짐).
//! 그래서 업데이트 직전에 "다음에 켜질 때 창 보이기" 표시를 남기고, 켜질 때 그 표시가 있으면 창을 보인다.
//! 표시에는 새 버전을 적어 두어, 설치가 안 된 채 다음에 자동 실행될 때는 창을 띄우지 않는다.

use std::path::Path;

const MARK_FILE: &str = "show-after-update";

/// 업데이트를 설치하기 직전: 새 버전 `version`으로 다시 켜지면 창을 보이게 표시한다.
pub fn mark(data_dir: &Path, version: &str) -> std::io::Result<()> {
    std::fs::write(data_dir.join(MARK_FILE), version.trim())
}

/// 켜질 때: 표시가 있고 지금 버전과 같으면 `true`. 표시는 늘 지운다(한 번만 쓰임).
pub fn take(data_dir: &Path, current_version: &str) -> bool {
    let path = data_dir.join(MARK_FILE);
    let Ok(version) = std::fs::read_to_string(&path) else {
        return false;
    };
    let _ = std::fs::remove_file(&path);
    version.trim() == current_version
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_once_after_the_update_is_installed() {
        let d = tempfile::tempdir().unwrap();
        assert!(!take(d.path(), "0.1.25"));
        mark(d.path(), "0.1.25").unwrap();
        assert!(take(d.path(), "0.1.25"));
        // 한 번만
        assert!(!take(d.path(), "0.1.25"));
    }

    #[test]
    fn ignores_mark_when_the_update_did_not_install() {
        let d = tempfile::tempdir().unwrap();
        mark(d.path(), "0.1.25").unwrap();
        // 설치가 안 되어 예전 버전으로 켜짐: 창을 띄우지 않고 표시만 지운다
        assert!(!take(d.path(), "0.1.24"));
        assert!(!d.path().join(MARK_FILE).exists());
    }
}
