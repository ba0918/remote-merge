//! AppState → JSON / 画面 → テキストのファイルダンプ。
//!
//! LLMエージェントが `cat state.json` / `cat screen.txt` で
//! TUI状態を取得するための部品。
//!
//! AppState全体をシリアライズするのではなく、
//! LLMに必要な情報だけを抽出した `StateSnapshot` に変換する（純粋関数）。
//! センシティブなファイル内容はダンプに含めない。

use std::io::Write;
use std::path::Path;

use serde::Serialize;

use crate::app::types::{Badge, FlatNode};
use crate::app::{AppState, Focus, MergeScanState, ScanState};

/// LLMが取得するAppState スナップショット
///
/// ファイルキャッシュの内容は含めない（セキュリティ）。
/// パス・バッジ・UI状態のみ。
#[derive(Debug, Clone, Serialize)]
pub struct StateSnapshot {
    /// 現在のフォーカス
    pub focus: String,
    /// 左側ソース
    pub left_source: String,
    /// 右側ソース
    pub right_source: String,
    /// SSH接続状態
    pub is_connected: bool,
    /// ステータスメッセージ
    pub status_message: String,
    /// ダイアログが表示されているか
    pub has_dialog: bool,
    /// ダイアログの種類（表示中の場合）
    pub dialog_kind: Option<String>,
    /// 選択中のファイルパス
    pub selected_path: Option<String>,
    /// ツリーカーソル位置
    pub tree_cursor: usize,
    /// diffスクロール位置
    pub diff_scroll: usize,
    /// diffカーソル位置
    pub diff_cursor: usize,
    /// hunkカーソル位置
    pub hunk_cursor: usize,
    /// diffモード
    pub diff_mode: String,
    /// ファイルツリーの表示行
    pub tree_files: Vec<FileEntry>,
    /// 走査状態
    pub scan_state: String,
    /// マージ走査状態
    pub merge_scan_state: String,
    /// diffフィルターモード
    pub diff_filter_mode: bool,
    /// ref diff 表示中か
    pub showing_ref_diff: bool,
    /// 変更ファイル数（ツリーバッジから集計）
    pub file_counts: FileCounts,
}

/// ファイルツリーの1エントリ（LLM向け）
#[derive(Debug, Clone, Serialize)]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub badge: String,
}

/// ファイル数集計
#[derive(Debug, Clone, Serialize)]
pub struct FileCounts {
    pub modified: usize,
    pub equal: usize,
    pub left_only: usize,
    pub right_only: usize,
    pub unchecked: usize,
    pub error: usize,
}

/// AppState から StateSnapshot を構築する（純粋関数）
pub fn build_snapshot(state: &AppState) -> StateSnapshot {
    let tree_files: Vec<FileEntry> = state
        .flat_nodes
        .iter()
        .map(|n| FileEntry {
            path: n.path.clone(),
            name: n.name.clone(),
            is_dir: n.is_dir,
            badge: n.badge.label().to_string(),
        })
        .collect();

    let file_counts = count_badges(&state.flat_nodes);

    StateSnapshot {
        focus: format_focus(state.focus),
        left_source: state.left_source.display_name().to_string(),
        right_source: state.right_source.display_name().to_string(),
        is_connected: state.is_connected,
        status_message: state.status_message.clone(),
        has_dialog: state.has_dialog(),
        dialog_kind: format_dialog_kind(&state.dialog),
        selected_path: state.selected_path.clone(),
        tree_cursor: state.tree_cursor,
        diff_scroll: state.diff_scroll,
        diff_cursor: state.diff_cursor,
        hunk_cursor: state.hunk_cursor,
        diff_mode: format_diff_mode(state.diff_mode),
        tree_files,
        scan_state: format_scan_state(&state.scan_state),
        merge_scan_state: format_merge_scan_state(&state.merge_scan_state),
        diff_filter_mode: state.diff_filter_mode,
        showing_ref_diff: state.showing_ref_diff,
        file_counts,
    }
}

/// FlatNode のバッジからファイル数を集計する（純粋関数）
pub fn count_badges(nodes: &[FlatNode]) -> FileCounts {
    let mut counts = FileCounts {
        modified: 0,
        equal: 0,
        left_only: 0,
        right_only: 0,
        unchecked: 0,
        error: 0,
    };

    for node in nodes {
        if node.is_dir {
            continue;
        }
        match node.badge {
            Badge::Modified => counts.modified += 1,
            Badge::Equal => counts.equal += 1,
            Badge::LeftOnly => counts.left_only += 1,
            Badge::RightOnly => counts.right_only += 1,
            Badge::Unchecked | Badge::Loading => counts.unchecked += 1,
            Badge::Error => counts.error += 1,
        }
    }

    counts
}

/// StateSnapshot を JSON ファイルに書き出す
pub fn dump_state_to_file(state: &AppState, path: &Path) -> std::io::Result<()> {
    let snapshot = build_snapshot(state);
    let json = serde_json::to_string_pretty(&snapshot).map_err(std::io::Error::other)?;
    atomic_write(path, json.as_bytes())
}

/// 画面テキストをファイルに書き出す
pub fn dump_screen_to_file(screen_text: &str, path: &Path) -> std::io::Result<()> {
    atomic_write(path, screen_text.as_bytes())
}

/// ratatui の Buffer からプレーンテキストを抽出する（純粋関数）
///
/// ANSI エスケープを含まないプレーンテキストに変換する。
/// 各行の末尾空白はトリムする。
pub fn buffer_to_text(buf: &ratatui::buffer::Buffer) -> String {
    let area = buf.area();
    let mut lines = Vec::with_capacity(area.height as usize);

    for y in area.y..area.y + area.height {
        let mut line = String::with_capacity(area.width as usize);
        for x in area.x..area.x + area.width {
            let cell = &buf[(x, y)];
            line.push_str(cell.symbol());
        }
        lines.push(line.trim_end().to_string());
    }

    // 末尾の空行をトリム
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }

    lines.join("\n")
}

/// ファイルをアトミックに書き込む（tmp → rename）
fn atomic_write(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let tmp_path = path.with_extension("tmp");
    let mut file = std::fs::File::create(&tmp_path)?;
    file.write_all(content)?;
    file.flush()?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

fn format_focus(focus: Focus) -> String {
    match focus {
        Focus::FileTree => "file_tree".to_string(),
        Focus::DiffView => "diff_view".to_string(),
    }
}

fn format_diff_mode(mode: crate::app::DiffMode) -> String {
    match mode {
        crate::app::DiffMode::Unified => "unified".to_string(),
        crate::app::DiffMode::SideBySide => "side_by_side".to_string(),
    }
}

fn format_scan_state(state: &ScanState) -> String {
    match state {
        ScanState::Idle => "idle".to_string(),
        ScanState::Scanning => "scanning".to_string(),
        ScanState::Complete(_, _) => "complete".to_string(),
        ScanState::PartialComplete(_, _, reason) => format!("partial: {}", reason),
        ScanState::Error(e) => format!("error: {}", e),
    }
}

fn format_merge_scan_state(state: &MergeScanState) -> String {
    match state {
        MergeScanState::Idle => "idle".to_string(),
        MergeScanState::Scanning { dir_path, .. } => format!("scanning: {}", dir_path),
    }
}

fn format_dialog_kind(dialog: &crate::ui::dialog::DialogState) -> Option<String> {
    use crate::ui::dialog::DialogState;
    match dialog {
        DialogState::None => None,
        DialogState::Confirm(_) => Some("confirm".to_string()),
        DialogState::BatchConfirm(_) => Some("batch_confirm".to_string()),
        DialogState::ServerSelect(_) => Some("server_select".to_string()),
        DialogState::Filter(_) => Some("filter".to_string()),
        DialogState::HunkMergePreview(_) => Some("hunk_merge_preview".to_string()),
        DialogState::Help(_) => Some("help".to_string()),
        DialogState::Info(_) => Some("info".to_string()),
        DialogState::Progress(_) => Some("progress".to_string()),
        DialogState::WriteConfirmation => Some("write_confirmation".to_string()),
        DialogState::UnsavedChanges => Some("unsaved_changes".to_string()),
        DialogState::MtimeWarning(_) => Some("mtime_warning".to_string()),
        DialogState::PairServerSelect(_) => Some("pair_server_select".to_string()),
    }
}

/// state.json / screen.txt を保存するデフォルトディレクトリ
pub fn default_dump_dir() -> std::path::PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("remote-merge")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::types::{Badge, FlatNode};

    fn sample_flat_nodes() -> Vec<FlatNode> {
        vec![
            FlatNode {
                path: "src".to_string(),
                name: "src".to_string(),
                depth: 0,
                is_dir: true,
                is_symlink: false,
                expanded: true,
                badge: Badge::Unchecked,
            },
            FlatNode {
                path: "src/main.rs".to_string(),
                name: "main.rs".to_string(),
                depth: 1,
                is_dir: false,
                is_symlink: false,
                expanded: false,
                badge: Badge::Modified,
            },
            FlatNode {
                path: "src/lib.rs".to_string(),
                name: "lib.rs".to_string(),
                depth: 1,
                is_dir: false,
                is_symlink: false,
                expanded: false,
                badge: Badge::Equal,
            },
            FlatNode {
                path: "new.rs".to_string(),
                name: "new.rs".to_string(),
                depth: 0,
                is_dir: false,
                is_symlink: false,
                expanded: false,
                badge: Badge::LeftOnly,
            },
            FlatNode {
                path: "old.rs".to_string(),
                name: "old.rs".to_string(),
                depth: 0,
                is_dir: false,
                is_symlink: false,
                expanded: false,
                badge: Badge::RightOnly,
            },
            FlatNode {
                path: "err.rs".to_string(),
                name: "err.rs".to_string(),
                depth: 0,
                is_dir: false,
                is_symlink: false,
                expanded: false,
                badge: Badge::Error,
            },
        ]
    }

    #[test]
    fn test_count_badges() {
        let nodes = sample_flat_nodes();
        let counts = count_badges(&nodes);
        assert_eq!(counts.modified, 1);
        assert_eq!(counts.equal, 1);
        assert_eq!(counts.left_only, 1);
        assert_eq!(counts.right_only, 1);
        assert_eq!(counts.error, 1);
        assert_eq!(counts.unchecked, 0); // dir は除外される
    }

    #[test]
    fn test_count_badges_empty() {
        let counts = count_badges(&[]);
        assert_eq!(counts.modified, 0);
        assert_eq!(counts.equal, 0);
    }

    #[test]
    fn test_format_focus() {
        assert_eq!(format_focus(Focus::FileTree), "file_tree");
        assert_eq!(format_focus(Focus::DiffView), "diff_view");
    }

    #[test]
    fn test_format_diff_mode() {
        assert_eq!(format_diff_mode(crate::app::DiffMode::Unified), "unified");
        assert_eq!(
            format_diff_mode(crate::app::DiffMode::SideBySide),
            "side_by_side"
        );
    }

    #[test]
    fn test_format_scan_state() {
        assert_eq!(format_scan_state(&ScanState::Idle), "idle");
        assert_eq!(format_scan_state(&ScanState::Scanning), "scanning");
        assert_eq!(
            format_scan_state(&ScanState::Error("timeout".into())),
            "error: timeout"
        );
    }

    #[test]
    fn test_format_merge_scan_state() {
        assert_eq!(format_merge_scan_state(&MergeScanState::Idle), "idle");
    }

    #[test]
    fn test_format_dialog_kind_none() {
        use crate::ui::dialog::DialogState;
        assert!(format_dialog_kind(&DialogState::None).is_none());
    }

    #[test]
    fn test_format_dialog_kind_info() {
        use crate::ui::dialog::DialogState;
        assert_eq!(
            format_dialog_kind(&DialogState::Info("test".into())),
            Some("info".to_string())
        );
    }

    #[test]
    fn test_file_entry_serialization() {
        let entry = FileEntry {
            path: "src/main.rs".to_string(),
            name: "main.rs".to_string(),
            is_dir: false,
            badge: "[M]".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"badge\":\"[M]\""));
        assert!(json.contains("\"is_dir\":false"));
    }

    #[test]
    fn test_buffer_to_text() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;

        let area = Rect::new(0, 0, 10, 3);
        let mut buf = Buffer::empty(area);
        buf[(0, 0)].set_symbol("H");
        buf[(1, 0)].set_symbol("i");
        // row 1 all spaces (default)
        buf[(0, 2)].set_symbol("!");

        let text = buffer_to_text(&buf);
        let lines: Vec<&str> = text.split('\n').collect();
        assert_eq!(lines[0], "Hi");
        assert_eq!(lines[1], ""); // empty line (spaces trimmed)
        assert_eq!(lines[2], "!");
    }

    #[test]
    fn test_buffer_to_text_trims_trailing_empty_lines() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;

        let area = Rect::new(0, 0, 5, 4);
        let mut buf = Buffer::empty(area);
        buf[(0, 0)].set_symbol("A");
        // rows 1-3 are all spaces

        let text = buffer_to_text(&buf);
        assert_eq!(text, "A");
    }

    #[test]
    fn test_atomic_write_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.json");

        atomic_write(&path, b"{\"ok\":true}").unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "{\"ok\":true}");

        // tmp ファイルが残っていないこと
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn test_dump_screen_to_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("screen.txt");

        dump_screen_to_file("Hello TUI", &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "Hello TUI");
    }

    #[test]
    fn test_default_dump_dir() {
        let dir = default_dump_dir();
        assert!(dir.to_string_lossy().contains("remote-merge"));
    }
}
