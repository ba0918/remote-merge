//! treat の単体テストが使う、比べた結果の組み立て
use std::collections::BTreeMap;

use remote_merge_core::compare::Verdict;
use remote_merge_core::compare::{Pair, PairVerdict, PathComparison, PathStates, Role, SideState};
use remote_merge_core::content::{Content, Interpretation, interpret};
use remote_merge_core::scan::outcome::LinkPoint;

use super::content::Prepared;
use super::diff_file::Sources;
use crate::service::types::SourceInfo;

pub fn file() -> SideState {
    SideState::File {
        size: Some(1),
        mtime: None,
    }
}

pub fn link(target: &str) -> SideState {
    SideState::Symlink {
        target: target.as_bytes().to_vec(),
        points_to: LinkPoint::NotDirectory,
    }
}

pub fn pair(first: Role, second: Role, verdict: Verdict) -> PairVerdict {
    PairVerdict {
        pair: Pair { first, second },
        verdict,
        followed: None,
    }
}

/// 状態と組の判定と、読んだ中身から作る比べた結果
pub fn compared_with(
    states: &[(Role, SideState)],
    pairs: Vec<PairVerdict>,
    contents: &[(Role, &[u8])],
) -> PathComparison {
    PathComparison {
        states: PathStates {
            states: states.iter().cloned().collect(),
            ..PathStates::default()
        },
        pairs,
        hashes: BTreeMap::new(),
        contents: contents
            .iter()
            .map(|(role, bytes)| (*role, Ok(bytes.to_vec())))
            .collect(),
    }
}

/// 内容の指定がないときの `bytes` の解釈（テキストか、バイナリか、デコードの誤りの文）
pub fn decoded(bytes: &[u8]) -> Result<Prepared<'_>, String> {
    match interpret(bytes, Interpretation::Unspecified) {
        Ok(Content::Text(text)) => Ok(Prepared::Text(text)),
        Ok(Content::Binary) => Ok(Prepared::Binary(bytes)),
        Err(error) => Err(error.to_string()),
    }
}

/// 左右の状態と、その組の判定から作る比べた結果
pub fn left_right(left: SideState, right: SideState, verdict: Verdict) -> PathComparison {
    compared_with(
        &[(Role::Left, left), (Role::Right, right)],
        vec![pair(Role::Left, Role::Right, verdict)],
        &[],
    )
}

/// 左右と参照先の状態と、左右・左と参照先・右と参照先の判定から作る比べた結果
pub fn three(states: [SideState; 3], verdicts: [Verdict; 3]) -> PathComparison {
    let [left, right, reference] = states;
    let [lr, lref, rref] = verdicts;
    compared_with(
        &[
            (Role::Left, left),
            (Role::Right, right),
            (Role::Ref, reference),
        ],
        vec![
            pair(Role::Left, Role::Right, lr),
            pair(Role::Left, Role::Ref, lref),
            pair(Role::Right, Role::Ref, rref),
        ],
        &[],
    )
}

/// パスごとの比べた結果
pub fn paths(entries: Vec<(&str, PathComparison)>) -> BTreeMap<String, PathComparison> {
    entries
        .into_iter()
        .map(|(path, comparison)| (path.to_string(), comparison))
        .collect()
}

pub fn source(label: &str) -> SourceInfo {
    SourceInfo {
        label: label.into(),
        root: format!("/{label}"),
    }
}

/// 項目に載せる左右（"left"・"right"）と、`with_ref` なら参照先（"ref"）
pub fn sources(with_ref: bool) -> Sources {
    Sources {
        left: source("left"),
        right: source("right"),
        reference: with_ref.then(|| source("ref")),
    }
}
