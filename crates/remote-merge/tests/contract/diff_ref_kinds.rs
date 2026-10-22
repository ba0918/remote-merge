#![cfg(unix)]
//! diff の --ref で、参照先の同じパスが左右のファイルと種類の違うもの（ディレクトリ・末尾の
//! symlink）であるとき（docs/ir/cli/unreadable.md の REQ-cli-090、docs/ir/cli/reference.md の
//! REQ-cli-063）の契約テスト。
//!
//! 種類の違う参照先は読まずに "ref" だけを出すので、ディレクトリの読み込みの失敗として errors に
//! 載らず、終了コードにも数えない。参照先はリモートの staging と、ローカルの local の両方で確かめる。

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use serde_json::Value;

use super::common::{CliEnv, stdout_json};
use super::diff_ref_support::diff;

/// 三つのサーバに中身の違う "file.txt" を置き、`reference` のものだけを `replace` で差し替える
fn env_with_reference(reference: &str, replace: impl FnOnce(&Path)) -> CliEnv {
    let env = CliEnv::new_3way(
        &[("file.txt", "local\n"), ("other.txt", "base\n")],
        &[("file.txt", "develop\n"), ("other.txt", "base\n")],
        &[("file.txt", "staging\n"), ("other.txt", "base\n")],
    );
    let path = env.temp_root().join(reference).join("file.txt");
    fs::remove_file(&path).unwrap();
    replace(&path);
    env
}

fn make_directory(path: &Path) {
    fs::create_dir(path).unwrap();
    fs::write(path.join("inner.txt"), "inner\n").unwrap();
}

fn make_symlink(path: &Path) {
    symlink("other.txt", path).unwrap();
}

/// 左右は参照先以外の二つ
fn sides_without(reference: &str) -> [&'static str; 2] {
    match reference {
        "staging" => ["local", "develop"],
        _ => ["develop", "staging"],
    }
}

fn diff_with_reference(env: &CliEnv, reference: &str) -> (Value, Option<i32>) {
    let [left, right] = sides_without(reference);
    let output = diff(
        env,
        &[
            "file.txt", "--left", left, "--right", right, "--ref", reference, "--format", "json",
        ],
    );
    (stdout_json(&output), output.status.code())
}

fn assert_only_the_ref_without_errors(json: &Value, code: Option<i32>, reference: &str) {
    let file = json["files"]
        .as_array()
        .and_then(|files| files.iter().find(|file| file["path"] == "file.txt"))
        .unwrap_or_else(|| panic!("file.txt missing: {json}"));
    assert_eq!(file["ref"]["label"], reference, "{json}");
    assert!(
        !file.as_object().unwrap().contains_key("ref_hunks"),
        "{json}"
    );
    // errors は空なら出さない
    assert!(
        json["errors"].as_array().is_none_or(Vec::is_empty),
        "{json}"
    );
    // 左右に差があるので 1、参照先の種類の違いは終了コード 2 にしない
    assert_eq!(code, Some(1), "{json}");
}

// @kotowari[REQ-cli-090, REQ-cli-063]
#[test]
fn a_reference_that_is_a_directory_gives_only_the_ref_without_errors() {
    for reference in ["staging", "local"] {
        let env = env_with_reference(reference, make_directory);

        let (json, code) = diff_with_reference(&env, reference);

        assert_only_the_ref_without_errors(&json, code, reference);
    }
}

// @kotowari[REQ-cli-090, REQ-cli-063]
#[test]
fn a_reference_that_is_a_symlink_gives_only_the_ref_without_reading_it() {
    for reference in ["staging", "local"] {
        let env = env_with_reference(reference, make_symlink);

        let (json, code) = diff_with_reference(&env, reference);

        assert_only_the_ref_without_errors(&json, code, reference);
    }
}
