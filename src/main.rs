use std::io;
use clap::{Parser, Subcommand};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use remote_merge::app::{AppState, Focus};
use remote_merge::config::{self, AppConfig};
use remote_merge::diff::engine::HunkDirection;
use remote_merge::local;
use remote_merge::merge::executor::{self, MergeDirection};
use remote_merge::ssh::client::SshClient;
use remote_merge::tree::FileTree;
use remote_merge::ui::dialog::{ConfirmDialogWidget, DialogState, FilterPanelWidget, HelpOverlayWidget, HunkMergePreviewWidget, ServerMenuWidget};
use remote_merge::ui::diff_view::DiffView;
use remote_merge::ui::layout::AppLayout;
use remote_merge::ui::tree_view::TreeView;

/// ローカルとリモートサーバ間のファイル差分をTUIでグラフィカルに表示・マージするツール
#[derive(Parser, Debug)]
#[command(name = "remote-merge", version, about)]
struct Cli {
    /// 比較対象のサーバ名（localとの比較）
    #[arg(short, long)]
    server: Option<String>,

    /// 比較の左側（デフォルト: local）
    #[arg(long)]
    left: Option<String>,

    /// 比較の右側
    #[arg(long)]
    right: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// プロジェクト設定ファイルを初期化する
    Init,

    /// 差分があるファイルの一覧を表示
    Status {
        /// 比較対象のサーバ名
        #[arg(short, long)]
        server: Option<String>,

        /// 比較の左側
        #[arg(long)]
        left: Option<String>,

        /// 比較の右側
        #[arg(long)]
        right: Option<String>,

        /// 出力フォーマット (text / json)
        #[arg(long, default_value = "text")]
        format: String,

        /// サマリーのみ出力
        #[arg(long)]
        summary: bool,
    },

    /// 特定ファイルの差分を表示
    Diff {
        /// 対象パス
        path: String,

        /// 比較の左側
        #[arg(long)]
        left: Option<String>,

        /// 比較の右側
        #[arg(long)]
        right: Option<String>,

        /// 出力フォーマット (text / json)
        #[arg(long, default_value = "text")]
        format: String,

        /// 出力行数の上限
        #[arg(long)]
        max_lines: Option<usize>,

        /// 出力ファイル数の上限（ディレクトリ指定時）
        #[arg(long)]
        max_files: Option<usize>,
    },

    /// ファイルをマージする
    Merge {
        /// 対象パス
        path: String,

        /// マージ元（この内容でマージ先を上書き）
        #[arg(long)]
        left: Option<String>,

        /// マージ先
        #[arg(long)]
        right: Option<String>,

        /// 実行せず確認のみ
        #[arg(long)]
        dry_run: bool,

        /// 確認プロンプトを省略
        #[arg(long)]
        force: bool,
    },
}

/// tokio ランタイム（TUI 内で同期的に非同期操作を呼ぶため）
struct TuiRuntime {
    rt: tokio::runtime::Runtime,
    ssh_client: Option<SshClient>,
    config: AppConfig,
}

impl TuiRuntime {
    fn new(config: AppConfig) -> Self {
        Self {
            rt: tokio::runtime::Runtime::new().expect("tokio runtime creation failed"),
            ssh_client: None,
            config,
        }
    }

    /// SSH 接続を確立する
    fn connect(&mut self, server_name: &str) -> anyhow::Result<()> {
        let server_config = self.config.servers.get(server_name).ok_or_else(|| {
            anyhow::anyhow!("サーバ '{}' が設定に見つかりません", server_name)
        })?;

        let client = self.rt.block_on(SshClient::connect(
            server_name,
            server_config,
            &self.config.ssh,
        ))?;

        self.ssh_client = Some(client);
        Ok(())
    }

    /// リモートツリーを取得する
    fn fetch_remote_tree(&mut self, server_name: &str) -> anyhow::Result<FileTree> {
        let server_config = self.config.servers.get(server_name).ok_or_else(|| {
            anyhow::anyhow!("サーバ '{}' が設定に見つかりません", server_name)
        })?;
        let root_dir = server_config.root_dir.to_string_lossy().to_string();

        let client = self.ssh_client.as_mut().ok_or_else(|| {
            anyhow::anyhow!("SSH 未接続")
        })?;

        let nodes = self.rt.block_on(
            client.list_dir(&root_dir, &self.config.filter.exclude)
        )?;

        let mut tree = FileTree::new(&server_config.root_dir);
        tree.nodes = nodes;
        tree.sort();
        Ok(tree)
    }

    /// リモートファイル内容を取得する
    fn read_remote_file(&mut self, server_name: &str, rel_path: &str) -> anyhow::Result<String> {
        let server_config = self.config.servers.get(server_name).ok_or_else(|| {
            anyhow::anyhow!("サーバ '{}' が設定に見つかりません", server_name)
        })?;
        let remote_root = server_config.root_dir.to_string_lossy().to_string();
        let full_path = executor::validate_remote_path(&remote_root, rel_path)?;

        let client = self.ssh_client.as_mut().ok_or_else(|| {
            anyhow::anyhow!("SSH 未接続")
        })?;

        self.rt.block_on(client.read_file(&full_path))
    }

    /// リモートファイルに書き込む
    fn write_remote_file(&mut self, server_name: &str, rel_path: &str, content: &str) -> anyhow::Result<()> {
        let server_config = self.config.servers.get(server_name).ok_or_else(|| {
            anyhow::anyhow!("サーバ '{}' が設定に見つかりません", server_name)
        })?;
        let remote_root = server_config.root_dir.to_string_lossy().to_string();
        let full_path = executor::validate_remote_path(&remote_root, rel_path)?;

        let client = self.ssh_client.as_mut().ok_or_else(|| {
            anyhow::anyhow!("SSH 未接続")
        })?;

        self.rt.block_on(client.write_file(&full_path, content))
    }

    /// 切断する
    fn disconnect(&mut self) {
        if let Some(client) = self.ssh_client.take() {
            let _ = self.rt.block_on(client.disconnect());
        }
    }
}

fn main() -> anyhow::Result<()> {
    // ログ初期化
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init) => {
            remote_merge::init::run_init()?;
        }
        Some(Commands::Status { .. }) => {
            // TODO: Phase 4 で実装
            println!("remote-merge status: 差分一覧を表示します（未実装）");
        }
        Some(Commands::Diff { .. }) => {
            // TODO: Phase 4 で実装
            println!("remote-merge diff: 差分を表示します（未実装）");
        }
        Some(Commands::Merge { .. }) => {
            // TODO: Phase 4 で実装
            println!("remote-merge merge: マージを実行します（未実装）");
        }
        None => {
            // TUI モード
            let config = config::load_config()?;

            let server_name = cli
                .server
                .or(cli.right)
                .unwrap_or_else(|| {
                    config
                        .servers
                        .keys()
                        .next()
                        .cloned()
                        .unwrap_or_else(|| "develop".to_string())
                });

            tracing::info!("TUI モード起動: local ↔ {}", server_name);

            // ローカルツリーを取得
            let local_tree = local::scan_local_tree(
                &config.local.root_dir,
                &config.filter.exclude,
            )?;

            // 利用可能なサーバ名一覧
            let available_servers: Vec<String> = config.servers.keys().cloned().collect();

            // TuiRuntime を構築
            let mut runtime = TuiRuntime::new(config.clone());

            // SSH 接続を試行してリモートツリーを取得
            let (remote_tree, is_connected) = match runtime.connect(&server_name) {
                Ok(()) => {
                    match runtime.fetch_remote_tree(&server_name) {
                        Ok(tree) => (tree, true),
                        Err(e) => {
                            tracing::warn!("リモートツリー取得に失敗: {}", e);
                            let root = config.servers.get(&server_name)
                                .map(|s| s.root_dir.clone())
                                .unwrap_or_default();
                            (FileTree::new(root), true) // 接続はできたがツリー取得失敗
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("SSH 接続に失敗（オフラインモード）: {}", e);
                    let root = config.servers.get(&server_name)
                        .map(|s| s.root_dir.clone())
                        .unwrap_or_default();
                    (FileTree::new(root), false)
                }
            };

            let mut app_state = AppState::new(local_tree, remote_tree, server_name.clone());
            app_state.available_servers = available_servers;
            app_state.is_connected = is_connected;
            app_state.exclude_patterns = config.filter.exclude.clone();

            if !is_connected {
                app_state.status_message = format!(
                    "local ↔ {} (offline) | s: server | q: quit",
                    server_name
                );
            }

            // TUI 起動
            run_tui(app_state, runtime)?;
        }
    }

    Ok(())
}

/// TUI イベントループを実行する
fn run_tui(mut state: AppState, mut runtime: TuiRuntime) -> anyhow::Result<()> {
    // ターミナルをセットアップ
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // メインループ
    let result = run_event_loop(&mut terminal, &mut state, &mut runtime);

    // 切断
    runtime.disconnect();

    // ターミナルをリストア（エラーでも必ず実行）
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

/// イベントループ本体
fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    runtime: &mut TuiRuntime,
) -> anyhow::Result<()> {
    loop {
        // 描画
        terminal.draw(|frame| {
            draw_ui(frame, state);
        })?;

        // イベント待ち
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            // ダイアログ表示中はダイアログのキーハンドリングを優先
            if state.has_dialog() {
                handle_dialog_key(state, runtime, key.code);

                if state.should_quit {
                    break;
                }
                continue;
            }

            match state.focus {
                Focus::FileTree => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        state.should_quit = true;
                    }
                    KeyCode::Tab => state.toggle_focus(),
                    KeyCode::Up | KeyCode::Char('k') => state.cursor_up(),
                    KeyCode::Down | KeyCode::Char('j') => state.cursor_down(),
                    KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                        if state.flat_nodes.get(state.tree_cursor).is_some_and(|n| n.is_dir) {
                            // 遅延読み込み: 未取得なら先にロード
                            if let Some(path) = state.current_path() {
                                let needs_load = state
                                    .local_tree
                                    .find_node(std::path::Path::new(&path))
                                    .is_some_and(|n| n.is_dir() && !n.is_loaded());
                                if needs_load {
                                    state.load_local_children(&path);
                                    // リモート側も遅延読み込み
                                    if state.is_connected {
                                        load_remote_children(state, runtime, &path);
                                    }
                                }
                            }
                            state.toggle_expand();
                        } else {
                            // ファイル選択時にコンテンツをロード
                            load_file_content(state, runtime);
                            state.select_file();
                        }
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        if state.flat_nodes.get(state.tree_cursor).is_some_and(|n| n.is_dir && n.expanded) {
                            state.toggle_expand();
                        }
                    }
                    KeyCode::Char('r') => {
                        // ディレクトリ上なら子ノードをリフレッシュ、それ以外はキャッシュクリア
                        if state.current_is_dir() {
                            if let Some(path) = state.current_path() {
                                state.refresh_directory(&path);
                            }
                        } else {
                            state.clear_cache();
                        }
                    }
                    KeyCode::Char('f') => state.show_filter_panel(),
                    KeyCode::Char('s') => state.show_server_menu(),
                    KeyCode::Char('c') => {
                        // 手動再接続
                        execute_reconnect(state, runtime);
                    }
                    KeyCode::Char('?') => state.show_help(),
                    KeyCode::Char('L') => {
                        // Shift+L: LeftMerge (local → remote)
                        if key.modifiers.contains(KeyModifiers::SHIFT) || key.code == KeyCode::Char('L') {
                            state.show_merge_dialog(MergeDirection::LeftMerge);
                        }
                    }
                    KeyCode::Char('R') => {
                        // Shift+R: RightMerge (remote → local)
                        if key.modifiers.contains(KeyModifiers::SHIFT) || key.code == KeyCode::Char('R') {
                            state.show_merge_dialog(MergeDirection::RightMerge);
                        }
                    }
                    _ => {}
                },
                Focus::DiffView => match key.code {
                    KeyCode::Char('q') => {
                        if state.has_unsaved_changes() {
                            state.dialog = DialogState::UnsavedChanges;
                        } else {
                            state.should_quit = true;
                        }
                    }
                    KeyCode::Esc => {
                        if state.pending_hunk_merge.is_some() {
                            state.cancel_hunk_merge();
                        } else if state.has_unsaved_changes() {
                            state.dialog = DialogState::UnsavedChanges;
                        } else {
                            state.should_quit = true;
                        }
                    }
                    KeyCode::Tab => {
                        state.cancel_hunk_merge();
                        state.toggle_focus();
                    }
                    // j/k, ↑/↓: 1行スクロール（スクロールではpending操作をキャンセルしない）
                    KeyCode::Up | KeyCode::Char('k') => {
                        state.scroll_up();
                        state.sync_hunk_cursor_to_scroll();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        state.scroll_down();
                        state.sync_hunk_cursor_to_scroll();
                    }
                    // n/N: ハンクジャンプ（ハンクジャンプ時のみpending操作をキャンセル）
                    KeyCode::Char('n') => {
                        state.cancel_hunk_merge();
                        state.hunk_cursor_down();
                    }
                    KeyCode::Char('N') => {
                        state.cancel_hunk_merge();
                        state.hunk_cursor_up();
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        // → キー: remote → local のハンク即時適用
                        if state.hunk_count() > 0 {
                            state.apply_hunk_merge(HunkDirection::RightToLeft);
                        }
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        // ← キー: local → remote のハンク即時適用
                        if state.is_connected {
                            if state.hunk_count() > 0 {
                                state.apply_hunk_merge(HunkDirection::LeftToRight);
                            }
                        } else if state.hunk_count() > 0 {
                            state.status_message = "SSH 未接続: リモートへのハンクマージはできません".to_string();
                        }
                    }
                    KeyCode::Char('w') => {
                        // w キー: 変更をファイルに書き込み（確認ダイアログ）
                        if state.has_unsaved_changes() {
                            state.dialog = DialogState::WriteConfirmation;
                        } else {
                            state.status_message = "No changes to write".to_string();
                        }
                    }
                    KeyCode::Char('u') => {
                        // u キー: 最後の操作を undo
                        state.undo_last();
                    }
                    KeyCode::Char('U') => {
                        // U キー: 全操作を undo
                        state.undo_all();
                    }
                    KeyCode::PageDown => {
                        state.scroll_page_down(20);
                        state.sync_hunk_cursor_to_scroll();
                    }
                    KeyCode::PageUp => {
                        state.scroll_page_up(20);
                        state.sync_hunk_cursor_to_scroll();
                    }
                    KeyCode::Home => {
                        state.scroll_to_home();
                        state.sync_hunk_cursor_to_scroll();
                    }
                    KeyCode::End => {
                        state.scroll_to_end();
                        state.sync_hunk_cursor_to_scroll();
                    }
                    KeyCode::Char('c') => {
                        // 手動再接続
                        execute_reconnect(state, runtime);
                    }
                    KeyCode::Char('d') => {
                        state.toggle_diff_mode();
                    }
                    KeyCode::Char('?') => state.show_help(),
                    _ => {}
                },
            }

            if state.should_quit {
                break;
            }
        }
    }

    Ok(())
}

/// ダイアログ表示中のキーハンドリング
fn handle_dialog_key(state: &mut AppState, runtime: &mut TuiRuntime, key: KeyCode) {
    let mut dialog = state.dialog.clone();
    match dialog {
        DialogState::Confirm(ref confirm) => match key {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                execute_merge(state, runtime, confirm);
                state.close_dialog();
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                state.status_message = "マージをキャンセルしました".to_string();
                state.close_dialog();
            }
            _ => {}
        },
        DialogState::ServerSelect(ref mut menu) => match key {
            KeyCode::Up | KeyCode::Char('k') => {
                if let DialogState::ServerSelect(ref mut m) = state.dialog {
                    m.cursor_up();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let DialogState::ServerSelect(ref mut m) = state.dialog {
                    m.cursor_down();
                }
            }
            KeyCode::Enter => {
                let selected = menu.selected().map(|s| s.to_string());
                if let Some(server_name) = selected {
                    if server_name != state.server_name {
                        execute_server_switch(state, runtime, &server_name);
                    }
                }
                state.close_dialog();
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                state.close_dialog();
            }
            _ => {}
        },
        DialogState::Filter(_) => match key {
            KeyCode::Up | KeyCode::Char('k') => {
                if let DialogState::Filter(ref mut panel) = state.dialog {
                    panel.cursor_up();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let DialogState::Filter(ref mut panel) = state.dialog {
                    panel.cursor_down();
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                if let DialogState::Filter(ref mut panel) = state.dialog {
                    panel.toggle();
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                // 閉じる前にフィルター変更を適用
                if let DialogState::Filter(ref panel) = state.dialog {
                    let panel_clone = panel.clone();
                    state.apply_filter_changes(&panel_clone);
                }
                state.close_dialog();
            }
            _ => {}
        },
        DialogState::Help(_) => match key {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                state.close_dialog();
            }
            _ => {}
        },
        DialogState::HunkMergePreview(ref preview) => match key {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let direction = preview.direction;
                state.pending_hunk_merge = None;
                state.close_dialog();
                execute_hunk_merge(state, runtime, direction);
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                state.pending_hunk_merge = None;
                state.status_message = "ハンクマージをキャンセルしました".to_string();
                state.close_dialog();
            }
            _ => {}
        },
        DialogState::WriteConfirmation => match key {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                state.close_dialog();
                execute_write_changes(state, runtime);
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                state.status_message = "書き込みをキャンセルしました".to_string();
                state.close_dialog();
            }
            _ => {}
        },
        DialogState::UnsavedChanges => match key {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                // 破棄して終了
                state.undo_stack.clear();
                state.close_dialog();
                state.should_quit = true;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                state.status_message = "終了をキャンセルしました | w:write u:undo".to_string();
                state.close_dialog();
            }
            _ => {}
        },
        DialogState::None => {}
    }
}

/// 変更をファイルに書き込む（w キー確定後）
fn execute_write_changes(state: &mut AppState, runtime: &mut TuiRuntime) {
    if let Some(path) = state.selected_path.clone() {
        let changes = state.undo_stack.len();

        // ローカルファイルに書き込む
        if let Some(local_content) = state.local_cache.get(&path) {
            let local_root = state.local_tree.root.clone();
            if let Err(e) = executor::write_local_file(&local_root, &path, local_content) {
                state.status_message = format!("ローカル書き込み失敗: {}", e);
                return;
            }
        }

        // リモートファイルに書き込む
        if state.is_connected {
            if let Some(remote_content) = state.remote_cache.get(&path).cloned() {
                if let Err(e) = runtime.write_remote_file(&state.server_name, &path, &remote_content) {
                    state.status_message = format!("リモート書き込み失敗: {}", e);
                    return;
                }
            }
        }

        // undo スタックをクリア（書き込み済みなので）
        state.undo_stack.clear();
        state.status_message = format!(
            "{}: {} changes written | {} hunks remaining",
            path, changes, state.hunk_count()
        );
    }
}

/// リモートディレクトリの遅延読み込み
fn load_remote_children(state: &mut AppState, runtime: &mut TuiRuntime, rel_path: &str) {
    let server_name = state.server_name.clone();
    let server_config = match runtime.config.servers.get(&server_name) {
        Some(c) => c,
        None => return,
    };
    let remote_root = server_config.root_dir.to_string_lossy().to_string();
    let full_path = format!("{}/{}", remote_root.trim_end_matches('/'), rel_path);
    let exclude = state.active_exclude_patterns();

    let client = match runtime.ssh_client.as_mut() {
        Some(c) => c,
        None => return,
    };

    match runtime.rt.block_on(client.list_dir(&full_path, &exclude)) {
        Ok(children) => {
            if let Some(node) = state.remote_tree.find_node_mut(std::path::Path::new(rel_path)) {
                node.children = Some(children);
                node.sort_children();
            }
        }
        Err(e) => {
            tracing::debug!("リモートディレクトリ取得スキップ: {} - {}", rel_path, e);
            state.status_message = format!("リモートディレクトリ取得失敗: {} - {}", rel_path, e);
        }
    }
}

/// ファイル選択時にコンテンツをロードする
fn load_file_content(state: &mut AppState, runtime: &mut TuiRuntime) {
    let node = match state.flat_nodes.get(state.tree_cursor) {
        Some(n) if !n.is_dir => n.clone(),
        _ => return,
    };

    let path = &node.path;

    // ローカルキャッシュ
    if !state.local_cache.contains_key(path) {
        let local_root = &state.local_tree.root;
        match executor::read_local_file(local_root, path) {
            Ok(content) => {
                state.local_cache.insert(path.clone(), content);
                state.error_paths.remove(path);
            }
            Err(e) => {
                tracing::debug!("ローカルファイル読み込みスキップ: {} - {}", path, e);
                state.status_message = format!("ローカル読み込み失敗: {} - {}", path, e);
                state.error_paths.insert(path.clone());
            }
        }
    }

    // リモートキャッシュ
    if !state.remote_cache.contains_key(path) && state.is_connected {
        match runtime.read_remote_file(&state.server_name, path) {
            Ok(content) => {
                state.remote_cache.insert(path.clone(), content);
                state.error_paths.remove(path);
            }
            Err(e) => {
                tracing::debug!("リモートファイル読み込みスキップ: {} - {}", path, e);
                state.status_message = format!("リモート読み込み失敗: {} - {}", path, e);
                state.error_paths.insert(path.clone());
            }
        }
    }

    // バッジを更新
    state.rebuild_flat_nodes();
}

/// マージを実行する
fn execute_merge(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    confirm: &remote_merge::ui::dialog::ConfirmDialog,
) {
    let path = &confirm.file_path;
    let direction = confirm.direction;

    match direction {
        MergeDirection::LeftMerge => {
            // local → remote: ローカル内容をリモートに書き込む
            let content = match state.local_cache.get(path) {
                Some(c) => c.clone(),
                None => {
                    state.status_message = format!("{}: ローカル内容が未取得です", path);
                    return;
                }
            };

            if !state.is_connected {
                state.status_message = "SSH 未接続: マージを実行できません".to_string();
                return;
            }

            match runtime.write_remote_file(&state.server_name, path, &content) {
                Ok(()) => {
                    state.update_badge_after_merge(path, &content, direction);
                    state.status_message = format!(
                        "{}: local → {} にマージしました",
                        path, state.server_name
                    );
                }
                Err(e) => {
                    state.status_message = format!("マージ失敗: {}", e);
                }
            }
        }
        MergeDirection::RightMerge => {
            // remote → local: リモート内容をローカルに書き込む
            let content = match state.remote_cache.get(path) {
                Some(c) => c.clone(),
                None => {
                    state.status_message = format!("{}: リモート内容が未取得です", path);
                    return;
                }
            };

            let local_root = state.local_tree.root.clone();
            match executor::write_local_file(&local_root, path, &content) {
                Ok(()) => {
                    state.update_badge_after_merge(path, &content, direction);
                    state.status_message = format!(
                        "{}: {} → local にマージしました",
                        path, state.server_name
                    );
                }
                Err(e) => {
                    state.status_message = format!("マージ失敗: {}", e);
                }
            }
        }
    }
}

/// ハンクマージを実行する（2段階操作の確定時）
fn execute_hunk_merge(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    direction: HunkDirection,
) {
    if let Some(path) = state.apply_hunk_merge(direction) {
        match direction {
            HunkDirection::RightToLeft => {
                // ローカルファイルに書き込む
                let content = state.local_cache.get(&path).cloned().unwrap_or_default();
                let local_root = state.local_tree.root.clone();
                match executor::write_local_file(&local_root, &path, &content) {
                    Ok(()) => {
                        state.status_message = format!(
                            "Hunk merged: remote → local ({}) | {} hunks left",
                            path,
                            state.hunk_count(),
                        );
                    }
                    Err(e) => {
                        state.status_message = format!("ローカル書き込み失敗: {}", e);
                    }
                }
            }
            HunkDirection::LeftToRight => {
                // リモートファイルに書き込む
                let content = state.remote_cache.get(&path).cloned().unwrap_or_default();
                match runtime.write_remote_file(&state.server_name, &path, &content) {
                    Ok(()) => {
                        state.status_message = format!(
                            "Hunk merged: local → remote ({}) | {} hunks left",
                            path,
                            state.hunk_count(),
                        );
                    }
                    Err(e) => {
                        state.status_message = format!("リモート書き込み失敗: {}", e);
                    }
                }
            }
        }
    }
}

/// SSH 再接続を実行する（c キー）
fn execute_reconnect(state: &mut AppState, runtime: &mut TuiRuntime) {
    let server_name = state.server_name.clone();
    state.status_message = format!("{} に再接続中...", server_name);

    // 既存の接続を切断
    runtime.disconnect();

    match runtime.connect(&server_name) {
        Ok(()) => {
            match runtime.fetch_remote_tree(&server_name) {
                Ok(tree) => {
                    // 再接続成功: キャッシュとundoスタックをクリア
                    state.remote_tree = tree;
                    state.remote_cache.clear();
                    state.current_diff = None;
                    state.selected_path = None;
                    state.diff_scroll = 0;
                    state.hunk_cursor = 0;
                    state.pending_hunk_merge = None;
                    state.is_connected = true;
                    state.rebuild_flat_nodes();
                    state.status_message = format!(
                        "接続復旧: {} | 未保存の変更はリセットされました",
                        server_name
                    );
                }
                Err(e) => {
                    state.is_connected = false;
                    state.status_message = format!(
                        "{} のツリー取得に失敗: {}",
                        server_name, e
                    );
                }
            }
        }
        Err(e) => {
            state.is_connected = false;
            state.status_message = format!("{} への再接続に失敗: {} | c: retry", server_name, e);
        }
    }
}

/// サーバ切替を実行する
fn execute_server_switch(state: &mut AppState, runtime: &mut TuiRuntime, server_name: &str) {
    state.status_message = format!("{} に接続中...", server_name);

    // 既存の接続を切断
    runtime.disconnect();

    match runtime.connect(server_name) {
        Ok(()) => {
            match runtime.fetch_remote_tree(server_name) {
                Ok(tree) => {
                    state.switch_server(server_name.to_string(), tree);
                    state.status_message = format!(
                        "local ↔ {} | Tab: switch focus | s: server | q: quit",
                        server_name
                    );
                }
                Err(e) => {
                    state.status_message = format!(
                        "{} のツリー取得に失敗: {}",
                        server_name, e
                    );
                }
            }
        }
        Err(e) => {
            state.status_message = format!("{} への接続に失敗: {}", server_name, e);
        }
    }
}

/// UI を描画する
fn draw_ui(frame: &mut Frame, state: &AppState) {
    let layout = AppLayout::new(frame.area());

    // ヘッダ
    let conn_indicator = if state.is_connected { "●" } else { "○" };
    let conn_color = if state.is_connected { Color::Green } else { Color::Red };

    let header = Paragraph::new(Line::from(vec![
        Span::styled(" remote-merge ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("| "),
        Span::styled("local", Style::default().fg(Color::Green)),
        Span::raw(" ↔ "),
        Span::styled(&state.server_name, Style::default().fg(Color::Yellow)),
        Span::raw(" "),
        Span::styled(conn_indicator, Style::default().fg(conn_color)),
    ]))
    .style(Style::default().bg(Color::DarkGray));
    frame.render_widget(header, layout.header);

    // ファイルツリー
    let tree_view = TreeView::new(state);
    frame.render_widget(tree_view, layout.tree_pane);

    // Diff ビュー
    let diff_view = DiffView::new(state);
    frame.render_widget(diff_view, layout.diff_pane);

    // ステータスバー
    let status = Paragraph::new(Line::from(vec![
        Span::styled(&state.status_message, Style::default().fg(Color::White)),
    ]))
    .style(Style::default().bg(Color::DarkGray));
    frame.render_widget(status, layout.status_bar);

    // ダイアログ（最前面に描画）
    match &state.dialog {
        DialogState::Confirm(confirm) => {
            let widget = ConfirmDialogWidget::new(confirm);
            frame.render_widget(widget, frame.area());
        }
        DialogState::ServerSelect(menu) => {
            let widget = ServerMenuWidget::new(menu);
            frame.render_widget(widget, frame.area());
        }
        DialogState::Filter(panel) => {
            let widget = FilterPanelWidget::new(panel);
            frame.render_widget(widget, frame.area());
        }
        DialogState::Help(help) => {
            let widget = HelpOverlayWidget::new(help);
            frame.render_widget(widget, frame.area());
        }
        DialogState::HunkMergePreview(preview) => {
            let widget = HunkMergePreviewWidget::new(preview);
            frame.render_widget(widget, frame.area());
        }
        DialogState::WriteConfirmation => {
            render_simple_dialog(
                frame,
                " Write Changes ",
                &format!("{}件の変更をファイルに書き込みますか？", state.undo_stack.len()),
                Color::Green,
            );
        }
        DialogState::UnsavedChanges => {
            render_simple_dialog(
                frame,
                " Unsaved Changes ",
                "未保存の変更があります。破棄して終了しますか？",
                Color::Yellow,
            );
        }
        DialogState::None => {}
    }
}

/// シンプルな Y/n 確認ダイアログを描画する
fn render_simple_dialog(frame: &mut Frame, title: &str, message: &str, color: Color) {
    use remote_merge::ui::dialog::centered_rect;
    use ratatui::widgets::Clear;

    let dialog_area = centered_rect(60, 7, frame.area());
    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color).add_modifier(Modifier::BOLD));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let msg = Paragraph::new(Line::from(vec![
        Span::raw("  "),
        Span::styled(message, Style::default().fg(Color::White)),
    ]));
    frame.render_widget(msg, chunks[1]);

    let guide = Paragraph::new(Line::from(vec![
        Span::raw("  "),
        Span::styled("[Y]", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::raw(" はい  "),
        Span::styled("[n/Esc]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        Span::raw(" いいえ"),
    ]));
    frame.render_widget(guide, chunks[3]);
}
