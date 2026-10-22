#![cfg(unix)]
//! リンク先を辿れない symlink（ループしている、途中にディレクトリでないものを含む）が exclude に
//! 当たるか、exclude に当たるディレクトリの下にあるとき、どの経路の走査も止まらず、その symlink を
//! 載せないこと（docs/ir/config/filters.md の REQ-config-021・
//! REQ-config-044）の契約テスト。
//!
//! 左右に同じ "top.txt" を置き、調べる経路の側にだけ辿れない symlink を置く。一覧が "top.txt" の
//! "equal" だけになることを、走査が止まらず symlink を除いた証拠にする。

use std::path::Path;

use super::cli_launch_support::{AgentRootsFixture, all_equal, statuses_by_path};
use super::common::{place_files, place_symlink};

/// 辿れない symlink の置き方の名前、それを `root` に置く関数、それを除く exclude の行
pub(super) type Placement = (&'static str, fn(&Path), &'static str);

const EXCLUDED_UNRESOLVABLE: [Placement; 4] = [
    ("an excluded loop", place_a_loop, "exclude = [\"a\", \"b\"]"),
    (
        "an excluded link through a file",
        place_a_link_through_a_file,
        "exclude = [\"n\"]",
    ),
    // "[d]" は find のパターンに移せないので、SSH の find は "d" の下へ降りる
    (
        "a loop below an excluded directory",
        place_a_loop_below_d,
        "exclude = [\"[d]\"]",
    ),
    (
        "a link through a file below an excluded directory",
        place_a_link_through_a_file_below_d,
        "exclude = [\"[d]\"]",
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

fn place_a_loop_below_d(root: &Path) {
    place_symlink(root, "d/a", "b");
    place_symlink(root, "d/b", "a");
}

fn place_a_link_through_a_file_below_d(root: &Path) {
    place_symlink(root, "d/n", "../top.txt/x");
}

/// 左右に置くファイル
fn place_plain_files(root: &Path) {
    place_files(root, &[("top.txt", "same\n")]);
}

/// 左右の root_dir に通常のファイルを置き、`unresolvable_on_left` なら左、そうでなければ右に
/// 辿れない symlink を置く
pub(super) fn roots(
    base: &Path,
    label: &str,
    place: fn(&Path),
    unresolvable_on_left: bool,
) -> [std::path::PathBuf; 2] {
    let case = base.join(label.replace(' ', "-"));
    let roots = ["local", "remote"].map(|side| case.join(side));
    for root in &roots {
        place_plain_files(root);
    }
    place(&roots[usize::from(!unresolvable_on_left)]);
    roots
}

// @kotowari[REQ-config-044, REQ-config-021]
#[tokio::test(flavor = "multi_thread")]
async fn req_scan_015_the_local_scan_skips_an_excluded_symlink_whose_target_cannot_be_followed() {
    let fixture = AgentRootsFixture::new().await;
    for (label, place, exclude_line) in EXCLUDED_UNRESOLVABLE {
        let [local_root, remote_root] = roots(fixture.temp.path(), label, place, true);
        let output = fixture.status_via_agent_with_exclude(&local_root, &remote_root, exclude_line);
        assert!(output.status.success(), "{label}: {output:?}");
        assert_eq!(
            statuses_by_path(&output),
            all_equal(&["top.txt"]),
            "{label}"
        );
    }
}

// @kotowari[REQ-config-044, REQ-config-021]
#[tokio::test(flavor = "multi_thread")]
async fn req_scan_015_the_agent_scan_skips_an_excluded_symlink_whose_target_cannot_be_followed() {
    let fixture = AgentRootsFixture::new().await;
    for (label, place, exclude_line) in EXCLUDED_UNRESOLVABLE {
        let [local_root, remote_root] = roots(fixture.temp.path(), label, place, false);
        let output =
            fixture.status_via_agent_with_filter(&local_root, &remote_root, exclude_line, &[]);
        assert!(output.status.success(), "{label}: {output:?}");
        assert_eq!(
            statuses_by_path(&output),
            all_equal(&["top.txt"]),
            "{label}"
        );
    }
}
