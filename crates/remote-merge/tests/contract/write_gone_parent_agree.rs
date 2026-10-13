#![cfg(unix)]
//! 確かめたときにあった書き込み先が、開く前に親ごと、または root_dir ごと消えていたときの答えが三つの
//! 経路で同じであること（docs/ir/merge/concurrency.md の REQ-merge-043、docs/ir/merge/permissions.md の
//! REQ-merge-042、docs/decision/records/2026-10-12-foundation-3.md の D45・D47）の契約テスト。親ごと
//! 消えていれば、どの経路も root_dir の下のない親を作り直して新しいファイルとして作る。root_dir ごと
//! 消えていれば、どの経路も何も作らずに root_dir がない失敗にする。ローカルとエージェントは同じ
//! `machine_ops::open_for_write_with` を使い、SSH の経路は書き込みのコマンドを sh で動かす（消すのは
//! PATH に置いた dd の最初の呼び出しで起こす）。

use std::fs;
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

use remote_merge_core::machine_ops::open_for_write_with;
use remote_merge_core::op_failure::OpFailure;
use remote_merge_core::relative_path::EntryPath;
use remote_merge_ssh::change_scripts::{failure, write_under};

/// `root/sub/a.txt` に古い中身を置く
fn destination(root: &Path) -> std::path::PathBuf {
    fs::create_dir_all(root.join("sub")).unwrap();
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

/// ローカル（とエージェント）の経路で、確かめた後に `gone` を消して `root` の下の "sub/a.txt" に書く
fn written_on_this_machine(root: &Path, gone: &Path) -> Result<bool, OpFailure> {
    let mut opened = open_for_write_with(root, &EntryPath::parse("sub/a.txt").unwrap(), |_| {
        fs::remove_dir_all(gone).unwrap()
    })?;
    opened.file.write_all(b"new").unwrap();
    Ok(opened.created)
}

/// SSH の経路で、確かめた後に `gone` を消して `root` の下の "sub/a.txt" に書く
fn written_over_ssh(root: &Path, gone: &Path) -> Result<(), OpFailure> {
    let bin = tempfile::tempdir().unwrap();
    put_dd_removing_first(bin.path(), gone);
    let full = root.join("sub/a.txt");
    let mut child = Command::new("/bin/sh")
        .args([
            "-c",
            &write_under(root.to_str().unwrap(), full.to_str().unwrap(), false),
        ])
        .env("PATH", format!("{}:/usr/bin:/bin", bin.path().display()))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"new").unwrap();
    let output = child.wait_with_output().unwrap();
    if output.status.success() {
        Ok(())
    } else {
        Err(failure(&output.stderr))
    }
}

// @kotowari[REQ-merge-043]
#[test]
fn a_destination_gone_with_its_parent_after_the_check_is_made_again_on_every_route() {
    let local = tempfile::tempdir().unwrap();
    let local_path = destination(local.path());
    let ssh = tempfile::tempdir().unwrap();
    let ssh_path = destination(ssh.path());

    let on_local = written_on_this_machine(local.path(), &local.path().join("sub"));
    let on_ssh = written_over_ssh(ssh.path(), &ssh.path().join("sub"));

    // ローカルは作ったものとして返す（sudo のエージェントが所有者を付ける）
    assert_eq!(on_local, Ok(true));
    assert_eq!(on_ssh, Ok(()));
    assert_eq!(fs::read(&local_path).unwrap(), b"new");
    assert_eq!(fs::read(&ssh_path).unwrap(), b"new");
}

// @kotowari[REQ-merge-042]
#[test]
fn a_destination_gone_with_its_root_dir_after_the_check_makes_nothing_on_every_route() {
    let local = tempfile::tempdir().unwrap();
    let local_root = local.path().join("root");
    destination(&local_root);
    let ssh = tempfile::tempdir().unwrap();
    let ssh_root = ssh.path().join("root");
    destination(&ssh_root);

    let on_local = written_on_this_machine(&local_root, &local_root);
    let on_ssh = written_over_ssh(&ssh_root, &ssh_root);

    assert_eq!(on_local, Err(OpFailure::RootGone));
    assert_eq!(on_ssh, Err(OpFailure::RootGone));
    assert!(!local_root.exists());
    assert!(!ssh_root.exists());
}
