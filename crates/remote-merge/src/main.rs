use clap::{ArgAction, Parser, Subcommand};
use std::path::PathBuf;

use remote_merge::config;
use remote_merge::telemetry;

/// TUI tool for graphically displaying and merging file diffs between local and remote servers
#[derive(Parser, Debug)]
#[command(
    name = "remote-merge",
    version = remote_merge::agent::protocol::CLI_VERSION,
    about
)]
struct Cli {
    /// Path to project config file [overrides .remote-merge.toml in CWD]
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    /// Auto-accept prompts (e.g., unknown host key verification)
    #[arg(short = 'y', long = "yes", global = true)]
    yes: bool,

    /// Increase log verbosity (-v: info, -vv: debug, -vvv: trace)
    #[arg(short = 'v', long = "verbose", action = ArgAction::Count, global = true)]
    verbose: u8,

    /// Shorthand for --log-level debug
    #[arg(long, global = true)]
    debug: bool,

    /// Set log level explicitly (error, warn, info, debug, trace)
    #[arg(long, global = true)]
    log_level: Option<String>,

    // 必須にして、サブコマンドなしの起動は解析の段階で使い方を出して終わらせる（何も書き込まない）
    #[command(subcommand)]
    command: Commands,
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
        /// Output format (text, json)
        #[arg(long, default_value = "text")]
        format: String,
        /// Show only summary counts instead of file list
        #[arg(long)]
        summary: bool,
        /// Include equal files in output (default: omitted)
        #[arg(long)]
        all: bool,
        /// Force content comparison for all files (bypass mtime/size quick check)
        #[arg(long)]
        checksum: bool,
        /// Maximum number of entries to scan (1-1,000,000). Overrides config.
        #[arg(long, value_name = "N")]
        max_entries: Option<usize>,
    },

    /// Show diff for file(s) or directory
    Diff {
        /// File or directory paths to diff (use "." for all files)
        #[arg(num_args = 0..)]
        paths: Vec<String>,
        /// Left side of comparison [default: local]. When specified alone, --right falls back to the default server
        #[arg(long)]
        left: Option<String>,
        /// Right side of comparison [default: first server in config, alphabetical]
        #[arg(long)]
        right: Option<String>,
        /// Reference server for 3-way comparison (shows ref vs left diff and conflicts)
        #[arg(long, alias = "reference")]
        r#ref: Option<String>,
        /// Output format (text, json)
        #[arg(long, default_value = "text")]
        format: String,
        /// Maximum number of change lines (added/removed) per file diff. Context lines are not counted. Use 0 for unlimited.
        #[arg(long)]
        max_lines: Option<usize>,
        /// Maximum number of files to process (0 for unlimited)
        #[arg(long, default_value = "100")]
        max_files: usize,
        /// No effect; accepted for compatibility with existing scripts
        #[arg(long)]
        force: bool,
        /// Allow diff to read symlink targets outside the configured root
        #[arg(long)]
        follow_external_links: bool,
        /// Maximum number of entries to scan (1-1,000,000). Overrides config.
        #[arg(long, value_name = "N")]
        max_entries: Option<usize>,
    },

    /// Merge files
    Merge {
        /// File or directory paths to merge (use "." for all files)
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
        /// Preview merge without writing files
        #[arg(long)]
        dry_run: bool,
        /// Skip safety confirmations (remote-to-remote)
        #[arg(long)]
        force: bool,
        /// Delete files that exist only on target (rsync --delete equivalent)
        #[arg(long)]
        delete: bool,
        /// Copy source file permissions to destination
        #[arg(long)]
        with_permissions: bool,
        /// Compare file contents within directories even when metadata matches
        #[arg(long)]
        checksum: bool,
        /// Output format (text, json)
        #[arg(long, default_value = "text")]
        format: String,
        /// Maximum number of entries to scan (1-1,000,000). Overrides config.
        #[arg(long, value_name = "N")]
        max_entries: Option<usize>,
        /// Merge specific hunks by index (0-based, comma-separated). Requires exactly one path.
        #[arg(long, value_delimiter = ',', value_name = "INDICES")]
        hunks: Option<Vec<usize>>,
    },

    /// Sync files to multiple servers (1:N synchronization)
    Sync {
        /// File or directory paths to sync (use "." for all files)
        #[arg(required = true, num_args = 1..)]
        paths: Vec<String>,
        /// Source side
        #[arg(long)]
        left: Option<String>,
        /// Target servers (one or more, required)
        #[arg(long, num_args = 1..)]
        right: Vec<String>,
        /// Preview sync without writing files
        #[arg(long)]
        dry_run: bool,
        /// Skip safety confirmations (remote-to-remote)
        #[arg(long)]
        force: bool,
        /// Delete files that exist only on target (rsync --delete equivalent)
        #[arg(long)]
        delete: bool,
        /// Copy source file permissions to destination
        #[arg(long)]
        with_permissions: bool,
        /// Compare file contents within directories even when metadata matches
        #[arg(long)]
        checksum: bool,
        /// Output format (text, json)
        #[arg(long, default_value = "text")]
        format: String,
        /// Maximum number of entries to scan (1-1,000,000). Overrides config.
        #[arg(long, value_name = "N")]
        max_entries: Option<usize>,
    },

    /// Restore files from a backup session
    Rollback {
        /// Target side (server name or "local")
        #[arg(long)]
        target: Option<String>,

        /// List backup sessions instead of restoring
        #[arg(long)]
        list: bool,

        /// Specific session ID to restore (default: latest)
        #[arg(long)]
        session: Option<String>,

        /// Preview what would be restored without executing
        #[arg(long)]
        dry_run: bool,

        /// Skip confirmation prompt and allow restoring an expired session
        #[arg(long)]
        force: bool,

        /// Output format (text or json)
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

    /// Start agent server (used internally via SSH)
    Agent {
        /// Root directory for file operations
        #[arg(long)]
        root: PathBuf,
        /// Default UID for new files
        #[arg(long)]
        default_uid: Option<u32>,
        /// Default GID for new files
        #[arg(long)]
        default_gid: Option<u32>,
        /// Default permissions for new files (decimal)
        #[arg(long)]
        file_permissions: Option<u32>,
        /// Default permissions for new directories (decimal)
        #[arg(long)]
        dir_permissions: Option<u32>,
    },
}

fn main() {
    if let Err(e) = try_main() {
        eprintln!("Error: {e:#}");
        std::process::exit(remote_merge::service::types::exit_code::ERROR);
    }
}

/// format 引数を持つサブコマンドのエラーを適切にフォーマットする。
/// 成功時は exit code を返し、エラー時は JSON/text でエラー出力して exit code を返す。
fn handle_with_format(format: &str, result: anyhow::Result<i32>) -> i32 {
    match result {
        Ok(code) => code,
        Err(e) => {
            if format == "json" {
                let err = remote_merge::service::output::JsonError {
                    error: format!("{e:#}"),
                };
                let json = serde_json::to_string_pretty(&err)
                    .unwrap_or_else(|_| r#"{"error": "internal serialization error"}"#.to_string());
                println!("{}", json);
            } else {
                eprintln!("Error: {e:#}");
            }
            remote_merge::service::types::exit_code::ERROR
        }
    }
}

fn try_main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let log_level = resolve_log_level(cli.log_level.as_deref(), cli.debug, cli.verbose);
    let tracing_mode = if matches!(cli.command, Commands::Agent { .. }) {
        TracingMode::Agent
    } else {
        TracingMode::Cli
    };
    init_tracing(tracing_mode, log_level.as_deref());

    match cli.command {
        Commands::Init => {
            if cli.config.is_some() {
                eprintln!("Warning: --config is ignored for the 'init' subcommand");
            }
            remote_merge::init::run_init()?;
        }
        Commands::Status {
            left,
            right,
            r#ref,
            format,
            summary,
            all,
            checksum,
            max_entries,
        } => {
            let format_str = format.clone();
            let cfg =
                config::load_config_with_project_override(cli.config.as_deref()).map(|mut c| {
                    c.ssh.auto_yes = cli.yes;
                    c
                });
            let code = handle_with_format(
                &format_str,
                cfg.and_then(|cfg| {
                    remote_merge::cli::status::run_status(
                        remote_merge::cli::status::StatusArgs {
                            left,
                            right,
                            ref_server: r#ref,
                            format,
                            summary,
                            all,
                            checksum,
                            verbose: cli.verbose,
                            max_entries,
                        },
                        cfg,
                    )
                }),
            );
            std::process::exit(code);
        }
        Commands::Diff {
            paths,
            left,
            right,
            r#ref,
            format,
            max_lines,
            max_files,
            force,
            follow_external_links,
            max_entries,
        } => {
            let format_str = format.clone();
            let cfg =
                config::load_config_with_project_override(cli.config.as_deref()).map(|mut c| {
                    c.ssh.auto_yes = cli.yes;
                    c
                });
            let code = handle_with_format(
                &format_str,
                cfg.and_then(|cfg| {
                    remote_merge::cli::diff::run_diff(
                        remote_merge::cli::diff::DiffArgs {
                            paths,
                            left,
                            right,
                            ref_server: r#ref,
                            format,
                            max_lines,
                            max_files,
                            force,
                            follow_external_links,
                            max_entries,
                        },
                        cfg,
                    )
                }),
            );
            std::process::exit(code);
        }
        Commands::Merge {
            paths,
            left,
            right,
            r#ref,
            dry_run,
            force,
            delete,
            with_permissions,
            checksum,
            format,
            max_entries,
            hunks,
        } => {
            let format_str = format.clone();
            let cfg =
                config::load_config_with_project_override(cli.config.as_deref()).map(|mut c| {
                    c.ssh.auto_yes = cli.yes;
                    c
                });
            let code = handle_with_format(
                &format_str,
                cfg.and_then(|cfg| {
                    remote_merge::cli::merge::run_merge(
                        remote_merge::cli::merge::MergeArgs {
                            paths,
                            left,
                            right,
                            ref_server: r#ref,
                            dry_run,
                            force,
                            delete,
                            with_permissions,
                            checksum,
                            format,
                            max_entries,
                            hunks,
                        },
                        cfg,
                    )
                }),
            );
            std::process::exit(code);
        }
        Commands::Sync {
            paths,
            left,
            right,
            dry_run,
            force,
            delete,
            with_permissions,
            checksum,
            format,
            max_entries,
        } => {
            let format_str = format.clone();
            let cfg =
                config::load_config_with_project_override(cli.config.as_deref()).map(|mut c| {
                    c.ssh.auto_yes = cli.yes;
                    c
                });
            let code = handle_with_format(
                &format_str,
                cfg.and_then(|cfg| {
                    let args = remote_merge::cli::sync::SyncArgs {
                        paths,
                        left,
                        right,
                        dry_run,
                        force,
                        delete,
                        with_permissions,
                        checksum,
                        format,
                        max_entries,
                    };
                    remote_merge::cli::sync::run_sync(args, cfg)
                }),
            );
            std::process::exit(code);
        }
        Commands::Rollback {
            target,
            list,
            session,
            dry_run,
            force,
            format,
        } => {
            let format_str = format.clone();
            let cfg =
                config::load_config_with_project_override(cli.config.as_deref()).map(|mut c| {
                    c.ssh.auto_yes = cli.yes;
                    c
                });
            let code = handle_with_format(
                &format_str,
                cfg.and_then(|cfg| {
                    remote_merge::cli::rollback::run_rollback(
                        remote_merge::cli::rollback::RollbackArgs {
                            target,
                            list,
                            session,
                            dry_run,
                            force,
                            format,
                        },
                        cfg,
                    )
                }),
            );
            std::process::exit(code);
        }
        Commands::Logs {
            level,
            since,
            tail,
            format,
        } => {
            if cli.config.is_some() {
                eprintln!("Warning: --config is ignored for the 'logs' subcommand");
            }
            let format_str = format.clone();
            let code = handle_with_format(&format_str, {
                remote_merge::cli::logs::run_logs(remote_merge::cli::logs::LogsArgs {
                    level,
                    since,
                    tail,
                    format,
                })
            });
            std::process::exit(code);
        }
        Commands::Agent {
            root,
            default_uid,
            default_gid,
            file_permissions,
            dir_permissions,
        } => {
            #[cfg(unix)]
            {
                let metadata_config = remote_merge::agent::server::MetadataConfig {
                    default_uid,
                    default_gid,
                    file_permissions,
                    dir_permissions,
                };
                remote_merge::agent::server::run_agent_server(root, metadata_config)?;
            }
            #[cfg(not(unix))]
            {
                let _ = (
                    root,
                    default_uid,
                    default_gid,
                    file_permissions,
                    dir_permissions,
                );
                anyhow::bail!("The agent subcommand is only supported on Unix platforms");
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

/// tracing の出力先モード。
///
/// `Agent` / `Cli` は排他的 — 同時に複数のモードは成立しない。
enum TracingMode {
    /// Agent: stderr 専用、ANSI 無効（stdout はバイナリフレームプロトコルが占有）
    Agent,
    /// CLI サブコマンド: stderr にテキスト出力
    Cli,
}

/// tracing を初期化する。`cli_level` が Some の場合は環境変数より優先する。
fn init_tracing(mode: TracingMode, cli_level: Option<&str>) {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let env_filter = if let Some(level) = cli_level {
        tracing_subscriber::EnvFilter::new(level)
    } else {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"))
    };

    match mode {
        TracingMode::Agent => {
            // ANSI 無効（SSH ExtendedData 経由で転送されるため）。リモートで動くので診断ログは残さない
            tracing_subscriber::fmt()
                .with_writer(std::io::stderr)
                .with_ansi(false)
                .with_env_filter(env_filter)
                .init();
        }
        TracingMode::Cli => {
            use tracing_subscriber::Layer;

            let stderr_layer = tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(env_filter);
            // 標準エラーの既定（警告以上）とは別に、ファイルには指定がなくても情報レベル以上を残す
            let file_layer = telemetry::diagnostic_log::open_diagnostic_log(
                &telemetry::log_dir::default_log_dir(),
                telemetry::diagnostic_log::MAX_DIAGNOSTIC_LOG_BYTES,
            )
            .map(|file| {
                telemetry::JsonLogLayer::new(file).with_filter(tracing_subscriber::EnvFilter::new(
                    cli_level.unwrap_or("info"),
                ))
            });
            tracing_subscriber::registry()
                .with(stderr_layer)
                .with(file_layer)
                .init();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_content_comparison_can_be_requested_for_merge_and_sync() {
        let merge = Cli::try_parse_from([
            "remote-merge",
            "merge",
            "folder",
            "--left",
            "local",
            "--right",
            "develop",
            "--checksum",
        ])
        .unwrap();
        assert!(matches!(
            merge.command,
            Commands::Merge { checksum: true, .. }
        ));
        let sync = Cli::try_parse_from([
            "remote-merge",
            "sync",
            "folder",
            "--left",
            "local",
            "--right",
            "develop",
            "--checksum",
        ])
        .unwrap();
        assert!(matches!(
            sync.command,
            Commands::Sync { checksum: true, .. }
        ));
    }

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
        let cli = Cli::try_parse_from(["remote-merge", "-vvv", "status"]).unwrap();
        assert_eq!(cli.verbose, 3);
    }

    #[test]
    fn test_cli_parse_debug_flag() {
        let cli = Cli::try_parse_from(["remote-merge", "--debug", "status"]).unwrap();
        assert!(cli.debug);
    }

    #[test]
    fn test_cli_parse_log_level() {
        let cli = Cli::try_parse_from(["remote-merge", "--log-level", "trace", "status"]).unwrap();
        assert_eq!(cli.log_level, Some("trace".to_string()));
    }

    #[test]
    fn test_cli_parse_verbose_with_subcommand() {
        let cli = Cli::try_parse_from(["remote-merge", "-vv", "status"]).unwrap();
        assert_eq!(cli.verbose, 2);
        assert!(matches!(cli.command, Commands::Status { .. }));
    }

    #[test]
    fn test_cli_parse_debug_with_subcommand() {
        let cli = Cli::try_parse_from(["remote-merge", "--debug", "status"]).unwrap();
        assert!(cli.debug);
    }

    #[test]
    fn test_handle_with_format_ok_returns_code() {
        let code = handle_with_format("text", Ok(0));
        assert_eq!(code, 0);

        let code = handle_with_format("json", Ok(1));
        assert_eq!(code, 1);
    }

    #[test]
    fn test_handle_with_format_err_text_returns_error_code() {
        let code = handle_with_format("text", Err(anyhow::anyhow!("something failed")));
        assert_eq!(code, remote_merge::service::types::exit_code::ERROR);
    }

    #[test]
    fn test_handle_with_format_err_json_returns_error_code() {
        let code = handle_with_format("json", Err(anyhow::anyhow!("something failed")));
        assert_eq!(code, remote_merge::service::types::exit_code::ERROR);
    }

    // ── Agent サブコマンド引数パース ──

    #[test]
    fn test_agent_parse_all_metadata_args() {
        let cli = Cli::try_parse_from([
            "remote-merge",
            "agent",
            "--root",
            "/app",
            "--default-uid",
            "1000",
            "--default-gid",
            "1000",
            "--file-permissions",
            "436",
            "--dir-permissions",
            "509",
        ])
        .unwrap();

        match cli.command {
            Commands::Agent {
                root,
                default_uid,
                default_gid,
                file_permissions,
                dir_permissions,
            } => {
                assert_eq!(root, PathBuf::from("/app"));
                assert_eq!(default_uid, Some(1000));
                assert_eq!(default_gid, Some(1000));
                assert_eq!(file_permissions, Some(436));
                assert_eq!(dir_permissions, Some(509));
            }
            other => panic!("expected Agent command, got {other:?}"),
        }
    }

    #[test]
    fn test_agent_parse_no_optional_args() {
        let cli = Cli::try_parse_from(["remote-merge", "agent", "--root", "/app"]).unwrap();

        match cli.command {
            Commands::Agent {
                default_uid,
                default_gid,
                file_permissions,
                dir_permissions,
                ..
            } => {
                assert_eq!(default_uid, None);
                assert_eq!(default_gid, None);
                assert_eq!(file_permissions, None);
                assert_eq!(dir_permissions, None);
            }
            other => panic!("expected Agent command, got {other:?}"),
        }
    }

    #[test]
    fn test_agent_parse_partial_args() {
        let cli = Cli::try_parse_from([
            "remote-merge",
            "agent",
            "--root",
            "/app",
            "--default-uid",
            "500",
            "--file-permissions",
            "436",
        ])
        .unwrap();

        match cli.command {
            Commands::Agent {
                default_uid,
                default_gid,
                file_permissions,
                dir_permissions,
                ..
            } => {
                assert_eq!(default_uid, Some(500));
                assert_eq!(default_gid, None);
                assert_eq!(file_permissions, Some(436));
                assert_eq!(dir_permissions, None);
            }
            other => panic!("expected Agent command, got {other:?}"),
        }
    }

    // ── --yes フラグ ──

    #[test]
    fn test_cli_parse_yes_flag() {
        let cli = Cli::try_parse_from(["remote-merge", "--yes", "status"]).unwrap();
        assert!(cli.yes);
    }

    #[test]
    fn test_cli_parse_yes_short_flag() {
        let cli = Cli::try_parse_from(["remote-merge", "-y", "status"]).unwrap();
        assert!(cli.yes);
    }

    #[test]
    fn test_cli_parse_yes_flag_with_subcommand() {
        let cli = Cli::try_parse_from(["remote-merge", "-y", "status"]).unwrap();
        assert!(cli.yes);
        assert!(matches!(cli.command, Commands::Status { .. }));
    }

    #[test]
    fn test_cli_parse_yes_default_false() {
        let cli = Cli::try_parse_from(["remote-merge", "status"]).unwrap();
        assert!(!cli.yes);
    }
}
