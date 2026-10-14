#![cfg(unix)]
//! status と sync が、片側でも更新時刻が秒の精度しかなく秒が同じファイルを、中身を読み比べて判定
//! すること（docs/ir/cli/status.md の TBL-cli-001 と例 EX-cli-089、docs/ir/merge/comparison.md の
//! 例 EX-merge-041）の契約テスト。
//!
//! 秒の精度しかない更新時刻は、ナノ秒の部分が 0 の時刻として読まれる（秒の精度しかないファイル
//! システムでは、どの時刻もナノ秒の部分が 0 になる）。リモートの "a.txt" はちょうどその秒を指し、
//! ローカルの "a.txt" は同じ秒の中のナノ秒を持つ（例のとおり）。"b.txt" は左右ともちょうどその秒を指す。
//! 実行ファイルを `--left local --right develop` で起動する。

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use super::common::CliEnv;

const T0: u64 = 1_700_000_000;

fn write_at(root: &Path, name: &str, content: &[u8], mtime: SystemTime) {
    let path = root.join(name);
    fs::write(&path, content).unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
}

/// `dir` の下の "a.txt" と "b.txt" を、大きさが同じで中身が違い、同じ秒を指す更新時刻にして置く
fn place_in_the_same_second(env: &CliEnv, dir: &str) {
    let second = SystemTime::UNIX_EPOCH + Duration::from_secs(T0);
    for root in [&env.local_dir, &env.remote_dir] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    let local = Path::new(dir);
    write_at(
        &env.local_dir,
        local.join("a.txt").to_str().unwrap(),
        b"alpha\n",
        second + Duration::from_millis(300),
    );
    write_at(
        &env.remote_dir,
        local.join("a.txt").to_str().unwrap(),
        b"bravo\n",
        second,
    );
    for (root, content) in [(&env.local_dir, b"alpha\n"), (&env.remote_dir, b"bravo\n")] {
        write_at(root, local.join("b.txt").to_str().unwrap(), content, second);
    }
}

// @kotowari[REQ-cli-027, EX-cli-089]
#[test]
fn ex_cli_089_a_remote_time_with_seconds_only_in_the_same_second_is_decided_by_the_content() {
    let env = CliEnv::new(&[], &[]);
    place_in_the_same_second(&env, ".");

    let output = env
        .cmd_with("status")
        .args([
            "--left", "local", "--right", "develop", "--all", "--format", "json",
        ])
        .output()
        .unwrap();

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {output:?}"));
    for name in ["a.txt", "b.txt"] {
        let file = json["files"]
            .as_array()
            .and_then(|files| files.iter().find(|file| file["path"] == name))
            .unwrap_or_else(|| panic!("{name} missing: {json}"));
        assert_eq!(file["status"], "modified", "{name}: {json}");
    }
}

// @kotowari[REQ-merge-020, EX-merge-041]
#[test]
fn ex_merge_041_sync_writes_a_file_whose_remote_time_has_seconds_only_in_the_same_second() {
    let env = CliEnv::new(&[], &[]);
    place_in_the_same_second(&env, "dir");

    let output = env
        .cmd_with("sync")
        .args([
            "dir/", "--left", "local", "--right", "develop", "--force", "--format", "json",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    for name in ["dir/a.txt", "dir/b.txt"] {
        assert_eq!(
            fs::read_to_string(env.remote_dir.join(name)).unwrap(),
            "alpha\n",
            "{name}: {output:?}"
        );
    }
}
