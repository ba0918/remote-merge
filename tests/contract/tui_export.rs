use std::fs;

use remote_merge::app::clipboard_write::ClipboardResult;
use remote_merge::app::{AppState, Side};
use remote_merge::handler::tree_keys::{copy_diff_with, export_report_to};
use remote_merge::tree::{FileNode, FileTree};
use tempfile::TempDir;

fn tree() -> FileTree {
    FileTree {
        root: "/test".into(),
        nodes: vec![FileNode::new_file("one.txt"), FileNode::new_file("two.txt")],
    }
}

fn state(changed: bool) -> AppState {
    let mut state = AppState::new(
        tree(),
        tree(),
        Side::Local,
        Side::Remote("develop".into()),
        "default",
    );
    for (name, left, right) in [
        (
            "one.txt",
            "old one\n",
            if changed { "new one\n" } else { "old one\n" },
        ),
        (
            "two.txt",
            "old two\n",
            if changed { "new two\n" } else { "old two\n" },
        ),
    ] {
        state.left_cache.insert(name.into(), left.into());
        state.right_cache.insert(name.into(), right.into());
    }
    state
}

// @kotowari[EX-tui-015]
#[test]
fn exporting_two_differences_writes_both_diffs_and_a_summary() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("report.md");
    let mut state = state(true);
    export_report_to(&mut state, &path);
    let report = fs::read_to_string(&path).unwrap();
    assert!(report.contains("2 changed"), "{report}");
    assert!(
        report.contains("## one.txt") && report.contains("-old one") && report.contains("+new one"),
        "{report}"
    );
    assert!(
        report.contains("## two.txt") && report.contains("-old two") && report.contains("+new two"),
        "{report}"
    );
    assert!(
        state.status_message.contains("exported"),
        "{}",
        state.status_message
    );
}

// @kotowari[EX-tui-016]
#[test]
fn exporting_unchanged_files_does_not_create_a_report() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("report.md");
    let mut state = state(false);
    export_report_to(&mut state, &path);
    assert!(!path.exists(), "no diff report should be written");
    assert!(
        state.status_message.contains("No differences"),
        "{}",
        state.status_message
    );
}

// @kotowari[EX-tui-013]
#[test]
fn copying_the_selected_diff_sends_its_lines_to_the_clipboard() {
    let mut state = state(true);
    state.tree_cursor = state
        .flat_nodes
        .iter()
        .position(|node| node.path == "one.txt")
        .unwrap();
    state.select_file();
    let mut copied = String::new();
    copy_diff_with(&mut state, |text| {
        copied.push_str(text);
        ClipboardResult::Ok
    });
    assert!(copied.contains("-old one"), "{copied}");
    assert!(copied.contains("+new one"), "{copied}");
    assert!(!copied.contains("new two"), "{copied}");
    assert!(
        state.status_message.contains("copied"),
        "{}",
        state.status_message
    );
}
