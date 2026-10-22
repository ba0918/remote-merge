//! status の扱い（docs/ir/cli/status.md の TBL-cli-001、status-targets.md の TBL-cli-005、
//! status-output.md の TBL-cli-002、docs/ir/compare/reading.md の TBL-compare-005）

use remote_merge_core::compare::{PathComparison, Present, Role, SideState, Verdict};

use super::only_directories;
use crate::service::types::{FileStatus, FileStatusKind, StatusSummary, exit_code as codes};

/// 一つのパスの status の扱い
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusEntry {
    /// 一覧に載せる値
    Listed(FileStatusKind),
    /// 値を付けず、パスと理由を errors に出す（TBL-compare-005）
    Unreadable(String),
    /// 一覧に載せない（両側にないパスと、どの側でもディレクトリそのもの）
    NotListed,
}

/// 左右の組の判定を status の扱いに訳す（TBL-cli-001）
pub fn status_entry(comparison: &PathComparison) -> StatusEntry {
    let Some(left_right) = comparison.pair(Role::Left, Role::Right) else {
        return StatusEntry::NotListed;
    };
    if only_directories(comparison) {
        return StatusEntry::NotListed;
    }
    match &left_right.verdict {
        Verdict::OneSided(Present::First) => StatusEntry::Listed(FileStatusKind::LeftOnly),
        Verdict::OneSided(Present::Second) => StatusEntry::Listed(FileStatusKind::RightOnly),
        Verdict::KindDiffers | Verdict::Differs => StatusEntry::Listed(FileStatusKind::Modified),
        Verdict::Same => StatusEntry::Listed(FileStatusKind::Equal),
        Verdict::Undecidable(why) => StatusEntry::Unreadable(why.to_string()),
        Verdict::BothAbsent => StatusEntry::NotListed,
    }
}

/// 参照先との違いの印（TBL-cli-005）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefBadge {
    MissingInRef,
    Differs,
    AllEqual,
}

impl RefBadge {
    /// JSON の "ref_badge" の値
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MissingInRef => "missing_in_ref",
            Self::Differs => "differs",
            Self::AllEqual => "all_equal",
        }
    }
}

/// 一つのパスの参照先との違いの扱い
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefMark {
    Badge(RefBadge),
    /// 参照先との組を判定できない。印を出さず、パスと理由を errors に出す（REQ-cli-088）
    Unreadable(String),
}

/// 左右と参照先の判定から、参照先との違いの印を決める（TBL-cli-005 を上の行から当てる。参照先との組
/// だけが判定できないときは REQ-cli-088 が「三つのどれかが違う」より先に当たる）
pub fn ref_mark(comparison: &PathComparison) -> RefMark {
    let present = |role: Role| {
        comparison
            .states
            .states
            .get(&role)
            .is_some_and(|state| !matches!(state, SideState::Absent))
    };
    if !present(Role::Ref) {
        return RefMark::Badge(RefBadge::MissingInRef);
    }
    if !(present(Role::Left) && present(Role::Right)) {
        return RefMark::Badge(RefBadge::Differs);
    }
    let verdicts: Vec<&Verdict> = [
        (Role::Left, Role::Right),
        (Role::Left, Role::Ref),
        (Role::Right, Role::Ref),
    ]
    .iter()
    .filter_map(|(first, second)| comparison.pair(*first, *second))
    .map(|pair| &pair.verdict)
    .collect();
    if verdicts
        .iter()
        .any(|verdict| matches!(verdict, Verdict::KindDiffers))
    {
        return RefMark::Badge(RefBadge::Differs);
    }
    let with_ref = [Role::Left, Role::Right]
        .iter()
        .filter_map(|role| comparison.pair(*role, Role::Ref));
    for pair in with_ref {
        if let Verdict::Undecidable(why) = &pair.verdict {
            return RefMark::Unreadable(why.to_string());
        }
    }
    if verdicts
        .iter()
        .all(|verdict| matches!(verdict, Verdict::Same))
    {
        RefMark::Badge(RefBadge::AllEqual)
    } else {
        RefMark::Badge(RefBadge::Differs)
    }
}

/// 参照先との違いの集計（JSON の "ref_differs"・"ref_only"・"ref_missing"）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefCounts {
    pub differs: usize,
    /// 参照先にだけあり、一覧に載せなかったパスの数
    pub only: usize,
    pub missing: usize,
}

/// 一覧に載せたファイルの印と、参照先にだけあるパスの数から集計する
pub fn ref_counts(marks: &[RefMark], only_in_ref: usize) -> RefCounts {
    let count = |badge: RefBadge| {
        marks
            .iter()
            .filter(|mark| **mark == RefMark::Badge(badge))
            .count()
    };
    RefCounts {
        differs: count(RefBadge::Differs),
        only: only_in_ref,
        missing: count(RefBadge::MissingInRef),
    }
}

/// --all の有無によらずすべてのファイルを値ごとに数え、errors に出したファイルの数を添える
/// （docs/ir/cli/status-output.md の REQ-cli-029・REQ-cli-032）
pub fn summary(files: &[FileStatus], unreadable: usize) -> StatusSummary {
    let count = |kind: FileStatusKind| files.iter().filter(|file| file.status == kind).count();
    StatusSummary {
        modified: count(FileStatusKind::Modified),
        left_only: count(FileStatusKind::LeftOnly),
        right_only: count(FileStatusKind::RightOnly),
        equal: count(FileStatusKind::Equal),
        unreadable,
        ..StatusSummary::default()
    }
}

/// 一覧に出すファイル。--all がなければ equal を外す（REQ-cli-006・REQ-cli-007）
pub fn shown(files: Vec<FileStatus>, all: bool) -> Vec<FileStatus> {
    if all {
        return files;
    }
    files
        .into_iter()
        .filter(|file| file.status != FileStatusKind::Equal)
        .collect()
}

/// 終了コード（TBL-cli-002）。errors に出したファイルがあれば 2、なければ違いがあれば 1、なければ 0
pub fn exit_code(files: &[FileStatus], unreadable: usize) -> i32 {
    if unreadable > 0 {
        return codes::ERROR;
    }
    let changed = files.iter().any(|file| match file.status {
        FileStatusKind::Modified | FileStatusKind::LeftOnly | FileStatusKind::RightOnly => true,
        FileStatusKind::Equal => false,
    });
    if changed {
        codes::DIFF_FOUND
    } else {
        codes::SUCCESS
    }
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
