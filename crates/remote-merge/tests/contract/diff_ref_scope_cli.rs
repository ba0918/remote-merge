#![cfg(unix)]
//! diff の --ref が比べる対象（docs/ir/cli/reference.md の REQ-cli-067 と 068）の契約テスト。
//!
//! 出力に含めるファイルは実行ファイルを試験 SSH サーバに対して起動して見る。
//! 構成は `CliEnv::new_3way` の local・develop・staging で、左を local、右を develop、
//! 参照先を staging にする。

use serde_json::Value;

use super::common::CliEnv;
use super::diff_ref_cli::{diff, json};

const WITH_STAGING_REF: [&str; 6] = ["--left", "local", "--right", "develop", "--ref", "staging"];

fn diff_json(env: &CliEnv, with_ref: bool) -> Value {
    let mut args: Vec<&str> = if with_ref {
        WITH_STAGING_REF.to_vec()
    } else {
        WITH_STAGING_REF[..4].to_vec()
    };
    args.extend(["--format", "json"]);
    json(&diff(env, &args))
}

fn paths(json: &Value) -> Vec<&str> {
    json["files"]
        .as_array()
        .expect("\"files\" missing")
        .iter()
        .map(|file| file["path"].as_str().expect("\"path\" missing"))
        .collect()
}

// @kotowari[REQ-cli-067]
#[test]
fn req_cli_067_a_file_equal_on_both_sides_is_not_added_by_a_different_ref() {
    let env = CliEnv::new_3way(
        &[("differs.txt", "local\n"), ("same.txt", "shared\n")],
        &[("differs.txt", "develop\n"), ("same.txt", "shared\n")],
        &[("differs.txt", "staging\n"), ("same.txt", "staging only\n")],
    );

    let json = diff_json(&env, true);

    let found = paths(&json);
    assert!(found.contains(&"differs.txt"), "{json}");
    assert!(!found.contains(&"same.txt"), "{json}");
}

// @kotowari[REQ-cli-068]
#[test]
fn req_cli_068_a_file_only_on_the_ref_is_in_neither_files_nor_summary() {
    let env = CliEnv::new_3way(
        &[("differs.txt", "local\n")],
        &[("differs.txt", "develop\n")],
        &[("differs.txt", "staging\n"), ("ref_only.txt", "staging\n")],
    );

    let with_ref = diff_json(&env, true);
    let without_ref = diff_json(&env, false);

    assert!(!with_ref.to_string().contains("ref_only.txt"), "{with_ref}");
    assert_eq!(paths(&with_ref), ["differs.txt"], "{with_ref}");
    assert_eq!(with_ref["summary"], without_ref["summary"], "{with_ref}");
}
