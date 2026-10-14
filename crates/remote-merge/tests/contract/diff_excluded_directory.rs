#![cfg(unix)]
//! 配下がすべて exclude に当たるディレクトリを指定した CLI diff が、比べる経路の組とパスの数に
//! よらず差分なしになること（docs/ir/cli/diff-strategy.md の REQ-cli-082）の契約テスト。
//!
//! ローカルどうしは diff_strategy_support の組み方で関数を呼ぶ。ローカルと SSH・SSH どうしは
//! 実行ファイルを試験 SSH サーバに対して起動し、ローカルとエージェントはエージェントを置いた
//! 試験サーバに対して起動する。

use std::fs;

use super::agent_support::{self, AgentFixture, Setup};
use super::common::{place_files, CliEnv};
use super::diff_strategy_support::{strings, Fixture};
use super::server_faults::Faults;

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
        }
    }
}

// @kotowari[REQ-cli-082, EX-cli-075]
#[tokio::test(flavor = "multi_thread")]
async fn ex_cli_075_a_directory_whose_children_are_all_excluded_has_no_diff_via_the_agent() {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fixture.disable_backup();
    let mut config = fs::read_to_string(&fixture.config).unwrap();
    config.push_str("[filter]\nexclude = [\"target\"]\n");
    fs::write(&fixture.config, config).unwrap();
    for ((path, left), (_, right)) in UNDER_TARGET_LEFT.iter().zip(UNDER_TARGET_RIGHT.iter()) {
        fixture.put(path, Some(left), Some(right));
    }

    for path in ["target", "target/"] {
        let output = fixture.run(&["diff", path]);

        let json = agent_support::json(&output);
        assert_eq!(json["files"], serde_json::json!([]), "{path}: {output:?}");
        assert_eq!(output.status.code(), Some(0), "{path}: {output:?}");
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains("falling back"),
            "{path}: {output:?}"
        );
    }
}
