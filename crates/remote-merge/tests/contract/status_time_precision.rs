#![cfg(unix)]
//! status が、片側でも更新時刻が秒の精度しかなく秒が同じファイルを、中身を読み比べて判定すること
//! （docs/ir/cli/status.md の TBL-cli-001 と例 EX-cli-089）の契約テスト。
//!
//! 秒の精度しかない更新時刻は、更新時刻を秒までしか出さない find（docs/ir/scan/listing.md の
//! REQ-scan-016）のリモートで生じる。試験 SSH サーバのログインシェルを、受けたコマンドの find の
//! 更新時刻の書式 `%T@` を秒までの `%Ts` に替えて実行するシェルにして、その find を再現する。
//! 実行ファイルを `status --left local --right develop` で起動し、右の走査は SSH の経路になる。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::{Duration, SystemTime};

use super::common::CliEnv;

const T0: u64 = 1_700_000_000;

/// 受けたコマンドの find の更新時刻の書式を秒までにして sh で実行するログインシェルを書き、そのパス
fn seconds_only_find_shell(dir: &Path) -> &'static str {
    let path = dir.join("seconds-only-find-shell");
    fs::write(
        &path,
        "#!/bin/sh\nexec sh -c \"$(printf '%s' \"$2\" | sed 's/%T@/%Ts/g')\"\n",
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    Box::leak(path.to_str().unwrap().to_string().into_boxed_str())
}

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

// @kotowari[REQ-cli-027, EX-cli-089]
#[test]
fn ex_cli_089_a_remote_time_with_seconds_only_in_the_same_second_is_decided_by_the_content() {
    let env = CliEnv::new(&[], &[]);
    let second = SystemTime::UNIX_EPOCH + Duration::from_secs(T0);
    // ローカルはナノ秒の精度で、ちょうどその秒を指す。リモートは同じ秒の中の時刻だが秒までしか出ない
    write_at(&env.local_dir, "a.txt", b"alpha\n", second);
    write_at(
        &env.remote_dir,
        "a.txt",
        b"bravo\n",
        second + Duration::from_millis(700),
    );
    env.use_remote_login_shell(seconds_only_find_shell(env.temp_root()));

    let output = env
        .cmd_with("status")
        .args([
            "--left", "local", "--right", "develop", "--all", "--format", "json",
        ])
        .output()
        .unwrap();

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {output:?}"));
    let file = json["files"]
        .as_array()
        .and_then(|files| files.iter().find(|file| file["path"] == "a.txt"))
        .unwrap_or_else(|| panic!("a.txt missing: {json}"));
    assert_eq!(file["status"], "modified", "{json}");
}
