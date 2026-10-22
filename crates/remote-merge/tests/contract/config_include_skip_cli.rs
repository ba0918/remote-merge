#![cfg(unix)]
//! 走査する側にない include の値と root_dir の外へ解決される値を飛ばすこと
//! （docs/ir/config/include.md の REQ-config-032・033）の契約テスト。
//!
//! `AgentFixture`（祖先がディレクトリでない値のテストは `CliEnv`）の試験サーバに対して、実行ファイルを
//! `status --left local --right develop` で起動する。どちらも右はエージェントの経路を通る。
//! 警告は利用者の見る標準エラーで確かめる。左のローカルの経路の警告も同じ標準エラーに出る。

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Output;

use super::common::{CliEnv, gen_config, place_files, strip_ansi, with_filter};
use super::config_filters_cli::{combined, status};
use super::scan_listing_cli::{AgentFixture, statuses_by_path};

/// include を `include` にした設定で、SSH の経路の `status --all --format json` を起動する
fn status_on_the_remote_server(dirs: &CliEnv, include: &[&str]) -> Output {
    let base = gen_config(&dirs.local_dir, &dirs.remote_dir, None, dirs.server_port());
    let config = with_filter(&base, &format!("include = {include:?}"));
    status(dirs, &config, &["--all", "--format", "json"])
}

/// 標準エラーの行のうち、警告の行
fn warning_lines(output: &Output) -> Vec<String> {
    strip_ansi(&output.stderr)
        .lines()
        .filter(|line| line.contains("WARN"))
        .map(str::to_string)
        .collect()
}

/// `value` の存在しない値の警告の行の数。行は警告の文言と値で終わる
fn missing_warnings(output: &Output, value: &str) -> usize {
    let expected = format!("Include path does not exist, skipping: {value}");
    warning_lines(output)
        .iter()
        .filter(|line| line.ends_with(&expected))
        .count()
}

/// `value` を一語として含み、存在しない値の警告でない警告の行の数
fn other_warnings_with(output: &Output, value: &str) -> usize {
    let has_word = |line: &str| {
        line.split_whitespace().any(|word| {
            word.trim_matches(|c: char| !(c.is_alphanumeric() || c == '/' || c == '.')) == value
        })
    };
    warning_lines(output)
        .iter()
        .filter(|line| has_word(line) && !line.contains("does not exist"))
        .count()
}

fn statuses(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(path, status)| (path.to_string(), status.to_string()))
        .collect()
}

/// `root` の下に、root_dir の外の `outside` を指す "ext" と、途中の要素 "via" が `outside` を指す
/// "via/inner" と、`outside` の中のない "missing" を指すリンク切れの "gone" を置く
fn place_outside_links(root: &Path, outside: &Path) {
    fs::create_dir_all(root).unwrap();
    symlink(outside, root.join("ext")).unwrap();
    symlink(outside, root.join("via")).unwrap();
    symlink(outside.join("missing"), root.join("gone")).unwrap();
}

/// root_dir の外のディレクトリ（"secret.txt" と "inner/deep.txt" を置く）
fn outside_dir(temp: &Path) -> std::path::PathBuf {
    let outside = temp.join("outside");
    place_files(
        &outside,
        &[("secret.txt", "secret\n"), ("inner/deep.txt", "deep\n")],
    );
    outside
}

// @kotowari[REQ-config-032, EX-config-010]
#[tokio::test(flavor = "multi_thread")]
async fn include_values_missing_on_the_local_or_agent_side_are_warned_the_same_way() {
    let fixture = AgentFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    place_files(
        &local_root,
        &[("src/a.txt", "local\n"), ("docs/d.txt", "doc\n")],
    );
    place_files(
        &remote_root,
        &[("src/a.txt", "remote\n"), ("extra/e.txt", "extra\n")],
    );
    let output =
        fixture.status_via_agent_with_include(&local_root, &remote_root, &["src", "docs", "extra"]);
    // 左のローカルの側に "extra"、右のエージェントの側に "docs" がない
    assert_eq!(missing_warnings(&output, "extra"), 1, "{output:?}");
    assert_eq!(missing_warnings(&output, "docs"), 1, "{output:?}");
    assert_eq!(
        statuses_by_path(&output),
        statuses(&[
            ("docs/d.txt", "left_only"),
            ("extra/e.txt", "right_only"),
            ("src/a.txt", "modified"),
        ]),
        "{output:?}"
    );

    // 値がすべて存在しないときは、どちらの側も全体を走査せずに空の一覧にする
    let output = fixture.status_via_agent_with_include(&local_root, &remote_root, &["nope"]);
    assert_eq!(missing_warnings(&output, "nope"), 2, "{output:?}");
    assert_eq!(statuses_by_path(&output), BTreeMap::new(), "{output:?}");
}

// @kotowari[REQ-config-033, REQ-config-034, EX-config-013]
#[tokio::test(flavor = "multi_thread")]
async fn include_values_resolving_outside_the_root_dir_are_warned_and_not_listed_via_the_agent() {
    let fixture = AgentFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    let outside = outside_dir(fixture.temp.path());
    for root in [&local_root, &remote_root] {
        place_files(root, &[("src/a.txt", "same\n")]);
        place_outside_links(root, &outside);
    }
    let output = fixture.status_via_agent_with_include(
        &local_root,
        &remote_root,
        &["src", "ext", "via/inner", "gone"],
    );
    // 左のローカルの側と右のエージェントの側がそれぞれ警告する。リンク切れでもリンク先が外なら同じ
    assert_eq!(other_warnings_with(&output, "ext"), 2, "{output:?}");
    assert_eq!(other_warnings_with(&output, "gone"), 2, "{output:?}");
    assert_eq!(other_warnings_with(&output, "via/inner"), 2, "{output:?}");
    assert_eq!(
        statuses_by_path(&output),
        statuses(&[("src/a.txt", "equal")]),
        "{output:?}"
    );
}

// @kotowari[REQ-config-032]
#[test]
fn include_value_whose_ancestor_is_not_a_directory_on_the_ssh_side_stops_with_that_error() {
    // 右の SSH の側では "src" が通常ファイルなので、"src/sub" の確認は祖先がディレクトリでないエラーになる
    let dirs = CliEnv::new(
        &[("src/sub/a.txt", "local\n"), ("top.txt", "local\n")],
        &[("src", "file\n"), ("top.txt", "remote\n")],
    );
    let output = status_on_the_remote_server(&dirs, &["src/sub"]);
    assert_ne!(output.status.code(), Some(0), "{output:?}");
    // --format json ではエラーは標準出力の JSON に出る
    assert!(
        combined(&output).contains("cannot resolve path: not a directory"),
        "{output:?}"
    );
    // 値を飛ばして続けていれば、左の "src/sub/a.txt" が左だけの項目として出る
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("src/sub/a.txt"),
        "{output:?}"
    );
    assert_eq!(missing_warnings(&output, "src/sub"), 0, "{output:?}");
}
