#![cfg(unix)]
//! merge が比べ方を選び、中身で比べたことを書き込みと failed に訳すこと（docs/ir/compare/ の
//! REQ-compare-010・013、TBL-compare-003、docs/ir/merge/ の REQ-merge-005・017・019・020）の CLI の
//! 契約テスト。構成は `CliEnv::new` の local（左）・develop（右）。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, SystemTime};

use super::common::set_mtime;
use super::common::{CliEnv, stdout_json};
use super::merge_flow_support::{LOCAL_TO_DEVELOP, error_of, merge, paths_in};

// 名前で指定したファイルは、root_dir 全体を走査するときも中身で比べ、指定したディレクトリの下の
// ファイルは大きさと更新時刻で比べる（TBL-compare-003）
// @kotowari[REQ-merge-005, REQ-merge-020]
#[test]
fn req_merge_005_a_named_file_is_compared_by_content_even_in_a_whole_scan() {
    let env = CliEnv::new(
        &[("a.txt", "alpha\n"), ("d/b.txt", "alpha\n")],
        &[("a.txt", "bravo\n"), ("d/b.txt", "bravo\n")],
    );
    let time = SystemTime::UNIX_EPOCH + Duration::new(1_600_000_000, 123_456_789);
    for root in [&env.local_dir, &env.remote_dir] {
        set_mtime(&root.join("a.txt"), time);
        set_mtime(&root.join("d/b.txt"), time);
    }

    let output = merge(&env, &[&["a.txt", "d/"][..], &LOCAL_TO_DEVELOP].concat());

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(paths_in(&stdout_json(&output), "merged"), ["a.txt"]);
    assert_eq!(
        fs::read_to_string(env.remote_dir.join("d/b.txt")).unwrap(),
        "bravo\n"
    );
}

/// 上限を 1 バイト超える、中身が 0 のファイル
fn place_big(path: &std::path::Path) {
    let file = fs::File::create(path).unwrap();
    file.set_len(remote_merge_core::op_failure::MAX_FILE_SIZE + 1)
        .unwrap();
}

// @kotowari[REQ-compare-010, REQ-cli-079, EX-compare-012]
#[test]
fn ex_compare_012_a_merge_of_big_files_with_the_same_content_writes_nothing() {
    let env = CliEnv::new(&[], &[]);
    place_big(&env.local_dir.join("big.bin"));
    place_big(&env.remote_dir.join("big.bin"));

    let output = merge(&env, &[&["big.bin"][..], &LOCAL_TO_DEVELOP].concat());

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let json = stdout_json(&output);
    assert!(
        json["failed"].as_array().is_none_or(Vec::is_empty),
        "{json}"
    );
    assert!(
        json["merged"].as_array().is_none_or(Vec::is_empty),
        "{json}"
    );
}

// @kotowari[REQ-compare-013, REQ-merge-017, REQ-merge-019, EX-compare-015]
#[test]
fn ex_compare_015_a_checksum_merge_reports_an_unreadable_file_with_the_read_failure() {
    let env = CliEnv::new(&[("a.txt", "left\n")], &[("a.txt", "rght\n")]);
    let remote = env.remote_dir.join("a.txt");
    fs::set_permissions(&remote, fs::Permissions::from_mode(0o000)).unwrap();
    let applies = fs::read(&remote).is_err();

    let output = merge(
        &env,
        &[&["--checksum", "."][..], &LOCAL_TO_DEVELOP].concat(),
    );
    fs::set_permissions(&remote, fs::Permissions::from_mode(0o644)).unwrap();
    if !applies {
        eprintln!("note: running as root; unreadable files cannot be made");
        return;
    }

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let error = error_of(&stdout_json(&output), "a.txt");
    assert!(error.starts_with("read failed: right: "), "{error}");
    assert_eq!(fs::read_to_string(&remote).unwrap(), "rght\n");
}
