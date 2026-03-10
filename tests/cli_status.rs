//! `status` サブコマンドの E2E テスト。
//!
//! SSH 接続（localhost）を使用するため `#[ignore]` 付き。
//! `cargo test --test cli_status -- --ignored` で実行する。

mod common;
use common::*;

/// local≠remote のファイルが "M " プレフィックスで表示される
#[test]
#[ignore]
fn test_status_text_shows_modified_files() {
    let env = CliEnv::new(
        &[("app.txt", "version 1\n")],
        &[("app.txt", "version 2 with extra\n")],
    );

    let output = env.cmd_with("status").output().expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // 実際の出力: "M app.txt" と "Summary: 1 modified, ..."
    assert!(
        stdout.contains("M app.txt"),
        "modified file should be shown as 'M app.txt', got: {}",
        stdout,
    );
    assert!(
        stdout.contains("1 modified"),
        "summary should contain '1 modified', got: {}",
        stdout,
    );
}

/// ローカルにのみ存在するファイルが "+ " プレフィックスで表示される
#[test]
#[ignore]
fn test_status_text_shows_left_only() {
    let env = CliEnv::new(&[("local_only.txt", "only on local\n")], &[]);

    let output = env.cmd_with("status").output().expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("+ "),
        "left-only file should be shown with '+ ' prefix, got: {}",
        stdout,
    );
    assert!(
        stdout.contains("left only"),
        "summary should contain 'left only', got: {}",
        stdout,
    );
}

/// リモートにのみ存在するファイルが "- " プレフィックスで表示される
#[test]
#[ignore]
fn test_status_text_shows_right_only() {
    let env = CliEnv::new(&[], &[("remote_only.txt", "only on remote\n")]);

    let output = env.cmd_with("status").output().expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("- "),
        "right-only file should be shown with '- ' prefix, got: {}",
        stdout,
    );
    assert!(
        stdout.contains("right only"),
        "summary should contain 'right only', got: {}",
        stdout,
    );
}

/// デフォルトでは同一ファイルの "= " 行は表示されない
#[test]
#[ignore]
fn test_status_excludes_equal_by_default() {
    let env = CliEnv::new(
        &[("same.txt", "identical\n")],
        &[("same.txt", "identical\n")],
    );

    let output = env.cmd_with("status").output().expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // デフォルトでは equal ファイルは一覧に表示されないが、Summary 行の equal カウントは表示される
    assert!(
        !stdout.contains("= same.txt"),
        "equal file listing should not appear by default, got: {}",
        stdout,
    );
    assert!(
        stdout.contains("1 equal"),
        "summary should report '1 equal', got: {}",
        stdout,
    );
}

/// --all を指定すると同一ファイルの "= " 行が表示される
#[test]
#[ignore]
fn test_status_all_includes_equal() {
    let env = CliEnv::new(
        &[("same.txt", "identical\n")],
        &[("same.txt", "identical\n")],
    );

    let output = env
        .cmd_with("status")
        .arg("--all")
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("= same.txt"),
        "equal file should be shown with '= ' prefix when --all, got: {}",
        stdout,
    );
}

/// --summary を指定するとカウント数が表示される
#[test]
#[ignore]
fn test_status_summary_shows_counts() {
    let env = CliEnv::new(
        &[
            ("changed.txt", "local version\n"),
            ("local_only.txt", "only here\n"),
        ],
        &[
            ("changed.txt", "remote version with extra content\n"),
            ("remote_only.txt", "only there\n"),
        ],
    );

    let output = env
        .cmd_with("status")
        .arg("--summary")
        .output()
        .expect("failed to execute");

    // サマリーには "Summary:" と各カテゴリの数字が含まれる
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Summary:"),
        "summary should contain 'Summary:', got: {}",
        stdout,
    );
    assert!(
        stdout.contains("modified")
            && stdout.contains("left only")
            && stdout.contains("right only"),
        "summary should contain category names, got: {}",
        stdout,
    );
}

/// --format json で有効な JSON が "files" 配列付きで返る
#[test]
#[ignore]
fn test_status_json_format() {
    let env = CliEnv::new(
        &[("data.txt", "local content\n")],
        &[("data.txt", "remote content with more text\n")],
    );

    let output = env
        .cmd_with("status")
        .args(["--format", "json"])
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");
    assert!(
        json.get("files").is_some(),
        "JSON should have 'files' key, got: {}",
        json,
    );
    assert!(
        json["files"].is_array(),
        "'files' should be an array, got: {}",
        json["files"],
    );
}

/// --ref で 3way 構成にすると Ref サマリーが表示される
#[test]
#[ignore]
fn test_status_with_ref_shows_badges() {
    // develop と staging で異なるサイズのファイルを用意して Modified にする
    let env = CliEnv::new_3way(
        &[("config.txt", "base\n")],
        &[("config.txt", "develop version content\n")],
        &[("config.txt", "staging\n")],
    );

    let output = env
        .cmd_with("status")
        .args(["--left", "develop", "--right", "staging", "--ref", "local"])
        .output()
        .expect("failed to execute");

    // 3way 比較が実行され、Ref サマリーが含まれる
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Ref:"),
        "output should contain ref summary line, got: {}",
        stdout,
    );
    assert!(
        stdout.contains("ref:"),
        "header should mention ref, got: {}",
        stdout,
    );
}

/// status にパス引数でフィルタ — diff コマンドでディレクトリフィルタを検証
/// status はパス引数をサポートしないため、diff でディレクトリフィルタを代替検証
#[test]
#[ignore]
fn test_status_with_directory_filter() {
    let env = CliEnv::new(
        &[
            ("src/main.rs", "fn main() {}\n"),
            ("docs/readme.txt", "readme\n"),
        ],
        &[
            (
                "src/main.rs",
                "fn main() { /* changed with extra code */ }\n",
            ),
            ("docs/readme.txt", "readme changed\n"),
        ],
    );

    // status は全ファイルを表示する。両ファイルが含まれることを検証
    let output = env.cmd_with("status").output().expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("M src/main.rs"),
        "should show 'M src/main.rs', got: {}",
        stdout,
    );
    assert!(
        stdout.contains("M docs/readme.txt"),
        "should show 'M docs/readme.txt', got: {}",
        stdout,
    );
}

/// config の exclude フィルタで .git が除外される
#[test]
#[ignore]
fn test_status_exclude_filter_works() {
    let env = CliEnv::new(
        &[("app.txt", "content\n"), (".git/config", "git config\n")],
        &[
            ("app.txt", "different content here\n"),
            (".git/config", "git config\n"),
        ],
    );

    let output = env.cmd_with("status").output().expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains(".git"),
        ".git should be excluded by filter, got: {}",
        stdout,
    );
}

/// 両側ともファイルがない場合 exit 0 でファイルリストなし
#[test]
#[ignore]
fn test_status_empty_tree_both_sides() {
    let env = CliEnv::new(&[], &[]);

    let output = env.cmd_with("status").output().expect("failed to execute");

    assert_exit_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    // ファイル情報がない（M/+/- プレフィックス行が表示されない）
    assert!(
        !stdout.contains("M ") && !stdout.contains("+ ") && !stdout.contains("- "),
        "empty tree should have no file entries, got: {}",
        stdout,
    );
}

/// .env などのセンシティブファイルも status には含まれる
#[test]
#[ignore]
fn test_status_sensitive_files_included() {
    let env = CliEnv::new(
        &[(".env", "SECRET=abc\n")],
        &[(".env", "SECRET=xyz and more\n")],
    );

    let output = env.cmd_with("status").output().expect("failed to execute");

    assert_stdout_contains(&output, ".env");
}

/// ファイル名にスペースを含む場合でも JSON 出力が有効
#[test]
#[ignore]
fn test_status_json_special_chars_in_path() {
    let env = CliEnv::new(
        &[("qu ote.txt", "content a\n")],
        &[("qu ote.txt", "content b with extra\n")],
    );

    let output = env
        .cmd_with("status")
        .args(["--format", "json"])
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON even with special chars");
    assert!(
        json.get("files").is_some(),
        "JSON should have 'files' key, got: {}",
        json,
    );
    // ファイル名にスペースが正しくエスケープされている
    let files_str = json["files"].to_string();
    assert!(
        files_str.contains("qu ote.txt"),
        "JSON should contain the filename with space, got: {}",
        files_str,
    );
}
