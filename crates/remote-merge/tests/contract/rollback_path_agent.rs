#![cfg(unix)]
//! rollback の書き戻す前のパスの確認（docs/ir/backup/rollback-path.md）の契約テストのうち、
//! 書き戻し先がエージェントの経路のもの。
//!
//! エージェントを有効にした試験 SSH サーバの develop に merge してから、実行ファイルで rollback を
//! 起動する。

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use super::scan_listing_cli::AgentFixture;

// @kotowari[EX-backup-027]
#[tokio::test(flavor = "multi_thread")]
async fn rollback_skips_a_deleted_file_whose_parent_became_a_regular_file_via_the_agent() {
    let fixture = AgentFixture::new().await;
    let remote_root =
        assert_skipped_after_an_ancestor_is_replaced_via_the_agent(&fixture, "dir/a.txt", |root| {
            fs::remove_dir_all(root.join("dir")).unwrap();
            fs::write(root.join("dir"), "now a file\n").unwrap();
        })
        .await;
    assert_eq!(
        fs::read_to_string(remote_root.join("other.txt")).unwrap(),
        "deleted other\n"
    );
    assert_eq!(
        fs::read_to_string(remote_root.join("dir")).unwrap(),
        "now a file\n"
    );
}

/// エージェントの経路で `deleted` と "other.txt" を --delete で消してから `replace` で祖先を置き換えて
/// rollback し、`deleted` だけが親ディレクトリのないファイルとしてスキップされることを確かめる。
/// 書き戻し先の root を返す
async fn assert_skipped_after_an_ancestor_is_replaced_via_the_agent(
    fixture: &AgentFixture,
    deleted: &str,
    replace: impl FnOnce(&Path),
) -> PathBuf {
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    fs::create_dir_all(&local_root).unwrap();
    let file = remote_root.join(deleted);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "deleted\n").unwrap();
    fs::write(remote_root.join("other.txt"), "deleted other\n").unwrap();
    let merge = fixture.run_with_agent(
        &local_root,
        &remote_root,
        &[
            "merge",
            deleted,
            "other.txt",
            "--delete",
            "--force",
            "--left",
            "local",
            "--right",
            "develop",
        ],
    );
    assert_eq!(merge.status.code(), Some(0), "{merge:?}");
    assert!(!remote_root.join("other.txt").exists(), "{merge:?}");
    replace(&remote_root);

    let output = fixture.run_with_agent(
        &local_root,
        &remote_root,
        &[
            "rollback", "--target", "develop", "--force", "--format", "json",
        ],
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("rollback output is not JSON ({error}): {output:?}"));
    assert_eq!(
        json["skipped"],
        serde_json::json!([{"path": deleted, "reason": "parent directory no longer exists"}]),
        "{json}"
    );
    assert_eq!(json["restored"][0]["path"], "other.txt", "{json}");
    assert_eq!(json["restored"].as_array().map(Vec::len), Some(1), "{json}");
    remote_root
}

// @kotowari[EX-backup-029]
#[tokio::test(flavor = "multi_thread")]
async fn ex_backup_029_rollback_skips_a_deleted_file_whose_grandparent_became_a_regular_file_via_the_agent()
 {
    let fixture = AgentFixture::new().await;
    let remote_root = assert_skipped_after_an_ancestor_is_replaced_via_the_agent(
        &fixture,
        "dir/sub/a.txt",
        |root| {
            fs::remove_dir_all(root.join("dir")).unwrap();
            fs::write(root.join("dir"), "now a file\n").unwrap();
        },
    )
    .await;
    assert_eq!(
        fs::read_to_string(remote_root.join("dir")).unwrap(),
        "now a file\n"
    );
}

// @kotowari[EX-backup-030]
#[tokio::test(flavor = "multi_thread")]
async fn ex_backup_030_rollback_skips_a_deleted_file_whose_parent_became_a_symlink_to_a_file_via_the_agent()
 {
    let fixture = AgentFixture::new().await;
    let remote_root =
        assert_skipped_after_an_ancestor_is_replaced_via_the_agent(&fixture, "dir/a.txt", |root| {
            fs::remove_dir_all(root.join("dir")).unwrap();
            fs::write(root.join("file.txt"), "kept\n").unwrap();
            symlink("file.txt", root.join("dir")).unwrap();
        })
        .await;
    assert_eq!(
        fs::read_to_string(remote_root.join("file.txt")).unwrap(),
        "kept\n"
    );
}
