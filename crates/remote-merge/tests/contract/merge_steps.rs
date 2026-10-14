//! merge を、比べる段と書き込み先を変える段に分けて動かす道具。二つの段の間に書き込み先を変えて、
//! 変更の直前の判定（docs/ir/compare/reading.md の REQ-compare-015）を確かめるのに使う

use remote_merge::cli::merge::{
    MergeArgs, MergeCommandOutput, MergeSides, apply_merge, plan_merge,
};
use remote_merge::runtime::CoreRuntime;
use remote_merge::service::types::{MergeOutcome, MergeOutput};
use remote_merge::side::Side;
use remote_merge_core::relative_path::RelPath;

/// `core` で `args` の左右を比べ、`between` を動かしてから書き込み先を変えた結果
pub fn merge_with_a_change_between(
    core: &mut CoreRuntime,
    args: &MergeArgs,
    between: impl FnOnce(),
) -> MergeOutput {
    let sides = MergeSides {
        left: Side::new(args.left.as_deref().expect("--left")),
        right: Side::new(args.right.as_deref().expect("--right")),
        reference: args.ref_server.as_deref().map(Side::new),
    };
    let paths: Vec<RelPath> = args
        .paths
        .iter()
        .map(|path| RelPath::parse(path).unwrap())
        .collect();
    let max_entries = core.config.max_scan_entries;
    let planned = plan_merge(core, args, &sides, &paths, max_entries).unwrap();
    between();
    match apply_merge(core, args, &sides, planned).unwrap().output {
        MergeCommandOutput::Files(output) => output,
        MergeCommandOutput::Outcome(MergeOutcome::NoFilesToMerge) => MergeOutput {
            merged: Vec::new(),
            skipped: Vec::new(),
            deleted: Vec::new(),
            failed: Vec::new(),
            ref_: None,
        },
        MergeCommandOutput::Outcome(other) => panic!("unexpected outcome: {other:?}"),
    }
}
