#![cfg(unix)]
//! "../" を含む exclude のパターンを設定の読み込み時に一度だけ警告して捨て、走査では当てず警告も
//! 出さないこと（docs/ir/config/filters.md の REQ-config-038）の契約テスト。
//!
//! 警告は標準エラーに出るので、実行ファイルを試験 SSH サーバに対して
//! `status --left local --right develop` で起動する。左右の走査がどちらも多くの項目を通るように
//! ファイルを置き、走査のたびに警告が出れば数が増えるようにする。

use std::fs;

use super::common::CliEnv;

const PATTERN: &str = "../outside";
const FILES: [(&str, &str); 4] = [
    ("a.txt", "same\n"),
    ("dir/b.txt", "same\n"),
    ("dir/c.txt", "same\n"),
    ("dir/sub/d.txt", "same\n"),
];

// @kotowari[REQ-config-038]
#[test]
fn req_config_038_a_suspicious_exclude_pattern_is_warned_once_and_not_applied() {
    let env = CliEnv::new(&FILES, &FILES);
    let config = fs::read_to_string(&env.config_path).unwrap();
    let replaced = config.replace(
        "exclude = [\".git\", \"target\"]",
        &format!("exclude = [{PATTERN:?}, \"*.log\"]"),
    );
    assert_ne!(replaced, config, "fixture exclude line missing");
    fs::write(&env.config_path, replaced).unwrap();

    let output = env
        .cmd_with("status")
        .args([
            "--left", "local", "--right", "develop", "--all", "--format", "json",
        ])
        .output()
        .unwrap();

    let warning = format!("Skipping suspicious exclude pattern containing '../': {PATTERN}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.matches(&warning).count(), 1, "{stderr}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {output:?}"));
    assert_eq!(
        json["files"].as_array().unwrap().len(),
        FILES.len(),
        "{json}"
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}
