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

use super::cli_launch_support::{
    AgentRootsFixture, launch_diff, launch_status, write_isolated_config,
};
use super::common::{CliEnv, gen_config, place_files, place_symlink};

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

/// ローカルの側に置いて、status と diff を起動する
fn status_and_diff_with_local_links() {
    for (label, place, links) in UNRESOLVABLE {
        let dirs = CliEnv::new(&[], &[]);
        let (local_root, remote_root) = (dirs.local_dir.clone(), dirs.remote_dir.clone());
        place_plain_files(&local_root, &remote_root);
        place(&local_root);
        let config = gen_config(&local_root, &remote_root, None, dirs.server_port());

        let config_path = write_isolated_config(&dirs, &config, &local_root);

        let status = launch_status(dirs.temp.path(), &config_path, &[]);
        assert_listed_with_the_links_in_errors(label, &status, links);

        let diff = launch_diff(dirs.temp.path(), &config_path, &["--format", "json"]);
        assert_listed_with_the_links_in_errors(label, &diff, links);
    }
}

// @kotowari[REQ-scan-018, EX-scan-018]
#[test]
fn req_scan_018_status_and_diff_report_a_local_link_that_cannot_be_followed_and_go_on() {
    status_and_diff_with_local_links();
}

// @kotowari[REQ-scan-018, EX-scan-018]
#[tokio::test(flavor = "multi_thread")]
async fn req_scan_018_the_agent_scan_lists_a_link_that_cannot_be_followed() {
    let fixture = AgentRootsFixture::new().await;
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
    let fixture = AgentRootsFixture::new().await;
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

// ローカルの書き込み先でも、親が辿れない symlink なら書き込む前に止まる
// @kotowari[REQ-scan-018]
#[test]
fn req_scan_018_a_merge_to_a_local_destination_stops_below_a_link_that_cannot_be_followed() {
    use super::backup_support::{config, merge_args, targets};
    let local = tempfile::TempDir::new().unwrap();
    let develop = tempfile::TempDir::new().unwrap();
    let store = tempfile::TempDir::new().unwrap();
    fs::create_dir(local.path().join("cycle")).unwrap();
    fs::write(local.path().join("cycle/file.txt"), "replacement\n").unwrap();
    std::os::unix::fs::symlink("cycle", develop.path().join("cycle")).unwrap();

    let error = remote_merge::cli::merge::execute_merge(
        merge_args("cycle/file.txt"),
        config(&local, &develop, true),
        targets(&develop, &store),
    )
    .err()
    .expect("the scan below a link that cannot be followed must stop");

    assert!(
        error.to_string().contains("Cannot follow symbolic link"),
        "{error:#}"
    );
    assert!(
        develop
            .path()
            .join("cycle")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink()
    );
}
