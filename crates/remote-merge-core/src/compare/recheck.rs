//! 変更の直前に書き込み先をもう一度判定する規則（docs/ir/compare/reading.md の REQ-compare-015）

use super::contents::{read_outcome, ReadOutcome};
use super::verdict::{SideFailure, Undecidable};

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

/// 変更してよいか
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeCheck {
    /// 比べたときと同じ
    Unchanged,
    /// 種類・リンク文字列・ハッシュのどれかが違うか、比べたときになかったパスが現れた
    Changed,
    /// 状態かハッシュを集め直せなかった（REQ-compare-013）
    Undecidable(Undecidable),
    /// 集め直しが `接続断` になった。つなぎ直しの規則に任せる
    Disconnected(SideFailure),
}

/// 比べたときの状態 `compared` と、集め直した状態 `now` から、変えてよいかを決める
pub fn check_before_change(compared: &Observed, now: Result<Observed, SideFailure>) -> ChangeCheck {
    match now {
        Ok(now) if now == *compared => ChangeCheck::Unchanged,
        Ok(_) => ChangeCheck::Changed,
        Err(failed) => match read_outcome([failed]) {
            ReadOutcome::Disconnected(failed) => ChangeCheck::Disconnected(failed),
            ReadOutcome::Undecidable(why) => ChangeCheck::Undecidable(why),
            // 失敗を一つ渡したので、読めたことにはならない
            ReadOutcome::Read => ChangeCheck::Changed,
        },
    }
}
