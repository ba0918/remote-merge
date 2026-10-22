#![cfg(unix)]
//! diff が比べる流れの CLI の契約テスト（diff_flow_cli・diff_flow_kinds_cli）の道具。

use std::process::Output;

use serde_json::Value;

use super::common::CliEnv;

/// `CliEnv` の設定で `command` を起動する
pub(super) fn run(env: &CliEnv, command: &str, args: &[&str]) -> Output {
    env.cmd_with(command)
        .args(args)
        .output()
        .expect("failed to execute remote-merge")
}

/// `json` の errors の、`path` の項目の reason
pub(super) fn error_reason(json: &Value, path: &str) -> String {
    json["errors"]
        .as_array()
        .and_then(|errors| errors.iter().find(|error| error["path"] == path))
        .unwrap_or_else(|| panic!("{path} not in errors: {json}"))["reason"]
        .as_str()
        .unwrap()
        .to_string()
}
