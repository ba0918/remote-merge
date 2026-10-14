//! このマシン（ローカルの経路）の範囲の操作: パスの確認・走査・子の一覧。一つの項目への操作は
//! core の `machine_ops` にある

use std::path::Path;

use remote_merge_core::path_resolution::{self, ResolvedPath};
use remote_merge_core::relative_path::RelPath;
use remote_merge_core::scan::children::ChildListing;
use remote_merge_core::scan::exclude::Exclude;
use remote_merge_core::scan::outcome::{Listing, ScanError, ScanRequest, ScanScope};
use remote_merge_core::scan::this_machine::MachineCollector;

use crate::child_listing::into_child_listing;
use crate::scan_result::into_listing;

/// 走査が件数の上限に当たって打ち切られたこと（REQ-scan-008）。文は上限の上げ方とパスの指定の
/// 仕方を案内する
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanTruncated {
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

/// 絶対パスの root_dir の下の `rel_path` を、このマシンで "/" から辿って確かめる
pub fn inspect_local_path(root: &Path, rel_path: &RelPath) -> anyhow::Result<ResolvedPath> {
    if !root.is_absolute() {
        anyhow::bail!("local root_dir is not an absolute path: {}", root.display());
    }
    Ok(path_resolution::os::resolve_on_this_machine(
        root,
        rel_path.path_to_resolve(),
    )?)
}

/// root_dir の下をこのマシンで走査する。エージェントも同じ集め方と規則を使う
/// （docs/decision/records/2026-10-12-foundation-2.md の C6）
pub fn scan(root: &Path, exclude: &[String], request: &ScanRequest) -> Result<Listing, ScanError> {
    let mut collector = MachineCollector::new(root.to_path_buf());
    remote_merge_core::scan::drive::scan(&mut collector, &Exclude::new(exclude), request)
}

/// このマシンで走査して一覧にする。件数の上限での打ち切りと循環は一覧の印のまま返す
pub fn scan_listing(
    root: &Path,
    exclude: &[String],
    request: &ScanRequest,
) -> anyhow::Result<Listing> {
    into_listing(
        &root.display().to_string(),
        scan(root, exclude, request),
        || missing_root(root),
    )
}

/// ローカルの root_dir がないときの誤り
pub fn missing_root(root: &Path) -> anyhow::Error {
    crate::error::AppError::PathNotFound {
        path: root.to_path_buf(),
    }
    .into()
}

/// root_dir からの `path` の直下の子を、走査の集め方を深さ一つにしたもので集め、三つの結果を
/// 区別して返す（docs/decision/records/2026-10-12-foundation-3.md の D6）。子は走査と同じ件数の
/// 上限 `max_entries` で数え、超えたら [`ChildrenTruncated`](crate::child_listing::ChildrenTruncated)
/// の誤りにする（docs/ir/cli/symlink-diff.md の REQ-cli-021）
pub fn list_children(
    root: &Path,
    exclude: &[String],
    path: &RelPath,
    max_entries: usize,
) -> anyhow::Result<ChildListing> {
    let request = ScanRequest {
        scope: ScanScope::Children(path.as_str().to_string()),
        max_entries,
    };
    into_child_listing(
        &root.display().to_string(),
        path.as_str(),
        scan(root, exclude, &request),
        request.max_entries,
        || missing_root(root),
    )
}
