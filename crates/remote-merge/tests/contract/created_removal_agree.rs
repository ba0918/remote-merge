#![cfg(unix)]
//! 書き込みと symlink の作成が失敗した後に、作ったディレクトリを消せたかの判断が三つの経路で同じで
//! あること（docs/ir/merge/permissions.md の REQ-merge-042）の契約テスト。消えていたものは消せたもの、
//! 別のもの（ディレクトリでないもの）に替わっていたものは残ったものにする。ローカルとエージェントは
//! 同じ `machine_ops::remove_created_dir` を使い、SSH の経路は書き込みのコマンドと同じ文を sh で動かす。

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;

use remote_merge_core::machine_ops::remove_created_dir;
use remote_merge_ssh::change_steps::remove_created_dirs;

/// 作った後のディレクトリの状態。名前と、消せたものになるか
const STATES: [(&str, bool); 6] = [
    ("gone", true),
    ("empty", true),
    ("holding", false),
    ("file", false),
    ("dangling", false),
    ("to_dir", false),
];

fn state(root: &Path) {
    fs::create_dir(root.join("empty")).unwrap();
    fs::create_dir(root.join("holding")).unwrap();
    fs::write(root.join("holding/a"), "a").unwrap();
    fs::write(root.join("file"), "x").unwrap();
    symlink("nowhere", root.join("dangling")).unwrap();
    fs::create_dir(root.join("real")).unwrap();
    symlink("real", root.join("to_dir")).unwrap();
}

/// SSH の経路で `dir` を消せたか（残ったものの目印が出なかったか）
fn removed_over_ssh(dir: &Path) -> bool {
    let output = Command::new("/bin/sh")
        .args(["-c", &remove_created_dirs(&[dir.to_str().unwrap()])])
        .output()
        .unwrap();
    !output
        .stderr
        .windows(8)
        .any(|window| window == b"@RMLEFT\0")
}

// @kotowari[REQ-merge-042]
#[test]
fn whether_a_created_directory_was_removed_is_judged_the_same_on_every_route() {
    let local = tempfile::tempdir().unwrap();
    let ssh = tempfile::tempdir().unwrap();
    state(local.path());
    state(ssh.path());

    for (name, expected) in STATES {
        let on_local = remove_created_dir(&local.path().join(name)).is_ok();
        let on_ssh = removed_over_ssh(&ssh.path().join(name));

        assert_eq!((on_local, on_ssh), (expected, expected), "{name}");
    }
    for root in [local.path(), ssh.path()] {
        assert!(root.join("file").is_file());
        assert!(fs::symlink_metadata(root.join("dangling")).is_ok());
        assert!(root.join("holding/a").exists());
    }
}
