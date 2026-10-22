//! treat の単体テストが使う、比べた結果の組み立て
use std::collections::BTreeMap;

use remote_merge_core::compare::Verdict;
use remote_merge_core::compare::{Pair, PairVerdict, PathComparison, PathStates, Role, SideState};
use remote_merge_core::content::{Content, Interpretation, interpret};
use remote_merge_core::scan::outcome::LinkPoint;

use super::content::Prepared;

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
