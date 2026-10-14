#![cfg(unix)]
//! 確かめたときにあった書き込み先が、開く前に親ごと、または root_dir ごと消えていたときの答え
//! （docs/ir/merge/concurrency.md の REQ-merge-043、docs/ir/merge/permissions.md の REQ-merge-042、
//! docs/decision/records/2026-10-12-foundation-3.md の D45・D47）の契約テスト。親ごと消えていれば、
//! root_dir の下のない親を作り直して新しいファイルとして作る。root_dir ごと消えていれば、何も作らずに
//! root_dir がない失敗にする。ローカルとエージェントは同じ `machine_ops::open_for_write_with` を使う。

use std::fs;
use std::io::Write as _;
use std::path::Path;

use remote_merge_core::machine_ops::open_for_write_with;
use remote_merge_core::op_failure::OpFailure;
use remote_merge_core::relative_path::EntryPath;

/// `root/sub/a.txt` に古い中身を置く
fn destination(root: &Path) -> std::path::PathBuf {
    fs::create_dir_all(root.join("sub")).unwrap();
    let path = root.join("sub/a.txt");
    fs::write(&path, "old").unwrap();
    path
}

/// 確かめた後に `gone` を消して `root` の下の "sub/a.txt" に書く
fn written_on_this_machine(root: &Path, gone: &Path) -> Result<bool, OpFailure> {
    let mut opened = open_for_write_with(root, &EntryPath::parse("sub/a.txt").unwrap(), |_| {
        fs::remove_dir_all(gone).unwrap()
    })?;
    opened.file.write_all(b"new").unwrap();
    Ok(opened.created)
}

// @kotowari[REQ-merge-043]
#[test]
fn a_destination_gone_with_its_parent_after_the_check_is_made_again() {
    let local = tempfile::tempdir().unwrap();
    let local_path = destination(local.path());

    let on_local = written_on_this_machine(local.path(), &local.path().join("sub"));

    // 作ったものとして返す（sudo のエージェントが所有者を付ける）
    assert_eq!(on_local, Ok(true));
    assert_eq!(fs::read(&local_path).unwrap(), b"new");
}

// @kotowari[REQ-merge-042]
#[test]
fn a_destination_gone_with_its_root_dir_after_the_check_makes_nothing() {
    let local = tempfile::tempdir().unwrap();
    let local_root = local.path().join("root");
    destination(&local_root);

    let on_local = written_on_this_machine(&local_root, &local_root);

    assert_eq!(on_local, Err(OpFailure::RootGone));
    assert!(!local_root.exists());
}
