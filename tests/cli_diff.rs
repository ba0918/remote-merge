#![cfg(unix)]
//! `diff` サブコマンドの E2E テスト。
//!
//! SSH 接続（localhost）を使用するため `#[ignore]` 付き。
//! `cargo test --test cli_diff -- --ignored` で実行する。

mod common;
use common::*;

/// 異なるファイルの diff で unified diff 形式（+/- 行）が出力される
#[test]
#[ignore]
fn test_diff_text_shows_unified_diff() {
    let env = CliEnv::new(
        &[("file.txt", "hello local\nline2\n")],
        &[("file.txt", "hello remote\nline2\n")],
    );

    let output = env
        .cmd_with("diff")
        .arg("file.txt")
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // 実際の出力: "--- a/file.txt (local)" / "+++ b/file.txt (develop)" / "-hello local" / "+hello remote"
    assert!(
        stdout.contains("--- a/file.txt") && stdout.contains("+++ b/file.txt"),
        "Expected unified diff headers, got: {}",
        stdout
    );
    assert!(
        stdout.contains("-hello local") && stdout.contains("+hello remote"),
        "Expected diff content lines, got: {}",
        stdout
    );
}

/// JSON フォーマットで diff を出力し、有効な JSON でありファイル情報を含む
#[test]
#[ignore]
fn test_diff_json_format() {
    let env = CliEnv::new(
        &[("file.txt", "local content\n")],
        &[("file.txt", "remote content\n")],
    );

    let output = env
        .cmd_with("diff")
        .args(["file.txt", "--format", "json"])
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // 有効な JSON であることを確認
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");
    // ファイルパス情報が含まれることを確認
    let json_str = parsed.to_string();
    assert!(
        json_str.contains("file.txt"),
        "JSON output should contain file path info, got: {}",
        json_str
    );
}

/// 同一ファイルの diff は exit 0 で差分なし
#[test]
#[ignore]
fn test_diff_equal_file() {
    let env = CliEnv::new(
        &[("file.txt", "same content\n")],
        &[("file.txt", "same content\n")],
    );

    let output = env
        .cmd_with("diff")
        .arg("file.txt")
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);
}

/// ローカルのみに存在するファイルの diff で削除行（-）が表示される
#[test]
#[ignore]
fn test_diff_left_only_file() {
    let env = CliEnv::new(&[("file.txt", "local only\n")], &[]);

    let output = env
        .cmd_with("diff")
        .arg("file.txt")
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // left-only ファイルは unified diff で `-` 行として表示される
    // 実際の出力: "--- a/file.txt (local)" / "+++ b/file.txt (develop)" / "-local only"
    assert!(
        stdout.contains("--- a/file.txt"),
        "Expected unified diff header for left-only file, got: {}",
        stdout
    );
    assert!(
        stdout.contains("-local only"),
        "Expected '-local only' deletion line, got: {}",
        stdout
    );
}

/// リモートのみに存在するファイルの diff で追加行（+）が表示される
#[test]
#[ignore]
fn test_diff_right_only_file() {
    let env = CliEnv::new(&[], &[("file.txt", "remote only\n")]);

    let output = env
        .cmd_with("diff")
        .arg("file.txt")
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // right-only ファイルは unified diff で `+` 行として表示される
    // 実際の出力: "--- a/file.txt (local)" / "+++ b/file.txt (develop)" / "+remote only"
    assert!(
        stdout.contains("+++ b/file.txt"),
        "Expected unified diff header for right-only file, got: {}",
        stdout
    );
    assert!(
        stdout.contains("+remote only"),
        "Expected '+remote only' addition line, got: {}",
        stdout
    );
}

/// バイナリファイルの diff で SHA-256 ハッシュが表示される
#[test]
#[ignore]
fn test_diff_binary_file() {
    let env = CliEnv::new(&[], &[]);
    // NUL バイトを含むバイナリファイルを配置
    let binary_content = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";
    place_binary_file(&env.local_dir, "image.png", binary_content);
    place_binary_file(
        &env.remote_dir,
        "image.png",
        b"\x89PNG\r\n\x1a\n\x00\x00\x00\rDIFF",
    );

    let output = env
        .cmd_with("diff")
        .arg("image.png")
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // 実際の出力: "Binary files differ (left: sha256=..., right: sha256=...)"
    assert!(
        stdout.contains("Binary files differ"),
        "Expected 'Binary files differ' in output, got: {}",
        stdout
    );
    assert!(
        stdout.contains("sha256="),
        "Expected 'sha256=' hash in output, got: {}",
        stdout
    );
}

/// シンボリックリンクの diff でリンクターゲット情報が表示される
#[test]
#[ignore]
fn test_diff_symlink() {
    let env = CliEnv::new(&[("target.txt", "target content\n")], &[]);
    place_symlink(&env.local_dir, "link.txt", "target.txt");
    // リモートにもリンク先ファイルとシンボリックリンクを配置（異なるターゲット）
    place_files(&env.remote_dir, &[("other.txt", "other\n")]);
    place_symlink(&env.remote_dir, "link.txt", "other.txt");

    let output = env
        .cmd_with("diff")
        .arg("link.txt")
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);
    // 実際の出力: "Symbolic link targets differ"
    assert!(
        combined.contains("Symbolic link targets differ"),
        "Expected 'Symbolic link targets differ' in output, got: {}",
        combined
    );
}

/// 機密ファイル (.env) の diff で内容が隠され、--force の案内が表示される
#[test]
#[ignore]
fn test_diff_sensitive_file_warning() {
    let env = CliEnv::new(
        &[(".env", "SECRET_KEY=abc123\n")],
        &[(".env", "SECRET_KEY=xyz789\n")],
    );

    let output = env
        .cmd_with("diff")
        .arg(".env")
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // 実際の出力: "Content hidden (sensitive file). Use --force to show."
    assert!(
        stdout.contains("Content hidden (sensitive file). Use --force to show."),
        "Expected 'Content hidden (sensitive file). Use --force to show.' in output, got: {}",
        stdout
    );
    // 機密内容は表示されないことを確認
    assert!(
        !stdout.contains("abc123") && !stdout.contains("xyz789"),
        "Sensitive content should not be shown without --force, got: {}",
        stdout
    );
}

/// 機密ファイルに --force を付けると内容が表示される
#[test]
#[ignore]
fn test_diff_sensitive_file_force() {
    let env = CliEnv::new(
        &[(".env", "SECRET_KEY=abc123\n")],
        &[(".env", "SECRET_KEY=xyz789\n")],
    );

    let output = env
        .cmd_with("diff")
        .args([".env", "--force"])
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // 実際の出力: "-SECRET_KEY=abc123" / "+SECRET_KEY=xyz789"
    assert!(
        stdout.contains("-SECRET_KEY=abc123") && stdout.contains("+SECRET_KEY=xyz789"),
        "With --force, sensitive diff content should be shown, got: {}",
        stdout
    );
}

/// 複数ファイルを指定した diff で両方のファイルの差分が表示される
#[test]
#[ignore]
fn test_diff_multiple_files() {
    let env = CliEnv::new(
        &[("a.txt", "aaa local\n"), ("b.txt", "bbb local\n")],
        &[("a.txt", "aaa remote\n"), ("b.txt", "bbb remote\n")],
    );

    let output = env
        .cmd_with("diff")
        .args(["a.txt", "b.txt"])
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("a.txt") && stdout.contains("b.txt"),
        "Expected both file names in output, got: {}",
        stdout
    );
}

/// ディレクトリ指定の diff で配下全ファイルの差分が表示される
#[test]
#[ignore]
fn test_diff_directory() {
    let env = CliEnv::new(
        &[
            ("src/main.rs", "fn main() {}\n"),
            ("src/lib.rs", "pub fn lib() {}\n"),
        ],
        &[
            ("src/main.rs", "fn main() { changed }\n"),
            ("src/lib.rs", "pub fn lib() { changed }\n"),
        ],
    );

    let output = env
        .cmd_with("diff")
        .arg("src/")
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("main.rs") && stdout.contains("lib.rs"),
        "Expected both files in directory diff, got: {}",
        stdout
    );
}

/// 3way diff で --ref を指定するとリファレンス差分情報が表示される
#[test]
#[ignore]
fn test_diff_with_ref() {
    let env = CliEnv::new_3way(
        &[("file.txt", "local ref version\n")],
        &[("file.txt", "develop version of the file\n")],
        &[("file.txt", "staging\n")],
    );

    let output = env
        .cmd_with("diff")
        .args([
            "file.txt", "--left", "develop", "--right", "staging", "--ref", "local",
        ])
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);
    // 実際の出力: "--- a/file.txt (develop)" / "+++ b/file.txt (staging)" /
    //   "--- ref:local:file.txt (reference diff vs left)" / "Conflicts: ..."
    assert!(
        combined.contains("--- a/file.txt (develop)")
            && combined.contains("+++ b/file.txt (staging)"),
        "Expected 3-way diff headers with server names, got: {}",
        combined
    );
    assert!(
        combined.contains("ref:local:file.txt"),
        "Expected reference diff section, got: {}",
        combined
    );
}

/// 大きなファイルに --max-lines を指定すると出力が制限される
#[test]
#[ignore]
fn test_diff_max_lines() {
    // 100行以上のファイルを生成
    let large_local: String = (0..120).map(|i| format!("local line {}\n", i)).collect();
    let large_remote: String = (0..120).map(|i| format!("remote line {}\n", i)).collect();
    let env = CliEnv::new(&[("big.txt", &large_local)], &[("big.txt", &large_remote)]);

    let output = env
        .cmd_with("diff")
        .args(["big.txt", "--max-lines", "10"])
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line_count = stdout.lines().count();
    // --max-lines 10 なので出力行数が制限されていることを確認
    // ヘッダー等含めて多少の余裕を見る
    assert!(
        line_count <= 30,
        "Expected output to be truncated with --max-lines 10, got {} lines: {}",
        line_count,
        stdout
    );
}

/// 存在しないファイルの diff でエラーが返る
#[test]
#[ignore]
fn test_diff_nonexistent_file() {
    let env = CliEnv::new(&[], &[]);

    let output = env
        .cmd_with("diff")
        .arg("nonexistent.txt")
        .output()
        .expect("failed to execute");

    // 実際の出力 (すべて stderr):
    //   "Warning: 'nonexistent.txt' not found on either side"
    //   "Error: specified path(s) not found on either side"
    //   exit code: 2
    assert!(
        !output.status.success(),
        "Expected non-zero exit for nonexistent file, got exit={:?}",
        output.status.code()
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not found on either side"),
        "Expected 'not found on either side' in stderr, got: {}",
        stderr
    );
    assert!(
        stderr.contains("specified path(s) not found on either side"),
        "Expected 'specified path(s) not found on either side' error in stderr, got: {}",
        stderr
    );
}

/// 0バイトファイルが両方に存在する場合は exit 0 (Equal)
#[test]
#[ignore]
fn test_diff_empty_file() {
    let env = CliEnv::new(&[("empty.txt", "")], &[("empty.txt", "")]);

    let output = env
        .cmd_with("diff")
        .arg("empty.txt")
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);
}

/// 末尾スラッシュの有無で同じ結果が得られる（パス正規化）
#[test]
#[ignore]
fn test_diff_trailing_slash_normalized() {
    let env = CliEnv::new(
        &[("src/app.rs", "fn app() {}\n")],
        &[("src/app.rs", "fn app() { changed }\n")],
    );

    let output_with_slash = env
        .cmd_with("diff")
        .arg("src/")
        .output()
        .expect("failed to execute");

    let output_without_slash = env
        .cmd_with("diff")
        .arg("src")
        .output()
        .expect("failed to execute");

    let stdout_with = String::from_utf8_lossy(&output_with_slash.stdout);
    let stdout_without = String::from_utf8_lossy(&output_without_slash.stdout);
    assert_eq!(
        stdout_with, stdout_without,
        "Trailing slash should not affect diff result"
    );
}

/// "." パスがフルツリー diff に解決される（d62cd21 の回帰テスト）
#[test]
#[ignore]
fn test_diff_dot_path_resolves() {
    let env = CliEnv::new(&[("file.txt", "local\n")], &[("file.txt", "remote\n")]);

    let output = env
        .cmd_with("diff")
        .arg(".")
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("file.txt") || stdout.contains('+') || stdout.contains('-'),
        "dot path should resolve to full tree diff, got: {}",
        stdout
    );
}

/// テキスト中に NUL バイトを含むファイルがバイナリとして検出され SHA-256 表示される
#[test]
#[ignore]
fn test_diff_null_bytes_detected_as_binary() {
    let env = CliEnv::new(&[], &[]);
    // テキストの途中に NUL バイトを含むファイル
    let content_with_null = b"hello\x00world\n";
    place_binary_file(&env.local_dir, "mixed.dat", content_with_null);
    place_binary_file(&env.remote_dir, "mixed.dat", b"hello\x00different\n");

    let output = env
        .cmd_with("diff")
        .arg("mixed.dat")
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // 実際の出力: "Binary files differ (left: sha256=..., right: sha256=...)"
    assert!(
        stdout.contains("Binary files differ"),
        "File with NUL bytes should be detected as binary with 'Binary files differ', got: {}",
        stdout
    );
    assert!(
        stdout.contains("sha256="),
        "Expected 'sha256=' hash for binary file, got: {}",
        stdout
    );
}
