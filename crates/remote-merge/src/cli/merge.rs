//! merge サブコマンドの実装。
//!
//! 読み込み元（左）と書き込み先（右）と参照先を比べる流れ（docs/ir/compare/flow.md）で比べ、判定を
//! engine の treat の表で書き込み・スキップ・failed に訳し、書き込み先を変更の直前にもう一度判定して
//! から変える。ファイル全体の merge は merge_files、--hunks は merge_hunks が行う。

use remote_merge_engine::treat::sides::{Reference, merge_sides, reference_side};

use crate::config::{AppConfig, resolve_max_entries};
use crate::runtime::{CompareSides, CoreRuntime, RuntimeTargets};
use crate::service::output::{
    OutputFormat, format_json, format_merge_outcome_json, format_merge_outcome_text,
    format_merge_text,
};
use crate::service::path_resolver::parse_path_arguments;
use crate::service::types::{MergeOutcome, MergeOutput, exit_code};
use crate::side::Side;
use remote_merge_core::relative_path::RelPath;

/// merge サブコマンドの引数
pub struct MergeArgs {
    pub encodings: remote_merge_engine::treat::content::Overrides,
    pub paths: Vec<String>,
    pub left: Option<String>,
    pub right: Option<String>,
    pub ref_server: Option<String>,
    pub dry_run: bool,
    pub force: bool,
    pub delete: bool,
    pub with_permissions: bool,
    pub checksum: bool,
    pub format: String,
    /// スキャン最大エントリ数（1–1,000,000）。config の max_scan_entries を上書きする。
    pub max_entries: Option<usize>,
    /// hunk 単位マージ用: 適用する hunk インデックス（0-based）
    pub hunks: Option<Vec<usize>>,
}

pub enum MergeCommandOutput {
    Outcome(MergeOutcome),
    Files(MergeOutput),
}

pub struct MergeCommandResult {
    pub output: MergeCommandOutput,
    pub exit_code: i32,
}

/// merge 引数のバリデーション: --left と --right の両方が必須、paths は1つ以上必須
fn validate_merge_args(args: &MergeArgs) -> anyhow::Result<Vec<RelPath>> {
    if args.paths.is_empty() {
        anyhow::bail!("at least one path is required for merge");
    }
    if args.left.is_none() || args.right.is_none() {
        anyhow::bail!(
            "--left and --right are required for merge command (e.g. --left local --right staging)"
        );
    }
    let paths = parse_path_arguments(&args.paths)?;
    // --hunks 固有のバリデーション
    if let Some(ref hunks) = args.hunks {
        if args.paths.len() != 1 {
            anyhow::bail!(
                "--hunks requires exactly one path (got {})",
                args.paths.len()
            );
        }
        if args.delete {
            anyhow::bail!("--hunks and --delete cannot be used together");
        }
        if hunks.is_empty() {
            anyhow::bail!("--hunks requires at least one hunk index");
        }
    }
    Ok(paths)
}

/// merge サブコマンドを実行する
pub fn run_merge(args: MergeArgs, config: AppConfig) -> anyhow::Result<i32> {
    let format = OutputFormat::parse(&args.format)?;
    let result = execute_merge(args, config, RuntimeTargets::production())?;
    print_merge_result(&result.output, format)?;
    Ok(result.exit_code)
}

pub use super::merge_files::{PlannedMerge, apply_merge, plan_merge};

#[expect(
    clippy::needless_pass_by_value,
    reason = "コマンドの入口は clap が渡す引数を値で受け取り、下の段に引き回す形に保つ"
)]
pub fn execute_merge(
    args: MergeArgs,
    config: AppConfig,
    targets: RuntimeTargets,
) -> anyhow::Result<MergeCommandResult> {
    let rel_paths = validate_merge_args(&args)?;
    // フォーマットを先にパースして不正値を早期エラーにする
    OutputFormat::parse(&args.format)?;
    let max_entries = resolve_max_entries(args.max_entries, &config)?;
    let servers: Vec<String> = config.servers.keys().cloned().collect();
    let (left, right) = merge_sides(args.left.as_deref(), args.right.as_deref(), &servers)
        .map_err(anyhow::Error::msg)?;
    let reference = match reference_side(
        args.ref_server.as_deref(),
        &(left.clone(), right.clone()),
        &servers,
    )
    .map_err(anyhow::Error::msg)?
    {
        Reference::None => None,
        Reference::Use(side) => Some(side),
        Reference::Skipped(warning) => {
            eprintln!("{warning}");
            None
        }
    };
    // 強制指定のないリモート間の merge は、つなぐ前に止める（REQ-cli-073）
    let remote = |side: &Side| matches!(side, Side::Remote(_));
    if remote(&left) && remote(&right) && !args.force && !args.dry_run {
        let blocked = MergeOutcome::R2rBlocked {
            left: left.display_name().to_string(),
            right: right.display_name().to_string(),
        };
        return Ok(MergeCommandResult {
            output: MergeCommandOutput::Outcome(blocked),
            exit_code: exit_code::ERROR,
        });
    }
    let sides = CompareSides {
        left,
        right,
        reference,
    };
    let mut core = CoreRuntime::with_targets(config, targets);
    let result = match &args.hunks {
        Some(hunks) => super::merge_hunks::merge_hunks(
            &mut core,
            &args,
            &sides,
            &rel_paths,
            hunks,
            max_entries,
        ),
        None => super::merge_files::merge_files(&mut core, &args, &sides, &rel_paths, max_entries),
    };
    core.disconnect_all();
    result
}

fn print_merge_result(output: &MergeCommandOutput, format: OutputFormat) -> anyhow::Result<()> {
    let rendered = match output {
        MergeCommandOutput::Outcome(outcome) => match format {
            OutputFormat::Text => format_merge_outcome_text(outcome),
            OutputFormat::Json => format_merge_outcome_json(outcome)?,
        },
        MergeCommandOutput::Files(output) => match format {
            OutputFormat::Text => format_merge_text(output),
            OutputFormat::Json => format_json(output)?,
        },
    };
    println!("{}", rendered);
    Ok(())
}

#[cfg(test)]
#[path = "merge_tests.rs"]
mod tests;
