//! 一つの側のパスの状態（docs/ir/compare/flow.md の REQ-compare-001）。走査の一覧の項目から作る

use crate::scan::outcome::{LinkPoint, ListedEntry, ListedKind};

/// 更新時刻と、読めた桁（秒の小数の桁数。0 なら秒の精度しかない）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mtime {
    pub secs: i64,
    pub nanos: u32,
    pub digits: u8,
}

/// 一つの側のパスの状態
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SideState {
    /// その側にない
    Absent,
    /// 名前が UTF-8 として読めない（docs/ir/scan/listing.md の REQ-scan-012）
    UnreadableName,
    /// 通常ファイル・ディレクトリ・symlink のどれでもないもの（FIFO・ソケット・デバイス）
    Other,
    /// 通常ファイル。大きさと更新時刻は分からないこともある（辿った先の状態）
    File {
        size: Option<u64>,
        mtime: Option<Mtime>,
    },
    Directory,
    /// 末尾の symlink。`target` はリンク文字列のバイト列
    Symlink {
        target: Vec<u8>,
        points_to: LinkPoint,
    },
}

impl SideState {
    /// 走査の一覧に載った項目の状態
    pub fn of_entry(entry: &ListedEntry) -> Self {
        match &entry.kind {
            ListedKind::File => Self::File {
                size: Some(entry.size),
                mtime: Some(Mtime {
                    secs: entry.mtime_secs,
                    nanos: entry.mtime_nanos,
                    digits: entry.mtime_digits,
                }),
            },
            ListedKind::Directory => Self::Directory,
            ListedKind::Symlink { target, points_to } => Self::Symlink {
                target: target.clone(),
                points_to: *points_to,
            },
            ListedKind::Other => Self::Other,
        }
    }
}

/// symlink を辿った先の状態（docs/ir/compare/judgement.md の REQ-compare-008）。通常ファイルは、
/// 大きさと更新時刻の分からない通常ファイルとし、中身のハッシュで決める
pub fn followed_state(points_to: LinkPoint) -> SideState {
    match points_to {
        LinkPoint::Directory => SideState::Directory,
        LinkPoint::Missing => SideState::Absent,
        // 辿った先がその他の種類なら、読まずに判定できないとする（REQ-compare-009）
        LinkPoint::Other => SideState::Other,
        LinkPoint::NotDirectory => SideState::File {
            size: None,
            mtime: None,
        },
    }
}
