//! merge で書き込み先の変更を黙って失わないこと（docs/ir/cli/merge.md の REQ-cli-051 と
//! docs/ir/merge/hunks.md の REQ-merge-032）の契約テスト。
//!
//! 書き込み先 develop と参照先 staging を一時ディレクトリに差し替えて execute_merge を呼ぶ。

use serde_json::json;

use super::merge_support::{args, fixture, Fixture};

const PATH: &str = "file.txt";

/// 参照先の中身。先頭と末尾の行の間に変わらない 8 行を挟む
fn base() -> String {
    let middle: String = (0..8).map(|i| format!("stable {i}\n")).collect();
    format!("first\n{middle}last\n")
}

/// 先頭の行だけを変えた中身
fn first_changed() -> String {
    base().replacen("first\n", "FIRST\n", 1)
}

/// 末尾の行だけを変えた中身
fn last_changed() -> String {
    base().replacen("last\n", "LAST\n", 1)
}

/// local・develop・staging に `PATH` を置く
fn place(fixture: &Fixture, local: &str, develop: &str, staging: &str) {
    fixture.write("local", PATH, local);
    fixture.write("develop", PATH, develop);
    fixture.write("staging", PATH, staging);
}

/// 参照先 staging を使い `force` を指定して `PATH` を merge する引数
fn ref_args(force: bool) -> remote_merge::cli::merge::MergeArgs {
    let mut args = args(&[PATH]);
    args.ref_server = Some("staging".into());
    args.force = force;
    args
}

// @kotowari[REQ-cli-051, EX-cli-063]
#[test]
fn a_file_whose_sides_changed_different_lines_fails_as_a_three_way_conflict() {
    let fixture = fixture();
    place(&fixture, &first_changed(), &last_changed(), &base());

    let (json, _) = fixture.merge_json(ref_args(false));

    assert_eq!(
        json["failed"],
        json!([{ "path": PATH, "error": "three-way conflict" }]),
        "{json}"
    );
    assert_eq!(fixture.read("develop", PATH), last_changed());
}

// @kotowari[REQ-cli-051, EX-cli-064]
#[test]
fn a_file_changed_only_on_the_destination_fails_and_is_not_written() {
    let fixture = fixture();
    place(&fixture, &base(), &last_changed(), &base());

    let (json, _) = fixture.merge_json(ref_args(false));

    assert_eq!(
        json["failed"],
        json!([{ "path": PATH, "error": "destination changed since reference" }]),
        "{json}"
    );
    assert_eq!(fixture.read("develop", PATH), last_changed());
}

// @kotowari[REQ-cli-051, EX-cli-065]
#[test]
fn a_file_changed_only_on_the_source_is_written() {
    let fixture = fixture();
    place(&fixture, &first_changed(), &base(), &base());

    let (json, _) = fixture.merge_json(ref_args(false));

    assert_eq!(json["failed"], json!([]), "{json}");
    assert_eq!(fixture.read("develop", PATH), first_changed());
}

// @kotowari[REQ-cli-051, EX-cli-066]
#[test]
fn force_writes_a_file_whose_sides_changed_different_lines() {
    let fixture = fixture();
    place(&fixture, &first_changed(), &last_changed(), &base());

    let (json, _) = fixture.merge_json(ref_args(true));

    assert_eq!(json["failed"], json!([]), "{json}");
    assert_eq!(fixture.read("develop", PATH), first_changed());
}

// @kotowari[REQ-cli-011, EX-cli-021]
#[test]
fn a_merge_with_a_reference_updates_only_the_destination() {
    let fixture = fixture();
    place(&fixture, &first_changed(), &base(), &base());

    let (json, _) = fixture.merge_json(ref_args(false));

    assert_eq!(json["merged"][0]["path"], PATH, "{json}");
    assert_eq!(fixture.read("develop", PATH), first_changed());
    assert_eq!(fixture.read("staging", PATH), base());
    assert_eq!(fixture.read("local", PATH), first_changed());
}
