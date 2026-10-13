//! 走査のスクリプトが、検索の権限のない root_dir を、権限がないとして答えること（REQ-scan-015・
//! REQ-paths-020）。root_dir を確かめる部分は、REMOTE_MERGE_TEST_SCAN_IMAGE のコンテナで、uid 1000 の
//! 利用者として動かす（手元の試験を root で動かしていても権限が効く。BusyBox のように find が走査に
//! 使えないコンテナでも確かめられるよう、root_dir を確かめる部分だけを動かす。docs/remote-tools.md）
use remote_merge_core::scan::raw::{Depth, ReadCause, Unreached};

use crate::scan_script::{root_setup, ScriptQuestion};
use crate::scan_test_shell::container_shell_as_user;

/// コンテナの中だけに置いた root_dir の権限を `mode` にして、root_dir を確かめる部分の標準出力
fn root_check_with_mode(mode: &str) -> Option<Vec<u8>> {
    let root = "/tmp/rmscan/root";
    let script = format!(
        "mkdir -p {root}/d && chmod {mode} {root}\n{}printf '@OK\\000'\n",
        root_setup(root.as_bytes())
    );
    Some(container_shell_as_user(&script)?.output().unwrap().stdout)
}

// @kotowari[REQ-scan-015, REQ-paths-020]
#[test]
fn a_root_dir_without_search_permission_is_answered_as_permission_denied_by_the_shell() {
    for mode in ["000", "644"] {
        let Some(stdout) = root_check_with_mode(mode) else {
            return;
        };
        assert_eq!(
            stdout,
            b"@F\0P\0",
            "{mode}: {}",
            String::from_utf8_lossy(&stdout)
        );
    }
    assert_eq!(root_check_with_mode("755").unwrap(), b"@OK\0");
}

// @kotowari[REQ-scan-015, REQ-paths-020]
#[test]
fn the_answer_for_a_root_dir_without_search_permission_is_permission_denied() {
    let questions = [ScriptQuestion {
        path: b"",
        depth: Depth::All,
    }];
    let stdout = [crate::scan_script::MARKER.as_bytes(), b"\0@F\0P\0"].concat();
    let parsed = crate::scan_output::parse(&stdout, b"", &questions).unwrap();
    assert_eq!(
        parsed.answers[0].entry.as_ref().map(|_| ()),
        Err(&Unreached::Unreadable(ReadCause::PermissionDenied))
    );
}
