//! 二つの側の状態の組の判定（docs/ir/compare/judgement.md の TBL-compare-001）。上の行から順に当てる

use super::method::Method;
use super::state::{Mtime, SideState};
use super::verdict::{Judgement, Present, Role, Undecidable, Verdict};
use crate::scan::outcome::LinkPoint;

/// 側 `first.0` の状態 `first.1` と、側 `second.0` の状態 `second.1` の組を判定する
pub fn judge_states(
    first: (Role, &SideState),
    second: (Role, &SideState),
    method: Method,
) -> Judgement {
    use SideState::*;
    let (first_role, first) = first;
    let (second_role, second) = second;
    let verdict = match (first, second) {
        (Absent, Absent) => Verdict::BothAbsent,
        (UnreadableName, _) | (_, UnreadableName) => {
            Verdict::Undecidable(Undecidable::UnreadableName)
        }
        (Other, _) | (_, Other) => {
            let others = [(first_role, first), (second_role, second)]
                .into_iter()
                .filter(|(_, state)| matches!(state, Other))
                .map(|(role, _)| role)
                .collect();
            Verdict::Undecidable(Undecidable::NotRegularFile(others))
        }
        _ if unfollowable(first) || unfollowable(second) => {
            let roles = [(first_role, first), (second_role, second)]
                .into_iter()
                .filter(|(_, state)| unfollowable(state))
                .map(|(role, _)| role)
                .collect();
            Verdict::Undecidable(Undecidable::UnfollowableLink(roles))
        }
        (_, Absent) => Verdict::OneSided(Present::First),
        (Absent, _) => Verdict::OneSided(Present::Second),
        (Symlink { target: a, .. }, Symlink { target: b, .. }) => same_or_differs(a == b),
        (Directory, Directory) => Verdict::Same,
        (
            File {
                size: first_size,
                mtime: first_time,
            },
            File {
                size: second_size,
                mtime: second_time,
            },
        ) => {
            return judge_files(
                (*first_size, *first_time),
                (*second_size, *second_time),
                method,
            )
        }
        // ここまで来た組は、両側に通常ファイル・ディレクトリ・末尾の symlink のどれかがあり、種類が違う
        (File { .. } | Directory | Symlink { .. }, File { .. } | Directory | Symlink { .. }) => {
            Verdict::KindDiffers
        }
    };
    Judgement::Decided(verdict)
}

/// リンク先を辿れない symlink か（docs/ir/scan/directory-links.md の REQ-scan-018）
fn unfollowable(state: &SideState) -> bool {
    matches!(
        state,
        SideState::Symlink {
            points_to: LinkPoint::Unresolvable,
            ..
        }
    )
}

fn same_or_differs(same: bool) -> Verdict {
    if same {
        Verdict::Same
    } else {
        Verdict::Differs
    }
}

type FileFacts = (Option<u64>, Option<Mtime>);

fn judge_files(first: FileFacts, second: FileFacts, method: Method) -> Judgement {
    if let (Some(a), Some(b)) = (first.0, second.0) {
        if a != b {
            return Judgement::Decided(Verdict::Differs);
        }
    }
    let same_size = matches!((first.0, second.0), (Some(a), Some(b)) if a == b);
    match (method, first.1, second.1) {
        (Method::Metadata, Some(a), Some(b)) if same_size && same_time(a, b) => {
            Judgement::Decided(Verdict::Same)
        }
        (Method::Metadata, _, _) | (Method::Content, _, _) => Judgement::ByContent,
    }
}

/// 両側がともに秒より細かい精度を持ち、共通する最も細かい精度で同じ時刻か
fn same_time(a: Mtime, b: Mtime) -> bool {
    if a.digits == 0 || b.digits == 0 {
        return false;
    }
    let digits = a.digits.min(b.digits).min(9);
    let unit = 10u32.pow(9 - u32::from(digits));
    a.secs == b.secs && a.nanos / unit == b.nanos / unit
}
