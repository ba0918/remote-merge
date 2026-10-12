//! 空のディレクトリを含むツリーを、スクリプトをこのマシンの sh と find で動かす集め方と、
//! ローカルとエージェントの集め方で走査し、結果が同じになることを確かめる（REQ-scan-017）。
//! 空に見えたディレクトリにもう一度尋ねても、結果は変わらない
use std::fs;
use std::os::unix::fs::symlink;

use remote_merge_core::scan::outcome::{ScanScope, Start};
use remote_merge_core::scan::this_machine::MachineCollector;

use crate::scan_test_shell::*;

// @kotowari[REQ-scan-017, REQ-config-044]
#[test]
fn a_tree_with_empty_directories_scans_the_same_on_every_route() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for dir in [
        "e1",
        "a/e2",
        "a/b/e3",
        ".hidden_empty",
        "only_cache/cache",
        "only_path/x",
        "only_tmp",
        "only_c2che",
    ] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    fs::write(root.join("only_cache/cache/c.txt"), "c").unwrap();
    fs::write(root.join("only_path/x/p.txt"), "p").unwrap();
    fs::write(root.join("only_tmp/t.tmp"), "t").unwrap();
    fs::write(root.join("only_c2che/c2che"), "q").unwrap();
    fs::write(root.join("a/f.txt"), "f").unwrap();
    symlink("e1", root.join("to_empty")).unwrap();
    let empty_root = tempfile::tempdir().unwrap();
    let scopes = [
        (root.to_path_buf(), ScanScope::Whole),
        (empty_root.path().to_path_buf(), ScanScope::Whole),
        (
            root.to_path_buf(),
            ScanScope::Starts(vec![
                Start::below("e1", 0),
                Start::below("to_empty", 1),
                Start::value("a/e2"),
            ]),
        ),
    ];
    for patterns in [
        &[][..],
        &["cache", "only_path/x", "*.tmp", "c?che"],
        &["e1", "a/**"],
    ] {
        let rule = exclude(patterns);
        for (root, scope) in &scopes {
            let shell = scan_with(&mut Shell(root.clone()), &rule, scope.clone());
            let machine = scan_with(
                &mut MachineCollector::new(root.clone()),
                &rule,
                scope.clone(),
            );
            assert!(shell.starts_with("Ok("), "{shell}");
            assert_eq!(shell, machine, "{patterns:?} {root:?} {scope:?}");
        }
    }
}
