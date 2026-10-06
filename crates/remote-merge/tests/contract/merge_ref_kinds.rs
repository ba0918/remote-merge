//! 参照先を使う merge で、通常ファイルとディレクトリのように種類のそろわないパス
//! （docs/ir/cli/merge.md の TBL-cli-009、REQ-cli-092、REQ-cli-048）の契約テスト。
//!
//! 種類のそろわないパスは中身を読まずに扱いを決めるので、ディレクトリの読み込みの失敗で
//! "three-way comparison incomplete" にならない。

use super::merge_support::{args, fixture, Fixture};

/// local と develop は中身の違う通常ファイル、参照先 staging だけが同じ名前のディレクトリ
fn reference_is_a_directory(fixture: &Fixture) {
    fixture.write("local", "file.txt", "incoming\n");
    fixture.write("develop", "file.txt", "develop old\n");
    fixture.create_dir("staging", "file.txt");
    fixture.write("staging", "file.txt/inner.txt", "base\n");
}

fn with_reference(paths: &[&str]) -> remote_merge::cli::merge::MergeArgs {
    let mut args = args(paths);
    args.ref_server = Some("staging".into());
    args
}

// @kotowari[TBL-cli-009]
#[test]
fn a_file_whose_reference_is_a_directory_is_not_written_as_a_three_way_conflict() {
    let fixture = fixture();
    reference_is_a_directory(&fixture);

    let (json, code) = fixture.merge_json(with_reference(&["file.txt"]));

    assert_eq!(
        json["failed"],
        serde_json::json!([{ "path": "file.txt", "error": "three-way conflict" }]),
        "{json}"
    );
    assert_eq!(json["merged"], serde_json::json!([]), "{json}");
    assert_eq!(code, 2, "{json}");
    assert_eq!(fixture.read("develop", "file.txt"), "develop old\n");
}

// merged の ref_badge は --force の有無によらず TBL-cli-005 に従い、種類がそろわなければ
// 中身を読まずに "differs" になる。--dry-run では TBL-cli-009 の読み込みに失敗した行だけを当てる
// @kotowari[REQ-cli-048]
#[test]
fn a_dry_run_gives_a_file_whose_reference_is_a_directory_a_differs_badge() {
    for force in [false, true] {
        let fixture = fixture();
        reference_is_a_directory(&fixture);
        let mut args = with_reference(&["file.txt"]);
        args.dry_run = true;
        args.force = force;

        let (json, _) = fixture.merge_json(args);

        assert_eq!(
            json["failed"],
            serde_json::json!([]),
            "force {force}: {json}"
        );
        assert_eq!(
            json["merged"][0]["path"], "file.txt",
            "force {force}: {json}"
        );
        assert_eq!(
            json["merged"][0]["ref_badge"], "differs",
            "force {force}: {json}"
        );
    }
}

// 読み込み元は通常ファイル、書き込み先は同じ名前のディレクトリ。--ref の確認より先に
// REQ-merge-001 のとおり理由付きでスキップする（--dry-run でも同じ）
// @kotowari[REQ-cli-092]
#[test]
fn a_source_and_destination_of_different_kinds_are_skipped_before_the_reference_check() {
    for dry_run in [false, true] {
        let fixture = fixture();
        fixture.write("local", "x", "file\n");
        fixture.create_dir("develop", "x");
        fixture.write("develop", "x/inner.txt", "inner\n");
        fixture.write("staging", "x", "base\n");
        let mut args = with_reference(&["x"]);
        args.dry_run = dry_run;

        let (json, _) = fixture.merge_json(args);

        assert_eq!(
            json["failed"],
            serde_json::json!([]),
            "dry {dry_run}: {json}"
        );
        let skipped = json["skipped"].as_array().unwrap();
        assert!(
            skipped.iter().any(|entry| entry["path"] == "x"
                && entry["reason"]
                    .as_str()
                    .is_some_and(|text| !text.is_empty())),
            "dry {dry_run}: {json}"
        );
        assert_eq!(fixture.read("develop", "x/inner.txt"), "inner\n");
    }
}
