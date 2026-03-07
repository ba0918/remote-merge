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
use remote_merge::config;
use remote_merge::handler::{dialog_keys, diff_keys, tree_keys};
use remote_merge::runtime::bootstrap::{self, TuiBootstrapParams};
use remote_merge::runtime::TuiRuntime;
use remote_merge::runtime::{merge_scan, scanner};
use remote_merge::telemetry;
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
        Some(Commands::Status {
            server,
            left,
            right,
            format,
            summary,
        }) => {
            let code =
                remote_merge::cli::status::run_status(remote_merge::cli::status::StatusArgs {
                    server,
                    left,
                    right,
                    format,
                    summary,
                })?;
            std::process::exit(code);
        }
        Some(Commands::Diff {
            path,
            left,
            right,
            format,
            max_lines,
            max_files: _,
        }) => {
            let code = remote_merge::cli::diff::run_diff(remote_merge::cli::diff::DiffArgs {
                path,
                left,
                right,
                format,
                max_lines,
            })?;
            std::process::exit(code);
        }
        Some(Commands::Merge {
            path,
            left,
            right,
            dry_run,
            force,
        }) => {
            let code = remote_merge::cli::merge::run_merge(remote_merge::cli::merge::MergeArgs {
                path,
                left,
                right,
                dry_run,
                force,
            })?;
            std::process::exit(code);
        }
        None => {
            let config = config::load_config()?;
            let right_server = cli.server.or(cli.right).unwrap_or_else(|| {
                config
                    .servers
                    .keys()
                    .next()
                    .cloned()
                    .unwrap_or_else(|| "develop".to_string())
            });
            let params = TuiBootstrapParams {
                right_server,
                left_server: cli.left,
            };
            let (app_state, runtime) = bootstrap::bootstrap_tui(params, config)?;
            run_tui(app_state, runtime)?;
        }
    }

    Ok(())
}

/// TUI イベントループを実行する
fn run_tui(mut state: AppState, mut runtime: TuiRuntime) -> anyhow::Result<()> {
    // テレメトリ: ダンプディレクトリ準備 + 起動時トランケーション
    let dump_dir = telemetry::state_dumper::default_dump_dir();
    let _ = std::fs::create_dir_all(&dump_dir);
    let _ = telemetry::truncate_file_lines(&dump_dir.join("events.jsonl"), 10_000);
    let _ = telemetry::truncate::truncate_file_bytes(
        &dump_dir.join("debug.log"),
        10 * 1024 * 1024, // 10MB
    );

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_event_loop(&mut terminal, &mut state, &mut runtime, &dump_dir);

    runtime.disconnect_all();

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

/// 描画遅延の閾値（ミリ秒）。これを超えたフレームのみイベント記録する。
const RENDER_SLOW_THRESHOLD_MS: u64 = 100;

/// イベントループ本体
fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    dump_dir: &std::path::Path,
) -> anyhow::Result<()> {
    let state_path = dump_dir.join("state.json");
    let screen_path = dump_dir.join("screen.txt");
    let mut event_recorder = telemetry::EventRecorder::new(&dump_dir.join("events.jsonl"));
    let mut frame_count: u64 = 0;

    loop {
        // tokio Runtime を駆動して SSH keepalive 等の pending タスクを処理
        runtime.drive_runtime();

        scanner::poll_scan_result(state, runtime);
        merge_scan::poll_merge_scan_result(state, runtime);

        // 描画 + 描画時間計測
        let render_start = std::time::Instant::now();
        terminal.draw(|frame| {
            draw_ui(frame, state);
        })?;
        let render_duration = render_start.elapsed();
        frame_count += 1;

        // テレメトリ: 描画遅延イベント（閾値超えのみ）
        let render_ms = render_duration.as_millis() as u64;
        if render_ms > RENDER_SLOW_THRESHOLD_MS {
            event_recorder.record_render_slow(frame_count, render_ms);
        }

        // テレメトリ: 画面テキストをダンプ
        let screen_text = telemetry::state_dumper::buffer_to_text(terminal.current_buffer_mut());
        let _ = telemetry::state_dumper::dump_screen_to_file(&screen_text, &screen_path);

        // テレメトリ: AppState スナップショットをダンプ
        let _ = telemetry::state_dumper::dump_state_to_file(state, &state_path);

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

            // テレメトリ: キー入力イベント記録
            let key_str = format!("{:?}", key.code);
            let focus_str = format!("{:?}", state.focus);

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

            event_recorder.record_key_press(&key_str, &focus_str);

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
