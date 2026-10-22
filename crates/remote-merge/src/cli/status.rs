//! status サブコマンドの実装。
//!
//! 左右（と参照先）を比べる流れ（docs/ir/compare/flow.md）で比べ、判定を engine の treat の表で
//! 一覧の値・参照先の印・errors に訳し、出力の型に詰めて書き出す。

use remote_merge_core::compare::{Command, Follow, Selection, choose_method};
use remote_merge_core::op_failure::SizeLimit;
use remote_merge_engine::treat::incomplete::incomplete_error;
use remote_merge_engine::treat::sides::{Reference, compared_sides, reference_side};
use remote_merge_engine::treat::status::{exit_code, shown, summary};
use remote_merge_engine::treat::status_listing::status_listing;
use remote_merge_engine::treat::uses::NothingUsed;

use crate::config::{AppConfig, resolve_max_entries};
use crate::runtime::{
    CompareRequest, CompareScope, CompareSides, CoreRuntime, EntryLimit, ExternalLinks,
    LostDestination, RuntimeTargets, UnfollowableLinks,
};
use crate::service::output::{OutputFormat, format_json, format_status_text};
use crate::service::types::StatusOutput;
use crate::side::Side;

/// status サブコマンドの引数
pub struct StatusArgs {
    pub left: Option<String>,
    pub right: Option<String>,
    pub ref_server: Option<String>,
    pub format: String,
    pub summary: bool,
    pub all: bool,
    /// 全ファイルのコンテンツ比較を強制する（メタデータベースの quick check をバイパス）
    pub checksum: bool,
    /// 冗長出力レベル（0=通常, 1+=agent 情報を含む）
    pub verbose: u8,
    /// スキャン最大エントリ数（1–1,000,000）。config の max_scan_entries を上書きする。
    pub max_entries: Option<usize>,
}

pub struct StatusCommandResult {
    pub output: StatusOutput,
    pub exit_code: i32,
    pub summary: bool,
}

/// status サブコマンドを実行する
pub fn run_status(args: StatusArgs, config: AppConfig) -> anyhow::Result<i32> {
    let format = OutputFormat::parse(&args.format)?;
    let result = execute_status(args, config, RuntimeTargets::production())?;
    print_status_result(&result, format)?;
    Ok(result.exit_code)
}

pub fn execute_status(
    args: StatusArgs,
    config: AppConfig,
    targets: RuntimeTargets,
) -> anyhow::Result<StatusCommandResult> {
    OutputFormat::parse(&args.format)?;
    let servers: Vec<String> = config.servers.keys().cloned().collect();
    let pair = compared_sides(args.left.as_deref(), args.right.as_deref(), &servers)
        .map_err(anyhow::Error::msg)?;
    let reference = match reference_side(args.ref_server.as_deref(), &pair, &servers)
        .map_err(anyhow::Error::msg)?
    {
        Reference::None => None,
        Reference::Use(side) => Some(side),
        Reference::Skipped(warning) => {
            eprintln!("{warning}");
            None
        }
    };
    let max_entries = resolve_max_entries(args.max_entries, &config)?;
    let mut core = CoreRuntime::with_targets(config, targets);
    let result = compare_and_list(&mut core, args, pair, reference, max_entries);
    core.disconnect_all();
    result
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "コマンドの入口は clap が渡す引数を値で受け取り、下の段に引き回す形に保つ"
)]
fn compare_and_list(
    core: &mut CoreRuntime,
    args: StatusArgs,
    (left, right): (Side, Side),
    reference: Option<Side>,
    max_entries: usize,
) -> anyhow::Result<StatusCommandResult> {
    for side in [Some(&left), Some(&right), reference.as_ref()]
        .into_iter()
        .flatten()
    {
        core.connect_side(side)?;
    }
    let sides = CompareSides {
        left,
        right,
        reference,
    };
    let request = CompareRequest {
        scope: CompareScope::Whole,
        max_entries,
        entry_limit: EntryLimit::Whole,
        counted_before: Default::default(),
        method: choose_method(Command::Status, args.checksum, Selection::BelowDirectory),
        named_files: None,
        follow: Follow::No,
        external_links: ExternalLinks::Follow,
        content_limit: SizeLimit::Unlimited,
        unfollowable_links: UnfollowableLinks::Report,
        lost_destination: LostDestination::StopCommand,
    };
    let comparison = core.compare(&sides, &request, &NothingUsed)?;
    if let Some(stopped) = core.root_gone_stop() {
        return Err(stopped);
    }
    if let Some(incomplete) = comparison.incomplete.first() {
        return Err(incomplete_error(incomplete));
    }
    let with_ref = sides.reference.is_some();
    let listing = status_listing(&comparison.paths, with_ref);
    let mut counts = summary(&listing.files, listing.errors.len());
    if let Some(reference) = listing.ref_counts {
        counts.ref_differs = Some(reference.differs);
        counts.ref_only = Some(reference.only);
        counts.ref_missing = Some(reference.missing);
    }
    let code = exit_code(&listing.files, listing.errors.len());
    let files = shown(listing.files, args.all);
    let output = StatusOutput {
        left: core.source_info(&sides.left),
        right: core.source_info(&sides.right),
        ref_: sides.reference.as_ref().map(|side| core.source_info(side)),
        agent: (args.verbose > 0)
            .then(|| determine_agent_status(&sides.right, core))
            .flatten(),
        files: (!args.summary).then_some(files),
        summary: counts,
        errors: listing.errors,
    };
    Ok(StatusCommandResult {
        output,
        exit_code: code,
        summary: args.summary,
    })
}

fn print_status_result(result: &StatusCommandResult, format: OutputFormat) -> anyhow::Result<()> {
    if format == OutputFormat::Text {
        for error in &result.output.errors {
            eprintln!("Error: {}: {}", error.path, error.reason);
        }
    }
    let text = match format {
        OutputFormat::Text => format_status_text(&result.output, result.summary),
        OutputFormat::Json => format_json(&result.output)?,
    };
    println!("{}", text);
    Ok(())
}

/// 右側ソースの Agent 接続状態。エージェントにつないだリモートのときだけある（ローカルと、ローカルに
/// 置き換えたサーバにはない）
fn determine_agent_status(
    right: &Side,
    core: &CoreRuntime,
) -> Option<crate::service::types::AgentStatus> {
    right
        .server_name()
        .filter(|name| core.has_agent(name))
        .map(|_| crate::service::types::AgentStatus::Connected)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_determine_agent_status_remote_without_an_agent_client() {
        let core = crate::runtime::CoreRuntime::new_for_test();
        let side = crate::side::Side::Remote("develop".to_string());
        let result = super::determine_agent_status(&side, &core);
        assert_eq!(result, None);
    }

    #[test]
    fn test_determine_agent_status_local() {
        let core = crate::runtime::CoreRuntime::new_for_test();
        let side = crate::side::Side::Local;
        let result = super::determine_agent_status(&side, &core);
        assert_eq!(result, None);
    }
}
