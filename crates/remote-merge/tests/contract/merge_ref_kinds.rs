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

// --dry-run も実行時と同じく参照先の確認を当てる（docs/ir/compare/flow.md の REQ-compare-005。段 4 の
// E10）。--force なしでは三者の競合で failed に出し、--force では書く予定に出す。merged の ref_badge は
// --force の有無によらず TBL-cli-005 に従い、種類がそろわなければ中身を読まずに "differs" になる
// @kotowari[REQ-cli-048, REQ-compare-005]
#[test]
fn a_dry_run_gives_a_file_whose_reference_is_a_directory_a_differs_badge() {
    for force in [false, true] {
        let fixture = fixture();
        reference_is_a_directory(&fixture);
        let mut args = with_reference(&["file.txt"]);
        args.dry_run = true;
        args.force = force;

        let (json, _) = fixture.merge_json(args);

        if !force {
            assert_eq!(
                json["failed"],
                serde_json::json!([{ "path": "file.txt", "error": "three-way conflict" }]),
                "force {force}: {json}"
            );
            continue;
        }
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
