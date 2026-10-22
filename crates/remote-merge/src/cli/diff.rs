//! diff サブコマンドの実装。
//!
//! 左右（と参照先）を比べる流れ（docs/ir/compare/flow.md）で、パスの指定の仕方によらず中身で比べ、
//! 判定を engine の treat の表で files・errors・summary に訳し、出力の型に詰めて書き出す。

use remote_merge_core::compare::{Command, Follow, Incomplete, Role, Selection, choose_method};
use remote_merge_core::op_failure::SizeLimit;
use remote_merge_engine::treat::content::{Context, Overrides};
use remote_merge_engine::treat::diff::exit_code;
use remote_merge_engine::treat::diff_file::Sources;
use remote_merge_engine::treat::diff_listing::{Coverage, Limits, diff_listing_with_content};
use remote_merge_engine::treat::diff_names::{
    DiffScope, diff_scope, missing_names_of_complete, nothing_named_found,
};
use remote_merge_engine::treat::incomplete::incomplete_errors;
use remote_merge_engine::treat::sides::{compared_sides, reference_side};
use remote_merge_engine::treat::uses::DiffUses;

use super::compare_steps::{connect_sides, used_reference};
use super::diff_compare::compare_scope;
use crate::config::{AppConfig, resolve_max_entries};
use crate::runtime::{
    CompareRequest, CompareScope, CompareSides, CoreRuntime, EntryLimit, ExternalLinks,
    LostDestination, RuntimeTargets, UnfollowableLinks,
};
use crate::service::output::{OutputFormat, format_json, format_multi_diff_text};
use crate::service::path_resolver::{parse_path_arguments, paths_not_found};
use crate::service::types::MultiDiffOutput;

/// diff サブコマンドの引数
pub struct DiffArgs {
    pub encodings: Overrides,
    pub paths: Vec<String>,
    pub left: Option<String>,
    pub right: Option<String>,
    pub ref_server: Option<String>,
    pub format: String,
    pub max_lines: Option<usize>,
    pub max_files: usize,
    pub force: bool,
    pub follow_external_links: bool,
    /// スキャン最大エントリ数（1–1,000,000）。config の max_scan_entries を上書きする。
    pub max_entries: Option<usize>,
}

/// diff サブコマンドを実行する
pub fn run_diff(args: DiffArgs, config: AppConfig) -> anyhow::Result<i32> {
    let format = OutputFormat::parse(&args.format)?;
    let (multi_output, code) = execute_diff(args, config, RuntimeTargets::production())?;
    let text = match format {
        OutputFormat::Text => {
            for error in &multi_output.errors {
                eprintln!("Error: {}: {}", error.path, error.reason);
            }
            format_multi_diff_text(&multi_output)
        }
        OutputFormat::Json => format_json(&multi_output)?,
    };
    println!("{}", text);
    Ok(code)
}

pub fn execute_diff(
    args: DiffArgs,
    config: AppConfig,
    targets: RuntimeTargets,
) -> anyhow::Result<(MultiDiffOutput, i32)> {
    OutputFormat::parse(&args.format)?;
    let max_entries = resolve_max_entries(args.max_entries, &config)?;
    let scope = diff_scope(&parse_path_arguments(&args.paths)?);
    let servers: Vec<String> = config.servers.keys().cloned().collect();
    let (left, right) = compared_sides(args.left.as_deref(), args.right.as_deref(), &servers)
        .map_err(anyhow::Error::msg)?;
    let reference = used_reference(
        reference_side(
            args.ref_server.as_deref(),
            &(left.clone(), right.clone()),
            &servers,
        )
        .map_err(anyhow::Error::msg)?,
    );
    let sides = CompareSides {
        left,
        right,
        reference,
    };
    let mut core = CoreRuntime::with_targets(config, targets);
    let result = compare_and_show(&mut core, args, &sides, scope, max_entries);
    core.disconnect_all();
    result
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "コマンドの入口は clap が渡す引数を値で受け取り、下の段に引き回す形に保つ"
)]
fn compare_and_show(
    core: &mut CoreRuntime,
    args: DiffArgs,
    sides: &CompareSides,
    scope: DiffScope,
    max_entries: usize,
) -> anyhow::Result<(MultiDiffOutput, i32)> {
    connect_sides(core, sides)?;
    let request = CompareRequest {
        scope: CompareScope::Whole,
        max_entries,
        entry_limit: EntryLimit::Whole,
        counted_before: Default::default(),
        method: choose_method(Command::Diff, false, Selection::BelowDirectory),
        named_files: None,
        follow: Follow::Yes,
        external_links: if args.follow_external_links {
            ExternalLinks::Follow
        } else {
            ExternalLinks::Stop
        },
        content_limit: SizeLimit::from_force(args.force),
        unfollowable_links: UnfollowableLinks::Report,
        lost_destination: LostDestination::StopCommand,
    };
    let uses = DiffUses {
        reference: sides.reference.is_some(),
    };
    let whole = matches!(scope, DiffScope::Whole(_));
    let compared = compare_scope(core, sides, request, &uses, scope)?;
    if let Some(stopped) = core.root_gone_stop() {
        return Err(stopped);
    }
    let incomplete: Vec<Role> = compared
        .incomplete
        .iter()
        .map(|incomplete| match incomplete {
            Incomplete::Truncated { role, .. } | Incomplete::Cycle { role, .. } => *role,
        })
        .collect();
    let missing = missing_names_of_complete(&compared.named, &compared.paths, &incomplete);
    for name in &missing {
        eprintln!("Warning: '{name}' not found on either side");
    }
    if nothing_named_found(whole, &missing, &compared.named, compared.unlistable.len()) {
        return Err(paths_not_found());
    }
    let sources = Sources {
        left: core.source_info(&sides.left),
        right: core.source_info(&sides.right),
        reference: sides.reference.as_ref().map(|side| core.source_info(side)),
    };
    let limits = Limits {
        max_lines: args.max_lines,
        max_files: args.max_files,
    };
    let coverage = Coverage {
        named: &compared.named,
        incomplete: incomplete.iter().copied().collect(),
    };
    let context = Context {
        config: &core.config.content,
        sides: sides.by_role(),
        overrides: args.encodings,
    };
    let mut output =
        diff_listing_with_content(&compared.paths, &sources, limits, &coverage, &context);
    let mut errors = compared.unlistable.clone();
    errors.append(&mut output.errors);
    errors.extend(incomplete_errors(&compared.incomplete));
    output.errors = errors;
    let changed = output.summary.files_with_changes > 0 || output.truncated;
    let code = exit_code(changed, !output.errors.is_empty());
    Ok((output, code))
}
