#![cfg(unix)]

use std::fs;
use std::os::unix::fs::symlink;

use crossterm::event::{KeyCode, KeyModifiers};
use remote_merge::app::{AppState, Side};
use remote_merge::config::load_config_from_paths;
use remote_merge::handler::tree_keys::handle_tree_key;
use remote_merge::local::scan_local_tree;
use remote_merge::runtime::{RuntimeTargets, TuiRuntime};
use remote_merge::theme::DEFAULT_THEME;
use remote_merge::tree::FileTree;
use tempfile::TempDir;

fn setup() -> (TempDir, TempDir, TempDir, AppState, TuiRuntime) {
    let root = TempDir::new().unwrap();
    let shared = TempDir::new().unwrap();
    let remote = TempDir::new().unwrap();
    fs::write(shared.path().join("child.txt"), "visible\n").unwrap();
    symlink(shared.path(), root.path().join("linked")).unwrap();
    let config_path = root.path().join("test.toml");
    fs::write(&config_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[backup]\nenabled = false\n",
        root.path().display().to_string(), remote.path().display().to_string(),
    )).unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    let tree = scan_local_tree(root.path(), &[]).unwrap();
    let state = AppState::new(
        tree,
        FileTree::new(remote.path()),
        Side::Local,
        Side::Remote("develop".into()),
        DEFAULT_THEME,
    );
    let runtime = TuiRuntime::with_targets(
        config,
        RuntimeTargets::production()
            .with_local("develop", remote.path())
            .with_startup_directory(root.path().to_path_buf()),
    );
    (root, shared, remote, state, runtime)
}

// @kotowari[EX-scan-002]
#[test]
fn unopened_directory_link_has_no_preloaded_children() {
    let (_root, _shared, _remote, state, _runtime) = setup();
    assert!(state
        .left_tree
        .find_node("linked")
        .unwrap()
        .children
        .is_none());
    assert!(state
        .flat_nodes
        .iter()
        .all(|node| node.path != "linked/child.txt"));
}

// @kotowari[EX-scan-001]
#[test]
fn opening_directory_link_loads_and_shows_its_immediate_children() {
    let (_root, _shared, _remote, mut state, mut runtime) = setup();
    state.tree_cursor = state
        .flat_nodes
        .iter()
        .position(|node| node.path == "linked")
        .unwrap();
    handle_tree_key(&mut state, &mut runtime, KeyCode::Enter, KeyModifiers::NONE);
    assert!(state.expanded_dirs.contains("linked"));
    assert!(state
        .flat_nodes
        .iter()
        .any(|node| node.path == "linked/child.txt"));
    assert!(state.left_tree.find_node("linked").unwrap().is_symlink());
}
