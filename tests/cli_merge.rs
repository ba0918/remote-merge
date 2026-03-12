#![cfg(unix)]
//! `merge` サブコマンドの E2E テスト。
//!
//! SSH 接続（localhost）を使用するため `#[ignore]` 付き。
//! `cargo test --test cli_merge -- --ignored` で実行する。

mod common;
use common::*;

use std::fs;

/// dry-run でマージ計画が表示され、ファイルは変更されない
#[test]
#[ignore]
fn test_merge_dry_run_shows_plan() {
    let env = CliEnv::new(
        &[("file.txt", "local content\n")],
        &[("file.txt", "remote content\n")],
    );

    let before = fs::read_to_string(env.remote_dir.join("file.txt")).unwrap();

    let output = env
        .cmd_with("merge")
        .args([
            "file.txt",
            "--left",
            "local",
            "--right",
            "develop",
            "--dry-run",
        ])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);
    // 実際の出力: "Would merge: file.txt"
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Would merge:"),
        "Expected 'Would merge:' in dry-run output, got: {}",
        stdout
    );

    // ファイルが変更されていないことを確認
    let after = fs::read_to_string(env.remote_dir.join("file.txt")).unwrap();
    assert_eq!(before, after, "File should not be modified in dry-run mode");
}

/// merge 実行後にリモートファイルがローカルの内容に更新される
#[test]
#[ignore]
fn test_merge_writes_file() {
    let env = CliEnv::new(
        &[("file.txt", "local content\n")],
        &[("file.txt", "remote content\n")],
    );

    let output = env
        .cmd_with("merge")
        .args(["file.txt", "--left", "local", "--right", "develop"])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);

    // リモートファイルがローカルの内容に更新されていることを確認
    let remote_content = fs::read_to_string(env.remote_dir.join("file.txt")).unwrap();
    assert_eq!(
        remote_content, "local content\n",
        "Remote file should match local after merge"
    );
}

/// JSON フォーマットで merge 結果が有効な JSON として出力される
#[test]
#[ignore]
fn test_merge_json_format() {
    let env = CliEnv::new(
        &[("file.txt", "local content\n")],
        &[("file.txt", "remote content\n")],
    );

    let output = env
        .cmd_with("merge")
        .args([
            "file.txt", "--left", "local", "--right", "develop", "--format", "json",
        ])
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let _parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Merge output should be valid JSON");
}

/// 複数ファイルを指定した merge で全ファイルがマージされる
#[test]
#[ignore]
fn test_merge_multiple_files() {
    let env = CliEnv::new(
        &[("a.txt", "aaa local\n"), ("b.txt", "bbb local\n")],
        &[("a.txt", "aaa remote\n"), ("b.txt", "bbb remote\n")],
    );

    let output = env
        .cmd_with("merge")
        .args(["a.txt", "b.txt", "--left", "local", "--right", "develop"])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);

    let a_content = fs::read_to_string(env.remote_dir.join("a.txt")).unwrap();
    let b_content = fs::read_to_string(env.remote_dir.join("b.txt")).unwrap();
    assert_eq!(a_content, "aaa local\n", "a.txt should be merged");
    assert_eq!(b_content, "bbb local\n", "b.txt should be merged");
}

/// ディレクトリ指定の merge で配下全ファイルがマージされる
#[test]
#[ignore]
fn test_merge_directory() {
    let env = CliEnv::new(
        &[
            ("src/main.rs", "fn main() { local }\n"),
            ("src/lib.rs", "pub fn lib() { local }\n"),
        ],
        &[
            ("src/main.rs", "fn main() { remote }\n"),
            ("src/lib.rs", "pub fn lib() { remote }\n"),
        ],
    );

    let output = env
        .cmd_with("merge")
        .args(["src/", "--left", "local", "--right", "develop"])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);

    let main_content = fs::read_to_string(env.remote_dir.join("src/main.rs")).unwrap();
    let lib_content = fs::read_to_string(env.remote_dir.join("src/lib.rs")).unwrap();
    assert_eq!(
        main_content, "fn main() { local }\n",
        "main.rs should be merged"
    );
    assert_eq!(
        lib_content, "pub fn lib() { local }\n",
        "lib.rs should be merged"
    );
}

/// merge 後にバックアップが作成される（.remote-merge-backup ディレクトリ内）
#[test]
#[ignore]
fn test_merge_creates_backup() {
    let env = CliEnv::new(
        &[("file.txt", "local content\n")],
        &[("file.txt", "remote content\n")],
    );

    let output = env
        .cmd_with("merge")
        .args(["file.txt", "--left", "local", "--right", "develop"])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);

    // バックアップディレクトリの存在を確認
    let backup_dir = env.remote_dir.join(".remote-merge-backup");
    assert!(
        backup_dir.exists(),
        "Backup directory should exist after merge at {:?}",
        backup_dir
    );
}

/// 機密ファイル (.env) は --force なしだとスキップされる
#[test]
#[ignore]
fn test_merge_sensitive_file_requires_force() {
    let env = CliEnv::new(
        &[(".env", "SECRET=local\n")],
        &[(".env", "SECRET=remote\n")],
    );

    let output = env
        .cmd_with("merge")
        .args([".env", "--left", "local", "--right", "develop"])
        .output()
        .expect("failed to execute");

    // sensitive ファイルのみの場合、"no files to merge" で exit 0 だがファイルは変更されない
    let remote_content = fs::read_to_string(env.remote_dir.join(".env")).unwrap();
    assert_eq!(
        remote_content, "SECRET=remote\n",
        ".env should NOT be merged without --force"
    );

    // 実際の出力:
    //   stderr: "1 sensitive file(s) will be skipped. Use --force to include them."
    //   stdout: "Skipped: .env (sensitive file)"
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("sensitive file(s) will be skipped"),
        "Expected sensitive skip warning in stderr, got: {}",
        stderr
    );
    assert!(
        stdout.contains("Skipped: .env (sensitive file)"),
        "Expected 'Skipped: .env (sensitive file)' in stdout, got: {}",
        stdout
    );
}

/// 機密ファイルに --force を付けると merge が成功する
#[test]
#[ignore]
fn test_merge_sensitive_file_with_force() {
    let env = CliEnv::new(
        &[(".env", "SECRET=local\n")],
        &[(".env", "SECRET=remote\n")],
    );

    let output = env
        .cmd_with("merge")
        .args([".env", "--left", "local", "--right", "develop", "--force"])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);

    let remote_content = fs::read_to_string(env.remote_dir.join(".env")).unwrap();
    assert_eq!(
        remote_content, "SECRET=local\n",
        ".env should be merged with --force"
    );
}

/// バイナリファイルの merge でバイナリが正しくコピーされる
#[test]
#[ignore]
fn test_merge_binary_file() {
    let env = CliEnv::new(&[], &[]);
    let local_binary = b"\x89PNG\r\n\x1a\n\x00\x00LOCAL";
    let remote_binary = b"\x89PNG\r\n\x1a\n\x00\x00REMOTE";
    place_binary_file(&env.local_dir, "image.png", local_binary);
    place_binary_file(&env.remote_dir, "image.png", remote_binary);

    let output = env
        .cmd_with("merge")
        .args(["image.png", "--left", "local", "--right", "develop"])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);

    let merged = fs::read(env.remote_dir.join("image.png")).unwrap();
    assert_eq!(
        merged, local_binary,
        "Binary file should be copied correctly after merge"
    );
}

/// リモート→リモート merge は --force なしだと拒否される（CLI モードでは非対話的に失敗）
#[test]
#[ignore]
fn test_merge_remote_to_remote_requires_force() {
    let env = CliEnv::new_3way(
        &[("file.txt", "local ref\n")],
        &[("file.txt", "develop content of the file\n")],
        &[("file.txt", "staging\n")],
    );

    let output = env
        .cmd_with("merge")
        .args(["file.txt", "--left", "develop", "--right", "staging"])
        .output()
        .expect("failed to execute");

    // R2R merge は --force なしでは ERROR で拒否される
    assert!(
        !output.status.success(),
        "Remote-to-remote merge without --force should fail in CLI mode, got exit {:?}",
        output.status.code()
    );

    // 実際の出力:
    //   "Warning: merging between two remote servers (develop → staging)"
    //   "Use --force to proceed, or --dry-run to preview changes."
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("merging between two remote servers"),
        "Expected R2R warning message, got: {}",
        stdout
    );
    assert!(
        stdout.contains("--force"),
        "Expected '--force' hint in R2R guard message, got: {}",
        stdout
    );
}

/// 同一ファイルの merge はスキップされ "no files to merge" と報告される
#[test]
#[ignore]
fn test_merge_equal_file_skipped() {
    let env = CliEnv::new(
        &[("file.txt", "same content\n")],
        &[("file.txt", "same content\n")],
    );

    let output = env
        .cmd_with("merge")
        .args(["file.txt", "--left", "local", "--right", "develop"])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);
    // 実際の出力: "no files to merge in the specified path(s)"
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("no files to merge"),
        "Equal file merge should report 'no files to merge', got: {}",
        stdout
    );
}

/// --dry-run ではファイルが実際に変更されないことを確認
#[test]
#[ignore]
fn test_merge_dry_run_does_not_modify() {
    let env = CliEnv::new(
        &[("file.txt", "new local\n")],
        &[("file.txt", "old remote\n")],
    );

    let before = fs::read_to_string(env.remote_dir.join("file.txt")).unwrap();

    let output = env
        .cmd_with("merge")
        .args([
            "file.txt",
            "--left",
            "local",
            "--right",
            "develop",
            "--dry-run",
        ])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);

    let after = fs::read_to_string(env.remote_dir.join("file.txt")).unwrap();
    assert_eq!(before, after, "File should not be modified in dry-run mode");
}

/// 同じパスを重複指定しても1回だけマージされる
#[test]
#[ignore]
fn test_merge_duplicate_paths_deduplicated() {
    let env = CliEnv::new(&[("a.txt", "local a\n")], &[("a.txt", "remote a\n")]);

    let output = env
        .cmd_with("merge")
        .args(["a.txt", "a.txt", "--left", "local", "--right", "develop"])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);

    // マージ結果が正しいことを確認
    let content = fs::read_to_string(env.remote_dir.join("a.txt")).unwrap();
    assert_eq!(content, "local a\n", "File should be merged correctly");

    // stdout で "Merged" が正確に1回だけ報告されることを確認（重複マージなし）
    // 実際の出力: "Merged: a.txt (backup: a.txt.YYYYMMDD-HHMMSS.bak)"
    let stdout = String::from_utf8_lossy(&output.stdout);
    let merge_count = stdout.matches("Merged:").count();
    assert_eq!(
        merge_count, 1,
        "a.txt should be merged exactly once, 'Merged:' appeared {} times in: {}",
        merge_count, stdout
    );
}

/// リモート→リモートの --dry-run ではサーバー名確認ガードがスキップされる
#[test]
#[ignore]
fn test_merge_r2r_with_dry_run_skips_guard() {
    let env = CliEnv::new_3way(
        &[("file.txt", "local ref\n")],
        &[("file.txt", "develop content of the file\n")],
        &[("file.txt", "staging\n")],
    );

    let output = env
        .cmd_with("merge")
        .args([
            "file.txt",
            "--left",
            "develop",
            "--right",
            "staging",
            "--dry-run",
        ])
        .output()
        .expect("failed to execute");

    // dry-run では R2R ガードがスキップされ、成功するはず
    assert_exit_success(&output);
    // 実際の出力: "Would merge: file.txt"
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Would merge:"),
        "R2R dry-run should show 'Would merge:', got: {}",
        stdout
    );
}

/// merge 後のバックアップがセッションディレクトリ構造に従っていることを確認
#[test]
#[ignore]
fn test_merge_backup_session_directory_structure() {
    let env = CliEnv::new(
        &[("sub/file.txt", "local content\n")],
        &[("sub/file.txt", "remote content\n")],
    );

    let output = env
        .cmd_with("merge")
        .args([
            "sub/file.txt",
            "--left",
            "local",
            "--right",
            "develop",
            "--force",
        ])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);

    // .remote-merge-backup/ ディレクトリが存在することを確認
    let backup_dir = env.remote_dir.join(".remote-merge-backup");
    assert!(
        backup_dir.exists(),
        "Backup directory should exist at {:?}",
        backup_dir
    );

    // セッションディレクトリが YYYYMMDD-HHMMSS 形式（15文字）であることを確認
    let session_entries: Vec<_> = fs::read_dir(&backup_dir)
        .expect("failed to read backup dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
        .collect();
    assert_eq!(
        session_entries.len(),
        1,
        "Expected exactly 1 session directory, found {}",
        session_entries.len()
    );

    let session_name = session_entries[0].file_name();
    let session_name_str = session_name.to_string_lossy();
    assert_eq!(
        session_name_str.len(),
        15,
        "Session directory name should be 15 chars (YYYYMMDD-HHMMSS), got '{}' ({})",
        session_name_str,
        session_name_str.len()
    );
    assert!(
        session_name_str
            .chars()
            .enumerate()
            .all(|(i, c)| if i == 8 { c == '-' } else { c.is_ascii_digit() }),
        "Session directory should match YYYYMMDD-HHMMSS format, got '{}'",
        session_name_str
    );

    // セッションディレクトリ内にバックアップファイルが相対パスで存在することを確認
    let backup_file = session_entries[0].path().join("sub/file.txt");
    assert!(
        backup_file.exists(),
        "Backup file should exist at {:?}",
        backup_file
    );

    // バックアップファイルがマージ前のリモート内容を保持していることを確認
    let backup_content = fs::read_to_string(&backup_file).unwrap();
    assert_eq!(
        backup_content, "remote content\n",
        "Backup should contain original remote content"
    );
}

/// 複数ファイルのマージで全バックアップが同一セッションディレクトリに格納されることを確認
#[test]
#[ignore]
fn test_merge_multiple_files_same_session() {
    let env = CliEnv::new(
        &[("a.txt", "aaa local\n"), ("b.txt", "bbb local\n")],
        &[("a.txt", "aaa remote\n"), ("b.txt", "bbb remote\n")],
    );

    let output = env
        .cmd_with("merge")
        .args([
            "a.txt", "b.txt", "--left", "local", "--right", "develop", "--force",
        ])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);

    let backup_dir = env.remote_dir.join(".remote-merge-backup");
    assert!(
        backup_dir.exists(),
        "Backup directory should exist at {:?}",
        backup_dir
    );

    // セッションディレクトリが1つだけ存在することを確認
    let session_entries: Vec<_> = fs::read_dir(&backup_dir)
        .expect("failed to read backup dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
        .collect();
    assert_eq!(
        session_entries.len(),
        1,
        "All backups should be under the same session directory, found {} session dirs",
        session_entries.len()
    );

    let session_path = session_entries[0].path();

    // 各バックアップファイルが存在し、元のリモート内容を保持していることを確認
    let backup_a = session_path.join("a.txt");
    let backup_b = session_path.join("b.txt");
    assert!(
        backup_a.exists(),
        "Backup for a.txt should exist at {:?}",
        backup_a
    );
    assert!(
        backup_b.exists(),
        "Backup for b.txt should exist at {:?}",
        backup_b
    );

    let content_a = fs::read_to_string(&backup_a).unwrap();
    let content_b = fs::read_to_string(&backup_b).unwrap();
    assert_eq!(
        content_a, "aaa remote\n",
        "Backup of a.txt should contain original remote content"
    );
    assert_eq!(
        content_b, "bbb remote\n",
        "Backup of b.txt should contain original remote content"
    );
}
