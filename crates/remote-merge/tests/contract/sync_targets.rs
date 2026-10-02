//! sync の読み込み元と書き込み先の指定・処理の順（docs/ir/cli/sync.md）の契約テスト。

use remote_merge::cli::sync::SyncArgs;

use super::sync_support::{args, fixture, labels, Fixture};

/// local に "incoming\n"、develop と staging に別の古い中身の file.txt を置いた構成
fn file_on_every_side() -> Fixture {
    let fixture = fixture();
    fixture.write("local", "file.txt", "incoming\n");
    fixture.write("develop", "file.txt", "develop old\n");
    fixture.write("staging", "file.txt", "staging old\n");
    fixture
}

// @kotowari[REQ-cli-038]
#[test]
fn one_source_and_two_targets_are_accepted_and_both_targets_are_written() {
    let fixture = file_on_every_side();

    let (output, _) = fixture.sync(args(&["file.txt"], &["develop", "staging"]));

    assert_eq!(output.left.label, "local");
    assert_eq!(labels(&output), ["develop", "staging"]);
    assert_eq!(fixture.read("develop", "file.txt"), "incoming\n");
    assert_eq!(fixture.read("staging", "file.txt"), "incoming\n");
}

// @kotowari[REQ-cli-038]
#[test]
fn each_invalid_specification_stops_with_its_error_and_changes_no_target() {
    let with = |left: Option<&str>, rights: &[&str]| -> SyncArgs {
        let mut args = args(&["file.txt"], rights);
        args.left = left.map(Into::into);
        args
    };
    let cases = [
        (
            with(None, &["develop"]),
            "--left is required for sync command",
        ),
        (
            with(Some("local"), &[]),
            "--right requires at least one target server for sync command",
        ),
        (
            with(Some("local"), &["develop", "staging", "develop"]),
            "Duplicate target server: develop",
        ),
        (
            with(Some("develop"), &["staging", "develop"]),
            "--left and --right must be different (both resolved to 'develop')",
        ),
        (
            with(Some("nowhere"), &["develop"]),
            "Server 'nowhere' not found in config",
        ),
        (
            with(Some("local"), &["develop", "nowhere"]),
            "Server 'nowhere' not found in config",
        ),
    ];

    for (args, expected) in cases {
        let fixture = file_on_every_side();

        let error = fixture.run(args).err().expect("expected an error");

        assert_eq!(error.to_string(), expected);
        assert_eq!(
            fixture.read("develop", "file.txt"),
            "develop old\n",
            "{expected}"
        );
        assert_eq!(
            fixture.read("staging", "file.txt"),
            "staging old\n",
            "{expected}"
        );
    }
}

// @kotowari[REQ-cli-039]
#[test]
fn targets_are_processed_and_reported_in_the_order_given() {
    for order in [["develop", "staging"], ["staging", "develop"]] {
        let fixture = file_on_every_side();

        let (output, _) = fixture.sync(args(&["file.txt"], &order));

        assert_eq!(labels(&output), order);
        assert_eq!(fixture.read("develop", "file.txt"), "incoming\n");
        assert_eq!(fixture.read("staging", "file.txt"), "incoming\n");
    }
}
