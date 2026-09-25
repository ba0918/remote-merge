use remote_merge::app::{AppState, Side};
use remote_merge::tree::{FileNode, FileTree};
use remote_merge::ui::dialog::DialogState;

fn tree(files: &[&str]) -> FileTree {
    FileTree {
        root: "/test".into(),
        nodes: files.iter().map(|name| FileNode::new_file(*name)).collect(),
    }
}

fn selected_file(left: &str, right: &str, reference: &str) -> AppState {
    let mut state = AppState::new(
        tree(&["file.txt"]),
        tree(&["file.txt"]),
        Side::Local,
        Side::Remote("develop".into()),
        "default",
    );
    state.set_reference(Side::Remote("reference".into()), tree(&["file.txt"]));
    state.left_cache.insert("file.txt".into(), left.into());
    state.right_cache.insert("file.txt".into(), right.into());
    state.ref_cache.insert("file.txt".into(), reference.into());
    state.tree_cursor = state
        .flat_nodes
        .iter()
        .position(|node| node.path == "file.txt")
        .unwrap();
    state.select_file();
    state
}

// @kotowari[EX-tui-009]
#[test]
fn three_distinct_versions_are_labelled_by_their_source_in_the_summary() {
    let mut state = selected_file("left\n", "right\n", "reference\n");
    state.open_three_way_summary();
    let DialogState::ThreeWaySummary(panel) = &state.dialog else {
        panic!("summary not shown")
    };
    assert_eq!(panel.left_label, "local");
    assert_eq!(panel.right_label, "develop");
    assert_eq!(panel.ref_label, "reference");
    assert!(
        panel
            .lines
            .iter()
            .any(|line| line.left_content.as_deref() == Some("left")
                && line.ref_content.as_deref() == Some("reference")),
        "{:?}",
        panel.lines
    );
    assert!(
        panel
            .lines
            .iter()
            .any(|line| line.right_content.as_deref() == Some("right")
                && line.ref_content.as_deref() == Some("reference")),
        "{:?}",
        panel.lines
    );
}

// @kotowari[EX-tui-010]
#[test]
fn summary_identifies_only_the_side_that_differs_from_reference() {
    let mut state = selected_file("shared\n", "changed\n", "shared\n");
    state.open_three_way_summary();
    let DialogState::ThreeWaySummary(panel) = &state.dialog else {
        panic!("summary not shown")
    };
    assert!(
        panel
            .lines
            .iter()
            .any(|line| line.left_content.as_deref() == Some("shared")
                && line.ref_content.as_deref() == Some("shared")),
        "{:?}",
        panel.lines
    );
    assert!(
        panel
            .lines
            .iter()
            .any(|line| line.right_content.as_deref() == Some("changed")
                && line.ref_content.as_deref() == Some("shared")),
        "{:?}",
        panel.lines
    );
}

// @kotowari[EX-tui-012]
#[test]
fn matching_versions_report_no_three_way_disagreement() {
    let mut state = selected_file("same\n", "same\n", "same\n");
    state.open_three_way_summary();
    assert!(matches!(state.dialog, DialogState::None));
    assert!(
        state.status_message.contains("equal"),
        "{}",
        state.status_message
    );
}
