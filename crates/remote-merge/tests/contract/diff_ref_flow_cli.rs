#![cfg(unix)]
//! --ref を指定した diff の参照先との差と、ファイル全体の競合（docs/ir/cli/reference.md の
//! REQ-cli-063・100・101、conflicts.md の REQ-cli-016・097・099、docs/ir/compare/flow.md の
//! REQ-compare-004）の CLI の契約テスト。構成は `CliEnv::new_3way` で、左を develop、右を staging、
//! 参照先を local にする。

use std::process::Output;

use serde_json::{Value, json};

use super::common::{
    CliEnv, file_entry, place_binary_file, place_files, place_symlink, stdout_json,
};

fn three_way(env: &CliEnv, extra: &[&str]) -> (Output, Value) {
    let mut args = vec![
        "--left", "develop", "--right", "staging", "--ref", "local", "--format", "json",
    ];
    args.extend(extra);
    let output = env
        .cmd_with("diff")
        .args(&args)
        .output()
        .expect("failed to execute diff");
    let json = stdout_json(&output);
    (output, json)
}

fn staging(env: &CliEnv) -> std::path::PathBuf {
    env.temp_root().join("staging")
}

/// ファイル全体で一つの競合（REQ-cli-099）
fn assert_whole_file_conflict(file: &Value) {
    assert_eq!(file["conflict_count"], 1, "{file}");
    let regions = file["conflict_regions"].as_array().expect("no regions");
    assert_eq!(regions.len(), 1, "{file}");
    assert_eq!(
        regions[0]["ref_range"],
        json!({"whole_file": true}),
        "{file}"
    );
    for key in ["left_lines", "right_lines"] {
        assert!(regions[0].get(key).is_none(), "{key}: {file}");
    }
}

// @kotowari[REQ-cli-097, REQ-cli-099, EX-cli-104]
#[test]
fn ex_cli_104_a_file_missing_in_the_ref_and_different_on_both_sides_is_one_whole_file_conflict() {
    let env = CliEnv::new_3way(&[], &[("new.txt", "left\n")], &[("new.txt", "right\n")]);

    let (_, json) = three_way(&env, &[]);

    assert_whole_file_conflict(file_entry(&json, "new.txt"));
}

// @kotowari[REQ-cli-097, REQ-cli-099, EX-cli-105]
#[test]
fn ex_cli_105_a_file_removed_on_one_side_and_changed_on_the_other_is_one_whole_file_conflict() {
    let env = CliEnv::new_3way(
        &[("a.txt", "base\n"), ("kept.txt", "base\n")],
        &[],
        &[("a.txt", "changed\n"), ("kept.txt", "base\n")],
    );

    let (_, json) = three_way(&env, &[]);

    assert_whole_file_conflict(file_entry(&json, "a.txt"));
    // 消した側と、参照先と同じままの側の組は競合ではない
    assert!(
        file_entry(&json, "kept.txt")
            .get("conflict_count")
            .is_none(),
        "{json}"
    );
}

// @kotowari[REQ-cli-016, REQ-cli-099, EX-cli-108]
#[test]
fn ex_cli_108_binaries_changed_differently_on_both_sides_are_one_whole_file_conflict() {
    let env = CliEnv::new_3way(&[], &[], &[]);
    place_binary_file(&env.local_dir, "img.bin", b"\0base");
    place_binary_file(&env.remote_dir, "img.bin", b"\0left");
    place_binary_file(&staging(&env), "img.bin", b"\0right");

    let (_, json) = three_way(&env, &[]);

    assert_whole_file_conflict(file_entry(&json, "img.bin"));
}

// @kotowari[REQ-cli-100, EX-cli-109]
#[test]
fn ex_cli_109_a_link_shows_the_link_target_of_the_ref() {
    let targets = [("l", "x\n"), ("r", "x\n"), ("base", "x\n")];
    let env = CliEnv::new_3way(&targets, &targets, &targets);
    place_symlink(&env.local_dir, "link", "base");
    place_symlink(&env.remote_dir, "link", "l");
    place_symlink(&staging(&env), "link", "r");

    let (_, json) = three_way(&env, &[]);

    let link = file_entry(&json, "link");
    assert_eq!(link["link_targets"]["ref"], "base", "{link}");
    assert!(link.get("ref_hunks").is_none(), "{link}");
}

// @kotowari[REQ-cli-100, EX-cli-110]
#[test]
fn ex_cli_110_a_binary_shows_the_hash_of_the_ref_and_no_ref_hunks() {
    let env = CliEnv::new_3way(&[], &[], &[]);
    place_binary_file(&env.local_dir, "img.bin", b"\0base");
    place_binary_file(&env.remote_dir, "img.bin", b"\0left");
    place_binary_file(&staging(&env), "img.bin", b"\0right");

    let (_, json) = three_way(&env, &[]);

    let image = file_entry(&json, "img.bin");
    let expected = remote_merge_core::sha256::hex_of_bytes(b"\0base");
    assert_eq!(image["ref_hash"], expected, "{image}");
    assert!(image.get("ref_hunks").is_none(), "{image}");
}

// @kotowari[REQ-cli-100, EX-cli-114]
#[test]
fn ex_cli_114_a_binary_ref_of_text_files_shows_the_hash_of_the_ref() {
    let env = CliEnv::new_3way(&[], &[("a.txt", "left\n")], &[("a.txt", "right\n")]);
    place_binary_file(&env.local_dir, "a.txt", b"\0base");

    let (_, json) = three_way(&env, &[]);

    let file = file_entry(&json, "a.txt");
    let expected = remote_merge_core::sha256::hex_of_bytes(b"\0base");
    assert_eq!(file["ref_hash"], expected, "{file}");
    assert!(file.get("ref_hunks").is_none(), "{file}");
    assert!(!file["hunks"].as_array().unwrap().is_empty(), "{file}");
}

// @kotowari[REQ-cli-101, REQ-cli-054, EX-cli-111]
#[test]
fn ex_cli_111_a_truncated_ref_diff_marks_the_file_truncated() {
    let base: String = (0..10).map(|index| format!("ref {index}\n")).collect();
    let env = CliEnv::new_3way(
        &[("a.txt", &base)],
        &[("a.txt", "left\n")],
        &[("a.txt", "right\n")],
    );

    let (_, json) = three_way(&env, &["--max-lines", "3"]);

    let file = file_entry(&json, "a.txt");
    assert_eq!(file["truncated"], true, "{file}");
}

// 参照先の途中が root_dir の外を指すときは、外の中身を参照先との差に出さない（E15）
// @kotowari[REQ-compare-004, EX-compare-004]
#[test]
fn ex_compare_004_the_ref_does_not_show_contents_outside_its_root_dir() {
    let env = CliEnv::new_3way(
        &[],
        &[("shared/a.txt", "left\n")],
        &[("shared/a.txt", "right\n")],
    );
    let outside = env.temp_root().join("outside");
    place_files(&outside, &[("a.txt", "outside content\n")]);
    place_symlink(&env.local_dir, "shared", outside.to_str().unwrap());

    let (output, json) = three_way(&env, &[]);

    let file = file_entry(&json, "shared/a.txt");
    assert!(file.get("ref").is_none(), "{file}");
    assert!(file.get("ref_hunks").is_none(), "{file}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("outside content"), "{stdout}");
}

// --follow-external-links なしで、参照先の側だけが root_dir の外を指す symlink の下にあるパスも、左右の
// 側と同じく比べなかった理由を errors に出して終了コード 2 を返す
// @kotowari[REQ-cli-026, EX-cli-115]
#[test]
fn ex_cli_115_a_path_below_an_outside_link_of_the_ref_is_reported() {
    let env = CliEnv::new_3way(
        &[],
        &[("shared/a.txt", "left\n")],
        &[("shared/a.txt", "right\n")],
    );
    let outside = env.temp_root().join("outside");
    place_files(&outside, &[("a.txt", "outside content\n")]);
    place_symlink(&env.local_dir, "shared", outside.to_str().unwrap());

    let (output, json) = three_way(&env, &[]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let errors = json["errors"].as_array().expect("no errors");
    assert!(
        errors.iter().any(|error| error["path"] == "shared/a.txt"
            && error["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("--follow-external-links"))),
        "{json}"
    );
    assert!(
        file_entry(&json, "shared/a.txt").get("ref").is_none(),
        "{json}"
    );
}
