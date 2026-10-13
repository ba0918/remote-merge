#![cfg(unix)]
//! 確かめたときにあった書き込み先が、開く前に親ごと消えていたときの答えが三つの経路で同じであること
//! （docs/ir/merge/concurrency.md の REQ-merge-043、docs/decision/records/2026-10-12-foundation-3.md の
//! D45）の契約テスト。どの経路も、ない親を作り直して新しいファイルとして作る。ローカルとエージェントは
//! 同じ `machine_ops::open_for_write_with` を使い、SSH の経路は書き込みのコマンドを sh で動かす（消すのは
//! PATH に置いた dd の最初の呼び出しで起こす）。

use std::fs;
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

use remote_merge_core::machine_ops::open_for_write_with;
use remote_merge_core::relative_path::EntryPath;
use remote_merge_ssh::change_scripts::write;

/// `root/sub/a.txt` に古い中身を置く
fn destination(root: &Path) -> std::path::PathBuf {
    fs::create_dir(root.join("sub")).unwrap();
    let path = root.join("sub/a.txt");
    fs::write(&path, "old").unwrap();
    path
}

/// 最初に呼ばれたときだけ `gone` を中身ごと消してから本物の dd を動かす dd を `bin` に置く
fn put_dd_removing_first(bin: &Path, gone: &Path) {
    let flag = bin.join("removed");
    let path = bin.join("dd");
    fs::write(
        &path,
        format!(
            "#!/bin/sh\n[ -e '{flag}' ] || {{ : >'{flag}'; rm -rf '{gone}'; }}\nfor d in /usr/bin /bin; do [ -x \"$d/dd\" ] && exec \"$d/dd\" \"$@\"; done\nexit 127\n",
            flag = flag.display(),
            gone = gone.display(),
        ),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// SSH の経路の書き込みのコマンドが 0 で終わったか
fn written_over_ssh(bin: &Path, path: &Path, content: &[u8]) -> bool {
    let mut child = Command::new("/bin/sh")
        .args(["-c", &write(path.to_str().unwrap(), false)])
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(content).unwrap();
    child.wait().unwrap().success()
}

// @kotowari[REQ-merge-043]
#[test]
fn a_destination_gone_with_its_parent_after_the_check_is_made_again_on_every_route() {
    let local = tempfile::tempdir().unwrap();
    let local_path = destination(local.path());
    let mut opened = open_for_write_with(
        local.path(),
        &EntryPath::parse("sub/a.txt").unwrap(),
        |path| fs::remove_dir_all(path.parent().unwrap()).unwrap(),
    )
    .unwrap();
    opened.file.write_all(b"new").unwrap();

    let ssh = tempfile::tempdir().unwrap();
    let ssh_path = destination(ssh.path());
    let bin = tempfile::tempdir().unwrap();
    put_dd_removing_first(bin.path(), &ssh.path().join("sub"));
    let on_ssh = written_over_ssh(bin.path(), &ssh_path, b"new");

    // ローカルは作ったものとして返す（sudo のエージェントが所有者を付ける）
    assert!(opened.created);
    assert!(on_ssh);
    assert_eq!(fs::read(&local_path).unwrap(), b"new");
    assert_eq!(fs::read(&ssh_path).unwrap(), b"new");
}
