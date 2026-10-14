#![cfg(unix)]
//! リンク文字列も参照先の中身も左右で同じ symlink を、CLI diff がパスの指定の仕方と走査方式に
//! よらず項目に出さないこと（docs/ir/cli/diff-strategy.md の REQ-cli-083）の契約テスト。
//! 組み方は diff_strategy_support にある。

use std::fs;

use super::common::place_files;
use super::diff_strategy_support::{Fixture, strings};

// ── REQ-cli-083: 同じ symlink は項目に出さない ──

// @kotowari[REQ-cli-083, EX-cli-076]
#[test]
fn ex_cli_076_an_identical_link_is_not_listed() {
    let fixture = Fixture::new("");
    for root in [fixture.left(), fixture.right()] {
        place_files(root, &[("target.txt", "same\n"), ("other.txt", "x\n")]);
        std::os::unix::fs::symlink("target.txt", root.join("link")).unwrap();
    }
    fs::write(fixture.right().join("other.txt"), "y\n").unwrap();

    for paths in [strings(&["link"]), fixture.padded(&["link"]), strings(&[])] {
        let run = fixture.diff(&paths, None);

        assert!(!run.paths().contains(&"link".to_string()), "{}", run.json);
        assert!(!run.text.contains("link"), "{}", run.text);
    }
}
