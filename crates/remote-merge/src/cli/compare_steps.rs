//! status・diff・merge が比べる前後に共有する段：使う参照先の決め方

use remote_merge_engine::treat::sides::Reference;

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
