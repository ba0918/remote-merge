//! 最も細かい指定でも、送信データと認証情報が診断ログ（docs/ir/cli/diagnostics.md）に残らないことの契約テスト。
//!
//! SSH の試験サーバーに password 認証でつなぎ、merge で中身を書き込む実行ファイルを起動する。

use std::fs;

use base64::Engine;

/// 書き込みで送る中身。診断ログに現れたら送信データが記録されたと分かる並び
const MERGED_CONTENT: &str = "diagnostics-must-not-keep-this-merged-content\n";

// @kotowari[REQ-cli-075, EX-cli-030]
#[test]
fn the_finest_log_level_keeps_sent_data_and_credentials_out_of_diagnostics() {
    let env = super::common::CliEnv::new(&[("file.txt", MERGED_CONTENT)], &[("file.txt", "old\n")]);

    let output = env
        .cmd()
        .args(["--log-level", "trace", "merge", "file.txt"])
        .args(["--left", "local", "--right", "develop"])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(env.remote_dir.join("file.txt")).unwrap(),
        MERGED_CONTENT
    );
    let saved =
        fs::read_to_string(env.temp_root().join("xdg-cache/remote-merge/debug.log")).unwrap();
    let encoded = base64::engine::general_purpose::STANDARD.encode(MERGED_CONTENT);
    let secrets = [
        ("the password", "fixture-password"),
        ("the merged content", MERGED_CONTENT.trim_end()),
        ("the merged content in base64", encoded.as_str()),
    ];
    let byte_arrays = decimal_byte_arrays(&saved);
    for (name, secret) in secrets {
        assert!(!saved.contains(secret), "diagnostic log contains {name}");
        assert!(
            !byte_arrays
                .iter()
                .any(|bytes| contains_bytes(bytes, secret.as_bytes())),
            "diagnostic log contains {name} as a byte array"
        );
    }
    let own_info = saved
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .any(|entry| {
            entry["level"] == "INFO"
                && entry["target"]
                    .as_str()
                    .is_some_and(|target| target.starts_with("remote_merge"))
        });
    assert!(
        own_info,
        "no information-level record of remote-merge was saved"
    );
}

/// `[1, 2, 3]` の形で記録された 10 進のバイト列をすべて取り出す
fn decimal_byte_arrays(text: &str) -> Vec<Vec<u8>> {
    text.split('[')
        .skip(1)
        .filter_map(|rest| {
            let inside = rest.split(']').next()?;
            let bytes: Option<Vec<u8>> = inside
                .split(',')
                .map(|number| number.trim().parse::<u8>().ok())
                .collect();
            bytes.filter(|bytes| !bytes.is_empty())
        })
        .collect()
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
