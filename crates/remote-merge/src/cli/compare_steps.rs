//! status・diff・merge が比べる前後に共有する段：使う参照先の決め方、比べる側への接続、名前で
//! 比べ足した結果の合わせ方

use std::collections::BTreeMap;

use remote_merge_core::compare::{Incomplete, PathComparison};
use remote_merge_engine::treat::sides::Reference;

use crate::runtime::{CompareSides, CoreRuntime};
use crate::side::Side;

/// 使う参照先。左右と同じで使わないときは、その警告を標準エラーに出す
pub(super) fn used_reference(reference: Reference) -> Option<Side> {
    match reference {
        Reference::None => None,
        Reference::Use(side) => Some(side),
        Reference::Skipped(warning) => {
            eprintln!("{warning}");
            None
        }
    }
}

/// 左・右・参照先の順に、比べる側につなぐ
pub(super) fn connect_sides(core: &mut CoreRuntime, sides: &CompareSides) -> anyhow::Result<()> {
    for (_, side) in sides.each() {
        core.connect_side(side)?;
    }
    Ok(())
}

/// 名前で比べ足した結果を合わせる。先に比べたパスはそのまま残し、打ち切った一覧は同じものを重ねない
pub(super) fn absorb_comparison(
    paths: &mut BTreeMap<String, PathComparison>,
    incomplete: &mut Vec<Incomplete>,
    more_paths: BTreeMap<String, PathComparison>,
    more_incomplete: Vec<Incomplete>,
) {
    for (path, found) in more_paths {
        paths.entry(path).or_insert(found);
    }
    for each in more_incomplete {
        if !incomplete.contains(&each) {
            incomplete.push(each);
        }
    }
}
