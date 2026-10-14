//! rollback サブコマンド。セッションを選び（docs/ir/backup/rollback-cli.md）、戻す先の状態を集めて
//! 書き戻し・スキップ・失敗を決めて書き戻し（runtime の `restore_session`）、出力に詰める

use crate::config::AppConfig;
use crate::runtime::{CoreRuntime, RestoreOutcome, RuntimeTargets};
use crate::service::output::{
    OutputFormat, format_backup_list_text, format_json, format_rollback_dry_run_text,
    format_rollback_text,
};
use crate::service::types::{
    BackupListOutput, RollbackFailure, RollbackFileResult, RollbackOutput, RollbackSkipped,
    SourceInfo,
};
use crate::side::Side;
use remote_merge_engine::treat::rollback_session::{
    RollbackCounts, choose_session, rollback_exit_code, with_expiry,
};

/// rollback サブコマンドの引数
pub struct RollbackArgs {
    pub target: Option<String>,
    pub list: bool,
    pub session: Option<String>,
    pub dry_run: bool,
    pub force: bool,
    pub format: String,
}

pub enum RollbackCommandOutput {
    List(BackupListOutput),
    Restore(RollbackOutput),
    DryRun {
        output: RollbackOutput,
        warnings: Vec<String>,
    },
    Aborted,
}

pub struct RollbackCommandResult {
    pub output: RollbackCommandOutput,
    pub exit_code: i32,
}

/// --target を Side に解決する。
/// --list モードでは省略時にローカルをデフォルトとする。
fn resolve_target(target: Option<&str>, is_list: bool) -> anyhow::Result<Side> {
    match target {
        Some(name) => Ok(Side::new(name)),
        None if is_list => Ok(Side::Local),
        None => anyhow::bail!("--target is required (e.g. --target local or --target develop)"),
    }
}

/// rollback サブコマンドを実行する
pub fn run_rollback(args: RollbackArgs, config: AppConfig) -> anyhow::Result<i32> {
    let format = OutputFormat::parse(&args.format)?;
    let result = execute_rollback(args, config, RuntimeTargets::production())?;
    print_rollback_result(&result.output, format)?;
    Ok(result.exit_code)
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "コマンドの入口は clap が渡す引数を値で受け取り、下の段に引き回す形に保つ"
)]
pub fn execute_rollback(
    args: RollbackArgs,
    config: AppConfig,
    targets: RuntimeTargets,
) -> anyhow::Result<RollbackCommandResult> {
    OutputFormat::parse(&args.format)?;
    let side = resolve_target(args.target.as_deref(), args.list)?;
    let now = targets.now();
    let retention_days = config.backup.retention_days;
    let mut core = CoreRuntime::with_targets(config, targets);
    let target_info = core.source_info(&side);
    let sessions = with_expiry(core.list_backup_sessions(&side)?, retention_days, now);
    if args.list {
        return Ok(RollbackCommandResult {
            output: RollbackCommandOutput::List(BackupListOutput {
                target: target_info,
                sessions,
            }),
            exit_code: 0,
        });
    }
    let session = choose_session(&sessions, args.session.as_deref(), args.force)?;
    let paths: Vec<String> = session.files.iter().map(|file| file.path.clone()).collect();
    // 確認の問いの前につなぎ、エージェントを使えなければ問わずに止める（REQ-ssh-005）
    core.connect_side(&side)?;
    if !args.force && !args.dry_run {
        let prompt = format!(
            "Restore {} file(s) from session {} to {}? [y/N] ",
            paths.len(),
            session.session_id,
            target_info.label,
        );
        if !confirm_prompt(&prompt)? {
            core.disconnect_all();
            eprintln!("Aborted.");
            return Ok(RollbackCommandResult {
                output: RollbackCommandOutput::Aborted,
                exit_code: 0,
            });
        }
    }
    let outcomes =
        core.restore_session(&side, &session.session_id, &paths, args.dry_run, args.force);
    let stopped = core.has_stopped();
    core.disconnect_all();
    let output = rollback_output(target_info, session.session_id.clone(), outcomes?);
    let counts = RollbackCounts {
        restored: output.restored.len(),
        skipped: output.skipped.len(),
        failed: output.failed.len(),
    };
    let exit_code = rollback_exit_code(counts, args.dry_run, stopped);
    let output = if args.dry_run {
        let warnings = if session.expired {
            vec![format!("backup session {} has expired", session.session_id)]
        } else {
            Vec::new()
        };
        RollbackCommandOutput::DryRun { output, warnings }
    } else {
        RollbackCommandOutput::Restore(output)
    };
    Ok(RollbackCommandResult { output, exit_code })
}

/// パスの順の結果を出力の型に詰める
fn rollback_output(
    target: SourceInfo,
    session_id: String,
    outcomes: Vec<(String, RestoreOutcome)>,
) -> RollbackOutput {
    let mut output = RollbackOutput {
        target,
        session_id,
        restored: Vec::new(),
        skipped: Vec::new(),
        failed: Vec::new(),
    };
    for (path, outcome) in outcomes {
        match outcome {
            RestoreOutcome::Restored {
                pre_rollback_backup,
            } => output.restored.push(RollbackFileResult {
                path,
                pre_rollback_backup,
            }),
            RestoreOutcome::Skipped(reason) => {
                output.skipped.push(RollbackSkipped { path, reason })
            }
            RestoreOutcome::Failed(error) => output.failed.push(RollbackFailure { path, error }),
        }
    }
    output
}

fn print_rollback_result(
    output: &RollbackCommandOutput,
    format: OutputFormat,
) -> anyhow::Result<()> {
    match output {
        RollbackCommandOutput::List(output) => match format {
            OutputFormat::Text => println!("{}", format_backup_list_text(output)),
            OutputFormat::Json => println!("{}", format_json(output)?),
        },
        RollbackCommandOutput::Restore(output) => match format {
            OutputFormat::Text => println!("{}", format_rollback_text(output)),
            OutputFormat::Json => println!("{}", format_json(output)?),
        },
        RollbackCommandOutput::DryRun { output, warnings } => match format {
            OutputFormat::Text => {
                println!("{}", format_rollback_dry_run_text(output));
                for warning in warnings {
                    eprintln!("Warning: {}", warning);
                }
            }
            OutputFormat::Json => println!("{}", format_json(output)?),
        },
        RollbackCommandOutput::Aborted => {}
    }
    Ok(())
}

/// 確認プロンプトを表示し、ユーザーの応答を返す
fn confirm_prompt(message: &str) -> anyhow::Result<bool> {
    eprint!("{}", message);
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let trimmed = input.trim().to_lowercase();
    Ok(trimmed == "y" || trimmed == "yes")
}

#[cfg(test)]
#[path = "rollback_tests.rs"]
mod tests;
