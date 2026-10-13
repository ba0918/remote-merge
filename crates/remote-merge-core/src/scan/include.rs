//! include の値を、走査する側でのパスの確認の結果から、走査に使うか飛ばすかに決める
//! （docs/ir/config/include.md の REQ-config-032〜034・036）。値の書き方の検査と正規化は設定の
//! 読み込みで済んでいる（REQ-config-024・035・037）。

use crate::path_resolution::{ResolveError, ResolvedPath};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueDecision {
    /// 走査の起点にする
    Scan,
    /// 走査する側にないので飛ばす
    Skip,
    /// symlink を辿ると root_dir の外へ解決されるので使わない
    Outside,
}

impl ValueDecision {
    /// 飛ばす値の警告。標準エラーに出す
    pub fn warning(self, value: &str) -> Option<String> {
        match self {
            Self::Scan => None,
            Self::Skip => Some(format!("Include path does not exist, skipping: {value}")),
            Self::Outside => Some(format!(
                "Include path resolves outside root_dir, skipping: {value}"
            )),
        }
    }
}

/// 値のパスの確認の結果 `resolved` と root_dir の実パス `real_root` から、値の使い方を決める。
/// 途中の祖先がディレクトリでない値は飛ばさずに止める
pub fn decide_value(
    resolved: &ResolvedPath,
    real_root: &str,
) -> Result<ValueDecision, ResolveError> {
    let real_path = match resolved {
        ResolvedPath::Missing { .. } => return Ok(ValueDecision::Skip),
        ResolvedPath::AncestorNotDirectory { .. } => return Err(ResolveError::NotADirectory),
        ResolvedPath::Exists { real_path, .. } | ResolvedPath::Symlink { real_path, .. } => {
            real_path
        }
    };
    Ok(if is_inside(real_path, real_root) {
        ValueDecision::Scan
    } else {
        ValueDecision::Outside
    })
}

/// 実パス `real_path` が root_dir の実パス `real_root` そのものか、その下にあるか
pub fn is_inside(real_path: &str, real_root: &str) -> bool {
    let root = real_root.trim_end_matches('/');
    real_path == real_root
        || real_path == root
        || real_path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
#[path = "include_tests.rs"]
mod tests;
