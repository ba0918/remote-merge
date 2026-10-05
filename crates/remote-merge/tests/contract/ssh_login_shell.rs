#![cfg(unix)]
//! リモートのログインシェルが POSIX の sh でなくても、Agent を使わない SSH の経路が同じに動く契約テスト。
//!
//! 隔離された SSH fixture の develop が受け取ったコマンドを、ログインシェルとして fish の `-c` で実行する。

use std::fs;
use std::path::Path;

use super::common::CliEnv;

fn fish_is_installed() -> bool {
    std::process::Command::new("fish")
        .args(["-c", "true"])
        .status()
        .is_ok_and(|status| status.success())
}

// @kotowari[REQ-ssh-020, EX-ssh-033]
#[test]
fn a_merge_with_writes_links_and_deletions_works_when_the_remote_login_shell_is_fish() {
    if !fish_is_installed() {
        eprintln!("note: fish is not installed; skipped the fish login shell contract test");
        return;
    }
    let env = CliEnv::new(
        &[("a.txt", "local\n"), ("sub/b.txt", "new\n")],
        &[("a.txt", "remote\n"), ("only.txt", "remove\n")],
    );
    super::common::place_symlink(&env.local_dir, "link", "a.txt");
    env.use_remote_login_shell("fish");

    let output = env
        .cmd_with("merge")
        .args([".", "--delete", "--left", "local", "--right", "develop"])
        .args(["--format", "json"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let remote = &env.remote_dir;
    assert_eq!(fs::read_to_string(remote.join("a.txt")).unwrap(), "local\n");
    assert_eq!(
        fs::read_to_string(remote.join("sub/b.txt")).unwrap(),
        "new\n"
    );
    assert_eq!(
        fs::read_link(remote.join("link")).unwrap(),
        Path::new("a.txt")
    );
    assert!(!remote.join("only.txt").exists(), "{output:?}");
}
