//! merge の symlink と削除（docs/ir/merge/symlink.md、docs/ir/merge/deletion.md）の契約テスト。
//!
//! 書き込み先 develop を一時ディレクトリに差し替えて execute_merge と execute_sync を呼ぶ。
//! 終了コードと標準エラーは確かめない。

use std::fs;
use std::path::Path;

use remote_merge::cli::merge::MergeArgs;
use remote_merge::cli::sync::SyncArgs;

use super::merge_support::{
    args, fixture, fixture_with_backup, fixture_with_sensitive, sync_args, Fixture,
};

const RIGHT_ONLY_REASON: &str = "right-only file (use --delete to remove)";
const SENSITIVE_REASON: &str = "sensitive file (use --force to include)";
/// 設定で機密ファイルのパターンにする。既定の機密ファイルのパターンには一致しない
const SENSITIVE_PATTERN: &str = "*.vault";
const SENSITIVE_FILE: &str = "deploy.vault";

/// 読み込み元に、root の中の target.txt を指す link.txt を作る
fn source_link(fixture: &Fixture) {
    fixture.write("local", "target.txt", "linked contents\n");
    fixture.symlink("local", "link.txt", "target.txt");
}

/// 書き込み先の link.txt が同じリンク先の symlink で、リンク先のファイルが作られていないことを確かめる
fn assert_link_copied_without_following(fixture: &Fixture) {
    let link = fixture.root("develop").join("link.txt");
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink(),
        "develop/link.txt is not a symlink"
    );
    assert_eq!(fs::read_link(&link).unwrap(), Path::new("target.txt"));
    assert!(
        fs::symlink_metadata(fixture.root("develop").join("target.txt")).is_err(),
        "develop/target.txt must not be created"
    );
}

fn reasons_of<'a>(skipped: &'a serde_json::Value, path: &str) -> Vec<&'a str> {
    skipped
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["path"] == path)
        .map(|entry| entry["reason"].as_str().unwrap())
        .collect()
}

fn delete_args(paths: &[&str]) -> MergeArgs {
    MergeArgs {
        delete: true,
        ..args(paths)
    }
}

// @kotowari[REQ-merge-023]
#[test]
fn merge_creates_a_missing_destination_symlink_without_following_it() {
    let fixture = fixture();
    source_link(&fixture);

    let (json, _) = fixture.merge_json(args(&["link.txt"]));

    assert_eq!(json["merged"][0]["path"], "link.txt", "{json}");
    assert_link_copied_without_following(&fixture);
}

// @kotowari[REQ-merge-023]
#[test]
fn sync_creates_a_missing_destination_symlink_without_following_it() {
    let fixture = fixture();
    source_link(&fixture);

    let (json, _) = fixture.sync_json(sync_args(&["link.txt"]));

    assert_eq!(
        json["targets"][0]["merged"][0]["path"], "link.txt",
        "{json}"
    );
    assert_link_copied_without_following(&fixture);
}

// @kotowari[REQ-merge-024]
#[test]
fn merge_without_delete_skips_a_destination_only_file() {
    let fixture = fixture();
    fixture.write("develop", "only-here.txt", "preserve me\n");

    let (json, _) = fixture.merge_json(args(&["."]));

    assert_eq!(fixture.read("develop", "only-here.txt"), "preserve me\n");
    assert_eq!(
        reasons_of(&json["skipped"], "only-here.txt"),
        [RIGHT_ONLY_REASON],
        "{json}"
    );
}

// @kotowari[REQ-merge-024]
#[test]
fn sync_without_delete_skips_a_destination_only_file() {
    let fixture = fixture();
    fixture.write("develop", "only-here.txt", "preserve me\n");

    let (json, _) = fixture.sync_json(sync_args(&["."]));

    assert_eq!(fixture.read("develop", "only-here.txt"), "preserve me\n");
    assert_eq!(
        reasons_of(&json["targets"][0]["skipped"], "only-here.txt"),
        [RIGHT_ONLY_REASON],
        "{json}"
    );
}

// @kotowari[REQ-merge-025]
#[test]
fn merge_delete_without_force_keeps_a_destination_only_sensitive_file() {
    let fixture = fixture_with_sensitive(&[SENSITIVE_PATTERN]);
    fixture.write("develop", SENSITIVE_FILE, "SECRET=kept\n");

    let (json, _) = fixture.merge_json(delete_args(&["."]));

    assert_eq!(fixture.read("develop", SENSITIVE_FILE), "SECRET=kept\n");
    assert_eq!(
        reasons_of(&json["skipped"], SENSITIVE_FILE),
        [SENSITIVE_REASON],
        "{json}"
    );
    assert_eq!(json["deleted"], serde_json::json!([]), "{json}");
}

// @kotowari[REQ-merge-025]
#[test]
fn sync_delete_without_force_keeps_a_destination_only_sensitive_file() {
    let fixture = fixture_with_sensitive(&[SENSITIVE_PATTERN]);
    fixture.write("develop", SENSITIVE_FILE, "SECRET=kept\n");

    // 書き込むものも削除するものもないため、確認のプロンプトの前に戻り標準入力を読まない
    let (json, _) = fixture.sync_json(SyncArgs {
        delete: true,
        force: false,
        dry_run: false,
        ..sync_args(&["."])
    });

    assert_eq!(fixture.read("develop", SENSITIVE_FILE), "SECRET=kept\n");
    assert_eq!(
        reasons_of(&json["targets"][0]["skipped"], SENSITIVE_FILE),
        [SENSITIVE_REASON],
        "{json}"
    );
    assert_eq!(
        json["targets"][0]["deleted"],
        serde_json::json!([]),
        "{json}"
    );
}

/// 書き込み先にだけある old/obsolete.txt を作る
fn destination_only_file(fixture: &Fixture) -> &'static str {
    fixture.create_dir("develop", "old");
    fixture.write("develop", "old/obsolete.txt", "obsolete\n");
    "old/obsolete.txt"
}

/// backup が "セッションID/パス" の形で、パスの部分が `path` であることを確かめる
fn assert_backup_names(backup: &str, path: &str) {
    let (session, backed_up_path) = backup.split_once('/').unwrap();
    assert!(!session.is_empty(), "{backup}");
    assert_eq!(backed_up_path, path, "{backup}");
}

// @kotowari[REQ-merge-026]
#[test]
fn merge_reports_a_deleted_file_with_its_backup_when_backup_is_enabled() {
    let fixture = fixture_with_backup();
    let path = destination_only_file(&fixture);

    let (json, _) = fixture.merge_json(delete_args(&["."]));

    assert!(!fixture.root("develop").join(path).exists());
    let deleted = json["deleted"].as_array().unwrap();
    assert_eq!(deleted.len(), 1, "{json}");
    assert_eq!(deleted[0]["path"], path, "{json}");
    assert_eq!(deleted[0]["status"], "ok", "{json}");
    assert_backup_names(deleted[0]["backup"].as_str().unwrap(), path);
}

// @kotowari[REQ-merge-026]
#[test]
fn merge_reports_a_deleted_file_without_backup_when_backup_is_disabled() {
    let fixture = fixture();
    let path = destination_only_file(&fixture);

    let (json, _) = fixture.merge_json(delete_args(&["."]));

    assert!(!fixture.root("develop").join(path).exists());
    let deleted = json["deleted"].as_array().unwrap();
    assert_eq!(deleted.len(), 1, "{json}");
    assert_eq!(deleted[0]["path"], path, "{json}");
    assert_eq!(deleted[0]["status"], "ok", "{json}");
    assert!(deleted[0].get("backup").is_none(), "{json}");
}

fn deleted_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| line.starts_with("Deleted: "))
        .collect()
}

// @kotowari[REQ-merge-027]
#[test]
fn merge_text_shows_a_deleted_file_with_its_backup_when_backup_is_enabled() {
    let fixture = fixture_with_backup();
    let path = destination_only_file(&fixture);

    let (json, text) = fixture.merge_json_and_text(delete_args(&["."]));

    let backup = json["deleted"][0]["backup"].as_str().unwrap();
    assert_backup_names(backup, path);
    assert_eq!(
        deleted_lines(&text),
        [format!("Deleted: {path} (backup: {backup})")],
        "{text}"
    );
}

// @kotowari[REQ-merge-027]
#[test]
fn merge_text_shows_a_deleted_file_without_backup_when_backup_is_disabled() {
    let fixture = fixture();
    let path = destination_only_file(&fixture);

    let (_, text) = fixture.merge_json_and_text(delete_args(&["."]));

    assert_eq!(deleted_lines(&text), [format!("Deleted: {path}")], "{text}");
}
