#![cfg(unix)]
//! 書き込み先のツリーにない symlink を作るときも、書く前のパスの確認で失敗するパスを、
//! --dry-run の有無によらず同じ失敗として出す（docs/ir/paths の REQ-paths-011・REQ-paths-003、
//! docs/ir/cli/safety.md の REQ-cli-091）契約テスト。
//!
//! 書き込み先 develop を一時ディレクトリに差し替えて execute_merge と execute_sync を呼ぶ。

use super::merge_support::{args, fixture, sync_args, Fixture};
use remote_merge::cli::merge::MergeArgs;
use remote_merge::cli::sync::SyncArgs;

const NOT_A_DIRECTORY: &str = "cannot resolve path: not a directory";
const PATH_TOO_LONG: &str = "cannot resolve path: path too long";
const DESTINATION_FILE: &str = "a regular file\n";

/// 読み込み元の "x/lnk" を symlink にし、書き込み先の "x" を通常ファイルにする
fn link_under_a_destination_file() -> Fixture {
    let fixture = fixture();
    fixture.create_dir("local", "x");
    fixture.symlink("local", "x/lnk", "target");
    fixture.write("develop", "x", DESTINATION_FILE);
    fixture
}

/// `path` だけが `error` の失敗に出て、何も書かない結果であることを確かめる
fn assert_only_failed(label: &str, result: &serde_json::Value, code: i32, path: &str, error: &str) {
    let failed = result["failed"].as_array().unwrap();
    assert_eq!(failed.len(), 1, "{label}: {result}");
    assert_eq!(failed[0]["path"], path, "{label}: {result}");
    assert_eq!(failed[0]["error"], error, "{label}: {result}");
    assert!(
        result["merged"].as_array().unwrap().is_empty(),
        "{label}: {result}"
    );
    assert_eq!(code, 2, "{label}: {result}");
}

// @kotowari[REQ-paths-011, REQ-cli-091]
#[test]
fn merge_fails_a_new_link_under_a_destination_file_as_not_a_directory() {
    for dry_run in [true, false] {
        let fixture = link_under_a_destination_file();

        let (json, code) = fixture.merge_json(MergeArgs {
            dry_run,
            ..args(&["x/lnk"])
        });

        let label = format!("dry_run={dry_run}");
        assert_only_failed(&label, &json, code, "x/lnk", NOT_A_DIRECTORY);
        assert_eq!(fixture.read("develop", "x"), DESTINATION_FILE);
    }
}

// @kotowari[REQ-paths-011, REQ-cli-091]
#[test]
fn sync_fails_a_new_link_under_a_destination_file_as_not_a_directory() {
    for dry_run in [true, false] {
        let fixture = link_under_a_destination_file();

        let (json, code) = fixture.sync_json(SyncArgs {
            dry_run,
            ..sync_args(&["x/lnk"])
        });

        let label = format!("dry_run={dry_run}");
        assert_only_failed(&label, &json["targets"][0], code, "x/lnk", NOT_A_DIRECTORY);
        assert_eq!(fixture.read("develop", "x"), DESTINATION_FILE);
    }
}

/// `base` の下に、実パスがちょうど `length` バイトになるまでディレクトリを重ねて作る
fn deep_directory(base: &std::path::Path, length: usize) -> std::path::PathBuf {
    let mut path = base.to_path_buf();
    while path.as_os_str().len() < length {
        let room = length - path.as_os_str().len() - 1;
        path.push("d".repeat(room.min(200)));
    }
    std::fs::create_dir_all(&path).unwrap();
    assert_eq!(path.as_os_str().len(), length);
    path
}

/// 書き込み先の "deep" を、実パスが 4085 バイトのディレクトリへの symlink にし、
/// 読み込み元の "deep/longlink.txt" を symlink にする。書き込み先でつないだ実パスは
/// 4098 バイトで上限の 4096 バイトを超える
fn link_too_long_under_a_known_directory() -> (Fixture, tempfile::TempDir) {
    let outside = tempfile::TempDir::new().unwrap();
    let real = std::fs::canonicalize(outside.path()).unwrap();
    let deep = deep_directory(&real, 4085);
    let fixture = fixture();
    fixture.create_dir("local", "deep");
    fixture.symlink("local", "deep/longlink.txt", "target");
    fixture.symlink("develop", "deep", deep.to_str().unwrap());
    (fixture, outside)
}

/// 書き込み先の "deep" を通った先に symlink ができているか
fn link_created(fixture: &Fixture) -> bool {
    std::fs::symlink_metadata(fixture.root("develop").join("deep/longlink.txt")).is_ok()
}

// @kotowari[REQ-paths-003, REQ-paths-011, REQ-cli-091]
#[test]
fn merge_fails_a_new_link_whose_destination_path_is_too_long() {
    for dry_run in [true, false] {
        let (fixture, _outside) = link_too_long_under_a_known_directory();

        let (json, code) = fixture.merge_json(MergeArgs {
            dry_run,
            ..args(&["."])
        });

        let label = format!("dry_run={dry_run}");
        assert_only_failed(&label, &json, code, "deep/longlink.txt", PATH_TOO_LONG);
        assert!(!link_created(&fixture), "{label}");
    }
}

// @kotowari[REQ-paths-003, REQ-paths-011, REQ-cli-091]
#[test]
fn sync_fails_a_new_link_whose_destination_path_is_too_long() {
    for dry_run in [true, false] {
        let (fixture, _outside) = link_too_long_under_a_known_directory();

        let (json, code) = fixture.sync_json(SyncArgs {
            dry_run,
            ..sync_args(&["."])
        });

        let label = format!("dry_run={dry_run}");
        assert_only_failed(
            &label,
            &json["targets"][0],
            code,
            "deep/longlink.txt",
            PATH_TOO_LONG,
        );
        assert!(!link_created(&fixture), "{label}");
    }
}
