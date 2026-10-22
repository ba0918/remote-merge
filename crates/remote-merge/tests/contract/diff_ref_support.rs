#![cfg(unix)]
//! diff の --ref の CLI の契約テスト（diff_ref_cli・diff_ref_kinds・diff_ref_scope_cli・
//! diff_conflict_count）の道具。

use std::process::Output;

use super::common::CliEnv;

/// diff を起動する
pub(super) fn diff(env: &CliEnv, args: &[&str]) -> Output {
    env.cmd_with("diff")
        .args(args)
        .output()
        .expect("failed to execute diff")
}

/// 標準出力の文字
pub(super) fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}
