#![cfg(unix)]
//! 書き込みと symlink の作成が失敗した後に、作ったディレクトリを消せたかの判断（docs/ir/merge/permissions.md
//! の REQ-merge-042）の契約テスト。消えていたものは消せたもの、別のもの（ディレクトリでないもの）に
//! 替わっていたものは残ったものにする。ローカルとエージェントは同じ `machine_ops::remove_created_dir`
//! を使う。

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use remote_merge_core::machine_ops::remove_created_dir;

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

// @kotowari[REQ-merge-042]
#[test]
fn whether_a_created_directory_was_removed_is_judged_by_what_is_there() {
    let local = tempfile::tempdir().unwrap();
    state(local.path());

    for (name, expected) in STATES {
        let on_local = remove_created_dir(&local.path().join(name)).is_ok();

        assert_eq!(on_local, expected, "{name}");
    }
    let root = local.path();
    assert!(root.join("file").is_file());
    assert!(fs::symlink_metadata(root.join("dangling")).is_ok());
    assert!(root.join("holding/a").exists());
}
