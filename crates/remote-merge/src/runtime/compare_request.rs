//! 比べ方の求め（範囲・件数の上限・比べ方・中身で比べるパス・辿り方・中身の読み込みの上限）

use std::collections::{BTreeMap, BTreeSet};

use remote_merge_core::compare::{Follow, Method, Role};
use remote_merge_core::op_failure::SizeLimit;
use remote_merge_core::scan::outcome::{LinkPoint, ListedKind, Listing, ScanFailure, ScanScope};

/// 比べるパスの範囲。どの側にも同じに当てる
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompareScope {
    /// root_dir 全体
    Whole,
    /// 指定したディレクトリの下
    Below(Vec<String>),
    /// 名前で指定したパスそのもの
    Entries(Vec<String>),
}

/// root_dir の外へ解決される symlink を辿るか（docs/ir/cli/symlink-diff.md の REQ-cli-026）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalLinks {
    Follow,
    /// 外へ解決される symlink の辿った先と下を集めず、そのことを結果に残す
    Stop,
}

/// 比べる範囲にリンク先を辿れない symlink があるときの扱い（docs/ir/scan/directory-links.md の
/// REQ-scan-018）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnfollowableLinks {
    /// 一覧に残し、判定できないとする（読むだけのコマンド）
    Report,
    /// 書き込みを始める前に止める（merge・sync・rollback）
    Stop,
}

impl UnfollowableLinks {
    /// 止める扱いなら、一覧の最初のリンク先を辿れない symlink を走査の誤りにする。名前を読めない
    /// ものも同じに数える
    pub(super) fn check(self, listing: &Listing) -> anyhow::Result<()> {
        if self == Self::Report {
            return Ok(());
        }
        let listed = listing.entries.iter().filter_map(|entry| {
            matches!(
                entry.kind,
                ListedKind::Symlink {
                    points_to: LinkPoint::Unresolvable,
                    ..
                }
            )
            .then_some(&entry.path)
        });
        match listed.chain(&listing.unresolvable_names).min() {
            Some(path) => Err(ScanFailure::UnresolvableLink { path: path.clone() }.into()),
            None => Ok(()),
        }
    }
}

/// 指定したディレクトリの下を比べるときの、件数の上限の当て方
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryLimit {
    /// 指定した比較全体に一度だけ当てる（diff。docs/ir/cli/symlink-diff.md の REQ-cli-021）
    Whole,
    /// 指定したディレクトリごとの走査に当てる（merge・sync。docs/ir/scan/limits.md の TBL-scan-001）
    PerDirectory,
}

impl EntryLimit {
    /// 前の走査が `counted` 件を数えた後に、名前で指定した起点そのものを集める走査に残す件数。比較
    /// 全体に一度だけ当てるなら残りだけを、ディレクトリごとに当てるなら上限そのものを残す
    pub(super) fn left_after(self, max_entries: usize, counted: usize) -> usize {
        match self {
            Self::Whole => max_entries.saturating_sub(counted),
            Self::PerDirectory => max_entries,
        }
    }

    /// 走査の範囲の起点を、この当て方の組にする
    pub(super) fn grouped(self, mut scope: ScanScope) -> ScanScope {
        if let (Self::Whole, ScanScope::Starts(starts)) = (self, &mut scope) {
            for start in starts {
                start.group = 0;
            }
        }
        scope
    }
}

/// 比べるときに書き込み先（右）の中身のハッシュをまとめて取る処理が、つなぎ直しても `接続断` に
/// なったときの扱い（docs/ir/ssh/reconnect.md の REQ-ssh-003）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LostDestination {
    /// コマンドを止める（status・diff）
    StopCommand,
    /// そのまとめのパスを、右との組を判定できない読み込みの失敗にして続ける（merge・sync）
    FailPaths,
}

/// 比べ方の求め
#[derive(Debug, Clone)]
pub struct CompareRequest {
    pub scope: CompareScope,
    pub max_entries: usize,
    pub entry_limit: EntryLimit,
    /// 同じ比較の前の走査が、側ごとに件数の上限に当てて数えた数（`EntryLimit::Whole` で、root_dir
    /// 全体の diff に名前で比べ足すとき。docs/ir/cli/symlink-diff.md の REQ-cli-021）
    pub counted_before: BTreeMap<Role, usize>,
    pub method: Method,
    /// `method` によらず中身で比べるパス（merge・sync が名前で指定したファイル。docs/ir/compare/
    /// judgement.md の TBL-compare-003）
    pub content_paths: BTreeSet<String>,
    /// 二つの symlink を辿った先の組も判定するか（diff）
    pub follow: Follow,
    pub external_links: ExternalLinks,
    /// 中身をメモリに持つ読み込みの上限（REQ-cli-079。ハッシュには当てない）
    pub content_limit: SizeLimit,
    pub unfollowable_links: UnfollowableLinks,
    pub lost_destination: LostDestination,
}
