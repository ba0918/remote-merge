//! 同じ中身のツリーを、スクリプトをこのマシンの sh と find で動かす集め方と、ローカルと
//! エージェントの集め方で走査し、結果が同じになることを確かめる（REQ-scan-017）
use std::fs;

use remote_merge_core::scan::outcome::{ScanScope, Start};
use remote_merge_core::scan::this_machine::MachineCollector;
use remote_merge_core::test_tree::TestTree;

use crate::scan_test_shell::*;

// @kotowari[REQ-scan-017, REQ-scan-015]
#[test]
fn the_scan_over_the_shell_agrees_with_the_scan_on_this_machine() {
    let temp = tricky_root();
    fs::write(temp.path().join("file"), "").unwrap();
    let scopes = [
        (temp.path().to_path_buf(), ScanScope::Whole),
        (temp.path().join("gone"), ScanScope::Whole),
        (temp.path().join("file"), ScanScope::Whole),
        (
            temp.path().to_path_buf(),
            ScanScope::Starts(vec![
                Start::below("d", 0),
                Start::below("to_dir", 1),
                Start::below("gone", 2),
            ]),
        ),
        (
            temp.path().to_path_buf(),
            ScanScope::Starts(vec![
                Start::value("to_dir"),
                Start::value("dangling"),
                Start::value("d/sub/f.txt"),
            ]),
        ),
    ];
    for patterns in [
        &["loop", "through_file"][..],
        &["l*p", "through_file", "c?che"],
    ] {
        let rule = exclude(patterns);
        for (root, scope) in &scopes {
            let shell = scan_with(&mut Shell(root.clone()), &rule, scope.clone());
            let machine = scan_with(
                &mut MachineCollector::new(root.clone()),
                &rule,
                scope.clone(),
            );
            assert_eq!(shell, machine, "{patterns:?} {root:?} {scope:?}");
        }
    }
}

/// root でない利用者として動いているか（root は権限のないディレクトリも読める）
fn permissions_apply(dir: &std::path::Path) -> bool {
    fs::read_dir(dir).is_err()
}

// @kotowari[REQ-scan-017, REQ-config-044]
#[test]
fn a_directory_without_search_permission_holding_only_excluded_entries_agrees() {
    for pattern in ["cache", "c?che"] {
        let temp = TestTree::new();
        let root = temp.path();
        fs::create_dir_all(root.join("d/cache")).unwrap();
        fs::write(root.join("top.txt"), "t").unwrap();
        temp.lock(root.join("d"), 0o644);
        let applies = fs::symlink_metadata(root.join("d/cache")).is_err();
        let rule = exclude(&[pattern]);
        let machine = scan_with(
            &mut MachineCollector::new(root.to_path_buf()),
            &rule,
            ScanScope::Whole,
        );
        let shell = scan_with(&mut Shell(root.to_path_buf()), &rule, ScanScope::Whole);
        if !applies {
            eprintln!("note: running as root; entries can be stat'ed without permission");
            return;
        }
        assert!(machine.starts_with("Ok("), "{pattern}: {machine}");
        assert_eq!(shell, machine, "{pattern}");
    }
}

// @kotowari[REQ-scan-017, REQ-scan-014, REQ-config-044]
#[test]
fn unreadable_directories_with_odd_names_agree() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    for name in [
        &b"lo\ncked"[..],
        b"t\tab",
        b"bad\xff",
        b"q'uo: te",
        b"back\\sl",
    ] {
        for parent in ["cache", "plain"] {
            let temp = TestTree::new();
            let root = temp.path();
            let dir = root.join(parent).join(OsStr::from_bytes(name));
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("h.txt"), "h").unwrap();
            temp.lock(&dir, 0o000);
            let applies = permissions_apply(&dir);
            let rule = exclude(&["c?che"]);
            let machine = scan_with(
                &mut MachineCollector::new(root.to_path_buf()),
                &rule,
                ScanScope::Whole,
            );
            let shell = scan_with(&mut Shell(root.to_path_buf()), &rule, ScanScope::Whole);
            if !applies {
                eprintln!("note: running as root; unreadable directories cannot be made");
                return;
            }
            assert_eq!(shell, machine, "{name:?} under {parent}");
        }
    }
}
