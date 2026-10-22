//! 書き込み先をまたいで使い回す、一つの側について集めた事実（sync の読み込み元）。sync は書き込み先
//! ごとに比べるが、読み込み元の走査の一覧と、読んだハッシュと中身は一度だけ集めて、どの書き込み先
//! にも同じものを使う（docs/ir/compare/reading.md の REQ-compare-011）

use std::collections::BTreeMap;
use std::rc::Rc;

use remote_merge_core::op_failure::OpFailure;
use remote_merge_core::scan::outcome::Listing;

use crate::side::Side;

/// パスごとの答え
pub(super) type ByPath<T> = BTreeMap<String, Result<T, OpFailure>>;

/// 使い回す側について集めた事実
pub struct Reused {
    pub(super) side: Side,
    /// 走査の一覧と、外へのリンクを辿らないときの root_dir の外へ解決される symlink のパス。一覧は
    /// 書き込み先ごとの状態の表が借りるだけなので、写さずに分け合う
    pub(super) listing: Option<(Rc<Listing>, Option<Vec<String>>)>,
    pub(super) hashes: ByPath<String>,
    pub(super) contents: ByPath<Vec<u8>>,
}

impl Reused {
    /// `side` について集めたものを使い回す（まだ何も集めていない）
    pub fn new(side: Side) -> Self {
        Self {
            side,
            listing: None,
            hashes: BTreeMap::new(),
            contents: BTreeMap::new(),
        }
    }
}
