#![cfg(unix)]
//! rollback の書き戻す前のパスの確認（docs/ir/backup/rollback-path.md）の契約テストのうち、
//! 書き戻し先がエージェントの経路のもの。
//!
//! エージェントを有効にした試験 SSH サーバの develop に merge してから、実行ファイルで rollback を
//! 起動する。

use std::fs;

use super::scan_listing_cli::AgentFixture;

// @kotowari[EX-backup-027]
#[tokio::test(flavor = "multi_thread")]
async fn rollback_skips_a_deleted_file_whose_parent_became_a_regular_file_via_the_agent() {
    let fixture = AgentFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    fs::create_dir_all(&local_root).unwrap();
    fs::create_dir_all(remote_root.join("dir")).unwrap();
    fs::write(remote_root.join("dir/a.txt"), "deleted a\n").unwrap();
    fs::write(remote_root.join("other.txt"), "deleted other\n").unwrap();
    let merge = fixture.run_with_agent(
        &local_root,
        &remote_root,
        &[
            "merge",
            "dir/a.txt",
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
    fs::remove_dir_all(remote_root.join("dir")).unwrap();
    fs::write(remote_root.join("dir"), "now a file\n").unwrap();

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
        serde_json::json!([{"path": "dir/a.txt", "reason": "parent directory no longer exists"}]),
        "{json}"
    );
    assert_eq!(json["restored"][0]["path"], "other.txt", "{json}");
    assert_eq!(json["restored"].as_array().map(Vec::len), Some(1), "{json}");
    assert_eq!(
        fs::read_to_string(remote_root.join("other.txt")).unwrap(),
        "deleted other\n"
    );
    assert_eq!(
        fs::read_to_string(remote_root.join("dir")).unwrap(),
        "now a file\n"
    );
}
