//! 側ごとの走査の一覧。比べる流れが状態を集めるのに使う（docs/ir/compare/flow.md の REQ-compare-001）

use remote_merge_core::scan::outcome::{Listing, ScanRequest};

use super::core::CoreRuntime;
use crate::side::Side;

impl CoreRuntime {
    /// `side` を `request` の範囲で走査した一覧。件数の上限での打ち切りと循環は一覧の印のまま返す
    pub fn scan_listing(&mut self, side: &Side, request: &ScanRequest) -> anyhow::Result<Listing> {
        let mut io = super::target_io::for_side(side, self);
        io.scan_listing(self, request)
    }
}
