//! ダイアログ状態のドメイン型定義。
//!
//! ダイアログの「何を表示するか」「どのような状態か」を表す純粋データ型。
//! 描画ロジック (ratatui Widget) は `ui/dialog/` に置き、ここでは一切含まない。
//!
//! これにより `app/` や `runtime/` から `ui/dialog` への逆依存を解消する。

use crate::app::three_way_summary::ThreeWaySummaryPanel;
use crate::app::Badge;
use crate::diff::engine::HunkDirection;
use crate::merge::executor::MergeDirection;
use crate::merge::optimistic_lock::MtimeConflict;

// ── DialogState (ドメイン層のダイアログ状態) ──

/// アプリのダイアログ状態
#[derive(Debug, Clone, Default)]
pub enum DialogState {
    /// ダイアログなし
    #[default]
    None,
    /// マージ確認ダイアログ
    Confirm(ConfirmDialog),
    /// バッチマージ確認ダイアログ（ディレクトリ選択時）
    BatchConfirm(BatchConfirmDialog),
    /// サーバ選択メニュー（右側のみ切り替え、後方互換）
    ServerSelect(ServerMenu),
    /// ペアサーバ選択メニュー（LEFT/RIGHT 両方選択可能、3way diff 用）
    PairServerSelect(PairServerMenu),
    /// フィルターパネル
    Filter(FilterPanel),
    /// ハンクマージプレビュー
    HunkMergePreview(HunkMergePreview),
    /// ヘルプオーバーレイ
    Help(HelpOverlay),
    /// 情報ダイアログ（メッセージ表示のみ、Esc/Enter で閉じる）
    Info(String),
    /// プログレスダイアログ（走査・マージ進捗表示）
    Progress(ProgressDialog),
    /// 書き込み確認ダイアログ（w キー）
    WriteConfirmation,
    /// 未保存変更確認ダイアログ（q キー時）
    UnsavedChanges,
    /// mtime 衝突警告ダイアログ（楽観的ロック）
    MtimeWarning(MtimeWarningDialog),
    /// 3way サマリーパネル（W キー）
    ThreeWaySummary(ThreeWaySummaryPanel),
}

// ── ConfirmDialog ──

/// マージ確認ダイアログの状態
#[derive(Debug, Clone)]
pub struct ConfirmDialog {
    /// マージ対象のファイルパス
    pub file_path: String,
    /// マージの方向
    pub direction: MergeDirection,
    /// ソース名（例: "local"）
    pub source_name: String,
    /// ターゲット名（例: "develop"）
    pub target_name: String,
    /// リモート間マージかどうか（追加の警告表示に使用）
    pub is_remote_to_remote: bool,
}

impl ConfirmDialog {
    pub fn new(
        file_path: String,
        direction: MergeDirection,
        source_name: String,
        target_name: String,
    ) -> Self {
        Self {
            file_path,
            direction,
            source_name,
            target_name,
            is_remote_to_remote: false,
        }
    }

    /// リモート間マージフラグを設定する
    pub fn with_remote_to_remote(mut self, is_r2r: bool) -> Self {
        self.is_remote_to_remote = is_r2r;
        self
    }

    /// ダイアログのメッセージ行を生成
    pub fn message_lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!("Merge {} from", self.file_path),
            format!("{} → {}?", self.source_name, self.target_name),
        ];
        if self.is_remote_to_remote {
            lines.push(String::new());
            lines.push("⚠ Remote-to-remote merge".to_string());
        }
        lines
    }

    /// ダイアログのメッセージを生成（テスト用の後方互換）
    pub fn message(&self) -> String {
        self.message_lines().join("\n")
    }
}

// ── BatchConfirmDialog ──

/// バッチマージ確認ダイアログの状態
#[derive(Debug, Clone)]
pub struct BatchConfirmDialog {
    /// 対象ファイルとバッジ一覧
    pub files: Vec<(String, Badge)>,
    /// マージ方向
    pub direction: MergeDirection,
    /// ソース名
    pub source_name: String,
    /// ターゲット名
    pub target_name: String,
    /// スクロール位置（大量ファイル対応）
    pub scroll: usize,
    /// 未比較(Unchecked)ディレクトリ数（警告用）
    pub unchecked_count: usize,
    /// センシティブファイル一覧
    pub sensitive_files: Vec<String>,
}

impl BatchConfirmDialog {
    pub fn new(
        files: Vec<(String, Badge)>,
        direction: MergeDirection,
        source_name: String,
        target_name: String,
        unchecked_count: usize,
    ) -> Self {
        Self {
            files,
            direction,
            source_name,
            target_name,
            scroll: 0,
            unchecked_count,
            sensitive_files: Vec::new(),
        }
    }

    /// センシティブファイルパターンでチェックを行い、マッチするファイルを記録する
    pub fn check_sensitive(&mut self, patterns: &[String]) {
        self.sensitive_files = self
            .files
            .iter()
            .filter(|(path, _)| {
                let filename = path.rsplit('/').next().unwrap_or(path);
                patterns
                    .iter()
                    .any(|p| glob_match::glob_match(p, filename) || glob_match::glob_match(p, path))
            })
            .map(|(path, _)| path.clone())
            .collect();
    }

    /// 大量ファイル（21件以上）かどうか
    pub fn is_large_batch(&self) -> bool {
        self.files.len() > 20
    }

    /// メッセージを生成
    pub fn message(&self) -> String {
        format!(
            "Merge {} file(s) from {} → {}",
            self.files.len(),
            self.source_name,
            self.target_name
        )
    }

    /// スクロールダウン
    pub fn scroll_down(&mut self) {
        if self.scroll + 1 < self.files.len() {
            self.scroll += 1;
        }
    }

    /// スクロールアップ
    pub fn scroll_up(&mut self) {
        if self.scroll > 0 {
            self.scroll -= 1;
        }
    }
}

// ── ServerMenu ──

/// サーバ選択メニューの状態
#[derive(Debug, Clone)]
pub struct ServerMenu {
    /// 利用可能なサーバ名リスト
    pub servers: Vec<String>,
    /// 現在選択中のインデックス
    pub cursor: usize,
    /// 現在接続中のサーバ名
    pub connected: String,
}

impl ServerMenu {
    pub fn new(servers: Vec<String>, connected: String) -> Self {
        let cursor = servers.iter().position(|s| s == &connected).unwrap_or(0);
        Self {
            servers,
            cursor,
            connected,
        }
    }

    /// カーソルを上に移動
    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// カーソルを下に移動
    pub fn cursor_down(&mut self) {
        if self.cursor + 1 < self.servers.len() {
            self.cursor += 1;
        }
    }

    /// 現在選択中のサーバ名を返す
    pub fn selected(&self) -> Option<&str> {
        self.servers.get(self.cursor).map(|s| s.as_str())
    }
}

// ── PairServerMenu ──

/// アクティブ列
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Left,
    Right,
}

/// ペアサーバ選択メニューの状態
#[derive(Debug, Clone)]
pub struct PairServerMenu {
    /// 利用可能なサーバ名リスト（"local" を含む）
    pub servers: Vec<String>,
    /// LEFT 列のカーソル位置
    pub left_cursor: usize,
    /// RIGHT 列のカーソル位置
    pub right_cursor: usize,
    /// アクティブ列
    pub active_column: Column,
}

impl PairServerMenu {
    /// 新しい PairServerMenu を構築する。
    /// カーソルは現在の left/right サーバに初期配置。
    pub fn new(servers: Vec<String>, current_left: &str, current_right: &str) -> Self {
        let left_cursor = servers.iter().position(|s| s == current_left).unwrap_or(0);
        let right_cursor = servers
            .iter()
            .position(|s| s == current_right)
            .unwrap_or(if servers.len() > 1 { 1 } else { 0 });
        Self {
            servers,
            left_cursor,
            right_cursor,
            active_column: Column::Left,
        }
    }

    /// アクティブ列を切り替える
    pub fn toggle_column(&mut self) {
        self.active_column = match self.active_column {
            Column::Left => Column::Right,
            Column::Right => Column::Left,
        };
    }

    /// アクティブ列のカーソルを上に移動
    pub fn cursor_up(&mut self) {
        let cursor = self.active_cursor_mut();
        if *cursor > 0 {
            *cursor -= 1;
        }
    }

    /// アクティブ列のカーソルを下に移動
    pub fn cursor_down(&mut self) {
        let max = self.servers.len().saturating_sub(1);
        let cursor = self.active_cursor_mut();
        if *cursor < max {
            *cursor += 1;
        }
    }

    /// LEFT 列の選択サーバ名
    pub fn selected_left(&self) -> Option<&str> {
        self.servers.get(self.left_cursor).map(|s| s.as_str())
    }

    /// RIGHT 列の選択サーバ名
    pub fn selected_right(&self) -> Option<&str> {
        self.servers.get(self.right_cursor).map(|s| s.as_str())
    }

    /// 左右が同じサーバを選択しているか
    pub fn is_same_pair(&self) -> bool {
        self.left_cursor == self.right_cursor
    }

    /// アクティブ列のカーソルへの可変参照
    fn active_cursor_mut(&mut self) -> &mut usize {
        match self.active_column {
            Column::Left => &mut self.left_cursor,
            Column::Right => &mut self.right_cursor,
        }
    }
}

// ── FilterPanel ──

/// フィルターパネルの状態
#[derive(Debug, Clone)]
pub struct FilterPanel {
    /// フィルターパターンとその有効/無効状態
    pub patterns: Vec<(String, bool)>,
    /// カーソル位置
    pub cursor: usize,
}

impl FilterPanel {
    pub fn new(patterns: &[String]) -> Self {
        Self {
            patterns: patterns.iter().map(|p| (p.clone(), true)).collect(),
            cursor: 0,
        }
    }

    /// カーソルを上に移動
    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// カーソルを下に移動
    pub fn cursor_down(&mut self) {
        if self.cursor + 1 < self.patterns.len() {
            self.cursor += 1;
        }
    }

    /// 現在のパターンの有効/無効をトグル
    pub fn toggle(&mut self) {
        if let Some(item) = self.patterns.get_mut(self.cursor) {
            item.1 = !item.1;
        }
    }

    /// 有効なパターンのみを返す
    pub fn active_patterns(&self) -> Vec<String> {
        self.patterns
            .iter()
            .filter(|(_, enabled)| *enabled)
            .map(|(pattern, _)| pattern.clone())
            .collect()
    }
}

// ── HelpOverlay ──

/// ヘルプオーバーレイのセクション
#[derive(Debug, Clone)]
pub struct HelpSection {
    pub title: String,
    pub bindings: Vec<(String, String)>, // (キー, 説明)
}

/// ヘルプオーバーレイの状態
#[derive(Debug, Clone)]
pub struct HelpOverlay {
    pub sections: Vec<HelpSection>,
    /// スクロールオフセット（行単位）
    pub scroll: usize,
}

impl Default for HelpOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl HelpOverlay {
    /// 全セクション合計の行数を計算する
    pub fn total_lines(&self) -> usize {
        self.sections
            .iter()
            .map(|s| s.bindings.len() + 2) // タイトル行 + 空行 + bindings
            .sum()
    }

    /// 下にスクロール
    pub fn scroll_down(&mut self) {
        let max = self.total_lines().saturating_sub(1);
        if self.scroll < max {
            self.scroll += 1;
        }
    }

    /// 上にスクロール
    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    /// ページ下スクロール
    pub fn page_down(&mut self, page_size: usize) {
        let max = self.total_lines().saturating_sub(1);
        self.scroll = (self.scroll + page_size).min(max);
    }

    /// ページ上スクロール
    pub fn page_up(&mut self, page_size: usize) {
        self.scroll = self.scroll.saturating_sub(page_size);
    }

    pub fn new() -> Self {
        Self {
            scroll: 0,
            sections: vec![
                HelpSection {
                    title: "File Tree".to_string(),
                    bindings: vec![
                        ("j/↓".to_string(), "Move cursor down".to_string()),
                        ("k/↑".to_string(), "Move cursor up".to_string()),
                        ("Enter/l/→".to_string(), "Expand / Select file".to_string()),
                        ("h/←".to_string(), "Collapse".to_string()),
                        (
                            "L (Shift)".to_string(),
                            "Merge remote → local (dir supported)".to_string(),
                        ),
                        (
                            "R (Shift)".to_string(),
                            "Merge local → remote (dir supported)".to_string(),
                        ),
                        (
                            "F (Shift)".to_string(),
                            "Show changed files only (full scan)".to_string(),
                        ),
                        ("c".to_string(), "Copy diff to clipboard".to_string()),
                        ("r".to_string(), "Refresh dir / Reconnect SSH".to_string()),
                        ("f".to_string(), "Filter panel".to_string()),
                        ("s".to_string(), "Server select".to_string()),
                        ("W (Shift)".to_string(), "3way summary panel".to_string()),
                        (
                            "X (Shift)".to_string(),
                            "Swap right ↔ ref server".to_string(),
                        ),
                        ("/".to_string(), "Search files".to_string()),
                        ("n".to_string(), "Next search match".to_string()),
                        ("N (Shift)".to_string(), "Previous search match".to_string()),
                        (
                            "E (Shift)".to_string(),
                            "Export report (Markdown)".to_string(),
                        ),
                    ],
                },
                HelpSection {
                    title: "Diff View".to_string(),
                    bindings: vec![
                        ("j/k/↑/↓".to_string(), "Scroll one line".to_string()),
                        ("n".to_string(), "Next hunk / search match".to_string()),
                        ("N".to_string(), "Prev hunk / search match".to_string()),
                        ("/".to_string(), "Search in diff".to_string()),
                        ("PageDown".to_string(), "Page down".to_string()),
                        ("PageUp".to_string(), "Page up".to_string()),
                        ("Home".to_string(), "Go to top".to_string()),
                        ("End".to_string(), "Go to bottom".to_string()),
                        ("→/l".to_string(), "Hunk: apply remote → local".to_string()),
                        ("←/h".to_string(), "Hunk: apply local → remote".to_string()),
                        ("w".to_string(), "Write changes to file".to_string()),
                        ("u".to_string(), "Undo last change".to_string()),
                        ("U".to_string(), "Undo all changes".to_string()),
                        ("d".to_string(), "Toggle Unified / Side-by-Side".to_string()),
                        ("c".to_string(), "Copy diff to clipboard".to_string()),
                        ("r".to_string(), "Reconnect SSH".to_string()),
                        ("W (Shift)".to_string(), "3way summary panel".to_string()),
                    ],
                },
                HelpSection {
                    title: "Global".to_string(),
                    bindings: vec![
                        ("Tab".to_string(), "Toggle focus".to_string()),
                        ("T".to_string(), "Cycle theme".to_string()),
                        ("S".to_string(), "Syntax highlight ON/OFF".to_string()),
                        ("?".to_string(), "Toggle help".to_string()),
                        ("q".to_string(), "Quit".to_string()),
                    ],
                },
            ],
        }
    }
}

// ── HunkMergePreview ──

/// ハンクマージプレビューの状態
#[derive(Debug, Clone)]
pub struct HunkMergePreview {
    /// 対象ファイルパス
    pub file_path: String,
    /// マージ方向
    pub direction: HunkDirection,
    /// 適用前テキスト（対象ファイルの変更部分周辺）
    pub before_text: String,
    /// 適用後テキスト
    pub after_text: String,
    /// マージ方向の文字列表示
    pub direction_label: String,
}

impl HunkMergePreview {
    pub fn new(
        file_path: String,
        direction: HunkDirection,
        before_text: String,
        after_text: String,
    ) -> Self {
        let direction_label = match direction {
            HunkDirection::RightToLeft => "remote → local".to_string(),
            HunkDirection::LeftToRight => "local → remote".to_string(),
        };
        Self {
            file_path,
            direction,
            before_text,
            after_text,
            direction_label,
        }
    }
}

// ── ProgressDialog ──

/// プログレスダイアログのフェーズ（サービス層はフェーズだけ設定し、UI層が表示テキストを生成）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressPhase {
    /// ディレクトリ走査中（ファイル発見フェーズ）
    Scanning,
    /// ファイルコンテンツ読み込み中
    LoadingFiles,
    /// リモートファイル読み込み中
    LoadingRemote,
    /// マージ実行中
    Merging,
}

impl ProgressPhase {
    /// UI 表示用のタイトルテキストを生成する
    pub fn title(&self) -> &'static str {
        match self {
            ProgressPhase::Scanning => "Scanning",
            ProgressPhase::LoadingFiles => "Loading files",
            ProgressPhase::LoadingRemote => "Loading remote files",
            ProgressPhase::Merging => "Merging",
        }
    }

    /// プログレスバーの不定形式テキスト（total 不明時）
    pub fn indeterminate_text(&self, current: usize) -> String {
        match self {
            ProgressPhase::Scanning => format!("Discovering files... {} found", current),
            _ => format!("Processing... {}", current),
        }
    }
}

/// プログレスダイアログの状態
#[derive(Debug, Clone)]
pub struct ProgressDialog {
    /// 進捗フェーズ
    pub phase: ProgressPhase,
    /// 走査対象のコンテキスト（例: ディレクトリパス）
    pub context: String,
    /// 現在の進捗値
    pub current: usize,
    /// 全体の件数（不明な場合は None）
    pub total: Option<usize>,
    /// 現在処理中のパス（表示用）
    pub current_path: Option<String>,
    /// Esc でキャンセル可能か
    pub cancelable: bool,
}

impl ProgressDialog {
    /// 新しいプログレスダイアログを作成する
    pub fn new(phase: ProgressPhase, context: impl Into<String>, cancelable: bool) -> Self {
        Self {
            phase,
            context: context.into(),
            current: 0,
            total: None,
            current_path: None,
            cancelable,
        }
    }

    /// UI 表示用のタイトルを生成する（フェーズ + コンテキスト）
    pub fn display_title(&self) -> String {
        if self.context.is_empty() {
            self.phase.title().to_string()
        } else {
            format!("{} {}", self.phase.title(), self.context)
        }
    }
}

// ── MtimeWarningDialog ──

/// mtime 警告ダイアログの状態
#[derive(Debug, Clone)]
pub struct MtimeWarningDialog {
    /// 衝突したファイルのリスト
    pub conflicts: Vec<MtimeConflict>,
    /// 元のマージ操作を再試行するための情報
    pub merge_context: MtimeWarningMergeContext,
}

/// 警告ダイアログから復帰するために必要なマージコンテキスト
#[derive(Debug, Clone)]
pub enum MtimeWarningMergeContext {
    /// 単一ファイルマージ
    Single {
        path: String,
        direction: MergeDirection,
    },
    /// バッチマージ
    Batch { direction: MergeDirection },
    /// 変更書き込み（w キー）
    Write,
    /// ハンクマージ（HunkMergePreview 確認後）
    HunkMerge { direction: HunkDirection },
}

// ── テスト ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_state_default_is_none() {
        let state = DialogState::default();
        assert!(matches!(state, DialogState::None));
    }

    #[test]
    fn confirm_dialog_message() {
        let dialog = ConfirmDialog::new(
            "src/main.rs".to_string(),
            MergeDirection::LeftToRight,
            "local".to_string(),
            "develop".to_string(),
        );
        assert!(dialog.message().contains("src/main.rs"));
        assert!(dialog.message().contains("local"));
    }

    #[test]
    fn confirm_dialog_remote_to_remote() {
        let dialog = ConfirmDialog::new(
            "file.rs".to_string(),
            MergeDirection::LeftToRight,
            "staging".to_string(),
            "production".to_string(),
        )
        .with_remote_to_remote(true);
        assert!(dialog.is_remote_to_remote);
        assert!(dialog.message().contains("Remote-to-remote"));
    }

    #[test]
    fn batch_confirm_dialog_basic() {
        let dialog = BatchConfirmDialog::new(
            vec![("a.rs".to_string(), Badge::Modified)],
            MergeDirection::LeftToRight,
            "local".to_string(),
            "develop".to_string(),
            0,
        );
        assert_eq!(dialog.files.len(), 1);
        assert!(!dialog.is_large_batch());
        assert!(dialog.message().contains("1 file(s)"));
    }

    #[test]
    fn server_menu_navigation() {
        let mut menu = ServerMenu::new(
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
            "b".to_string(),
        );
        assert_eq!(menu.cursor, 1);
        menu.cursor_up();
        assert_eq!(menu.cursor, 0);
        menu.cursor_down();
        assert_eq!(menu.cursor, 1);
        assert_eq!(menu.selected(), Some("b"));
    }

    #[test]
    fn pair_server_menu_toggle() {
        let mut menu = PairServerMenu::new(
            vec!["local".to_string(), "dev".to_string(), "stg".to_string()],
            "local",
            "dev",
        );
        assert_eq!(menu.active_column, Column::Left);
        menu.toggle_column();
        assert_eq!(menu.active_column, Column::Right);
    }

    #[test]
    fn filter_panel_toggle() {
        let mut panel = FilterPanel::new(&["*.log".to_string(), "*.tmp".to_string()]);
        assert_eq!(panel.active_patterns().len(), 2);
        panel.toggle();
        assert_eq!(panel.active_patterns().len(), 1);
    }

    #[test]
    fn help_overlay_scroll() {
        let mut help = HelpOverlay::new();
        assert_eq!(help.scroll, 0);
        help.scroll_down();
        assert_eq!(help.scroll, 1);
        help.scroll_up();
        assert_eq!(help.scroll, 0);
    }

    #[test]
    fn progress_dialog_title() {
        let progress = ProgressDialog::new(ProgressPhase::Scanning, "src/", true);
        assert_eq!(progress.display_title(), "Scanning src/");
    }

    #[test]
    fn progress_phase_indeterminate() {
        assert!(ProgressPhase::Scanning
            .indeterminate_text(42)
            .contains("42"));
    }

    #[test]
    fn hunk_preview_direction_label() {
        let preview = HunkMergePreview::new(
            "file.rs".to_string(),
            HunkDirection::LeftToRight,
            "before".to_string(),
            "after".to_string(),
        );
        assert_eq!(preview.direction_label, "local → remote");
    }
}
