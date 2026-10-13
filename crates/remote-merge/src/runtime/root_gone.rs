//! 変更の途中で root_dir がなくなっていた失敗に、利用者に見せる root_dir を添える（走査の
//! "root_dir changed during the scan: サーバ名:パス" と同じ考え。
//! docs/decision/records/2026-10-12-foundation-3.md の D47）。止めるのは `root_watch`

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
    use crate::runtime::core::CoreRuntime;
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

    // root_dir の祖先が通常ファイルに替わったときも、確かめ直しで root_dir がないとして止める（D48）
    // @kotowari[REQ-merge-044]
    #[test]
    fn a_root_dir_whose_ancestor_became_a_file_stops_the_side_on_a_removal() {
        use crate::runtime::target_io::for_side;
        use remote_merge_core::relative_path::EntryPath;
        let tmp = tempfile::tempdir().unwrap();
        let top = tmp.path().join("top");
        let root = top.join("root");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.txt"), "a").unwrap();
        let mut runtime = CoreRuntime::new_for_test();
        runtime.config.local.root_dir = root.clone();
        std::fs::remove_dir_all(&top).unwrap();
        std::fs::write(&top, "now a file").unwrap();
        let mut io = for_side(&crate::side::Side::Local, &runtime);

        let error = io
            .remove_file(&mut runtime, &EntryPath::parse("a.txt").unwrap())
            .unwrap_err();

        assert_eq!(failure_of(&error), Some(&OpFailure::RootGone), "{error}");
        assert!(runtime.has_stopped());
    }

    // ローカルの側も、root_dir がなくなったらその側への処理を止め、残りは行わなかったものにする
    // @kotowari[REQ-merge-044]
    #[test]
    fn a_local_root_dir_gone_during_a_change_stops_the_local_side() {
        use crate::runtime::target_io::for_side;
        use remote_merge_core::relative_path::EntryPath;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let mut runtime = CoreRuntime::new_for_test();
        runtime.config.local.root_dir = root.clone();
        let mut io = for_side(&crate::side::Side::Local, &runtime);

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

        // 止めた後は、手順の側の確かめによらず、比較先そのものが何も行わない（D49）
        std::fs::create_dir(&root).unwrap();
        let mut io = for_side(&crate::side::Side::Local, &runtime);
        let refused = io
            .write_file_bytes(&mut runtime, &EntryPath::parse("b.txt").unwrap(), b"x")
            .unwrap_err();
        assert!(
            refused.to_string().starts_with("not attempted"),
            "{refused}"
        );
        assert!(!root.join("b.txt").exists());
    }

    #[test]
    fn other_failures_are_left_as_they_are() {
        let denied = OpError::new(Operation::Write, "a.txt", OpFailure::PermissionDenied).into();

        let error = with_shown_root(denied, "web:/srv/app");

        assert_eq!(error.to_string(), "cannot write a.txt: permission denied");
    }
}
