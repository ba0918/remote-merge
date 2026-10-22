//! merge と sync の扱い（docs/ir/merge/ の REQ-merge-001・002・003・008・018・021・023・024・035、
//! docs/ir/compare/reading.md の TBL-compare-005、docs/ir/cli/merge.md の REQ-cli-047）。読み込み元を
//! 左（`Role::Left`）、書き込み先を右（`Role::Right`）とする

use remote_merge_core::compare::{PathComparison, Present, Role, SideState, Verdict};
use remote_merge_core::relative_path::is_below;
use remote_merge_core::scan::outcome::LinkPoint;

use crate::service::types::exit_code as codes;

/// merge と sync の指定のうち、扱いを変えるもの
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MergeOptions {
    /// --delete
    pub delete: bool,
}

/// 一つのパスの扱い
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeEntry {
    /// 読み込み元の通常ファイルの中身を書き込み先に書く（ないなら作り、あるならその場で書き換える）
    WriteFile,
    /// 読み込み元の symlink を書き込み先に作る（ないなら作り、symlink 同士なら作り直す）
    WriteLink,
    /// 書き込み先にだけある通常ファイルを消す
    Delete,
    Skip(SkipReason),
    /// 書かず消さず、理由とともに failed に出す
    Fail(String),
    /// 名前で指定したパスが左右のどちらにもない（failed に出す。REQ-merge-035）。扱いの並びだけが持つ
    NotFound,
    /// 左右のどちらにもない（扱いの並びには載せず、名前で指定したパスは NotFound にする）
    Absent,
    /// 何もしない（同じ組・ディレクトリそのもの・ディレクトリとディレクトリへの symlink の組）
    Nothing,
}

/// 項目の種類（REQ-merge-001 の通常ファイル・ディレクトリ・末尾の symlink）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    Directory,
    Symlink,
    /// 種類の違いの組にならないもの（ない・その他の種類・読めない名前）
    Other,
}

impl Kind {
    pub fn of(state: &SideState) -> Self {
        match state {
            SideState::File { .. } => Self::File,
            SideState::Directory => Self::Directory,
            SideState::Symlink { .. } => Self::Symlink,
            SideState::Absent | SideState::Other | SideState::UnreadableName => Self::Other,
        }
    }
}

/// スキップの理由
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// 読み込み元と書き込み先の種類が違う（REQ-merge-001）
    KindMismatch { source: Kind, destination: Kind },
    /// --delete のない、書き込み先にだけあるファイル（REQ-merge-024）
    DestinationOnly,
    /// --delete でも消さない、書き込み先にだけある末尾の symlink（REQ-merge-002）
    LinkNotDeleted,
    /// --hunks で差分のないファイル（docs/ir/merge/hunks.md の REQ-merge-030）
    NoChanges,
}

impl SkipReason {
    /// skipped の reason
    pub fn text(&self) -> String {
        match self {
            // 種類の違いの文言は契約にせず、今までの文言のままにする（2026-10-04 の記録の A2）
            Self::KindMismatch { .. } => {
                "source and destination have different file types".to_string()
            }
            Self::DestinationOnly => "right-only file (use --delete to remove)".to_string(),
            Self::LinkNotDeleted => "destination is a symlink; deletion skipped".to_string(),
            Self::NoChanges => "no changes".to_string(),
        }
    }

    /// 終了コードと sync の書き込み先の状態で失敗と同じに数えるか（REQ-cli-047・TBL-cli-007）
    pub fn counts_as_failure(&self) -> bool {
        match self {
            Self::KindMismatch { .. } => true,
            Self::DestinationOnly | Self::LinkNotDeleted | Self::NoChanges => false,
        }
    }
}

/// 左右の組の判定を merge と sync の扱いに訳す
pub(crate) fn merge_entry(comparison: &PathComparison, options: MergeOptions) -> MergeEntry {
    let Some(pair) = comparison.pair(Role::Left, Role::Right) else {
        return MergeEntry::Nothing;
    };
    let state = |role: Role| comparison.states.states.get(&role);
    let (Some(source), Some(destination)) = (state(Role::Left), state(Role::Right)) else {
        return MergeEntry::Nothing;
    };
    match &pair.verdict {
        Verdict::BothAbsent => MergeEntry::Absent,
        Verdict::Undecidable(why) => MergeEntry::Fail(why.to_string()),
        Verdict::OneSided(Present::First) => written(source),
        Verdict::OneSided(Present::Second) => destination_only(destination, options),
        Verdict::KindDiffers if directory_and_link_to_directory(source, destination) => {
            MergeEntry::Nothing
        }
        Verdict::KindDiffers => MergeEntry::Skip(SkipReason::KindMismatch {
            source: Kind::of(source),
            destination: Kind::of(destination),
        }),
        Verdict::Differs => written(source),
        Verdict::Same => MergeEntry::Nothing,
    }
}

/// 読み込み元の項目を書く扱い。ディレクトリそのものは書かず、その下のパスを書く
fn written(source: &SideState) -> MergeEntry {
    match Kind::of(source) {
        Kind::File => MergeEntry::WriteFile,
        Kind::Symlink => MergeEntry::WriteLink,
        Kind::Directory | Kind::Other => MergeEntry::Nothing,
    }
}

fn destination_only(destination: &SideState, options: MergeOptions) -> MergeEntry {
    match (Kind::of(destination), options.delete) {
        (Kind::File, true) => MergeEntry::Delete,
        (Kind::Symlink, true) => MergeEntry::Skip(SkipReason::LinkNotDeleted),
        (Kind::File | Kind::Symlink, false) => MergeEntry::Skip(SkipReason::DestinationOnly),
        (Kind::Directory | Kind::Other, _) => MergeEntry::Nothing,
    }
}

/// ディレクトリと、ディレクトリへの symlink の組か（組そのものは扱わず、中を比べる）
fn directory_and_link_to_directory(source: &SideState, destination: &SideState) -> bool {
    let link_to_directory = |state: &SideState| {
        matches!(
            state,
            SideState::Symlink {
                points_to: LinkPoint::Directory,
                ..
            }
        )
    };
    matches!(
        (source, destination),
        (SideState::Directory, other) | (other, SideState::Directory) if link_to_directory(other)
    )
}

/// 種類の違いでスキップしたパスの配下を外す（配下は書かず、消さず、結果に出さない）。`entries` は
/// パスの順に並んでいる
pub(crate) fn without_skipped_subtrees(
    entries: Vec<(String, MergeEntry)>,
) -> Vec<(String, MergeEntry)> {
    let mut skipped: Vec<String> = Vec::new();
    let mut kept = Vec::new();
    for (path, entry) in entries {
        if skipped.iter().any(|root| is_below(&path, root)) {
            continue;
        }
        if matches!(entry, MergeEntry::Skip(SkipReason::KindMismatch { .. })) {
            skipped.push(path.clone());
        }
        kept.push((path, entry));
    }
    kept
}

/// merge の終了コード（REQ-cli-047）。失敗か種類の違いのスキップがあれば 2、なければ 0
pub fn exit_code(failed: bool, skipped: &[SkipReason]) -> i32 {
    if failed || skipped.iter().any(SkipReason::counts_as_failure) {
        codes::ERROR
    } else {
        codes::SUCCESS
    }
}

#[cfg(test)]
#[path = "merge_tests.rs"]
mod tests;
