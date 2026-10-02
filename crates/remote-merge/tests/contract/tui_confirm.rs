#![cfg(unix)]

use std::fs;

use crossterm::event::KeyCode;
use remote_merge::app::{AppState, Side};
use remote_merge::config::load_config_from_paths;
use remote_merge::diff::engine::HunkDirection;
use remote_merge::handler::dialog_keys::handle_dialog_key;
use remote_merge::local::scan_local_tree;
use remote_merge::merge::executor::MergeDirection;
use remote_merge::runtime::{RuntimeTargets, TuiRuntime};
use remote_merge::theme::DEFAULT_THEME;
use remote_merge::ui::dialog::{DialogState, HunkMergePreview};
use tempfile::TempDir;

fn setup() -> (TempDir, TempDir, AppState, TuiRuntime) {
    let local = TempDir::new().unwrap();
    let source = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "original\n").unwrap();
    fs::write(source.path().join("file.txt"), "replacement\n").unwrap();
    let config_path = local.path().join("test.toml");
    fs::write(&config_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[backup]\nenabled = false\n",
        local.path().display().to_string(), source.path().display().to_string(),
    )).unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    let left_tree = scan_local_tree(local.path(), &[]).unwrap();
    let right_tree = scan_local_tree(source.path(), &[]).unwrap();
    let mut state = AppState::new(
        left_tree,
        right_tree,
        Side::Local,
        Side::Remote("develop".into()),
        DEFAULT_THEME,
    );
    state.tree_cursor = state
        .flat_nodes
        .iter()
        .position(|node| node.path == "file.txt")
        .unwrap();
    state
        .left_cache
        .insert("file.txt".into(), "original\n".into());
    state
        .right_cache
        .insert("file.txt".into(), "replacement\n".into());
    state.select_file();
    let runtime = TuiRuntime::with_targets(
        config,
        RuntimeTargets::production()
            .with_local("develop", source.path())
            .with_startup_directory(local.path().to_path_buf()),
    );
    (local, source, state, runtime)
}

// @kotowari[EX-tui-003]
#[test]
fn declining_a_merge_confirmation_keeps_the_destination_unchanged() {
    let (local, _source, mut state, mut runtime) = setup();
    state.show_merge_dialog(MergeDirection::RightToLeft);
    assert!(matches!(state.dialog, DialogState::Confirm(_)));
    handle_dialog_key(&mut state, &mut runtime, KeyCode::Char('n'));
    assert_eq!(
        fs::read_to_string(local.path().join("file.txt")).unwrap(),
        "original\n"
    );
    assert!(matches!(state.dialog, DialogState::None));
    assert!(state.status_message.contains("cancelled"));
}

// @kotowari[EX-tui-004]
#[test]
fn accepting_a_merge_confirmation_updates_the_destination() {
    let (local, source, mut state, mut runtime) = setup();
    state.show_merge_dialog(MergeDirection::RightToLeft);
    assert!(matches!(state.dialog, DialogState::Confirm(_)));
    assert_eq!(
        fs::read_to_string(local.path().join("file.txt")).unwrap(),
        "original\n"
    );
    handle_dialog_key(&mut state, &mut runtime, KeyCode::Char('y'));
    assert_eq!(
        fs::read_to_string(local.path().join("file.txt")).unwrap(),
        "replacement\n"
    );
    assert_eq!(
        fs::read_to_string(source.path().join("file.txt")).unwrap(),
        "replacement\n"
    );
    assert!(matches!(state.dialog, DialogState::None));
}

// @kotowari[EX-merge-019]
#[test]
fn declining_a_selected_hunk_keeps_the_destination_unchanged() {
    let (local, _source, mut state, mut runtime) = setup();
    let direction = HunkDirection::RightToLeft;
    state.stage_hunk_merge(direction);
    let (before, after) = state
        .preview_hunk_merge(direction)
        .expect("selected hunk preview");
    state.dialog = DialogState::HunkMergePreview(HunkMergePreview::new(
        "file.txt".into(),
        direction,
        before,
        after,
    ));
    assert!(state.pending_hunk_merge.is_some());
    handle_dialog_key(&mut state, &mut runtime, KeyCode::Char('n'));
    assert_eq!(
        fs::read_to_string(local.path().join("file.txt")).unwrap(),
        "original\n"
    );
    assert!(state.pending_hunk_merge.is_none());
    assert!(matches!(state.dialog, DialogState::None));
}

// @kotowari[EX-merge-020]
#[test]
fn an_external_edit_to_a_hunk_destination_is_reported_without_overwriting_it() {
    let (local, _source, mut state, mut runtime) = setup();
    let direction = HunkDirection::RightToLeft;
    state.stage_hunk_merge(direction);
    let (before, after) = state
        .preview_hunk_merge(direction)
        .expect("selected hunk preview");
    state.dialog = DialogState::HunkMergePreview(HunkMergePreview::new(
        "file.txt".into(),
        direction,
        before,
        after,
    ));
    let target = local.path().join("file.txt");
    fs::write(&target, "another writer\n").unwrap();
    let future = std::time::SystemTime::now() + std::time::Duration::from_secs(10);
    fs::OpenOptions::new()
        .write(true)
        .open(&target)
        .unwrap()
        .set_modified(future)
        .unwrap();
    handle_dialog_key(&mut state, &mut runtime, KeyCode::Char('y'));
    assert!(
        matches!(state.dialog, DialogState::MtimeWarning(_)),
        "{}",
        state.status_message
    );
    assert_eq!(fs::read_to_string(target).unwrap(), "another writer\n");
}

// @kotowari[REQ-merge-011]
#[test]
fn a_merge_confirmation_does_not_overwrite_an_external_edit_with_restored_timestamp() {
    let (local, _source, mut state, mut runtime) = setup();
    state.show_merge_dialog(MergeDirection::RightToLeft);
    assert!(matches!(state.dialog, DialogState::Confirm(_)));
    let target = local.path().join("file.txt");
    let timestamp = fs::metadata(&target).unwrap().modified().unwrap();
    fs::write(&target, "intruder\n").unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&target)
        .unwrap()
        .set_modified(timestamp)
        .unwrap();
    assert_eq!(
        fs::metadata(&target).unwrap().modified().unwrap(),
        timestamp
    );
    handle_dialog_key(&mut state, &mut runtime, KeyCode::Char('y'));
    assert_eq!(fs::read_to_string(target).unwrap(), "intruder\n");
    assert!(
        state.status_message.contains("changed"),
        "{}",
        state.status_message
    );
}

// @kotowari[REQ-merge-011]
#[test]
fn a_selected_hunk_does_not_overwrite_an_external_edit_with_restored_timestamp() {
    let (local, _source, mut state, mut runtime) = setup();
    let direction = HunkDirection::RightToLeft;
    state.stage_hunk_merge(direction);
    let (before, after) = state
        .preview_hunk_merge(direction)
        .expect("selected hunk preview");
    state.dialog = DialogState::HunkMergePreview(HunkMergePreview::new(
        "file.txt".into(),
        direction,
        before,
        after,
    ));
    let target = local.path().join("file.txt");
    let timestamp = fs::metadata(&target).unwrap().modified().unwrap();
    fs::write(&target, "intruder\n").unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&target)
        .unwrap()
        .set_modified(timestamp)
        .unwrap();
    handle_dialog_key(&mut state, &mut runtime, KeyCode::Char('y'));
    assert_eq!(fs::read_to_string(target).unwrap(), "intruder\n");
    assert!(
        state.status_message.contains("changed"),
        "{}",
        state.status_message
    );
}

// @kotowari[REQ-merge-010]
#[test]
fn accepting_an_unchanged_hunk_destination_writes_the_selected_change() {
    let (local, _source, mut state, mut runtime) = setup();
    let direction = HunkDirection::RightToLeft;
    state.stage_hunk_merge(direction);
    let (before, after) = state
        .preview_hunk_merge(direction)
        .expect("selected hunk preview");
    state.dialog = DialogState::HunkMergePreview(HunkMergePreview::new(
        "file.txt".into(),
        direction,
        before,
        after,
    ));
    handle_dialog_key(&mut state, &mut runtime, KeyCode::Char('y'));
    assert_eq!(
        fs::read_to_string(local.path().join("file.txt")).unwrap(),
        "replacement\n"
    );
}
