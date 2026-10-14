//! 参照先を使う merge の確認（docs/ir/cli/merge.md の TBL-cli-009・REQ-cli-051・REQ-cli-092）。
//! --ref があり参照先を実際に使い、--force のないファイル全体の merge で、書く扱いになったパスにだけ
//! 当てる（読み込み元と書き込み先の種類が違うパスは REQ-merge-001 のスキップによる）

use remote_merge_core::compare::{PathComparison, Role, SideState, Undecidable, Verdict};

use super::merge::Kind;

/// 書くか
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceCheck {
    Write,
    Fail(ReferenceFailure),
}

/// 書かずに failed に出す理由
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceFailure {
    /// 書き込み先だけが参照先から変わった
    DestinationChanged,
    /// 左右の両方が参照先から変わったか、参照先だけが左右と種類が違う
    Conflict,
    /// 左右にあり参照先にないので、書き込み先が変わっていないことを確かめられない
    NotInReference,
    /// 三つのどれかの読み込みに失敗した
    Incomplete,
    /// 読み込みの失敗でなく判定できない組がある（その他の種類・読めない名前。REQ-compare-009）。
    /// 値はその理由
    Undecidable(String),
}

impl ReferenceFailure {
    /// failed の error
    pub fn text(&self) -> &str {
        match self {
            Self::DestinationChanged => "destination changed since reference",
            Self::Conflict => "three-way conflict",
            Self::NotInReference => {
                "destination exists but is not in the reference, so it cannot be told unchanged"
            }
            Self::Incomplete => "three-way comparison incomplete",
            Self::Undecidable(why) => why,
        }
    }
}

/// 読み込み元（左）・書き込み先（右）・参照先の状態と判定から、書くかを決める（TBL-cli-009 を
/// 上の行から当てる）
pub fn reference_check(comparison: &PathComparison) -> ReferenceCheck {
    let state = |role: Role| comparison.states.states.get(&role);
    let present = |role: Role| state(role).is_some_and(|state| !matches!(state, SideState::Absent));
    let verdict =
        |first: Role, second: Role| comparison.pair(first, second).map(|pair| &pair.verdict);
    match (present(Role::Right), present(Role::Ref)) {
        (false, false) => return ReferenceCheck::Write,
        (false, true) => return ReferenceCheck::Fail(ReferenceFailure::DestinationChanged),
        (true, false) => return ReferenceCheck::Fail(ReferenceFailure::NotInReference),
        (true, true) => {}
    }
    let kind = |role: Role| state(role).map(Kind::of);
    let (source, destination, reference) = (kind(Role::Left), kind(Role::Right), kind(Role::Ref));
    let real =
        |kind: Option<Kind>| matches!(kind, Some(Kind::File | Kind::Directory | Kind::Symlink));
    if real(source) && destination == source && real(reference) && reference != source {
        return ReferenceCheck::Fail(ReferenceFailure::Conflict);
    }
    let verdicts = [
        verdict(Role::Left, Role::Right),
        verdict(Role::Left, Role::Ref),
        verdict(Role::Right, Role::Ref),
    ];
    for verdict in verdicts {
        match verdict {
            Some(Verdict::Undecidable(Undecidable::ReadFailed(_))) | None => {
                return ReferenceCheck::Fail(ReferenceFailure::Incomplete);
            }
            Some(Verdict::Undecidable(why)) => {
                return ReferenceCheck::Fail(ReferenceFailure::Undecidable(why.to_string()));
            }
            Some(_) => {}
        }
    }
    let changed = |role: Role| same_as_reference(comparison, role).map(|same| !same);
    match (changed(Role::Left), changed(Role::Right)) {
        (None, _) | (_, None) => ReferenceCheck::Fail(ReferenceFailure::Incomplete),
        (_, Some(false)) => ReferenceCheck::Write,
        (Some(false), Some(true)) => ReferenceCheck::Fail(ReferenceFailure::DestinationChanged),
        (Some(true), Some(true)) => ReferenceCheck::Fail(ReferenceFailure::Conflict),
    }
}

/// `role` の側が参照先と同じか。通常ファイルどうしは中身のハッシュで、symlink どうしはリンク文字列で
/// 比べる（比べ方によらない。REQ-cli-051・REQ-compare-008）。ハッシュを読めなかったときはない。
/// ハッシュを取っていない組は、その組の判定による
fn same_as_reference(comparison: &PathComparison, role: Role) -> Option<bool> {
    let states = &comparison.states.states;
    match (states.get(&role), states.get(&Role::Ref)) {
        (Some(SideState::File { .. }), Some(SideState::File { .. })) => {
            match (
                comparison.hashes.get(&role),
                comparison.hashes.get(&Role::Ref),
            ) {
                (Some(Ok(hash)), Some(Ok(reference))) => Some(hash == reference),
                (Some(Err(_)), _) | (_, Some(Err(_))) => None,
                (None, _) | (_, None) => Some(by_verdict(comparison, role)),
            }
        }
        (
            Some(SideState::Symlink { target, .. }),
            Some(SideState::Symlink {
                target: reference, ..
            }),
        ) => Some(target == reference),
        _ => Some(by_verdict(comparison, role)),
    }
}

fn by_verdict(comparison: &PathComparison, role: Role) -> bool {
    matches!(
        comparison.pair(role, Role::Ref).map(|pair| &pair.verdict),
        Some(Verdict::Same)
    )
}

#[cfg(test)]
#[path = "merge_reference_tests.rs"]
mod tests;
