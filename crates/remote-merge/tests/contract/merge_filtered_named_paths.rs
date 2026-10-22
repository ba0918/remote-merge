#![cfg(unix)]
//! 名前で指定したファイルには include と exclude を当てずに書くこと
//! （docs/ir/merge/basic.md の REQ-merge-022。diff の docs/ir/cli/diff-strategy.md の REQ-cli-081 と
//! 同じ規則）の契約テスト。
//!
//! 書き込み先 develop を一時ディレクトリに差し替えて execute_merge と execute_sync を呼ぶ。

use super::args_support::{merge_args, sync_args};
use super::merge_support::fixture_with_filter;

// exclude に当たるファイルを名前で指定した merge は、そのファイルを書く
// @kotowari[REQ-merge-022]
#[test]
fn merge_writes_a_named_file_that_the_exclude_matches() {
    let fixture = fixture_with_filter("exclude = [\"*.log\"]");
    fixture.write("local", "app.log", "local\n");
    fixture.write("develop", "app.log", "remote\n");

    let (json, code) = fixture.merge_json(merge_args(&["app.log"]));

    assert_eq!(code, 0, "{json}");
    assert_eq!(fixture.read("develop", "app.log"), "local\n");
}

// sync も同じく、exclude に当たる名前で指定したファイルを書く
// @kotowari[REQ-merge-022]
#[test]
fn sync_writes_a_named_file_that_the_exclude_matches() {
    let fixture = fixture_with_filter("exclude = [\"*.log\"]");
    fixture.write("local", "app.log", "local\n");
    fixture.write("develop", "app.log", "remote\n");

    let (json, code) = fixture.sync_json(sync_args(&["app.log"]));

    assert_eq!(code, 0, "{json}");
    assert_eq!(fixture.read("develop", "app.log"), "local\n");
}

// include の外の名前で指定したファイルも、同じく書く
// @kotowari[REQ-merge-022]
#[test]
fn merge_writes_a_named_file_outside_the_include() {
    let fixture = fixture_with_filter("include = [\"other\"]");
    fixture.create_dir("local", "other");
    fixture.create_dir("develop", "other");
    fixture.write("local", "other/in.txt", "same\n");
    fixture.write("develop", "other/in.txt", "same\n");
    fixture.write("local", "app.txt", "local\n");
    fixture.write("develop", "app.txt", "remote\n");

    let (json, code) = fixture.merge_json(merge_args(&["app.txt"]));

    assert_eq!(code, 0, "{json}");
    assert_eq!(fixture.read("develop", "app.txt"), "local\n");
}
