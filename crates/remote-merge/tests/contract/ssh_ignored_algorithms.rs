#![cfg(unix)]
//! ssh_options に書いた、このツールが使えない方式の名前を、名前を含む警告とともに無視し、すべて
//! 無視されるときは書かないときと同じ方式で接続すること（docs/ir/ssh/compatibility.md の REQ-ssh-006）の
//! 契約テスト。
//!
//! 警告はつなぐときに標準エラーに出るので、実行ファイルを試験 SSH サーバに対して
//! `status --left local --right develop` で起動する。利用者に向けた警告なので、診断の記録の細かさ
//! （--log-level）によらず出ることも確かめる。

use std::fs;

use super::common::CliEnv;

const FILES: [(&str, &str); 1] = [("a.txt", "same\n")];

/// 設定と、そこに書く使えない名前。ホスト鍵の "ssh-dss" は、このツールが DSA の署名を確かめられない
const IGNORED: [(&str, &str); 4] = [
    ("host_key_algorithms", "ssh-dss"),
    ("kex_algorithms", "no-such-kex"),
    ("ciphers", "no-such-cipher"),
    ("mac_algorithms", "no-such-mac"),
];

// @kotowari[REQ-ssh-006, EX-ssh-063]
#[test]
fn unusable_algorithm_names_are_warned_about_and_the_connection_uses_the_defaults() {
    let env = CliEnv::new(&FILES, &FILES);
    let mut config = fs::read_to_string(&env.config_path).unwrap();
    config.push_str("\n[servers.develop.ssh_options]\n");
    for (setting, name) in IGNORED {
        config.push_str(&format!("{setting} = [{name:?}]\n"));
    }
    fs::write(&env.config_path, config).unwrap();

    for flags in [&[][..], &["--log-level", "error"][..]] {
        let output = env
            .cmd_with("status")
            .args(["--left", "local", "--right", "develop"])
            .args(flags)
            .output()
            .unwrap();

        let stderr = String::from_utf8_lossy(&output.stderr);
        for (_, name) in IGNORED {
            assert!(stderr.contains(name), "{flags:?} {name}: {stderr}");
        }
        assert_eq!(output.status.code(), Some(0), "{flags:?}: {output:?}");
    }
}
