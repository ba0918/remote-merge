use clap::{Parser, Subcommand};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::prelude::*;
use std::io;

use remote_merge::app::{AppState, Focus, MergeScanState, ScanState};
use remote_merge::config::{self, AppConfig};
use remote_merge::handler::{dialog_keys, diff_keys, tree_keys};
use remote_merge::local;
use remote_merge::runtime::TuiRuntime;
use remote_merge::runtime::{merge_scan, scanner};
use remote_merge::tree::FileTree;
use remote_merge::ui::render::draw_ui;

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
        #[arg(short, long)]
        server: Option<String>,
        #[arg(long)]
        left: Option<String>,
        #[arg(long)]
        right: Option<String>,
        #[arg(long, default_value = "text")]
        format: String,
        #[arg(long)]
        summary: bool,
    },

    /// 特定ファイルの差分を表示
    Diff {
        path: String,
        #[arg(long)]
        left: Option<String>,
        #[arg(long)]
        right: Option<String>,
        #[arg(long, default_value = "text")]
        format: String,
        #[arg(long)]
        max_lines: Option<usize>,
        #[arg(long)]
        max_files: Option<usize>,
    },

    /// ファイルをマージする
    Merge {
        path: String,
        #[arg(long)]
        left: Option<String>,
        #[arg(long)]
        right: Option<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        force: bool,
    },
}

fn main() -> anyhow::Result<()> {
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
            println!("remote-merge status: not yet implemented (Phase 4)");
        }
        Some(Commands::Diff { .. }) => {
            println!("remote-merge diff: not yet implemented (Phase 4)");
        }
        Some(Commands::Merge { .. }) => {
            println!("remote-merge merge: not yet implemented (Phase 4)");
        }
        None => {
            let config = config::load_config()?;
            run_tui_mode(cli, config)?;
        }
    }

    Ok(())
}

/// TUI モードを起動する
fn run_tui_mode(cli: Cli, config: AppConfig) -> anyhow::Result<()> {
    let server_name = cli.server.or(cli.right).unwrap_or_else(|| {
        config
            .servers
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| "develop".to_string())
    });

    tracing::info!("TUI mode: local <-> {}", server_name);

    let local_tree = local::scan_local_tree(&config.local.root_dir, &config.filter.exclude)?;
    let available_servers: Vec<String> = config.servers.keys().cloned().collect();

    let mut runtime = TuiRuntime::new(config.clone());

    let (remote_tree, is_connected) = match runtime.connect(&server_name) {
        Ok(()) => match runtime.fetch_remote_tree(&server_name) {
            Ok(tree) => (tree, true),
            Err(e) => {
                tracing::warn!("Remote tree fetch failed: {}", e);
                let root = config
                    .servers
                    .get(&server_name)
                    .map(|s| s.root_dir.clone())
                    .unwrap_or_default();
                (FileTree::new(root), true)
            }
        },
        Err(e) => {
            tracing::warn!("SSH connection failed (offline mode): {}", e);
            let root = config
                .servers
                .get(&server_name)
                .map(|s| s.root_dir.clone())
                .unwrap_or_default();
            (FileTree::new(root), false)
        }
    };

    let mut app_state = AppState::new(local_tree, remote_tree, server_name.clone());
    app_state.available_servers = available_servers;
    app_state.is_connected = is_connected;
    app_state.exclude_patterns = config.filter.exclude.clone();
    app_state.sensitive_patterns = config.filter.sensitive.clone();

    if !is_connected {
        app_state.status_message =
            format!("local <-> {} (offline) | s: server | q: quit", server_name);
    }

    run_tui(app_state, runtime)
}

/// TUI イベントループを実行する
fn run_tui(mut state: AppState, mut runtime: TuiRuntime) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_event_loop(&mut terminal, &mut state, &mut runtime);

    runtime.disconnect();

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
        scanner::poll_scan_result(state, runtime);
        merge_scan::poll_merge_scan_result(state, runtime);

        terminal.draw(|frame| {
            draw_ui(frame, state);
        })?;

        let is_scanning = matches!(state.scan_state, ScanState::Scanning)
            || !matches!(state.merge_scan_state, MergeScanState::Idle);
        let timeout = if is_scanning {
            std::time::Duration::from_millis(100)
        } else {
            std::time::Duration::from_secs(60)
        };

        if !event::poll(timeout)? {
            continue;
        }

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            if state.has_dialog() {
                dialog_keys::handle_dialog_key(state, runtime, key.code);
            } else {
                match state.focus {
                    Focus::FileTree => {
                        tree_keys::handle_tree_key(state, runtime, key.code, key.modifiers);
                    }
                    Focus::DiffView => {
                        diff_keys::handle_diff_key(state, runtime, key.code);
                    }
                }
            }

            if state.should_quit {
                break;
            }
        }
    }

    Ok(())
}
