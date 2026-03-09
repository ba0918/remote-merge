use clap::{ArgAction, Parser, Subcommand};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::prelude::*;
use std::io;
use std::path::PathBuf;

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
    /// Path to project config file [overrides .remote-merge.toml in CWD]
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    /// Left side of comparison [default: local]
    #[arg(long)]
    left: Option<String>,

    /// Right side of comparison [default: first server in config, alphabetical]
    #[arg(long)]
    right: Option<String>,

    /// Reference server for 3-way comparison (shows [ref≠] badges and ref vs left diff)
    #[arg(long, alias = "reference")]
    r#ref: Option<String>,

    /// Increase log verbosity (-v: info, -vv: debug, -vvv: trace)
    #[arg(short = 'v', long = "verbose", action = ArgAction::Count, global = true)]
    verbose: u8,

    /// Shorthand for --log-level debug
    #[arg(long, global = true)]
    debug: bool,

    /// Set log level explicitly (error, warn, info, debug, trace)
    #[arg(long, global = true)]
    log_level: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize project config file
    Init,

    /// List files with differences
    Status {
        /// Left side of comparison [default: local]. When specified alone, --right falls back to the default server
        #[arg(long)]
        left: Option<String>,
        /// Right side of comparison [default: first server in config, alphabetical]
        #[arg(long)]
        right: Option<String>,
        /// Reference server for 3-way comparison (shows [ref≠] badges and ref vs left diff)
        #[arg(long, alias = "reference")]
        r#ref: Option<String>,
        #[arg(long, default_value = "text")]
        format: String,
        #[arg(long)]
        summary: bool,
        /// Include equal files in output (default: omitted)
        #[arg(long)]
        all: bool,
    },

    /// Show diff for file(s) or directory
    Diff {
        #[arg(num_args = 0..)]
        paths: Vec<String>,
        /// Left side of comparison [default: local]. When specified alone, --right falls back to the default server
        #[arg(long)]
        left: Option<String>,
        /// Right side of comparison [default: first server in config, alphabetical]
        #[arg(long)]
        right: Option<String>,
        /// Reference server for 3-way comparison (shows [ref≠] badges and ref vs left diff)
        #[arg(long, alias = "reference")]
        r#ref: Option<String>,
        #[arg(long, default_value = "text")]
        format: String,
        #[arg(long)]
        max_lines: Option<usize>,
        /// Maximum number of files to process (0 for unlimited)
        #[arg(long, default_value = "100")]
        max_files: usize,
        /// Override safety guards (show sensitive file contents)
        #[arg(long)]
        force: bool,
    },

    /// Merge files
    Merge {
        #[arg(required = true, num_args = 1..)]
        paths: Vec<String>,
        /// Source side of merge (required)
        #[arg(long)]
        left: Option<String>,
        /// Target side of merge (required)
        #[arg(long)]
        right: Option<String>,
        /// Reference server for 3-way comparison (shows [ref≠] badges)
        #[arg(long, alias = "reference")]
        r#ref: Option<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        force: bool,
        /// Copy source file permissions to destination
        #[arg(long)]
        with_permissions: bool,
        /// Output format (text, json)
        #[arg(long, default_value = "text")]
        format: String,
    },

    /// Show debug logs
    Logs {
        /// Filter by log level (info, warn, error, debug, trace)
        #[arg(long)]
        level: Option<String>,
        /// Show logs since duration (e.g. 5m, 1h, 30s)
        #[arg(long)]
        since: Option<String>,
        /// Show last N lines
        #[arg(long)]
        tail: Option<usize>,
        /// Output format (text, json)
        #[arg(long, default_value = "text")]
        format: String,
    },

    /// Show TUI events
    Events {
        /// Filter by event type (key_press, error, render_slow, ssh_exec, state_change, dialog)
        #[arg(long, name = "type")]
        event_type: Option<String>,
        /// Show events since duration (e.g. 5m, 1h, 30s)
        #[arg(long)]
        since: Option<String>,
        /// Show last N events
        #[arg(long)]
        tail: Option<usize>,
    },
}

fn main() {
    if let Err(e) = try_main() {
        eprintln!("Error: {e:#}");
        std::process::exit(remote_merge::service::types::exit_code::ERROR);
    }
}

fn try_main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let log_level = resolve_log_level(cli.log_level.as_deref(), cli.debug, cli.verbose);
    init_tracing(cli.command.is_none(), log_level.as_deref());

    match cli.command {
        Some(Commands::Init) => {
            if cli.config.is_some() {
                eprintln!("Warning: --config is ignored for the 'init' subcommand");
            }
            remote_merge::init::run_init()?;
        }
        Some(Commands::Status {
            left,
            right,
            r#ref,
            format,
            summary,
            all,
        }) => {
            let cfg = config::load_config_with_project_override(cli.config.as_deref())?;
            let code = remote_merge::cli::status::run_status(
                remote_merge::cli::status::StatusArgs {
                    left,
                    right,
                    ref_server: r#ref,
                    format,
                    summary,
                    all,
                },
                cfg,
            )?;
            std::process::exit(code);
        }
        Some(Commands::Diff {
            paths,
            left,
            right,
            r#ref,
            format,
            max_lines,
            max_files,
            force,
        }) => {
            let cfg = config::load_config_with_project_override(cli.config.as_deref())?;
            let code = remote_merge::cli::diff::run_diff(
                remote_merge::cli::diff::DiffArgs {
                    paths,
                    left,
                    right,
                    ref_server: r#ref,
                    format,
                    max_lines,
                    max_files,
                    force,
                },
                cfg,
            )?;
            std::process::exit(code);
        }
        Some(Commands::Merge {
            paths,
            left,
            right,
            r#ref,
            dry_run,
            force,
            with_permissions,
            format,
        }) => {
            let cfg = config::load_config_with_project_override(cli.config.as_deref())?;
            let code = remote_merge::cli::merge::run_merge(
                remote_merge::cli::merge::MergeArgs {
                    paths,
                    left,
                    right,
                    ref_server: r#ref,
                    dry_run,
                    force,
                    with_permissions,
                    format,
                },
                cfg,
            )?;
            std::process::exit(code);
        }
        Some(Commands::Logs {
            level,
            since,
            tail,
            format,
        }) => {
            if cli.config.is_some() {
                eprintln!("Warning: --config is ignored for the 'logs' subcommand");
            }
            let code = remote_merge::cli::logs::run_logs(remote_merge::cli::logs::LogsArgs {
                level,
                since,
                tail,
                format,
            })?;
            std::process::exit(code);
        }
        Some(Commands::Events {
            event_type,
            since,
            tail,
        }) => {
            if cli.config.is_some() {
                eprintln!("Warning: --config is ignored for the 'events' subcommand");
            }
            let code =
                remote_merge::cli::events::run_events(remote_merge::cli::events::EventsArgs {
                    event_type,
                    since,
                    tail,
                })?;
            std::process::exit(code);
        }
        None => {
            let config = config::load_config_with_project_override(cli.config.as_deref())?;
            let right_server = cli.right.map(Ok).unwrap_or_else(|| {
                config.servers.keys().next().cloned().ok_or_else(|| {
                    anyhow::anyhow!(
                        "No server specified and no servers found in config. \
                             Use --right, or add servers to config."
                    )
                })
            })?;
            let params = TuiBootstrapParams {
                right_server,
                left_server: cli.left,
                ref_server: cli.r#ref,
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

/// CLIフラグからログレベルを解決する。
///
/// 優先順序: `--log-level` > `--debug` > `-v` > None（環境変数にフォールバック）
fn resolve_log_level(log_level: Option<&str>, debug: bool, verbose: u8) -> Option<String> {
    if let Some(level) = log_level {
        return Some(level.to_string());
    }
    if debug {
        return Some("debug".to_string());
    }
    match verbose {
        1 => Some("info".to_string()),
        2 => Some("debug".to_string()),
        v if v >= 3 => Some("trace".to_string()),
        _ => None, // 環境変数にフォールバック
    }
}

/// tracing を初期化する。
///
/// TUI モードでは debug.log に JSONL 形式で出力（JsonLogLayer）。
/// CLI モード（サブコマンド実行時）は従来どおり stderr にテキスト出力。
/// `cli_level` が Some の場合は環境変数より優先する。
fn init_tracing(is_tui: bool, cli_level: Option<&str>) {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let env_filter = if let Some(level) = cli_level {
        tracing_subscriber::EnvFilter::new(level)
    } else {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"))
    };

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

        tracing_subscriber::registry()
            .with(env_filter)
            .with(telemetry::JsonLogLayer::new(file))
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_log_level_none_returns_none() {
        assert_eq!(resolve_log_level(None, false, 0), None);
    }

    #[test]
    fn test_resolve_log_level_v_returns_info() {
        assert_eq!(resolve_log_level(None, false, 1), Some("info".to_string()));
    }

    #[test]
    fn test_resolve_log_level_vv_returns_debug() {
        assert_eq!(resolve_log_level(None, false, 2), Some("debug".to_string()));
    }

    #[test]
    fn test_resolve_log_level_vvv_returns_trace() {
        assert_eq!(resolve_log_level(None, false, 3), Some("trace".to_string()));
    }

    #[test]
    fn test_resolve_log_level_v_more_than_3_returns_trace() {
        assert_eq!(resolve_log_level(None, false, 5), Some("trace".to_string()));
    }

    #[test]
    fn test_resolve_log_level_debug_flag_returns_debug() {
        assert_eq!(resolve_log_level(None, true, 0), Some("debug".to_string()));
    }

    #[test]
    fn test_resolve_log_level_explicit_level_takes_priority() {
        // --log-level trace は --debug や -v より優先
        assert_eq!(
            resolve_log_level(Some("trace"), true, 3),
            Some("trace".to_string())
        );
    }

    #[test]
    fn test_resolve_log_level_explicit_overrides_verbose() {
        assert_eq!(
            resolve_log_level(Some("error"), false, 3),
            Some("error".to_string())
        );
    }

    #[test]
    fn test_resolve_log_level_debug_overrides_verbose() {
        // --debug は -v（info）より優先
        assert_eq!(resolve_log_level(None, true, 1), Some("debug".to_string()));
    }

    #[test]
    fn test_cli_parse_verbose_count() {
        // clap の ArgAction::Count が正しく動くか確認
        let cli = Cli::try_parse_from(["remote-merge", "-vvv"]).unwrap();
        assert_eq!(cli.verbose, 3);
    }

    #[test]
    fn test_cli_parse_debug_flag() {
        let cli = Cli::try_parse_from(["remote-merge", "--debug"]).unwrap();
        assert!(cli.debug);
    }

    #[test]
    fn test_cli_parse_log_level() {
        let cli = Cli::try_parse_from(["remote-merge", "--log-level", "trace"]).unwrap();
        assert_eq!(cli.log_level, Some("trace".to_string()));
    }

    #[test]
    fn test_cli_parse_verbose_with_subcommand() {
        let cli = Cli::try_parse_from(["remote-merge", "-vv", "status"]).unwrap();
        assert_eq!(cli.verbose, 2);
        assert!(cli.command.is_some());
    }

    #[test]
    fn test_cli_parse_debug_with_subcommand() {
        let cli = Cli::try_parse_from(["remote-merge", "--debug", "status"]).unwrap();
        assert!(cli.debug);
    }
}
