//! 走査が件数の上限か循環で打ち切った一覧の誤り（docs/ir/scan/limits.md の REQ-scan-004・008、
//! directory-links.md の REQ-scan-002）。打ち切った一覧からは判定を出さず、書き込みも始めない

use remote_merge_core::compare::Incomplete;

use crate::service::types::DiffError;

/// 走査が件数の上限に当たって打ち切られたこと（REQ-scan-008）。文は上限の上げ方とパスの指定の
/// 仕方を案内する
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScanTruncated {
    pub max_entries: usize,
}

impl std::fmt::Display for ScanTruncated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Tree scan truncated at {} entries. Use --max-entries <value> to increase the limit, \
             set max_scan_entries in the config, or specify file paths instead of scanning \
             the whole tree.",
            self.max_entries
        )
    }
}

impl std::error::Error for ScanTruncated {}

/// 循環で止めた走査
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CycleFound {
    pub path: String,
}

impl std::fmt::Display for CycleFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Directory scan incomplete: symbolic link cycle at {}",
            self.path
        )
    }
}

impl std::error::Error for CycleFound {}

/// 打ち切った一覧の誤り。件数の上限は上げ方の案内付き（REQ-scan-008）
pub fn incomplete_error(incomplete: &Incomplete) -> anyhow::Error {
    match incomplete {
        Incomplete::Truncated { max_entries, .. } => ScanTruncated {
            max_entries: *max_entries,
        }
        .into(),
        Incomplete::Cycle { path, .. } => CycleFound { path: path.clone() }.into(),
    }
}

/// diff の errors に載せる、打ち切った一覧の誤り（REQ-cli-084・REQ-cli-021）。読めた差分は残すので、
/// 誤りは一覧ごとに載せる。件数の上限は root_dir からの比べ方全体のことなので、パスを "." とする。
/// 両側で同じ誤りは一度だけ載せる
pub fn incomplete_errors(incomplete: &[Incomplete]) -> Vec<DiffError> {
    let mut errors: Vec<DiffError> = Vec::new();
    for each in incomplete {
        let path = match each {
            Incomplete::Truncated { .. } => ".".to_string(),
            Incomplete::Cycle { path, .. } => path.clone(),
        };
        let reason = incomplete_error(each).to_string();
        if !errors
            .iter()
            .any(|error| error.path == path && error.reason == reason)
        {
            errors.push(DiffError { path, reason });
        }
    }
    errors
}

#[cfg(test)]
#[path = "incomplete_tests.rs"]
mod tests;
