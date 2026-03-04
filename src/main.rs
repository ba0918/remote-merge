use std::io;

use clap::{Parser, Subcommand};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use remote_merge::app::{AppState, Focus};
use remote_merge::config;
use remote_merge::local;
use remote_merge::tree::FileTree;
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
            // TODO: Phase 1-4 で実装
            println!("remote-merge init: .remote-merge.toml を生成します（未実装）");
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

            // リモートツリーは接続後に取得（現段階では空ツリー）
            // TODO: Phase 1-3 で SSH 接続してリモートツリーを取得する
            let remote_tree = FileTree::new(
                config.servers.get(&server_name)
                    .map(|s| s.root_dir.clone())
                    .unwrap_or_default(),
            );

            let app_state = AppState::new(local_tree, remote_tree, server_name);

            // TUI 起動
            run_tui(app_state)?;
        }
    }

    Ok(())
}

/// TUI イベントループを実行する
fn run_tui(mut state: AppState) -> anyhow::Result<()> {
    // ターミナルをセットアップ
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // メインループ
    let result = run_event_loop(&mut terminal, &mut state);

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
                            state.toggle_expand();
                        } else {
                            state.select_file();
                        }
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        // ディレクトリなら折りたたみ
                        if state.flat_nodes.get(state.tree_cursor).is_some_and(|n| n.is_dir && n.expanded) {
                            state.toggle_expand();
                        }
                    }
                    KeyCode::Char('r') => state.clear_cache(),
                    _ => {}
                },
                Focus::DiffView => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        state.should_quit = true;
                    }
                    KeyCode::Tab => state.toggle_focus(),
                    KeyCode::Up | KeyCode::Char('k') => state.scroll_up(),
                    KeyCode::Down | KeyCode::Char('j') => state.scroll_down(),
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

/// UI を描画する
fn draw_ui(frame: &mut Frame, state: &AppState) {
    let layout = AppLayout::new(frame.area());

    // ヘッダ
    let header = Paragraph::new(Line::from(vec![
        Span::styled(" remote-merge ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("| "),
        Span::styled("local", Style::default().fg(Color::Green)),
        Span::raw(" ↔ "),
        Span::styled(&state.server_name, Style::default().fg(Color::Yellow)),
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
}
