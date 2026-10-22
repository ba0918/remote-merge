//! diff が比べる範囲を比べる流れで比べる。名前の下の一覧を取れなかったとき
//! （docs/ir/cli/directory-paths.md の REQ-cli-096）は、その名前を errors に出して除き、残りの名前で
//! 比べ直す。どの名前の性質でもない走査の失敗は、diff 全体の誤りにする。root_dir 全体と一緒に名前で
//! 指定したパスのうち、全体の走査に載らなかったもの（exclude に当たるもの）は、名前で比べ足す
//! （diff-strategy.md の REQ-cli-081）

use std::collections::BTreeMap;

use remote_merge_core::compare::{Incomplete, PathComparison};
use remote_merge_core::scan::outcome::{ScanFailure, ScanStopped};
use remote_merge_engine::treat::diff_names::{DiffScope, missing_names, unlistable_name};
use remote_merge_engine::treat::uses::DiffUses;

use super::compare_steps::absorb_comparison;
use crate::runtime::{CompareRequest, CompareScope, CompareSides, CoreRuntime};
use crate::service::types::DiffError;

/// 比べた結果と、一覧を取れなかった名前
#[derive(Default)]
pub(super) struct Compared {
    pub paths: BTreeMap<String, PathComparison>,
    pub incomplete: Vec<Incomplete>,
    pub unlistable: Vec<DiffError>,
    /// 一覧を取れた名前
    pub named: Vec<String>,
}

pub(super) fn compare_scope(
    core: &mut CoreRuntime,
    sides: &CompareSides,
    mut request: CompareRequest,
    uses: &DiffUses,
    scope: DiffScope,
) -> anyhow::Result<Compared> {
    let extra = match scope {
        DiffScope::Named(named) => return compare_named(core, sides, request, uses, named),
        DiffScope::Whole(extra) => extra,
    };
    request.scope = CompareScope::Whole;
    let comparison = core.compare(sides, &request, uses)?;
    let mut compared = Compared {
        paths: comparison.paths,
        incomplete: comparison.incomplete,
        unlistable: Vec::new(),
        named: extra.clone(),
    };
    let unseen = missing_names(&extra, &compared.paths);
    if unseen.is_empty() {
        return Ok(compared);
    }
    // 名前で比べ足す走査も同じ比較の走査で、件数の上限は全体の走査が数えた残りに当てる（REQ-cli-021）
    request.counted_before = comparison.counted;
    let more = compare_named(core, sides, request, uses, unseen)?;
    absorb_comparison(
        &mut compared.paths,
        &mut compared.incomplete,
        more.paths,
        more.incomplete,
    );
    compared
        .named
        .retain(|name| !more.unlistable.iter().any(|error| error.path == *name));
    compared.unlistable = more.unlistable;
    Ok(compared)
}

fn compare_named(
    core: &mut CoreRuntime,
    sides: &CompareSides,
    mut request: CompareRequest,
    uses: &DiffUses,
    mut named: Vec<String>,
) -> anyhow::Result<Compared> {
    let mut unlistable = Vec::new();
    while !named.is_empty() {
        request.scope = CompareScope::Below(named.clone());
        let error = match core.compare(sides, &request, uses) {
            Ok(comparison) => {
                return Ok(Compared {
                    paths: comparison.paths,
                    incomplete: comparison.incomplete,
                    unlistable,
                    named,
                });
            }
            Err(error) => error,
        };
        let Some((name, failure)) = scan_failure(&error)
            .and_then(|failure| Some((unlistable_name(&named, failure)?, failure)))
        else {
            return Err(error);
        };
        unlistable.push(DiffError {
            path: name.clone(),
            reason: failure.to_string(),
        });
        named.retain(|each| *each != name);
    }
    // どの名前も一覧を取れず、比べるものがない
    Ok(Compared {
        unlistable,
        ..Compared::default()
    })
}

/// 走査を止めた失敗（経路の文言で包んだものを含む）
fn scan_failure(error: &anyhow::Error) -> Option<&ScanFailure> {
    error.chain().find_map(|cause| {
        cause.downcast_ref::<ScanFailure>().or_else(|| {
            cause
                .downcast_ref::<ScanStopped>()
                .map(|stopped| &stopped.failure)
        })
    })
}
