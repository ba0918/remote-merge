use clap::{Parser, Subcommand};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::prelude::*;
use std::io;
use std::sync::Mutex;

use remote_merge::app::{AppState, Focus, MergeScanState, ScanState};
use remote_merge::config::{self, AppConfig};
use remote_merge::handler::{dialog_keys, diff_keys, tree_keys};
use remote_merge::local;
use remote_merge::runtime::TuiRuntime;
use remote_merge::runtime::{merge_scan, scanner};
use remote_merge::tree::FileTree;
use remote_merge::ui::render::draw_ui;

/// TUI tool for graphically displaying and merging file diffs between local and remote servers
#[derive(Parser, Debug)]
#[command(name = "remote-merge", version, about)]
struct Cli {
    /// Server name to compare with local
    #[arg(short, long)]
    server: Option<String>,

    /// Left side of comparison (default: local)
    #[arg(long)]
    left: Option<String>,

    /// Right side of comparison
    #[arg(long)]
    right: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize project config file
    Init,

    /// List files with differences
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

    /// Show diff for a specific file
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

    /// Merge files
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
    let cli = Cli::parse();
    init_tracing(cli.command.is_none());

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
    use remote_merge::app::Side;

    let right_server = cli.server.or(cli.right).unwrap_or_else(|| {
        config
            .servers
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| "develop".to_string())
    });
    let left_server = cli.left;

    let available_servers: Vec<String> = config.servers.keys().cloned().collect();
    let mut runtime = TuiRuntime::new(config.clone());

    // 左側: --left が指定されたらリモート、なければローカル
    let (left_tree, left_source, left_connected) = if let Some(ref left_name) = left_server {
        tracing::info!("TUI mode: {} <-> {}", left_name, right_server);
        match runtime.connect(left_name) {
            Ok(()) => match runtime.fetch_remote_tree(left_name) {
                Ok(tree) => (tree, Side::Remote(left_name.clone()), true),
                Err(e) => {
                    tracing::warn!("Left remote tree fetch failed: {}", e);
                    let root = config
                        .servers
                        .get(left_name)
                        .map(|s| s.root_dir.clone())
                        .unwrap_or_default();
                    (FileTree::new(root), Side::Remote(left_name.clone()), true)
                }
            },
            Err(e) => {
                tracing::warn!("Left SSH connection failed: {}", e);
                let root = config
                    .servers
                    .get(left_name)
                    .map(|s| s.root_dir.clone())
                    .unwrap_or_default();
                (FileTree::new(root), Side::Remote(left_name.clone()), false)
            }
        }
    } else {
        tracing::info!("TUI mode: local <-> {}", right_server);
        let tree = local::scan_local_tree(&config.local.root_dir, &config.filter.exclude)?;
        (tree, Side::Local, true)
    };

    // 右側: 常にリモート
    let (right_tree, right_connected) = match runtime.connect(&right_server) {
        Ok(()) => match runtime.fetch_remote_tree(&right_server) {
            Ok(tree) => (tree, true),
            Err(e) => {
                tracing::warn!("Right remote tree fetch failed: {}", e);
                let root = config
                    .servers
                    .get(&right_server)
                    .map(|s| s.root_dir.clone())
                    .unwrap_or_default();
                (FileTree::new(root), true)
            }
        },
        Err(e) => {
            tracing::warn!("Right SSH connection failed (offline mode): {}", e);
            let root = config
                .servers
                .get(&right_server)
                .map(|s| s.root_dir.clone())
                .unwrap_or_default();
            (FileTree::new(root), false)
        }
    };

    let right_source = Side::Remote(right_server.clone());
    let is_connected = left_connected && right_connected;

    // 永続化された UI 状態を復元（テーマなど）
    let persisted = remote_merge::state::load_state();
    let label = remote_merge::app::side::comparison_label(&left_source, &right_source);
    let mut app_state = AppState::new(
        left_tree,
        right_tree,
        left_source,
        right_source,
        &persisted.theme,
    );
    app_state.available_servers = available_servers;
    app_state.is_connected = is_connected;
    app_state.exclude_patterns = config.filter.exclude.clone();
    app_state.sensitive_patterns = config.filter.sensitive.clone();

    if !is_connected {
        app_state.status_message = format!("{} (offline) | s: server | q: quit", label);
    }

    // 起動時に古いバックアップをクリーンアップ
    if config.backup.enabled {
        let backup_dir = config
            .local
            .root_dir
            .join(remote_merge::backup::BACKUP_DIR_NAME);
        match remote_merge::backup::cleanup_old_backups(
            &backup_dir,
            config.backup.retention_days,
            chrono::Utc::now(),
        ) {
            Ok(removed) if !removed.is_empty() => {
                tracing::info!("Cleaned up {} old backup(s)", removed.len());
            }
            Err(e) => {
                tracing::warn!("Backup cleanup failed: {}", e);
            }
            _ => {}
        }
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

    runtime.disconnect_all();

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
        // tokio Runtime を駆動して SSH keepalive 等の pending タスクを処理
        runtime.drive_runtime();

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

/// tracing を初期化する。
///
/// TUI モードでは stderr に書くと画面が崩壊するため、ログファイルに出力する。
/// CLI モード（サブコマンド実行時）は従来どおり stderr。
fn init_tracing(is_tui: bool) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));

    if is_tui {
        let log_dir = dirs::cache_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join("remote-merge");
        let _ = std::fs::create_dir_all(&log_dir);
        let log_path = log_dir.join("debug.log");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .expect("Failed to open log file");

        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(Mutex::new(file))
            .with_ansi(false)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    }
}
