use std::fs;

use crossterm::event::KeyCode;
use remote_merge::app::clipboard_write::ClipboardResult;
use remote_merge::app::{AppState, Side};
use remote_merge::config::load_config_from_paths;
use remote_merge::handler::dialog_keys::handle_dialog_key;
use remote_merge::handler::tree_keys::{copy_diff_with, export_report_to};
use remote_merge::runtime::{RuntimeTargets, TuiRuntime};
use remote_merge::tree::{FileNode, FileTree};
use remote_merge::ui::dialog::DialogState;
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

// @kotowari[EX-tui-017]
#[test]
fn declining_a_sensitive_copy_keeps_its_body_off_the_clipboard() {
    let mut state = state(true);
    state.sensitive_patterns = vec!["one.txt".into()];
    state.tree_cursor = state
        .flat_nodes
        .iter()
        .position(|node| node.path == "one.txt")
        .unwrap();
    state.select_file();
    copy_diff_with(&mut state, |_| panic!("copy must wait for approval"));
    assert!(matches!(&state.dialog, DialogState::SensitiveCopy(path) if path == "one.txt"));
    let directory = TempDir::new().unwrap();
    let config_file = directory.path().join("config.toml");
    fs::write(
        &config_file,
        format!(
            "[local]\nroot_dir = {:?}\n",
            directory.path().display().to_string()
        ),
    )
    .unwrap();
    let config = load_config_from_paths(Some(&config_file), None).unwrap();
    let mut runtime = TuiRuntime::with_targets(config, RuntimeTargets::production());
    handle_dialog_key(&mut state, &mut runtime, KeyCode::Char('n'));
    assert!(matches!(state.dialog, DialogState::None));
    assert!(
        state.status_message.contains("cancelled"),
        "{}",
        state.status_message
    );
}

// @kotowari[EX-tui-018]
#[test]
fn approving_a_sensitive_report_writes_the_named_file_only_after_confirmation() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("approved-report.md");
    let mut state = state(true);
    state.sensitive_patterns = vec!["one.txt".into()];
    export_report_to(&mut state, &path);
    assert!(!path.exists(), "sensitive body must not be exported yet");
    assert!(
        matches!(&state.dialog, DialogState::SensitiveReport { paths, destination }
        if paths == &["one.txt"] && destination == &path)
    );
    let config_file = directory.path().join("config.toml");
    fs::write(
        &config_file,
        format!(
            "[local]\nroot_dir = {:?}\n",
            directory.path().display().to_string()
        ),
    )
    .unwrap();
    let config = load_config_from_paths(Some(&config_file), None).unwrap();
    let mut runtime = TuiRuntime::with_targets(config, RuntimeTargets::production());
    handle_dialog_key(&mut state, &mut runtime, KeyCode::Char('y'));
    let report = fs::read_to_string(path).unwrap();
    assert!(
        report.contains("-old one") && report.contains("+new one"),
        "{report}"
    );
    assert!(report.contains("+new two"), "{report}");
}

// @kotowari[REQ-tui-009]
#[test]
fn declining_a_sensitive_report_leaves_the_destination_absent() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("private-report.md");
    let mut state = state(true);
    state.sensitive_patterns = vec!["one.txt".into()];
    export_report_to(&mut state, &path);
    assert!(matches!(state.dialog, DialogState::SensitiveReport { .. }));
    let config_file = directory.path().join("config.toml");
    fs::write(
        &config_file,
        format!(
            "[local]\nroot_dir = {:?}\n",
            directory.path().display().to_string()
        ),
    )
    .unwrap();
    let config = load_config_from_paths(Some(&config_file), None).unwrap();
    let mut runtime = TuiRuntime::with_targets(config, RuntimeTargets::production());
    handle_dialog_key(&mut state, &mut runtime, KeyCode::Char('n'));
    assert!(!path.exists());
    assert!(matches!(state.dialog, DialogState::None));
}
