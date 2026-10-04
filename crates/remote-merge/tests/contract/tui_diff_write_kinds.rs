#![cfg(unix)]

//! TUI の diff ビューからの書き込みで、左が通常ファイルで右が symlink のときの契約テスト。
//! 両側の中身を実際の読み込みで取得し、リンクとリンク先が変わらないことを確かめる。

use std::fs;
use std::path::Path;

use crossterm::event::KeyCode;
use remote_merge::app::{AppState, Side};
use remote_merge::config::load_config_from_paths;
use remote_merge::diff::engine::HunkDirection;
use remote_merge::handler::dialog_keys::handle_dialog_key;
use remote_merge::handler::diff_keys::handle_diff_key;
use remote_merge::handler::merge_content::load_file_content;
use remote_merge::local::scan_local_tree;
use remote_merge::runtime::{RuntimeTargets, TuiRuntime};
use remote_merge::theme::DEFAULT_THEME;
use remote_merge::ui::dialog::{DialogState, HunkMergePreview};
use tempfile::TempDir;

use super::tui_merge_kinds::link_to_referent;

const LEFT_TEXT: &str = "same\nleft line\n";
const REFERENT_TEXT: &str = "same\nreferent line\n";

struct DiffKindMismatch {
    left: TempDir,
    right: TempDir,
    state: AppState,
    runtime: TuiRuntime,
}

impl DiffKindMismatch {
    /// 左に通常ファイル file.txt、右に file.txt -> referent.txt の symlink を置き、
    /// バックアップを無効にして file.txt を選択した diff ビューの状態を作る。
    fn new() -> Self {
        let left = TempDir::new().unwrap();
        let right = TempDir::new().unwrap();
        fs::write(left.path().join("file.txt"), LEFT_TEXT).unwrap();
        link_to_referent(right.path(), REFERENT_TEXT);

        let config_dir = TempDir::new().unwrap();
        let config_path = config_dir.path().join("test.toml");
        fs::write(&config_path, format!(
            "[local]\nroot_dir = {:?}\n[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[backup]\nenabled = false\n",
            left.path().display().to_string(), right.path().display().to_string(),
        )).unwrap();
        let config = load_config_from_paths(Some(&config_path), None).unwrap();
        let mut state = AppState::new(
            scan_local_tree(left.path(), &[]).unwrap(),
            scan_local_tree(right.path(), &[]).unwrap(),
            Side::Local,
            Side::Remote("develop".into()),
            DEFAULT_THEME,
        );
        let mut runtime = TuiRuntime::with_targets(
            config,
            RuntimeTargets::production()
                .with_local("develop", right.path())
                .with_startup_directory(left.path().to_path_buf()),
        );
        state.tree_cursor = state
            .flat_nodes
            .iter()
            .position(|node| node.path == "file.txt")
            .unwrap();
        load_file_content(&mut state, &mut runtime);
        state.select_file();
        assert!(state.hunk_count() > 0, "{}", state.status_message);
        Self {
            left,
            right,
            state,
            runtime,
        }
    }

    fn assert_nothing_written(&self) {
        assert_eq!(
            fs::read_link(self.right.path().join("file.txt")).unwrap(),
            Path::new("referent.txt")
        );
        assert_eq!(
            fs::read_to_string(self.right.path().join("referent.txt")).unwrap(),
            REFERENT_TEXT
        );
        assert_eq!(
            fs::read_to_string(self.left.path().join("file.txt")).unwrap(),
            LEFT_TEXT
        );
        // 理由の文言は仕様で未決のため固定せず、mtime 警告などを挟まずにスキップされたことを見る
        assert!(
            matches!(self.state.dialog, DialogState::None),
            "{}",
            self.state.status_message
        );
    }
}

// @kotowari[EX-merge-001]
#[test]
fn a_confirmed_hunk_merge_keeps_a_destination_symlink_and_its_referent() {
    let mut fixture = DiffKindMismatch::new();
    let direction = HunkDirection::LeftToRight;
    let (before, after) = fixture
        .state
        .preview_hunk_merge(direction)
        .expect("selected hunk preview");
    fixture.state.dialog = DialogState::HunkMergePreview(HunkMergePreview::new(
        "file.txt".into(),
        direction,
        before,
        after,
    ));

    handle_dialog_key(&mut fixture.state, &mut fixture.runtime, KeyCode::Char('y'));

    fixture.assert_nothing_written();
    assert!(!fixture.state.has_unsaved_changes());
}

// @kotowari[EX-merge-001]
#[test]
fn writing_diff_changes_keeps_a_destination_symlink_and_both_sides_unchanged() {
    let mut fixture = DiffKindMismatch::new();
    handle_diff_key(&mut fixture.state, &mut fixture.runtime, KeyCode::Char('l'));
    handle_diff_key(&mut fixture.state, &mut fixture.runtime, KeyCode::Char('w'));
    assert!(matches!(
        fixture.state.dialog,
        DialogState::WriteConfirmation
    ));

    handle_dialog_key(&mut fixture.state, &mut fixture.runtime, KeyCode::Char('y'));

    fixture.assert_nothing_written();
}
