#![cfg(unix)]
//! include の値の扱い（docs/ir/config/include.md）の契約テスト。
//!
//! 左のローカルの経路と右の SSH の経路を通すため、実行ファイルを試験 SSH サーバに対して
//! `status --left local --right develop` で起動する。設定は `gen_config` の [filter] の exclude の
//! 行を include の行に置き換え、既存の隔離の確認を通してから起動する。

use std::collections::BTreeMap;

use super::common::{gen_config, TestDirs};
use super::config_filters_cli::{combined, status, with_include};
use super::scan_listing_cli::statuses_by_path;

/// include を `include_line` にした設定で `status --all --format json` を起動する
fn status_with_include(dirs: &mut TestDirs, include_line: &str) -> std::process::Output {
    let base = gen_config(&dirs.local_dir, &dirs.remote_dir, None, dirs.server_port());
    let config = with_include(&base, include_line);
    status(dirs, &config, &["--all", "--format", "json"])
}

fn modified(paths: &[&str]) -> BTreeMap<String, String> {
    paths
        .iter()
        .map(|path| (path.to_string(), "modified".to_string()))
        .collect()
}

// @kotowari[EX-config-014]
#[test]
fn include_with_only_absolute_values_warns_and_stops_without_scanning() {
    let mut dirs = TestDirs::new_2way(
        &[("src/a.txt", "local\n"), ("top.txt", "local\n")],
        &[("src/a.txt", "remote\n"), ("top.txt", "remote\n")],
    );
    for include_line in ["include = [\"/etc\"]", "include = [\"\", \"/etc\"]"] {
        let output = status_with_include(&mut dirs, include_line);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            combined(&output).contains("Absolute path is not allowed in include filter: /etc"),
            "{include_line}: {output:?}"
        );
        assert_ne!(output.status.code(), Some(0), "{include_line}: {output:?}");
        // 全体を走査していれば、root_dir の直下と "src" の下の違うファイルが一覧に出る
        for listed in ["top.txt", "a.txt"] {
            assert!(!stdout.contains(listed), "{include_line}: {output:?}");
        }
    }
}

// @kotowari[EX-config-017]
#[test]
fn include_with_only_a_dot_compares_the_whole_root_dir_over_ssh() {
    let mut dirs = TestDirs::new_2way(
        &[("src/a.txt", "local\n"), ("top.txt", "local\n")],
        &[("src/a.txt", "remote\n"), ("top.txt", "remote\n")],
    );
    let output = status_with_include(&mut dirs, "include = [\".\"]");
    assert_eq!(
        statuses_by_path(&output),
        modified(&["src/a.txt", "top.txt"]),
        "{output:?}"
    );
}
