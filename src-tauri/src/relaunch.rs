//! 업데이트 뒤 다시 켜질 때(SPEC 7장).
//!
//! 설치 프로그램은 업데이트를 마치면 프로그램을 **처음 켜졌을 때와 같은 인자로** 다시 켠다.
//! 컴퓨터를 켤 때 자동 실행(`--autostart`)된 프로그램에서 업데이트하면 다시 켜질 때도 `--autostart`가 붙어
//! 트레이로만 시작했다(사용자 보고: 업데이트하면 창이 다시 안 켜짐).
//! 그래서 업데이트 직전에 표시를 남기고, 켜질 때 그 표시를 본다.
//! - [지금 업데이트]를 눌렀으면 창을 보인다.
//! - 자동 업데이트(창을 쓰지 않을 때 뒤에서 설치)면 창은 띄우지 않고, 다음에 창을 열 때 "업데이트했어요"만 알린다.
//!
//! 표시에는 새 버전을 적어 두어, 설치가 안 된 채 다음에 켜질 때는 무시한다.

use std::path::Path;

const MARK_FILE: &str = "show-after-update";
/// 표시 둘째 줄: 자동 업데이트(창을 띄우지 않음)
const QUIET: &str = "quiet";

/// 업데이트 뒤 다시 켜진 경우
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relaunched {
    /// [지금 업데이트]를 눌러 설치: 창을 보인다
    Shown,
    /// 자동 업데이트: 창은 그대로(트레이), 다음에 창을 열 때 알린다
    Quiet,
}

/// 업데이트를 설치하기 직전: 새 버전 `version`으로 다시 켜지면 어떻게 할지 표시한다.
pub fn mark(data_dir: &Path, version: &str, quiet: bool) -> std::io::Result<()> {
    let body = if quiet {
        format!("{}\n{QUIET}", version.trim())
    } else {
        version.trim().to_string()
    };
    std::fs::write(data_dir.join(MARK_FILE), body)
}

/// 켜질 때: 표시가 있고 지금 버전과 같으면 어떻게 다시 켜졌는지. 표시는 늘 지운다(한 번만 쓰임).
pub fn take(data_dir: &Path, current_version: &str) -> Option<Relaunched> {
    let path = data_dir.join(MARK_FILE);
    let body = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    let mut lines = body.lines();
    if lines.next()?.trim() != current_version {
        return None;
    }
    Some(if lines.next().map(str::trim) == Some(QUIET) {
        Relaunched::Quiet
    } else {
        Relaunched::Shown
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_once_after_the_update_is_installed() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(take(d.path(), "0.1.25"), None);
        mark(d.path(), "0.1.25", false).unwrap();
        assert_eq!(take(d.path(), "0.1.25"), Some(Relaunched::Shown));
        // 한 번만
        assert_eq!(take(d.path(), "0.1.25"), None);
    }

    #[test]
    fn automatic_update_stays_quiet() {
        let d = tempfile::tempdir().unwrap();
        mark(d.path(), "0.1.35", true).unwrap();
        assert_eq!(take(d.path(), "0.1.35"), Some(Relaunched::Quiet));
    }

    #[test]
    fn ignores_mark_when_the_update_did_not_install() {
        let d = tempfile::tempdir().unwrap();
        mark(d.path(), "0.1.25", false).unwrap();
        // 설치가 안 되어 예전 버전으로 켜짐: 창을 띄우지 않고 표시만 지운다
        assert_eq!(take(d.path(), "0.1.24"), None);
        assert!(!d.path().join(MARK_FILE).exists());
    }
}
