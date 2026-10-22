//! 変更の直前に書き込み先をもう一度判定する規則（docs/ir/compare/reading.md の REQ-compare-015）

use super::contents::{ReadOutcome, read_outcome};
use super::verdict::{SideFailure, Undecidable};
use crate::op_failure::OpFailure;
use crate::scan::outcome::ListedKind;

/// 書き込み先のパスの、比べたときと変更の直前の状態。通常ファイルはハッシュで表す
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observed {
    Absent,
    File {
        hash: String,
    },
    Directory,
    /// `target` はリンク文字列のバイト列
    Symlink {
        target: Vec<u8>,
    },
    Other,
}

/// 変更の直前にそのパスそのものを集め直した一覧から決めた状態
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Relisted {
    /// 一覧の項目の種類だけで決まった状態
    Decided(Observed),
    /// 通常ファイル。ハッシュを取って [`Observed::File`] にする
    File,
}

/// 集め直した一覧のそのパスの項目の種類 `kind`（載らなければ None）から決めた状態。一覧に載らない
/// パスはない
pub fn relisted(kind: Option<&ListedKind>) -> Relisted {
    match kind {
        None => Relisted::Decided(Observed::Absent),
        Some(ListedKind::Other) => Relisted::Decided(Observed::Other),
        Some(ListedKind::Directory) => Relisted::Decided(Observed::Directory),
        Some(ListedKind::Symlink { target, .. }) => Relisted::Decided(Observed::Symlink {
            target: target.clone(),
        }),
        Some(ListedKind::File) => Relisted::File,
    }
}

/// 変更してよいか
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeCheck {
    /// 比べたときと同じ
    Unchanged,
    /// 種類・リンク文字列・ハッシュのどれかが違うか、比べたときになかったパスが現れた
    Changed,
    /// 状態かハッシュを集め直せなかった（REQ-compare-013）
    Undecidable(Undecidable),
    /// 集め直しが判定せずに止める失敗（`接続断` か root_dir がない）になった。`接続断` はつなぎ直しの
    /// 規則に、root_dir がないはその側を止める規則に任せる
    Stopped(SideFailure),
}

/// 比べたときの状態 `compared` と、集め直した状態 `now` から、変えてよいかを決める
pub fn check_before_change(compared: &Observed, now: Result<Observed, SideFailure>) -> ChangeCheck {
    match now {
        Ok(now) if now == *compared => ChangeCheck::Unchanged,
        Ok(_) => ChangeCheck::Changed,
        // 集め直した状態ではあったファイルのハッシュが「ない」の失敗なら、比べた後に消えたもの
        Err(failed) if failed.failure == OpFailure::NotFound => ChangeCheck::Changed,
        Err(failed) => match read_outcome([failed]) {
            ReadOutcome::Stopped(failed) => ChangeCheck::Stopped(failed),
            ReadOutcome::Undecidable(why) => ChangeCheck::Undecidable(why),
            // 失敗を一つ渡したので、読めたことにはならない
            ReadOutcome::Read => ChangeCheck::Changed,
        },
    }
}
