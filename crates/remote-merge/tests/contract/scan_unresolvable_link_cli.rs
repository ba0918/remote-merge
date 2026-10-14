#![cfg(unix)]
//! 走査が、リンク先がないのではなく辿れない symlink（ループしている、途中にディレクトリでないものを
//! 含む）を一覧に載せ、読むだけのコマンドはそのパスと理由を errors に出してほかの結果を出し、
//! merge は書き込みを始める前に止まること（docs/ir/scan/directory-links.md の REQ-scan-018）の
//! 契約テスト。
//!
//! 走査の一覧のテスト（scan_listing_cli）と同じく、実行ファイルを試験 SSH サーバに対して起動する。
//! 辿れない symlink は調べる経路の側にだけ置き、もう一方の側は通常のファイルだけにする。

use std::fs;
use std::path::Path;
use std::process::Output;

use super::common::{gen_config, place_files, place_symlink, TestDirs};
use super::scan_listing_cli::{status_on_the_remote_server, AgentFixture};

/// 辿れない symlink の置き方の名前と、それを `root` に置く関数と、置いたリンクのパス
type Placement = (&'static str, fn(&Path), &'static [&'static str]);

const UNRESOLVABLE: [Placement; 2] = [
    ("a loop", place_a_loop, &["a", "b"]),
    (
        "a non-directory component",
        place_a_link_through_a_file,
        &["n"],
    ),
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

/// 左右に同じ "top.txt" と、中身の違う "c.txt" を置く
fn place_plain_files(local: &Path, remote: &Path) {
    place_files(local, &[("top.txt", "same\n"), ("c.txt", "left\n")]);
    place_files(remote, &[("top.txt", "same\n"), ("c.txt", "right\n")]);
}

/// 読むだけのコマンドが止まらずに一覧を出し、辿れない symlink だけを errors に出して終了コード 2 に
/// した
fn assert_listed_with_the_links_in_errors(label: &str, output: &Output, links: &[&str]) {
    assert_eq!(output.status.code(), Some(2), "{label}: {output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("{label}: stdout is not JSON ({error}): {output:?}"));
    let files = json["files"].to_string();
    assert!(files.contains("c.txt"), "{label}: {json}");
    let errors: Vec<&str> = json["errors"]
        .as_array()
        .unwrap_or_else(|| panic!("{label}: no errors: {json}"))
        .iter()
        .map(|error| {
            assert!(
                error["reason"]
                    .as_str()
                    .is_some_and(|reason| reason.contains("cannot follow")),
                "{label}: {json}"
            );
            error["path"].as_str().unwrap()
        })
        .collect();
    assert_eq!(errors, links, "{label}: {json}");
}

/// 試験サーバの左右に置いて、status と diff を起動する
fn status_and_diff_with_links_on(local_side: bool) {
    for (label, place, links) in UNRESOLVABLE {
        let mut dirs = TestDirs::new_2way(&[], &[]);
        let (local_root, remote_root) = (dirs.local_dir.clone(), dirs.remote_dir.clone());
        place_plain_files(&local_root, &remote_root);
        place(if local_side {
            &local_root
        } else {
            &remote_root
        });
        let config = gen_config(&local_root, &remote_root, None, dirs.server_port());

        let status = status_on_the_remote_server(&mut dirs, &config, &local_root, &[]);
        assert_listed_with_the_links_in_errors(label, &status, links);

        let config_path = dirs.temp.path().join("scan-listing-config.toml");
        let diff = super::diff_output_cli::launch_diff(
            dirs.temp.path(),
            &config_path,
            &["--format", "json"],
        );
        assert_listed_with_the_links_in_errors(label, &diff, links);
    }
}

// @kotowari[REQ-scan-018, EX-scan-018]
#[test]
fn req_scan_018_status_and_diff_report_a_local_link_that_cannot_be_followed_and_go_on() {
    status_and_diff_with_links_on(true);
}

// @kotowari[REQ-scan-018, EX-scan-018]
#[test]
fn req_scan_018_status_and_diff_report_a_remote_link_that_cannot_be_followed_and_go_on() {
    status_and_diff_with_links_on(false);
}

// @kotowari[REQ-scan-018, EX-scan-018]
#[tokio::test(flavor = "multi_thread")]
async fn req_scan_018_the_agent_scan_lists_a_link_that_cannot_be_followed() {
    let fixture = AgentFixture::new().await;
    for (label, place, links) in UNRESOLVABLE {
        let case = fixture.temp.path().join(label.replace(' ', "-"));
        let [local_root, remote_root] = ["local", "remote"].map(|side| case.join(side));
        place_plain_files(&local_root, &remote_root);
        place(&remote_root);

        let status = fixture.status_via_agent(&local_root, &remote_root, &[]);
        assert_listed_with_the_links_in_errors(label, &status, links);

        let diff = fixture.diff_via_agent(&local_root, &remote_root, &["--format", "json"]);
        assert_listed_with_the_links_in_errors(label, &diff, links);
    }
}

// @kotowari[REQ-scan-018, EX-scan-019]
#[tokio::test(flavor = "multi_thread")]
async fn req_scan_018_merge_stops_before_writing_on_a_link_that_cannot_be_followed() {
    let fixture = AgentFixture::new().await;
    for (label, place, _) in UNRESOLVABLE {
        let case = fixture.temp.path().join(label.replace(' ', "-"));
        let [local_root, remote_root] = ["local", "remote"].map(|side| case.join(side));
        place_plain_files(&local_root, &remote_root);
        place_files(&local_root, &[("new.txt", "new\n")]);
        place(&remote_root);

        let merge = fixture.run_with_agent(
            &local_root,
            &remote_root,
            &[
                "merge", ".", "--left", "local", "--right", "develop", "--force", "--format",
                "json",
            ],
        );

        assert_eq!(merge.status.code(), Some(2), "{label}: {merge:?}");
        let shown = String::from_utf8_lossy(&merge.stdout) + String::from_utf8_lossy(&merge.stderr);
        assert!(
            shown.contains("Cannot follow symbolic link"),
            "{label}: {merge:?}"
        );
        assert!(!remote_root.join("new.txt").exists(), "{label}");
        assert_eq!(fs::read(remote_root.join("c.txt")).unwrap(), b"right\n");
    }
}
