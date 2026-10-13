//! 変更の途中で root_dir がなくなっていた失敗に、利用者に見せる root_dir を添える（走査の
//! "root_dir changed during the scan: サーバ名:パス" と同じ考え。
//! docs/decision/records/2026-10-12-foundation-3.md の D47）

use remote_merge_core::op_failure::{op_error_of, OpFailure, Prefixed};

/// `error` が root_dir がない失敗なら、文言の終わりに `shown_root`（リモートは "サーバ名:パス"、
/// ローカルはパス）を添える。種類はそのまま持つ。ほかの誤りは変えない
pub(crate) fn with_shown_root(error: anyhow::Error, shown_root: &str) -> anyhow::Error {
    match op_error_of(&error) {
        Some(op_error) if op_error.failure == OpFailure::RootGone => Prefixed {
            message: format!("{op_error}: {shown_root}"),
            error: op_error.clone(),
        }
        .into(),
        _ => error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use remote_merge_core::op_failure::{failure_of, OpError, Operation};

    // @kotowari[REQ-merge-042]
    #[test]
    fn a_root_dir_gone_during_a_change_is_shown_with_its_server_and_path() {
        let gone = OpError::new(Operation::Write, "sub/a.txt", OpFailure::RootGone).into();

        let error = with_shown_root(gone, "web:/srv/app");

        assert_eq!(
            error.to_string(),
            "cannot write sub/a.txt: root_dir is gone: web:/srv/app"
        );
        assert_eq!(failure_of(&error), Some(&OpFailure::RootGone));
    }

    #[test]
    fn other_failures_are_left_as_they_are() {
        let denied = OpError::new(Operation::Write, "a.txt", OpFailure::PermissionDenied).into();

        let error = with_shown_root(denied, "web:/srv/app");

        assert_eq!(error.to_string(), "cannot write a.txt: permission denied");
    }
}
