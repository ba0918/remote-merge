#![cfg(unix)]

//! TUI の単一ファイルマージで、元が通常ファイルで先が symlink のときの契約テスト。
//! 確認ダイアログの y から実ハンドラを通し、リンクとリンク先が変わらないことを確かめる。

use std::fs;
use std::os::unix::fs::symlink;

use crossterm::event::KeyCode;
use remote_merge::app::{AppState, Side};
use remote_merge::config::load_config_from_paths;
use remote_merge::handler::dialog_keys::handle_dialog_key;
use remote_merge::local::scan_local_tree;
use remote_merge::merge::executor::MergeDirection;
use remote_merge::runtime::{RuntimeTargets, TuiRuntime};
use remote_merge::theme::DEFAULT_THEME;
use remote_merge::ui::dialog::DialogState;
use tempfile::TempDir;

const SOURCE_TEXT: &str = "regular source\n";
const REFERENT_TEXT: &str = "referent stays\n";

struct KindMismatch {
    left: TempDir,
    right: TempDir,
    state: AppState,
    runtime: TuiRuntime,
}

impl KindMismatch {
    /// direction の書き込み先に file.txt -> referent.txt の symlink、読み込み元に通常ファイルを置く。
    /// バックアップは無効にし、読み込み元の中身はキャッシュ済みにする。
    fn new(direction: MergeDirection) -> Self {
        let left = TempDir::new().unwrap();
        let right = TempDir::new().unwrap();
        let (source, destination) = match direction {
            MergeDirection::LeftToRight => (left.path(), right.path()),
            MergeDirection::RightToLeft => (right.path(), left.path()),
        };
        fs::write(source.join("file.txt"), SOURCE_TEXT).unwrap();
        fs::write(destination.join("referent.txt"), REFERENT_TEXT).unwrap();
        symlink("referent.txt", destination.join("file.txt")).unwrap();

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
        state.tree_cursor = state
            .flat_nodes
            .iter()
            .position(|node| node.path == "file.txt")
            .unwrap();
        let cache = match direction {
            MergeDirection::LeftToRight => &mut state.left_cache,
            MergeDirection::RightToLeft => &mut state.right_cache,
        };
        cache.insert("file.txt".into(), SOURCE_TEXT.into());
        let runtime = TuiRuntime::with_targets(
            config,
            RuntimeTargets::production()
                .with_local("develop", right.path())
                .with_startup_directory(left.path().to_path_buf()),
        );
        Self {
            left,
            right,
            state,
            runtime,
        }
    }

    fn confirm_merge(&mut self, direction: MergeDirection) {
        self.state.show_merge_dialog(direction);
        assert!(matches!(self.state.dialog, DialogState::Confirm(_)));
        handle_dialog_key(&mut self.state, &mut self.runtime, KeyCode::Char('y'));
    }

    fn assert_destination_unchanged(&self, direction: MergeDirection) {
        let destination = match direction {
            MergeDirection::LeftToRight => self.right.path(),
            MergeDirection::RightToLeft => self.left.path(),
        };
        assert_eq!(
            fs::read_link(destination.join("file.txt")).unwrap(),
            std::path::Path::new("referent.txt")
        );
        assert_eq!(
            fs::read_to_string(destination.join("referent.txt")).unwrap(),
            REFERENT_TEXT
        );
        assert!(
            self.state.status_message.contains("different file types"),
            "{}",
            self.state.status_message
        );
    }
}

// @kotowari[EX-merge-001]
#[test]
fn a_confirmed_left_to_right_merge_keeps_a_destination_symlink_and_its_referent() {
    let direction = MergeDirection::LeftToRight;
    let mut fixture = KindMismatch::new(direction);
    fixture.confirm_merge(direction);
    fixture.assert_destination_unchanged(direction);
}

// @kotowari[EX-merge-001]
#[test]
fn a_confirmed_right_to_left_merge_keeps_a_destination_symlink_and_its_referent() {
    let direction = MergeDirection::RightToLeft;
    let mut fixture = KindMismatch::new(direction);
    fixture.confirm_merge(direction);
    fixture.assert_destination_unchanged(direction);
}
