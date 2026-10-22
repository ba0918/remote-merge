//! status・diff・merge が比べる前後に共有する段：使う参照先の決め方と、比べる側への接続

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
