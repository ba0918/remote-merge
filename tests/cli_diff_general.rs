#![cfg(unix)]

mod common;
use common::*;

/// 異なるファイルの diff で unified diff 形式（+/- 行）が出力される
#[test]
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

/// 同一ファイルの diff は exit 0 で差分なし
#[test]
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

/// 複数ファイルを指定した diff で両方のファイルの差分が表示される
#[test]
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
    assert!(
        stdout.contains("aaa local") && stdout.contains("aaa remote"),
        "{output:?}"
    );
    assert!(
        stdout.contains("bbb local") && stdout.contains("bbb remote"),
        "{output:?}"
    );
}

/// ディレクトリ指定の diff で配下全ファイルの差分が表示される
#[test]
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
    assert!(
        stdout.contains("fn main() { changed }") && stdout.contains("pub fn lib() { changed }"),
        "{output:?}"
    );
}

/// 3way diff で --ref を指定するとリファレンス差分情報が表示される
#[test]
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
    assert!(
        combined.contains("local ref version") && combined.contains("staging"),
        "{output:?}"
    );
}

/// 大きなファイルに --max-lines を指定すると出力が制限される
#[test]
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
    assert!(stdout.contains("-local line 0"), "{output:?}");
    assert!(stdout.contains("truncated"), "{output:?}");
}

/// 存在しないファイルの diff でエラーが返る
#[test]
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
fn test_diff_empty_file() {
    let env = CliEnv::new(&[("empty.txt", "")], &[("empty.txt", "")]);

    let output = env
        .cmd_with("diff")
        .arg("empty.txt")
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);
}

/// "." パスがフルツリー diff に解決される（d62cd21 の回帰テスト）
#[test]
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
