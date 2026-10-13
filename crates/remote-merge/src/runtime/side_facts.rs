//! 側ごとの事実の集め方（走査の一覧・中身・中身のハッシュ）。比べる流れが状態とハッシュと中身を
//! 集めるのに使う（docs/ir/compare/flow.md の REQ-compare-001）。経路は選んだものだけを使い、
//! 読み取りの `接続断` は一度だけやり直す

use remote_merge_core::op_failure::{PathResult, SizeLimit};
use remote_merge_core::relative_path::EntryPath;
use remote_merge_core::scan::outcome::{Listing, ScanRequest};

use super::core::CoreRuntime;
use crate::side::Side;

impl CoreRuntime {
    /// `side` を `request` の範囲で走査した一覧。件数の上限での打ち切りと循環は一覧の印のまま返す
    pub fn scan_listing(&mut self, side: &Side, request: &ScanRequest) -> anyhow::Result<Listing> {
        let mut io = super::target_io::for_side(side, self);
        io.scan_listing(self, request)
    }

    /// パスごとの中身か失敗の種類（symlink は辿った先）。`limit` を超えるファイルは読まない
    pub fn read_contents(
        &mut self,
        side: &Side,
        paths: &[EntryPath],
        limit: SizeLimit,
    ) -> anyhow::Result<Vec<PathResult<Vec<u8>>>> {
        let mut io = super::target_io::for_side(side, self);
        io.read_entries(self, paths, limit)
    }

    /// パスごとの中身の SHA-256 か失敗の種類（symlink も辿った先の中身）。大きさの上限は当てない
    pub fn hash_contents(
        &mut self,
        side: &Side,
        paths: &[EntryPath],
    ) -> anyhow::Result<Vec<PathResult<String>>> {
        let mut io = super::target_io::for_side(side, self);
        io.hash_contents(self, paths)
    }
}
