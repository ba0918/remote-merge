#![cfg(unix)]
//! rollback コマンドの振る舞い（docs/ir/backup/rollback-cli.md）の契約テストのうち、
//! 関数呼び出しで確かめるもの。

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};

use remote_merge::cli::rollback::{execute_rollback, RollbackCommandOutput};
use remote_merge::service::output::{format_backup_list_text, format_json, format_rollback_text};
use remote_merge::service::types::RollbackOutput;
use tempfile::TempDir;

use super::backup_support::{
    config, listed_sessions, merge_args, merge_files, restore, rollback_args, rollback_list_args,
    store_entries, targets,
};

/// 戻せるもの・スキップするもの・失敗するものを一つずつ含むセッションを作り、
/// 書き戻しの結果を返す。
fn restored_skipped_and_failed() -> (RollbackOutput, i32) {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::create_dir(develop.path().join("gone")).unwrap();
    for (path, content) in [
        ("restored.txt", "restored before\n"),
        ("failed.txt", "failed before\n"),
        ("gone/skipped.txt", "skipped before\n"),
    ] {
        fs::write(develop.path().join(path), content).unwrap();
    }
    fs::write(local.path().join("restored.txt"), "restored after\n").unwrap();
    fs::write(local.path().join("failed.txt"), "failed after\n").unwrap();
    let config = config(&local, &develop, true);
    let mut args = merge_args("restored.txt");
    args.paths = vec![
        "restored.txt".into(),
        "failed.txt".into(),
        "gone/skipped.txt".into(),
    ];
    args.delete = true;
    args.force = true;
    let output = merge_files(args, config.clone(), targets(&develop, &store));
    assert_eq!(output.merged.len(), 2, "{output:?}");
    assert_eq!(output.deleted.len(), 1, "{output:?}");
    fs::remove_dir(develop.path().join("gone")).unwrap();
    let blocked = develop.path().join("failed.txt");
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).unwrap();

    let result = execute_rollback(
        rollback_args("develop", None),
        config,
        targets(&develop, &store),
    );
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o600)).unwrap();
    let result = result.unwrap();
    let RollbackCommandOutput::Restore(output) = result.output else {
        panic!("expected restore output")
    };
    assert_eq!(output.restored.len(), 1, "{output:?}");
    assert_eq!(output.skipped.len(), 1, "{output:?}");
    assert_eq!(output.failed.len(), 1, "{output:?}");
    (output, result.exit_code)
}

fn line_for<'a>(text: &'a str, path: &str) -> &'a str {
    text.lines()
        .find(|line| line.contains(path))
        .unwrap_or_else(|| panic!("no line for {path} in {text}"))
        .trim()
}

// @kotowari[REQ-backup-037]
#[test]
fn rollback_text_marks_each_file_and_summarises_the_counts() {
    let (output, _) = restored_skipped_and_failed();

    let text = format_rollback_text(&output);

    assert!(line_for(&text, "restored.txt").starts_with("\u{2713} restored.txt"));
    let skipped = line_for(&text, "gone/skipped.txt");
    assert!(skipped.starts_with("- gone/skipped.txt"), "{text}");
    assert!(
        skipped.contains("parent directory no longer exists"),
        "{text}"
    );
    let failed = line_for(&text, "failed.txt");
    assert!(failed.starts_with("\u{2717} failed.txt"), "{text}");
    assert!(failed.contains(&output.failed[0].error), "{text}");
    let summary = text.lines().last().unwrap();
    let counts = summary
        .strip_prefix("Restored 1 file(s)")
        .unwrap_or_else(|| panic!("{summary}"));
    assert_eq!(counts.matches('1').count(), 2, "{summary}");
}

// @kotowari[REQ-backup-037]
#[test]
fn rollback_text_summary_has_no_counts_when_nothing_was_skipped_or_failed() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let config = config(&local, &develop, true);
    merge_files(
        merge_args("file.txt"),
        config.clone(),
        targets(&develop, &store),
    );

    let (output, _) = restore(
        rollback_args("develop", None),
        config,
        targets(&develop, &store),
    );

    let text = format_rollback_text(&output);
    let summary = text.lines().last().unwrap();
    let rest = summary
        .strip_prefix("Restored 1 file(s)")
        .unwrap_or_else(|| panic!("{summary}"));
    assert!(!rest.chars().any(|c| c.is_ascii_digit()), "{summary}");
}

// @kotowari[REQ-backup-037]
#[test]
fn rollback_json_has_the_result_fields() {
    let (output, _) = restored_skipped_and_failed();

    let json: serde_json::Value = serde_json::from_str(&format_json(&output).unwrap()).unwrap();

    assert_eq!(json["target"]["label"], "develop");
    assert!(json["target"]["root"].is_string());
    assert_eq!(json["session_id"], output.session_id.as_str());
    assert_eq!(json["restored"][0]["path"], "restored.txt");
    assert!(json["restored"][0]["pre_rollback_backup"].is_string());
    assert_eq!(json["skipped"][0]["path"], "gone/skipped.txt");
    assert_eq!(
        json["skipped"][0]["reason"],
        "parent directory no longer exists"
    );
    assert_eq!(json["failed"][0]["path"], "failed.txt");
    assert!(json["failed"][0]["error"]
        .as_str()
        .unwrap()
        .starts_with("backup failed: "));
}

// @kotowari[REQ-backup-038]
#[test]
fn rollback_exit_codes_follow_the_result() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let config = config(&local, &develop, true);
    merge_files(
        merge_args("file.txt"),
        config.clone(),
        targets(&develop, &store),
    );

    let listed = execute_rollback(
        rollback_list_args("develop"),
        config.clone(),
        targets(&develop, &store),
    )
    .unwrap();
    let mut preview = rollback_args("develop", None);
    preview.dry_run = true;
    let previewed = execute_rollback(preview, config.clone(), targets(&develop, &store)).unwrap();
    let (_, restored) = restore(
        rollback_args("develop", None),
        config,
        targets(&develop, &store),
    );
    let (_, partial) = restored_skipped_and_failed();

    assert_eq!(listed.exit_code, 0);
    assert_eq!(previewed.exit_code, 0);
    assert_eq!(restored, 0);
    assert_eq!(partial, 2);
}

// @kotowari[REQ-backup-038]
#[test]
fn rollback_exits_with_2_when_every_file_is_skipped_or_failed() {
    for (make_skip, expected_reason) in [(true, "skip"), (false, "fail")] {
        let local = TempDir::new().unwrap();
        let develop = TempDir::new().unwrap();
        let store = TempDir::new().unwrap();
        fs::create_dir(develop.path().join("dir")).unwrap();
        fs::write(local.path().join("file.txt"), "new\n").unwrap();
        fs::write(develop.path().join("file.txt"), "old\n").unwrap();
        let config = config(&local, &develop, true);
        merge_files(
            merge_args("file.txt"),
            config.clone(),
            targets(&develop, &store),
        );
        let file = develop.path().join("file.txt");
        if make_skip {
            fs::remove_file(&file).unwrap();
            symlink("dir", &file).unwrap();
        } else {
            fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();
        }

        let result = execute_rollback(
            rollback_args("develop", None),
            config,
            targets(&develop, &store),
        );
        if !make_skip {
            fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let result = result.unwrap();

        let RollbackCommandOutput::Restore(output) = &result.output else {
            panic!("expected restore output")
        };
        assert!(output.restored.is_empty(), "{expected_reason}: {output:?}");
        assert_eq!(
            output.skipped.len() + output.failed.len(),
            1,
            "{expected_reason}: {output:?}"
        );
        assert_eq!(result.exit_code, 2, "{expected_reason}");
    }
}

// @kotowari[REQ-backup-038]
#[test]
fn dry_run_reports_the_same_changed_path_skip_with_exit_code_0() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "merged\n").unwrap();
    fs::write(develop.path().join("file.txt"), "original\n").unwrap();
    fs::write(develop.path().join("other.txt"), "other\n").unwrap();
    let config = config(&local, &develop, true);
    merge_files(
        merge_args("file.txt"),
        config.clone(),
        targets(&develop, &store),
    );
    fs::remove_file(develop.path().join("file.txt")).unwrap();
    symlink("other.txt", develop.path().join("file.txt")).unwrap();
    let mut args = rollback_args("develop", None);
    args.force = false;
    args.dry_run = true;

    let result = execute_rollback(args, config, targets(&develop, &store)).unwrap();

    let RollbackCommandOutput::DryRun { output, .. } = result.output else {
        panic!("expected dry-run output")
    };
    assert_eq!(
        output.skipped[0].reason,
        "path now resolves to a different location"
    );
    assert_eq!(result.exit_code, 0);
    assert!(develop
        .path()
        .join("file.txt")
        .symlink_metadata()
        .unwrap()
        .file_type()
        .is_symlink());
}

// @kotowari[REQ-backup-036]
#[test]
fn sensitive_files_are_skipped_without_force_and_restored_with_it() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join(".env"), "SECRET=new\n").unwrap();
    fs::write(develop.path().join(".env"), "SECRET=original\n").unwrap();
    let config = config(&local, &develop, true);
    assert!(config
        .filter
        .sensitive
        .iter()
        .any(|pattern| pattern == ".env"));
    let mut merge = merge_args(".env");
    merge.force = true;
    merge_files(merge, config.clone(), targets(&develop, &store));

    let mut preview = rollback_args("develop", None);
    preview.force = false;
    preview.dry_run = true;
    let RollbackCommandOutput::DryRun { output, .. } =
        execute_rollback(preview, config.clone(), targets(&develop, &store))
            .unwrap()
            .output
    else {
        panic!("expected dry-run output")
    };
    assert!(output.restored.is_empty(), "{output:?}");
    assert_eq!(output.skipped.len(), 1, "{output:?}");
    assert_eq!(output.skipped[0].path, ".env");
    assert_eq!(output.skipped[0].reason, "sensitive");

    let (forced, _) = restore(
        rollback_args("develop", None),
        config,
        targets(&develop, &store),
    );
    assert_eq!(forced.restored.len(), 1, "{forced:?}");
    assert!(forced.skipped.is_empty(), "{forced:?}");
    assert_eq!(
        fs::read_to_string(develop.path().join(".env")).unwrap(),
        "SECRET=original\n"
    );
}

// @kotowari[REQ-backup-039]
#[test]
fn target_is_required_except_for_list_which_defaults_to_local() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "old local\n").unwrap();
    fs::write(develop.path().join("file.txt"), "new remote\n").unwrap();
    let config = config(&local, &develop, true);
    let mut down = merge_args("file.txt");
    down.left = Some("develop".into());
    down.right = Some("local".into());
    merge_files(down, config.clone(), targets(&develop, &store));

    let mut restore_args = rollback_args("develop", None);
    restore_args.target = None;
    let error = execute_rollback(restore_args, config.clone(), targets(&develop, &store))
        .err()
        .expect("restore without --target must fail");
    assert!(
        error.to_string().contains("--target is required"),
        "{error}"
    );
    assert_eq!(
        fs::read_to_string(local.path().join("file.txt")).unwrap(),
        "new remote\n"
    );

    let mut list_args = rollback_list_args("develop");
    list_args.target = None;
    let RollbackCommandOutput::List(listed) =
        execute_rollback(list_args, config, targets(&develop, &store))
            .unwrap()
            .output
    else {
        panic!("expected backup list")
    };
    assert_eq!(listed.target.label, "local");
    assert_eq!(listed.sessions.len(), 1, "{listed:?}");
    assert_eq!(listed.sessions[0].files[0].path, "file.txt");
}

// @kotowari[REQ-backup-040]
#[test]
fn written_target_lists_its_session_with_file_sizes_in_text_and_json() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let config = config(&local, &develop, true);
    merge_files(
        merge_args("file.txt"),
        config.clone(),
        targets(&develop, &store),
    );

    let RollbackCommandOutput::List(output) = execute_rollback(
        rollback_list_args("develop"),
        config,
        targets(&develop, &store),
    )
    .unwrap()
    .output
    else {
        panic!("expected backup list")
    };

    assert!(
        format_backup_list_text(&output).contains("file.txt (4 bytes)"),
        "{}",
        format_backup_list_text(&output)
    );
    let json: serde_json::Value = serde_json::from_str(&format_json(&output).unwrap()).unwrap();
    let session = &json["sessions"][0];
    assert_eq!(session["session_id"], "20260914-120000");
    assert_eq!(session["file_count"], 1);
    assert_eq!(session["files"][0]["path"], "file.txt");
    assert_eq!(session["files"][0]["size"], 4);
    assert!(session["files"][0].get("link_target").is_none());
    assert!(session.get("expired").is_none() || session["expired"] == false);
}

// @kotowari[REQ-backup-040]
#[test]
fn updated_symlink_is_listed_as_a_symlink_in_text_and_json() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    symlink("new-target.txt", local.path().join("link.txt")).unwrap();
    fs::write(develop.path().join("target.txt"), "linked content\n").unwrap();
    symlink("target.txt", develop.path().join("link.txt")).unwrap();
    let config = config(&local, &develop, true);
    merge_files(
        merge_args("link.txt"),
        config.clone(),
        targets(&develop, &store),
    );

    let RollbackCommandOutput::List(output) = execute_rollback(
        rollback_list_args("develop"),
        config,
        targets(&develop, &store),
    )
    .unwrap()
    .output
    else {
        panic!("expected backup list")
    };

    assert!(format_backup_list_text(&output).contains("link.txt -> target.txt (symlink)"));
    let json: serde_json::Value = serde_json::from_str(&format_json(&output).unwrap()).unwrap();
    assert_eq!(json["sessions"][0]["files"][0]["link_target"], "target.txt");
    assert!(json["sessions"][0]["files"][0].get("size").is_none());
}

/// 集約先から、指定の内容を持つバックアップの中身を一つ消す。
fn remove_stored_content(store: &TempDir, content: &[u8]) {
    let removed = store_entries(store.path())
        .into_iter()
        .filter(|path| path.is_file())
        .find(|path| fs::read(path).unwrap() == content)
        .expect("stored content not found");
    fs::remove_file(removed).unwrap();
}

// @kotowari[REQ-backup-041]
#[test]
fn rollback_treats_a_session_with_missing_content_as_not_found() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "unique rollback content\n").unwrap();
    let config = config(&local, &develop, true);
    merge_files(
        merge_args("file.txt"),
        config.clone(),
        targets(&develop, &store),
    );
    remove_stored_content(&store, b"unique rollback content\n");

    let error = execute_rollback(
        rollback_args("develop", Some("20260914-120000")),
        config,
        targets(&develop, &store),
    )
    .err()
    .unwrap();

    assert!(
        error.to_string().contains("No backup sessions found"),
        "{error}"
    );
    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "new content\n"
    );
}

// @kotowari[REQ-backup-041]
#[test]
fn session_with_missing_content_is_omitted_without_failing_the_list() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "unique old content\n").unwrap();
    let config = config(&local, &develop, true);
    merge_files(
        merge_args("file.txt"),
        config.clone(),
        targets(&develop, &store),
    );
    remove_stored_content(&store, b"unique old content\n");

    assert!(listed_sessions("develop", config, targets(&develop, &store)).is_empty());
}
