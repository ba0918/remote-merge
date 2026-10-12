//! 同じ中身のツリーを、スクリプトをこのマシンの sh と find で動かす集め方と、ローカルと
//! エージェントの集め方で走査し、結果が同じになることを確かめる（REQ-scan-017）
use std::fs;

use remote_merge_core::scan::outcome::{ScanScope, Start};
use remote_merge_core::scan::this_machine::MachineCollector;

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
