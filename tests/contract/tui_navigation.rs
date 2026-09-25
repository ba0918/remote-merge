use std::path::Path;

use crossterm::event::KeyCode;
use remote_merge::app::{AppState, Side};
use remote_merge::diff::engine::DiffResult;
use remote_merge::handler::search_keys::handle_search_key;
use remote_merge::tree::{FileNode, FileTree};

fn right_tree(files: &[&str]) -> FileTree {
    FileTree {
        root: "/test".into(),
        nodes: vec![FileNode::new_dir_with_children(
            "folder",
            files.iter().map(|name| FileNode::new_file(*name)).collect(),
        )],
    }
}

fn state(files: &[&str]) -> AppState {
    let mut state = AppState::new(
        right_tree(files),
        right_tree(files),
        Side::Local,
        Side::Remote("first".into()),
        "default",
    );
    state.expanded_dirs.insert("folder".into());
    state.rebuild_flat_nodes();
    state
}

// @kotowari[EX-tui-005]
#[test]
fn file_search_moves_the_tree_cursor_to_a_matching_name() {
    let mut state = state(&["main.rs", "README.md"]);
    state.search_state.activate();
    for key in "README".chars() {
        handle_search_key(&mut state, KeyCode::Char(key));
    }
    assert_eq!(state.flat_nodes[state.tree_cursor].path, "folder/README.md");
    assert!(state.status_message.contains("README"));
}

// @kotowari[EX-tui-006]
#[test]
fn searching_for_an_absent_name_reports_no_matches() {
    let mut state = state(&["main.rs", "README.md"]);
    state.search_state.activate();
    for key in "missing".chars() {
        handle_search_key(&mut state, KeyCode::Char(key));
    }
    assert!(
        state.status_message.contains("no match"),
        "{}",
        state.status_message
    );
    assert!(state
        .flat_nodes
        .iter()
        .all(|node| node.path != "folder/missing"));
}

// @kotowari[EX-tui-007]
#[test]
fn switching_servers_keeps_a_selected_path_and_its_expanded_parent() {
    let mut state = state(&["shared.txt"]);
    state.tree_cursor = state
        .flat_nodes
        .iter()
        .position(|node| node.path == "folder/shared.txt")
        .unwrap();
    state.select_file();
    assert_eq!(state.selected_path.as_deref(), Some("folder/shared.txt"));
    state.switch_server(Side::Remote("second".into()), right_tree(&["shared.txt"]));
    assert!(state.expanded_dirs.contains("folder"));
    assert_eq!(state.selected_path.as_deref(), Some("folder/shared.txt"));
    assert_eq!(
        state.flat_nodes[state.tree_cursor].path,
        "folder/shared.txt"
    );
    assert!(state
        .right_tree
        .find_node(Path::new("folder/shared.txt"))
        .is_some());
}

// @kotowari[EX-tui-008]
#[test]
fn switching_servers_clears_selection_if_the_new_server_lacks_the_path() {
    let mut state = state(&["old.txt"]);
    state.tree_cursor = state
        .flat_nodes
        .iter()
        .position(|node| node.path == "folder/old.txt")
        .unwrap();
    state.select_file();
    state.switch_server(Side::Remote("second".into()), right_tree(&["other.txt"]));
    assert_ne!(state.selected_path.as_deref(), Some("folder/old.txt"));
    assert!(state
        .flat_nodes
        .get(state.tree_cursor)
        .is_none_or(|node| node.path != "folder/old.txt"));
}

// @kotowari[EX-tui-001]
#[test]
fn expanding_a_directory_and_selecting_its_file_shows_both_sides_diff() {
    let mut state = state(&["changed.txt"]);
    state.expanded_dirs.clear();
    state.rebuild_flat_nodes();
    assert!(!state
        .flat_nodes
        .iter()
        .any(|node| node.path == "folder/changed.txt"));
    state.tree_cursor = state
        .flat_nodes
        .iter()
        .position(|node| node.path == "folder")
        .unwrap();
    state.toggle_expand();
    state
        .left_cache
        .insert("folder/changed.txt".into(), "before\n".into());
    state
        .right_cache
        .insert("folder/changed.txt".into(), "after\n".into());
    state.tree_cursor = state
        .flat_nodes
        .iter()
        .position(|node| node.path == "folder/changed.txt")
        .unwrap();
    state.select_file();
    assert_eq!(state.selected_path.as_deref(), Some("folder/changed.txt"));
    assert!(matches!(
        state.current_diff,
        Some(DiffResult::Modified { .. })
    ));
}

// @kotowari[EX-tui-002]
#[test]
fn choosing_another_file_replaces_the_diff_being_viewed() {
    let mut state = state(&["first.txt", "second.txt"]);
    state
        .left_cache
        .insert("folder/first.txt".into(), "old first\n".into());
    state
        .right_cache
        .insert("folder/first.txt".into(), "new first\n".into());
    state
        .left_cache
        .insert("folder/second.txt".into(), "old second\n".into());
    state
        .right_cache
        .insert("folder/second.txt".into(), "new second\n".into());
    state.tree_cursor = state
        .flat_nodes
        .iter()
        .position(|node| node.path == "folder/first.txt")
        .unwrap();
    state.select_file();
    assert_eq!(state.selected_path.as_deref(), Some("folder/first.txt"));
    state.tree_cursor = state
        .flat_nodes
        .iter()
        .position(|node| node.path == "folder/second.txt")
        .unwrap();
    state.select_file();
    assert_eq!(state.selected_path.as_deref(), Some("folder/second.txt"));
    let Some(DiffResult::Modified { lines, .. }) = &state.current_diff else {
        panic!("expected second diff")
    };
    assert!(format!("{lines:?}").contains("new second"), "{lines:?}");
    assert!(!format!("{lines:?}").contains("new first"), "{lines:?}");
}
