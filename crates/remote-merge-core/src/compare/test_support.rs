//! compare の単体テストが共有する状態の作り方と、左右の組の判定の呼び方
use super::*;
use crate::scan::outcome::LinkPoint;

/// どちらの比べ方でも同じになることを確かめるときの比べ方の並び
pub(super) const BOTH: [Method; 2] = [Method::Metadata, Method::Content];

/// 左右の組の判定
pub(super) fn judge(first: &SideState, second: &SideState, method: Method) -> Judgement {
    judge_states((Role::Left, first), (Role::Right, second), method)
}

pub(super) fn decided(verdict: Verdict) -> Judgement {
    Judgement::Decided(verdict)
}

/// 大きさと更新時刻（`digits` は秒の小数の桁数）の分かる通常ファイル
pub(super) fn file(size: u64, secs: i64, nanos: u32, digits: u8) -> SideState {
    SideState::File {
        size: Some(size),
        mtime: Some(Mtime {
            secs,
            nanos,
            digits,
        }),
    }
}

pub(super) fn link(target: &str, points_to: LinkPoint) -> SideState {
    SideState::Symlink {
        target: target.into(),
        points_to,
    }
}

/// 側ごとの状態だけを持つ一つのパス
pub(super) fn path(states: &[(Role, SideState)]) -> PathStates {
    PathStates {
        states: states.iter().cloned().collect(),
        ..PathStates::default()
    }
}
