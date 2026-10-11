#![cfg(unix)]
//! 走査が、リンク先がないのではなく辿れない symlink（ループしている、途中にディレクトリでないものを
//! 含む）を、ディレクトリへのリンクでないものとして載せずにエラーで止まること
//! （docs/ir/scan/listing.md の REQ-scan-015）の契約テスト。
//!
//! 走査の一覧のテスト（scan_listing_cli）と同じく、実行ファイルを試験 SSH サーバに対して
//! `status --left local --right develop --all --format json` で起動する。辿れない symlink は
//! 調べる経路の側にだけ置き、もう一方の側は通常のファイルだけにする。

use std::path::Path;
use std::process::Output;

use super::common::{gen_config, place_files, place_symlink, TestDirs};
use super::scan_listing_cli::{status_over_ssh, AgentFixture};

/// 辿れない symlink の置き方の名前と、それを `root` に置く関数
type Placement = (&'static str, fn(&Path));

const UNRESOLVABLE: [Placement; 2] = [
    ("a loop", place_a_loop),
    ("a non-directory component", place_a_link_through_a_file),
];

/// 互いを指す二つの symlink
fn place_a_loop(root: &Path) {
    place_symlink(root, "a", "b");
    place_symlink(root, "b", "a");
}

/// ファイル "top.txt" の下を指す symlink
fn place_a_link_through_a_file(root: &Path) {
    place_symlink(root, "n", "top.txt/x");
}

fn place_plain_files(root: &Path) {
    place_files(root, &[("top.txt", "same\n")]);
}

/// status が一覧を返さずに失敗した。差分があるときの失敗（一覧つき）と見分けるため、一覧の有無も見る
fn assert_stopped_without_a_listing(label: &str, output: &Output) {
    assert!(!output.status.success(), "{label}: {output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_default();
    assert!(json.get("files").is_none(), "{label}: {output:?}");
}

// @kotowari[REQ-scan-015]
#[test]
fn req_scan_015_the_local_scan_stops_on_a_symlink_whose_target_cannot_be_followed() {
    for (label, place) in UNRESOLVABLE {
        let mut dirs = TestDirs::new_2way(&[], &[]);
        let (local_root, remote_root) = (dirs.local_dir.clone(), dirs.remote_dir.clone());
        place_plain_files(&local_root);
        place_plain_files(&remote_root);
        place(&local_root);
        let config = gen_config(&local_root, &remote_root, None, dirs.server_port());

        let output = status_over_ssh(&mut dirs, &config, &local_root, &[]);

        assert_stopped_without_a_listing(label, &output);
    }
}

// @kotowari[REQ-scan-015]
#[test]
fn req_scan_015_the_ssh_scan_stops_on_a_symlink_whose_target_cannot_be_followed() {
    for (label, place) in UNRESOLVABLE {
        let mut dirs = TestDirs::new_2way(&[], &[]);
        let (local_root, remote_root) = (dirs.local_dir.clone(), dirs.remote_dir.clone());
        place_plain_files(&local_root);
        place_plain_files(&remote_root);
        place(&remote_root);
        let config = gen_config(&local_root, &remote_root, None, dirs.server_port());

        let output = status_over_ssh(&mut dirs, &config, &local_root, &[]);

        assert_stopped_without_a_listing(label, &output);
    }
}

// @kotowari[REQ-scan-015]
#[tokio::test(flavor = "multi_thread")]
async fn req_scan_015_the_agent_scan_stops_on_a_symlink_whose_target_cannot_be_followed() {
    let fixture = AgentFixture::new().await;
    for (label, place) in UNRESOLVABLE {
        let case = fixture.temp.path().join(label.replace(' ', "-"));
        let [local_root, remote_root] = ["local", "remote"].map(|side| case.join(side));
        place_plain_files(&local_root);
        place_plain_files(&remote_root);
        place(&remote_root);

        let output = fixture.status_via_agent(&local_root, &remote_root, &[]);

        assert_stopped_without_a_listing(label, &output);
    }
}
