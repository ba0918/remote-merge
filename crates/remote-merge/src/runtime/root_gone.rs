//! 変更の途中で root_dir がなくなっていた失敗に、利用者に見せる root_dir を添え、その書き込み先への
//! 処理を止める（走査の "root_dir changed during the scan: サーバ名:パス" と同じ考え。
//! docs/decision/records/2026-10-12-foundation-3.md の D47）

use remote_merge_core::op_failure::{failure_of, op_error_of, OpFailure, Prefixed};

use super::core::CoreRuntime;
use super::route_retry::StopKey;

/// 変更の失敗 `error` が root_dir がないものなら、`shown_root` を添え、`key` への処理を止める
/// （残りのファイルは行わない）。ほかの誤りは変えない
pub(crate) fn stop_if_root_gone(
    runtime: &mut CoreRuntime,
    key: StopKey,
    error: anyhow::Error,
    shown_root: &str,
) -> anyhow::Error {
    let error = with_shown_root(error, shown_root);
    if failure_of(&error) == Some(&OpFailure::RootGone) {
        runtime.stop_for_root_gone(key, &error);
    }
    error
}

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
    use remote_merge_core::op_failure::{OpError, Operation};

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

    // ローカルの側も、root_dir がなくなったらその側への処理を止め、残りは行わなかったものにする
    // @kotowari[REQ-merge-044]
    #[test]
    fn a_local_root_dir_gone_during_a_change_stops_the_local_side() {
        use crate::runtime::local_target::LocalTargetIo;
        use crate::runtime::target_io::TargetIo;
        use remote_merge_core::relative_path::EntryPath;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let mut runtime = CoreRuntime::new_for_test();
        runtime.config.local.root_dir = root.clone();
        let mut io = LocalTargetIo::from_runtime(&runtime);

        let error = io
            .write_file_bytes(&mut runtime, &EntryPath::parse("a.txt").unwrap(), b"x")
            .unwrap_err();

        let gone = format!("root_dir is gone: {}", root.display());
        assert!(error.to_string().ends_with(&gone), "{error}");
        assert!(runtime.has_stopped());
        let stopped = runtime
            .stopped_error_for(&crate::side::Side::Local)
            .unwrap()
            .to_string();
        assert!(stopped.starts_with("not attempted"), "{stopped}");
        assert!(!stopped.contains(&gone), "{stopped}");
        assert!(!root.exists());
    }

    #[test]
    fn other_failures_are_left_as_they_are() {
        let denied = OpError::new(Operation::Write, "a.txt", OpFailure::PermissionDenied).into();

        let error = with_shown_root(denied, "web:/srv/app");

        assert_eq!(error.to_string(), "cannot write a.txt: permission denied");
    }
}
