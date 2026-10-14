//! sync サブコマンドの実装。
//!
//! 読み込み元（左）一つを、指定した順の書き込み先へ一つずつ合わせる（docs/ir/cli/sync.md）。書き込み先
//! ごとに比べる流れで比べ、merge と同じ treat の表と変更の手順を使う。書き込む前に、書き込む予定を
//! まとめて一度だけ尋ねる。

use std::io::BufRead;

use remote_merge_engine::treat::sides::sync_sides;
use remote_merge_engine::treat::sync::exit_code;

use super::sync_plan::{plan_targets, TargetPlan};
use super::sync_targets::sync_targets;
use crate::config::{resolve_max_entries, AppConfig};
use crate::runtime::{CoreRuntime, RuntimeTargets};
use crate::service::output::{format_json, format_sync_text, OutputFormat};
use crate::service::path_resolver::parse_path_arguments;
use crate::service::types::{
    exit_code as codes, SourceInfo, SyncOutput, SyncSummary, SyncTargetStatus,
};
use remote_merge_core::relative_path::RelPath;

/// sync サブコマンドの引数
pub struct SyncArgs {
    pub paths: Vec<String>,
    pub left: Option<String>,
    pub right: Vec<String>,
    pub dry_run: bool,
    pub force: bool,
    pub delete: bool,
    pub with_permissions: bool,
    pub checksum: bool,
    pub format: String,
    /// スキャン最大エントリ数（1–1,000,000）。config の max_scan_entries を上書きする。
    pub max_entries: Option<usize>,
}

pub enum SyncCommandOutput {
    Result(SyncOutput),
    Cancelled,
}

pub struct SyncCommandResult {
    pub output: SyncCommandOutput,
    pub exit_code: i32,
    no_files: bool,
}

/// sync 引数のバリデーション
fn validate_sync_args(args: &SyncArgs) -> anyhow::Result<Vec<RelPath>> {
    if args.left.is_none() {
        anyhow::bail!("--left is required for sync command");
    }
    if args.right.is_empty() {
        anyhow::bail!("--right requires at least one target server for sync command");
    }
    if args.paths.is_empty() {
        anyhow::bail!("at least one path is required for sync");
    }
    parse_path_arguments(&args.paths)
}

/// sync サブコマンドを実行する
pub fn run_sync(args: SyncArgs, config: AppConfig) -> anyhow::Result<i32> {
    let format = OutputFormat::parse(&args.format)?;
    let result = execute_sync(args, config, RuntimeTargets::production())?;
    print_sync_result(&result, format)?;
    Ok(result.exit_code)
}

pub fn execute_sync(
    args: SyncArgs,
    config: AppConfig,
    targets: RuntimeTargets,
) -> anyhow::Result<SyncCommandResult> {
    let paths = validate_sync_args(&args)?;
    OutputFormat::parse(&args.format)?;
    let max_entries = resolve_max_entries(args.max_entries, &config)?;
    let servers: Vec<String> = config.servers.keys().cloned().collect();
    let (source, destinations) =
        sync_sides(args.left.as_deref(), &args.right, &servers).map_err(anyhow::Error::msg)?;
    let mut core = CoreRuntime::with_targets(config, targets);
    let result = (|| {
        let plans = plan_targets(
            &mut core,
            &args,
            &source,
            &destinations,
            &paths,
            max_entries,
        )?;
        if !args.force && !args.dry_run && plans.iter().any(TargetPlan::changes_anything) {
            let left = core.source_info(&source);
            if !confirmed(&left, &plans)? {
                eprintln!("Sync cancelled.");
                return Ok(SyncCommandResult {
                    output: SyncCommandOutput::Cancelled,
                    exit_code: codes::SUCCESS,
                    no_files: false,
                });
            }
        }
        let no_files = plans.iter().all(TargetPlan::is_empty);
        let results = sync_targets(&mut core, &args, plans)?;
        let statuses: Vec<SyncTargetStatus> = results.iter().map(|target| target.status).collect();
        let summary = SyncSummary {
            total_servers: results.len(),
            successful_servers: statuses
                .iter()
                .filter(|status| **status == SyncTargetStatus::Success)
                .count(),
            total_files_merged: results.iter().map(|target| target.merged.len()).sum(),
            total_files_deleted: results.iter().map(|target| target.deleted.len()).sum(),
            total_files_failed: results.iter().map(|target| target.failed.len()).sum(),
        };
        Ok(SyncCommandResult {
            output: SyncCommandOutput::Result(SyncOutput {
                left: core.source_info(&source),
                targets: results,
                summary,
            }),
            exit_code: exit_code(&statuses),
            no_files,
        })
    })();
    core.disconnect_all();
    result
}

/// 書き込む予定を示し、一度だけ尋ねる（REQ-cli-040）
fn confirmed(left: &SourceInfo, plans: &[TargetPlan]) -> anyhow::Result<bool> {
    let labels: Vec<&str> = plans
        .iter()
        .map(|plan| plan.target.label.as_str())
        .collect();
    eprintln!("Sync: {} -> {}", left.label, labels.join(", "));
    for plan in plans {
        let (merges, deletes) = plan.change_counts();
        let mut parts = Vec::new();
        if merges > 0 {
            parts.push(format!("{merges} files to merge"));
        }
        if deletes > 0 {
            parts.push(format!("{deletes} files to delete"));
        }
        if !parts.is_empty() {
            eprintln!("  [{}] {}", plan.target.label, parts.join(", "));
        }
    }
    eprint!("Proceed? [y/N] ");
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y"))
}

fn print_sync_result(result: &SyncCommandResult, format: OutputFormat) -> anyhow::Result<()> {
    let SyncCommandOutput::Result(output) = &result.output else {
        return Ok(());
    };
    match format {
        OutputFormat::Text => {
            if result.no_files {
                println!("No files to sync.");
            }
            println!("{}", format_sync_text(output));
        }
        OutputFormat::Json => println!("{}", format_json(output)?),
    }
    Ok(())
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
