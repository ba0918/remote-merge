//! 二つの通常ファイルの比べ方（docs/ir/compare/judgement.md の TBL-compare-003）と、二つの symlink の
//! 組と辿った先の組の合わせ方（REQ-compare-008）

use super::verdict::Verdict;

/// 二つの通常ファイルの比べ方
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// 大きさと更新時刻で、読まずに決められるものは決める
    Metadata,
    /// 大きさが同じなら中身のハッシュで決める
    Content,
}

/// 比べる流れを使うコマンド
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Status,
    Diff,
    Merge,
    Sync,
}

/// 比べるファイルの指定の仕方
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    /// 名前で指定したファイル
    NamedFile,
    /// 指定したディレクトリの下か、root_dir 全体の下のファイル
    BelowDirectory,
}

/// コマンドと --checksum と指定の仕方から比べ方を選ぶ（TBL-compare-003）
pub fn choose_method(command: Command, checksum: bool, selection: Selection) -> Method {
    match (command, checksum, selection) {
        (Command::Diff, _, _) => Method::Content,
        (Command::Status | Command::Merge | Command::Sync, true, _) => Method::Content,
        (Command::Merge | Command::Sync, false, Selection::NamedFile) => Method::Content,
        (Command::Merge | Command::Sync, false, Selection::BelowDirectory) => Method::Metadata,
        (Command::Status, false, _) => Method::Metadata,
    }
}

/// diff で、二つの symlink のリンクの組の判定と、辿った先の組の判定を合わせる。どちらかが同じで
/// なければ差分あり（docs/ir/cli/symlink-diff.md の REQ-cli-020）。リンクの組が同じで、辿った先を
/// 判定できなければ判定できない
pub fn combine_link_pair(link: &Verdict, followed: &Verdict) -> Verdict {
    match (link, followed) {
        (Verdict::Same, Verdict::Same | Verdict::BothAbsent) => Verdict::Same,
        (Verdict::Same, Verdict::Differs | Verdict::OneSided(_) | Verdict::KindDiffers) => {
            Verdict::Differs
        }
        (Verdict::Same, Verdict::Undecidable(why)) => Verdict::Undecidable(why.clone()),
        // リンク文字列が違えば、辿った先によらず差分あり。ほかの値は二つの symlink の組でない
        (
            Verdict::Differs
            | Verdict::BothAbsent
            | Verdict::Undecidable(_)
            | Verdict::OneSided(_)
            | Verdict::KindDiffers,
            _,
        ) => link.clone(),
    }
}
