// tests/cli_exit_code.rs
//
// CLI exit code の integration test。
// try_main() wrapper によりエラー時 exit code 2 が返ることを検証。

use std::io::Write;

fn remote_merge_cmd() -> std::process::Command {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_remote-merge"));
    // 環境変数をクリアして RUST_LOG 等の影響を排除
    cmd.env_clear();
    cmd
}

#[test]
fn test_exit_code_on_missing_config() {
    // 存在しない config ファイルを --config で指定 → exit code 2
    let output = remote_merge_cmd()
        .args(["--config", "/tmp/nonexistent-config-12345.toml", "status"])
        .output()
        .expect("failed to execute");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn test_exit_code_error_message_format() {
    // エラーメッセージが "Error: " を含むこと
    let output = remote_merge_cmd()
        .args(["--config", "/tmp/nonexistent-config-12345.toml", "status"])
        .output()
        .expect("failed to execute");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Error: "),
        "stderr should contain 'Error: ', got: {}",
        stderr
    );
}

#[test]
fn test_exit_code_on_invalid_toml() {
    // 不正な TOML ファイルを --config で指定 → exit code 2
    let dir = std::env::temp_dir().join("remote-merge-test-invalid-toml");
    let _ = std::fs::create_dir_all(&dir);
    let config_path = dir.join("invalid.toml");
    {
        let mut f = std::fs::File::create(&config_path).expect("failed to create temp config");
        f.write_all(b"this is not valid toml {{{{")
            .expect("failed to write");
    }

    let output = remote_merge_cmd()
        .args(["--config", config_path.to_str().unwrap(), "status"])
        .output()
        .expect("failed to execute");
    assert_eq!(
        output.status.code(),
        Some(2),
        "invalid TOML should exit with code 2, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // cleanup
    let _ = std::fs::remove_file(&config_path);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn test_exit_code_on_empty_config() {
    // 空の TOML ファイル（[local] セクション未設定）→ exit code 2
    let dir = std::env::temp_dir().join("remote-merge-test-empty-config");
    let _ = std::fs::create_dir_all(&dir);
    let config_path = dir.join("empty.toml");
    {
        let mut f = std::fs::File::create(&config_path).expect("failed to create temp config");
        f.write_all(b"# empty config\n").expect("failed to write");
    }

    let output = remote_merge_cmd()
        .args(["--config", config_path.to_str().unwrap(), "status"])
        .output()
        .expect("failed to execute");
    assert_eq!(
        output.status.code(),
        Some(2),
        "empty config (no [local]) should exit with code 2, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // cleanup
    let _ = std::fs::remove_file(&config_path);
    let _ = std::fs::remove_dir(&dir);
}
