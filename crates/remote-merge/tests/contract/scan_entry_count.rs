#![cfg(unix)]
//! 走査の件数の上限に当てる件数にファイルと symlink だけを数えること（docs/ir/scan/limits.md の
//! REQ-scan-010）と、上限を超えたときだけ不完全な走査として知らせること（同じく REQ-scan-004）を、
//! ローカルとエージェントの経路で確かめる契約テスト。
//!
//! 実行ファイルを試験 SSH サーバに対して `status --left local --right develop` で起動する（組み方は
//! scan_listing_cli と同じ）。左は常にローカルの経路、右はエージェントの経路になる。左右に同じ構成を置き、上限を超えずに全ての項目が "equal" で一覧に
//! 載ることを、左右の経路がどちらも上限に当たらなかった証拠にする。

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Output;

use super::cli_launch_support::{AgentRootsFixture, all_equal, statuses_by_path};
use super::common::place_files;

/// ディレクトリ 1 件とその中のファイル 2 件。上限 2 はファイルの数と同じ
const IN_ONE_DIRECTORY: [(&str, &str); 2] = [("dir/a.txt", "same\n"), ("dir/b.txt", "same\n")];
const LIMIT: [&str; 2] = ["--max-entries", "2"];

fn listed(output: &Output) -> BTreeMap<String, String> {
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    statuses_by_path(output)
}

// @kotowari[REQ-scan-010, EX-scan-012]
#[tokio::test(flavor = "multi_thread")]
async fn ex_scan_012_a_directory_is_not_counted_toward_the_limit_via_the_agent() {
    let fixture = AgentRootsFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    for root in [&local_root, &remote_root] {
        place_files(root, &IN_ONE_DIRECTORY);
    }

    let output = fixture.status_via_agent(&local_root, &remote_root, &LIMIT);

    assert_eq!(listed(&output), all_equal(&["dir/a.txt", "dir/b.txt"]));
}

// ─── 上限ちょうどの項目の後に除外する項目しかない ─────────────────────────────

/// find のパターンに移せず、走査の出力から後で除く exclude
const EXCLUDE_UNPRUNABLE: &str = "exclude = [\"x?.txt\"]";

/// `root` に root の外のファイルを指す symlink を 2 件と、exclude に当たる通常ファイルを置く。
/// SSH の経路は symlink を先に出すので、上限ちょうどの項目の後に除外する項目が並ぶ
fn links_then_excluded_file(root: &Path, outside: &Path) {
    place_files(outside, &[("one.txt", "1\n"), ("two.txt", "2\n")]);
    for (link, target) in [("l1", "one.txt"), ("l2", "two.txt")] {
        std::os::unix::fs::symlink(outside.join(target), root.join(link)).unwrap();
    }
    place_files(root, &[("xa.txt", "excluded\n")]);
}

// @kotowari[REQ-scan-010, REQ-scan-004]
#[tokio::test(flavor = "multi_thread")]
async fn an_excluded_entry_after_the_last_counted_one_does_not_truncate_the_scan_via_the_agent() {
    let fixture = AgentRootsFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    let outside = fixture.temp.path().join("outside");
    for root in [&local_root, &remote_root] {
        std::fs::create_dir_all(root).unwrap();
        links_then_excluded_file(root, &outside);
    }

    let output =
        fixture.status_via_agent_with_filter(&local_root, &remote_root, EXCLUDE_UNPRUNABLE, &LIMIT);

    assert_eq!(listed(&output), all_equal(&["l1", "l2"]));
}
