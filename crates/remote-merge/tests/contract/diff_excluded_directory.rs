#![cfg(unix)]
//! 配下がすべて exclude に当たるディレクトリを指定した CLI diff が、比べる経路の組とパスの数に
//! よらず差分なしになること（docs/ir/cli/diff-strategy.md の REQ-cli-082）の契約テスト。
//!
//! ローカルどうしは diff_strategy_support の組み方で関数を呼ぶ。ローカルとサーバ・サーバどうしは
//! 実行ファイルを試験 SSH サーバ（エージェントの経路）に対して起動する。

use super::common::{CliEnv, place_files};
use super::diff_strategy_support::{Fixture, strings};

// ── REQ-cli-082: 子がすべて除外されるディレクトリは差分なしとする ──

/// 配下がすべて "target" の exclude に当たる、中身の違うディレクトリ "target"
const UNDER_TARGET_LEFT: [(&str, &str); 2] =
    [("target/a.txt", "left\n"), ("target/deep/b.txt", "left\n")];
const UNDER_TARGET_RIGHT: [(&str, &str); 2] = [
    ("target/a.txt", "right\n"),
    ("target/deep/b.txt", "right\n"),
];

fn assert_no_changes(stdout: &str, code: Option<i32>, context: &str) {
    assert!(
        stdout.contains("0 file(s) with changes"),
        "{context}: {stdout}"
    );
    assert_eq!(code, Some(0), "{context}: {stdout}");
}

// @kotowari[REQ-cli-082, EX-cli-075]
#[test]
fn ex_cli_075_a_directory_whose_children_are_all_excluded_has_no_diff_locally() {
    let fixture = Fixture::new("exclude = [\"target\"]");
    place_files(fixture.left(), &UNDER_TARGET_LEFT);
    place_files(fixture.right(), &UNDER_TARGET_RIGHT);

    for paths in [
        strings(&["target"]),
        strings(&["target/"]),
        fixture.padded(&["target"]),
        fixture.padded(&["target/"]),
    ] {
        let run = fixture.diff(&paths, None);

        assert_no_changes(&run.text, Some(run.code), &format!("{} paths", paths.len()));
    }
}

// @kotowari[REQ-cli-082, EX-cli-075]
#[test]
fn ex_cli_075_a_directory_whose_children_are_all_excluded_has_no_diff_on_the_remote_server() {
    // gen_config の [filter] は exclude = [".git", "target"]
    let env = CliEnv::new_3way(&UNDER_TARGET_LEFT, &UNDER_TARGET_RIGHT, &UNDER_TARGET_LEFT);
    for (left, right) in [("local", "develop"), ("develop", "staging")] {
        for path in ["target", "target/"] {
            let output = env
                .cmd_with("diff")
                .args([path, "--left", left, "--right", right])
                .output()
                .unwrap();

            assert_no_changes(
                &String::from_utf8_lossy(&output.stdout),
                output.status.code(),
                &format!("{left}-{right} {path}: {output:?}"),
            );

            let output = env
                .cmd_with("diff")
                .args([path, "--left", left, "--right", right, "--format", "json"])
                .output()
                .unwrap();
            let json: serde_json::Value = serde_json::from_slice(&output.stdout)
                .unwrap_or_else(|error| panic!("{error}: {output:?}"));
            assert_eq!(json["files"], serde_json::json!([]), "{path}: {output:?}");
            assert_eq!(output.status.code(), Some(0), "{path}: {output:?}");
        }
    }
}
