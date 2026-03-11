//! `rollback` サブコマンドの E2E テスト。
//!
//! SSH 接続（localhost）を使用するため `#[ignore]` 付き。
//! `cargo test --test cli_rollback -- --ignored` で実行する。

mod common;
use common::*;

use std::fs;

// ─── Basic flow (merge → rollback) ─────────────────────────

/// merge → rollback --force でリモートファイルが元の内容に復元される
#[test]
#[ignore]
fn test_merge_then_rollback_restores_content() {
    let env = CliEnv::new(&[("file.txt", "new\n")], &[("file.txt", "original\n")]);

    // merge 実行: local → develop
    let merge_out = env
        .cmd_with("merge")
        .args([
            "file.txt", "--left", "local", "--right", "develop", "--force",
        ])
        .output()
        .expect("failed to execute merge");
    assert_exit_success(&merge_out);

    // マージ後にリモートが "new\n" になっていることを確認
    let after_merge = fs::read_to_string(env.remote_dir.join("file.txt")).unwrap();
    assert_eq!(after_merge, "new\n", "Remote should have merged content");

    // rollback 実行
    let rollback_out = env
        .cmd_with("rollback")
        .args(["--target", "develop", "--force"])
        .output()
        .expect("failed to execute rollback");
    assert_exit_success(&rollback_out);

    // ロールバック後にリモートが "original\n" に戻っていることを確認
    let after_rollback = fs::read_to_string(env.remote_dir.join("file.txt")).unwrap();
    assert_eq!(
        after_rollback, "original\n",
        "Remote should be restored to original content after rollback"
    );
}

/// 複数ファイルを merge → rollback --force で全ファイル復元される
#[test]
#[ignore]
fn test_rollback_multiple_files() {
    let env = CliEnv::new(
        &[("a.txt", "a-local\n"), ("b.txt", "b-local\n")],
        &[("a.txt", "a-original\n"), ("b.txt", "b-original\n")],
    );

    // merge 実行
    let merge_out = env
        .cmd_with("merge")
        .args([
            "a.txt", "b.txt", "--left", "local", "--right", "develop", "--force",
        ])
        .output()
        .expect("failed to execute merge");
    assert_exit_success(&merge_out);

    // マージ後の確認
    assert_eq!(
        fs::read_to_string(env.remote_dir.join("a.txt")).unwrap(),
        "a-local\n"
    );
    assert_eq!(
        fs::read_to_string(env.remote_dir.join("b.txt")).unwrap(),
        "b-local\n"
    );

    // rollback 実行
    let rollback_out = env
        .cmd_with("rollback")
        .args(["--target", "develop", "--force"])
        .output()
        .expect("failed to execute rollback");
    assert_exit_success(&rollback_out);

    // 全ファイルが復元されていることを確認
    let a_after = fs::read_to_string(env.remote_dir.join("a.txt")).unwrap();
    let b_after = fs::read_to_string(env.remote_dir.join("b.txt")).unwrap();
    assert_eq!(a_after, "a-original\n", "a.txt should be restored");
    assert_eq!(b_after, "b-original\n", "b.txt should be restored");
}

/// ネストされたディレクトリ配下のファイルを merge → rollback で復元できる
#[test]
#[ignore]
fn test_rollback_nested_directory() {
    let env = CliEnv::new(
        &[("src/deep/nested/file.rs", "fn new() {}\n")],
        &[("src/deep/nested/file.rs", "fn original() {}\n")],
    );

    // merge 実行
    let merge_out = env
        .cmd_with("merge")
        .args([
            "src/deep/nested/file.rs",
            "--left",
            "local",
            "--right",
            "develop",
            "--force",
        ])
        .output()
        .expect("failed to execute merge");
    assert_exit_success(&merge_out);

    let after_merge = fs::read_to_string(env.remote_dir.join("src/deep/nested/file.rs")).unwrap();
    assert_eq!(after_merge, "fn new() {}\n");

    // rollback 実行
    let rollback_out = env
        .cmd_with("rollback")
        .args(["--target", "develop", "--force"])
        .output()
        .expect("failed to execute rollback");
    assert_exit_success(&rollback_out);

    let after_rollback =
        fs::read_to_string(env.remote_dir.join("src/deep/nested/file.rs")).unwrap();
    assert_eq!(
        after_rollback, "fn original() {}\n",
        "Nested file should be restored after rollback"
    );
}

// ─── --list ────────────────────────────────────────────────

/// merge 後に rollback --list でセッションが表示される（テキスト出力）
#[test]
#[ignore]
fn test_rollback_list_after_merge() {
    let env = CliEnv::new(&[("file.txt", "local\n")], &[("file.txt", "remote\n")]);

    // merge 実行
    let merge_out = env
        .cmd_with("merge")
        .args([
            "file.txt", "--left", "local", "--right", "develop", "--force",
        ])
        .output()
        .expect("failed to execute merge");
    assert_exit_success(&merge_out);

    // --list でセッション一覧を取得
    let list_out = env
        .cmd_with("rollback")
        .args(["--list", "--target", "develop"])
        .output()
        .expect("failed to execute rollback --list");
    assert_exit_success(&list_out);

    let stdout = String::from_utf8_lossy(&list_out.stdout);

    // セッション ID（YYYYMMDD-HHMMSS 形式）が表示されていることを確認
    assert!(
        stdout.contains("Backup sessions for"),
        "Expected 'Backup sessions for' in output, got: {}",
        stdout
    );
    // ファイル数の表示を確認
    assert!(
        stdout.contains("1 file"),
        "Expected '1 file' in list output, got: {}",
        stdout
    );
}

/// merge 後に rollback --list --format json で有効な JSON が返る
#[test]
#[ignore]
fn test_rollback_list_json_after_merge() {
    let env = CliEnv::new(&[("file.txt", "local\n")], &[("file.txt", "remote\n")]);

    // merge 実行
    let merge_out = env
        .cmd_with("merge")
        .args([
            "file.txt", "--left", "local", "--right", "develop", "--force",
        ])
        .output()
        .expect("failed to execute merge");
    assert_exit_success(&merge_out);

    // --list --format json でセッション一覧を取得
    let list_out = env
        .cmd_with("rollback")
        .args(["--list", "--format", "json", "--target", "develop"])
        .output()
        .expect("failed to execute rollback --list --format json");
    assert_exit_success(&list_out);

    let stdout = String::from_utf8_lossy(&list_out.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("List output should be valid JSON");

    // sessions 配列の存在を確認
    let sessions = parsed["sessions"]
        .as_array()
        .expect("Expected 'sessions' array in JSON");
    assert!(
        !sessions.is_empty(),
        "Expected at least one session in list"
    );

    // 最初のセッションの構造を検証
    let session = &sessions[0];
    assert!(
        session["session_id"].is_string(),
        "Expected session_id to be a string"
    );

    let files = session["files"]
        .as_array()
        .expect("Expected 'files' array in session");
    assert_eq!(
        files.len(),
        1,
        "Expected exactly 1 file in session (merged file.txt only)"
    );

    // files の各エントリに path と size が含まれることを確認
    let file_entry = &files[0];
    assert!(
        file_entry["path"].is_string(),
        "Expected 'path' field in file entry"
    );
    assert!(
        file_entry["size"].is_number(),
        "Expected 'size' field in file entry"
    );
}

// ─── --dry-run ─────────────────────────────────────────────

/// --dry-run では復元計画が表示されるがファイルは変更されない
#[test]
#[ignore]
fn test_rollback_dry_run_shows_plan_without_changes() {
    let env = CliEnv::new(
        &[("file.txt", "local-content\n")],
        &[("file.txt", "remote-content\n")],
    );

    // merge 実行
    let merge_out = env
        .cmd_with("merge")
        .args([
            "file.txt", "--left", "local", "--right", "develop", "--force",
        ])
        .output()
        .expect("failed to execute merge");
    assert_exit_success(&merge_out);

    // マージ後の内容を記録
    let after_merge = fs::read_to_string(env.remote_dir.join("file.txt")).unwrap();
    assert_eq!(after_merge, "local-content\n");

    // --dry-run で rollback
    let dry_run_out = env
        .cmd_with("rollback")
        .args(["--dry-run", "--target", "develop"])
        .output()
        .expect("failed to execute rollback --dry-run");
    assert_exit_success(&dry_run_out);

    // stdout に dry-run プレフィックスとファイルパスが含まれることを確認
    let stdout = String::from_utf8_lossy(&dry_run_out.stdout);
    assert!(
        stdout.contains("Dry run"),
        "Expected 'Dry run' prefix in output, got: {}",
        stdout
    );
    assert!(
        stdout.contains("file.txt"),
        "Expected 'file.txt' in dry-run output, got: {}",
        stdout
    );

    // ファイルがまだマージ後の内容のままであることを確認（復元されていない）
    let still_merged = fs::read_to_string(env.remote_dir.join("file.txt")).unwrap();
    assert_eq!(
        still_merged, "local-content\n",
        "File should NOT be restored in dry-run mode"
    );
}

// ─── --force / sensitive ───────────────────────────────────

/// --force 付き rollback で正常に復元される
#[test]
#[ignore]
fn test_rollback_force_restores_content() {
    let env = CliEnv::new(&[("file.txt", "updated\n")], &[("file.txt", "original\n")]);

    // merge 実行
    let merge_out = env
        .cmd_with("merge")
        .args([
            "file.txt", "--left", "local", "--right", "develop", "--force",
        ])
        .output()
        .expect("failed to execute merge");
    assert_exit_success(&merge_out);

    // rollback --force
    let rollback_out = env
        .cmd_with("rollback")
        .args(["--target", "develop", "--force"])
        .output()
        .expect("failed to execute rollback");
    assert_exit_success(&rollback_out);

    let content = fs::read_to_string(env.remote_dir.join("file.txt")).unwrap();
    assert_eq!(
        content, "original\n",
        "File should be restored with --force"
    );
}

/// sensitive ファイル (.env) は --force なしの rollback ではスキップされる
///
/// 対話プロンプトを避けるため --dry-run を使用し、スキップリストを確認する。
/// デフォルトの FilterConfig に .env が sensitive パターンとして含まれている。
#[test]
#[ignore]
fn test_rollback_skips_sensitive_without_force() {
    let env = CliEnv::new(
        &[(".env", "SECRET=new\n")],
        &[(".env", "SECRET=original\n")],
    );

    // --force 付きで merge（sensitive ファイルをマージするため）
    let merge_out = env
        .cmd_with("merge")
        .args([".env", "--left", "local", "--right", "develop", "--force"])
        .output()
        .expect("failed to execute merge");
    assert_exit_success(&merge_out);

    // --dry-run で rollback（--force なし → sensitive ファイルはスキップされるはず）
    let dry_run_out = env
        .cmd_with("rollback")
        .args(["--dry-run", "--target", "develop"])
        .output()
        .expect("failed to execute rollback --dry-run");
    assert_exit_success(&dry_run_out);

    // .env がスキップリストに含まれていることを確認
    let stdout = String::from_utf8_lossy(&dry_run_out.stdout);
    assert!(
        stdout.contains(".env") && stdout.contains("skipped"),
        "Expected .env to appear as skipped in dry-run output, got: {}",
        stdout
    );

    // .env ファイルがマージ後の内容のまま（復元されていない）ことを確認
    let content = fs::read_to_string(env.remote_dir.join(".env")).unwrap();
    assert_eq!(
        content, "SECRET=new\n",
        ".env should remain merged (sensitive file not restored without --force)"
    );
}

// ─── --session (multiple sessions) ─────────────────────────

/// 複数セッションから特定の古いセッションを指定して rollback できる
#[test]
#[ignore]
fn test_rollback_specific_older_session() {
    let env = CliEnv::new(
        &[("a.txt", "a-local\n"), ("b.txt", "b-local\n")],
        &[("a.txt", "a-original\n"), ("b.txt", "b-original\n")],
    );

    // 1回目の merge: a.txt のみ
    let merge_a = env
        .cmd_with("merge")
        .args(["a.txt", "--left", "local", "--right", "develop", "--force"])
        .output()
        .expect("failed to execute merge A");
    assert_exit_success(&merge_a);

    // セッション間のタイムスタンプを確実に異なるものにする
    std::thread::sleep(std::time::Duration::from_secs(1));

    // 2回目の merge: b.txt のみ
    let merge_b = env
        .cmd_with("merge")
        .args(["b.txt", "--left", "local", "--right", "develop", "--force"])
        .output()
        .expect("failed to execute merge B");
    assert_exit_success(&merge_b);

    // --list --format json でセッション一覧を取得
    let list_out = env
        .cmd_with("rollback")
        .args(["--list", "--format", "json", "--target", "develop"])
        .output()
        .expect("failed to execute rollback --list");
    assert_exit_success(&list_out);

    let list_stdout = String::from_utf8_lossy(&list_out.stdout);
    let list_json: serde_json::Value =
        serde_json::from_str(&list_stdout).expect("List output should be valid JSON");

    let sessions = list_json["sessions"]
        .as_array()
        .expect("Expected sessions array");

    // ガードアサート: 2セッション以上存在すること
    assert!(
        sessions.len() >= 2,
        "Expected at least 2 sessions, got {}. Sessions: {}",
        sessions.len(),
        list_stdout
    );

    // 古い方のセッション（a.txt を含む）のIDを取得
    // セッション一覧は新しい順に並んでいる想定なので、最後のセッションが a.txt のもの
    let older_session_id = sessions
        .iter()
        .find(|s| {
            s["files"]
                .as_array()
                .map(|files| files.iter().any(|f| f["path"].as_str() == Some("a.txt")))
                .unwrap_or(false)
        })
        .expect("Expected to find session containing a.txt");
    let session_a_id = older_session_id["session_id"]
        .as_str()
        .expect("session_id should be a string");

    // 古いセッションのみを rollback
    let rollback_out = env
        .cmd_with("rollback")
        .args(["--session", session_a_id, "--target", "develop", "--force"])
        .output()
        .expect("failed to execute rollback --session");
    assert_exit_success(&rollback_out);

    // a.txt だけが復元され、b.txt はマージ後のまま
    let a_content = fs::read_to_string(env.remote_dir.join("a.txt")).unwrap();
    let b_content = fs::read_to_string(env.remote_dir.join("b.txt")).unwrap();
    assert_eq!(
        a_content, "a-original\n",
        "a.txt should be restored (session A rollback)"
    );
    assert_eq!(
        b_content, "b-local\n",
        "b.txt should remain merged (only session A was rolled back)"
    );
}

// ─── JSON output ───────────────────────────────────────────

/// rollback --format json の出力構造を検証する
#[test]
#[ignore]
fn test_rollback_json_output_structure() {
    let env = CliEnv::new(&[("file.txt", "local\n")], &[("file.txt", "remote\n")]);

    // merge 実行
    let merge_out = env
        .cmd_with("merge")
        .args([
            "file.txt", "--left", "local", "--right", "develop", "--force",
        ])
        .output()
        .expect("failed to execute merge");
    assert_exit_success(&merge_out);

    // rollback --format json --force
    let rollback_out = env
        .cmd_with("rollback")
        .args(["--target", "develop", "--format", "json", "--force"])
        .output()
        .expect("failed to execute rollback --format json");
    assert_exit_success(&rollback_out);

    let stdout = String::from_utf8_lossy(&rollback_out.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Rollback output should be valid JSON");

    // target フィールドの存在を確認
    assert!(
        parsed["target"].is_object(),
        "Expected 'target' object in JSON, got: {}",
        stdout
    );
    assert!(
        parsed["target"]["label"].is_string(),
        "Expected 'target.label' string"
    );

    // session_id フィールドの存在を確認
    let session_id = parsed["session_id"]
        .as_str()
        .expect("Expected 'session_id' string in JSON");
    // セッション ID のフォーマット検証: YYYYMMDD-HHMMSS (15文字)
    assert_eq!(
        session_id.len(),
        15,
        "session_id should be 15 chars (YYYYMMDD-HHMMSS), got: '{}'",
        session_id
    );
    assert!(
        session_id.chars().enumerate().all(|(i, c)| if i == 8 {
            c == '-'
        } else {
            c.is_ascii_digit()
        }),
        "session_id should match YYYYMMDD-HHMMSS format, got: '{}'",
        session_id
    );

    // restored 配列の存在とその中の path フィールドを確認
    let restored = parsed["restored"]
        .as_array()
        .expect("Expected 'restored' array in JSON");
    assert!(!restored.is_empty(), "Expected at least one restored file");
    assert!(
        restored[0]["path"].is_string(),
        "Expected 'path' field in restored entry"
    );
    assert_eq!(
        restored[0]["path"].as_str().unwrap(),
        "file.txt",
        "Restored path should be 'file.txt'"
    );

    // 正常系では skipped/failed は空のため JSON から省略される（skip_serializing_if）
    assert!(
        parsed.get("skipped").is_none() || parsed["skipped"].as_array().unwrap().is_empty(),
        "Expected 'skipped' to be absent or empty in successful rollback"
    );
    assert!(
        parsed.get("failed").is_none() || parsed["failed"].as_array().unwrap().is_empty(),
        "Expected 'failed' to be absent or empty in successful rollback"
    );
}

// ─── Exit codes ────────────────────────────────────────────

/// 正常な rollback は exit code 0 を返す
#[test]
#[ignore]
fn test_rollback_exit_code_success() {
    let env = CliEnv::new(&[("file.txt", "local\n")], &[("file.txt", "remote\n")]);

    // merge 実行
    let merge_out = env
        .cmd_with("merge")
        .args([
            "file.txt", "--left", "local", "--right", "develop", "--force",
        ])
        .output()
        .expect("failed to execute merge");
    assert_exit_success(&merge_out);

    // rollback 実行
    let rollback_out = env
        .cmd_with("rollback")
        .args(["--target", "develop", "--force"])
        .output()
        .expect("failed to execute rollback");

    assert_exit_success(&rollback_out);
}

/// バックアップが存在しない場合は exit code 2 を返す
#[test]
#[ignore]
fn test_rollback_exit_code_no_sessions() {
    let env = CliEnv::new(&[("file.txt", "local\n")], &[("file.txt", "remote\n")]);

    // merge せずにいきなり rollback → セッションが存在しないのでエラー
    let rollback_out = env
        .cmd_with("rollback")
        .args(["--target", "develop", "--force"])
        .output()
        .expect("failed to execute rollback");

    assert_exit_error(&rollback_out, 2);
}
