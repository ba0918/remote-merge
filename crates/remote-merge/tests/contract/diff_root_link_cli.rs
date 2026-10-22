#![cfg(unix)]
//! root_dir が symlink を経由するときの diff の root_dir の外かどうかの判定
//! （docs/ir/cli/symlink-diff.md の REQ-cli-026）の、エージェントの経路の契約テスト。
//!
//! 左右それぞれの root_dir を symlink にした場合は tests/cli_diff.rs でも確かめる。エージェントの経路は
//! cli_launch_support の `AgentRootsFixture` で試験サーバを起動し、右の root_dir を実ディレクトリへの
//! symlink にした設定で `diff --left local --right develop` を起動する。

use std::os::unix::fs::symlink;

use super::cli_launch_support::AgentRootsFixture;
use super::common::place_files;

// @kotowari[REQ-cli-026]
#[tokio::test(flavor = "multi_thread")]
async fn remote_root_dir_through_a_symlink_compares_files_inside_it_via_the_agent() {
    let fixture = AgentRootsFixture::new().await;
    let temp = fixture.temp.path();
    let (local_root, remote_real, remote_link) = (
        temp.join("local"),
        temp.join("remote"),
        temp.join("linked-remote"),
    );
    place_files(&local_root, &[("file.txt", "left body\n")]);
    place_files(&remote_real, &[("file.txt", "right body\n")]);
    symlink(&remote_real, &remote_link).unwrap();

    let output =
        fixture.diff_via_agent(&local_root, &remote_link, &["file.txt", "--format", "json"]);

    let result: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("{error}: {output:?}"));
    let hunks = result["files"][0]["hunks"].to_string();
    assert!(
        hunks.contains("left body") && hunks.contains("right body"),
        "{result}"
    );
    assert!(!result.to_string().contains("outside root_dir"), "{result}");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
}
