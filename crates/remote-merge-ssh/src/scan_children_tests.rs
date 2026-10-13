//! 子の一覧を、スクリプトをこのマシンの sh と find で動かす集め方と、ローカルとエージェントの
//! 集め方で取り、結果が同じになることと、-printf を受け付けない find では道具がないで止まることを
//! 確かめる（docs/ir/paths/operations.md の REQ-paths-017、docs/ir/ssh/compatibility.md の
//! REQ-ssh-026）
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;

use remote_merge_core::op_failure::Tool;
use remote_merge_core::scan::outcome::ScanScope;
use remote_merge_core::scan::raw::Depth;
use remote_merge_core::scan::this_machine::MachineCollector;

use crate::scan_collect::{collect_with, CollectFailure};
use crate::scan_test_shell::*;

fn children(path: &str) -> ScanScope {
    ScanScope::Children(path.to_string())
}

fn agree(root: &std::path::Path, rule: &remote_merge_core::scan::exclude::Exclude, path: &str) {
    let shell = scan_with(&mut Shell(root.to_path_buf()), rule, children(path));
    let machine = scan_with(
        &mut MachineCollector::new(root.to_path_buf()),
        rule,
        children(path),
    );
    assert_eq!(shell, machine, "{path:?}");
}

// @kotowari[REQ-paths-017, REQ-paths-013]
#[test]
fn the_children_over_the_shell_agree_with_the_children_on_this_machine() {
    let temp = tricky_root();
    let rule = exclude(&["c?che"]);
    for path in [
        "",
        "d",
        "to_dir",
        "d/sub",
        "x\ty.txt",
        "nope",
        "dangling",
        "loop",
        "through_file",
        "cache",
    ] {
        agree(temp.path(), &rule, path);
    }
}

// @kotowari[REQ-paths-017]
#[test]
fn a_child_directory_that_cannot_be_read_is_listed_without_reading_it() {
    let temp = remote_merge_core::test_tree::TestTree::new();
    let root = temp.path();
    fs::create_dir_all(root.join("a/locked/deep")).unwrap();
    fs::create_dir_all(root.join("a/empty")).unwrap();
    fs::write(root.join("a/f.txt"), "f").unwrap();
    temp.lock(root.join("a/locked"), 0o000);
    fs::create_dir_all(root.join("shut/inner")).unwrap();
    temp.lock(root.join("shut"), 0o000);

    let rule = exclude(&[]);
    let listed = scan_with(&mut Shell(root.to_path_buf()), &rule, children("a"));
    for path in ["a", "a/empty", "shut"] {
        agree(root, &rule, path);
    }

    assert!(listed.starts_with("Ok("), "{listed}");
    assert!(listed.contains("\"a/locked\""), "{listed}");
    assert!(!listed.contains("deep"), "{listed}");
}

/// -printf を受け付けない find（BusyBox の find と同じく、知らない式の誤りで 1 で終える）を
/// 置いた PATH
fn find_without_printf() -> tempfile::TempDir {
    let bin = tempfile::tempdir().unwrap();
    let find = bin.path().join("find");
    fs::write(
        &find,
        concat!(
            "#!/bin/sh\n",
            "for a do [ \"$a\" = -printf ] && { echo 'find: unrecognized: -printf' >&2; exit 1; }; done\n",
            "for d in /usr/bin /bin; do [ -x \"$d/find\" ] && exec \"$d/find\" \"$@\"; done\n",
            "exit 127\n",
        ),
    )
    .unwrap();
    fs::set_permissions(&find, fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

// @kotowari[REQ-ssh-026, EX-ssh-057]
#[test]
fn a_find_without_printf_stops_the_scan_and_the_listing_as_a_missing_tool() {
    let temp = tricky_root();
    let bin = find_without_printf();
    for depth in [Depth::All, Depth::Children] {
        let questions = [remote_merge_core::scan::raw::Question {
            path: Vec::new(),
            depth,
        }];
        let run = |script: &str| {
            shell_command_with_path(script, Some(bin.path()))
                .output()
                .map(|output| (output.stdout, output.stderr))
        };
        let result = collect_with(
            temp.path().as_os_str().as_bytes(),
            &questions,
            &exclude(&[]),
            crate::command_limit::MAX_COMMAND_LEN,
            run,
        );
        assert!(
            matches!(result, Err(CollectFailure::ToolMissing(Tool::FindPrintf))),
            "{depth:?} {result:?}"
        );
    }
}
