//! status で通常ファイルとディレクトリのように種類のそろわないパス（docs/ir/cli/status.md の
//! REQ-cli-008、docs/ir/cli/status-targets.md の TBL-cli-005）の契約テスト。
//!
//! 種類のそろわないパスは中身を読まずに違うとみなすので、ディレクトリの読み込みの失敗が errors や
//! 終了コード 2 に出ない。

use remote_merge::service::output::format_json;

use super::status_support::{args, fixture, write_at};

const T0: u64 = 1_700_000_000;

fn json_of(result: &remote_merge::cli::status::StatusCommandResult) -> serde_json::Value {
    serde_json::from_str(&format_json(&result.output).unwrap()).unwrap()
}

fn file<'a>(json: &'a serde_json::Value, path: &str) -> &'a serde_json::Value {
    json["files"]
        .as_array()
        .unwrap_or_else(|| panic!("files missing: {json}"))
        .iter()
        .find(|file| file["path"] == path)
        .unwrap_or_else(|| panic!("{path} missing: {json}"))
}

// 左右は同じ中身の通常ファイルで、参照先だけがディレクトリ
// @kotowari[TBL-cli-005]
#[test]
fn a_reference_of_another_kind_differs_without_reading_it() {
    let fixture = fixture();
    write_at(fixture.local.path(), "x", b"same\n", T0);
    write_at(fixture.develop.path(), "x", b"same\n", T0);
    write_at(fixture.staging.path(), "x/inner.txt", b"same\n", T0);
    let mut args = args(Some("local"), Some("develop"));
    args.all = true;
    args.ref_server = Some("staging".into());

    let result = fixture.run(args).unwrap();
    let json = json_of(&result);

    assert_eq!(file(&json, "x")["ref_badge"], "differs", "{json}");
    assert_eq!(json.get("errors"), None, "{json}");
    assert_eq!(result.exit_code, 0, "{json}");
}

// 右がサーバで --ref がないときはハッシュの経路、右が local のときは中身を読む経路で比べる。
// 左は通常ファイル、右は同じ名前のディレクトリ
// @kotowari[REQ-cli-008]
#[test]
fn checksum_reports_a_file_against_a_directory_as_modified_without_reading_it() {
    let fixture = fixture();
    write_at(fixture.local.path(), "x", b"file\n", T0);
    write_at(fixture.develop.path(), "x/inner.txt", b"file\n", T0);

    for (left, right) in [("local", "develop"), ("develop", "local")] {
        let mut args = args(Some(left), Some(right));
        args.all = true;
        args.checksum = true;

        let result = fixture.run(args).unwrap();
        let json = json_of(&result);

        assert_eq!(
            file(&json, "x")["status"],
            "modified",
            "{left} {right}: {json}"
        );
        assert_eq!(json.get("errors"), None, "{left} {right}: {json}");
        assert_eq!(result.exit_code, 1, "{left} {right}: {json}");
    }
}
