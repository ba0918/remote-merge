#![cfg(unix)]
//! merge が比べる流れの CLI の契約テスト（merge_flow_cli・merge_content_cli）の道具。

use std::process::Output;

use serde_json::Value;

use super::common::CliEnv;

/// `CliEnv` の設定で merge を起動する
pub(super) fn merge(env: &CliEnv, args: &[&str]) -> Output {
    env.cmd_with("merge")
        .args(args)
        .output()
        .expect("failed to execute merge")
}

/// `json` の `list` の項目のパス
pub(super) fn paths_in(json: &Value, list: &str) -> Vec<String> {
    json[list]
        .as_array()
        .unwrap_or_else(|| panic!("{list} missing: {json}"))
        .iter()
        .map(|entry| entry["path"].as_str().unwrap().to_string())
        .collect()
}

/// `json` の failed の、`path` の項目の error
pub(super) fn error_of(json: &Value, path: &str) -> String {
    json["failed"]
        .as_array()
        .and_then(|failed| failed.iter().find(|entry| entry["path"] == path))
        .unwrap_or_else(|| panic!("{path} not failed: {json}"))["error"]
        .as_str()
        .unwrap()
        .to_string()
}

/// local を左、develop を右にした JSON の出力の引数
pub(super) const LOCAL_TO_DEVELOP: [&str; 6] =
    ["--left", "local", "--right", "develop", "--format", "json"];
