use crossterm::event::{KeyCode, KeyModifiers};
use remote_merge::app::{AppState, Side};
use remote_merge::config::load_config_from_paths;
use remote_merge::handler::tree_keys::handle_tree_key;
use remote_merge::runtime::{RuntimeTargets, TuiRuntime};
use remote_merge::tree::{FileNode, FileTree};
use remote_merge::ui::dialog::DialogState;
use std::fs;
use tempfile::TempDir;

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

// @kotowari[EX-tui-011]
#[test]
fn tree_overview_shows_each_file_and_which_side_differs_from_reference() {
    let workspace = TempDir::new().unwrap();
    let config_path = workspace.path().join("config.toml");
    fs::write(
        &config_path,
        format!(
            "[local]\nroot_dir = {:?}\n",
            workspace.path().display().to_string()
        ),
    )
    .unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    let mut runtime = TuiRuntime::with_targets(config, RuntimeTargets::production());
    let files = ["first.txt", "second.txt"];
    let mut state = AppState::new(
        tree(&files),
        tree(&files),
        Side::Local,
        Side::Remote("develop".into()),
        "default",
    );
    state.set_reference(Side::Remote("reference".into()), tree(&files));
    for (path, left, right, reference) in [
        ("first.txt", "base", "updated", "base"),
        ("second.txt", "local edit", "base", "base"),
    ] {
        state.left_cache.insert(path.into(), left.into());
        state.right_cache.insert(path.into(), right.into());
        state.ref_cache.insert(path.into(), reference.into());
    }
    handle_tree_key(
        &mut state,
        &mut runtime,
        KeyCode::Char('W'),
        KeyModifiers::NONE,
    );
    let DialogState::ThreeWayOverview(overview) = &state.dialog else {
        panic!("file overview not shown")
    };
    assert_eq!(overview.files.len(), 2);
    assert_eq!(overview.files[0].path, "first.txt");
    assert_eq!(overview.files[0].left_matches_reference, Some(true));
    assert_eq!(overview.files[0].right_matches_reference, Some(false));
    assert_eq!(overview.files[1].path, "second.txt");
    assert_eq!(overview.files[1].left_matches_reference, Some(false));
    assert_eq!(overview.files[1].right_matches_reference, Some(true));
    let backend = ratatui::backend::TestBackend::new(120, 40);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| remote_merge::ui::render::draw_ui(frame, &state))
        .unwrap();
    let screen = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(
        screen.contains("first.txt"),
        "overview does not show first file"
    );
    assert!(
        screen.contains("second.txt"),
        "overview does not show second file"
    );
    assert!(
        screen.contains("different"),
        "overview does not distinguish changed files"
    );
}
