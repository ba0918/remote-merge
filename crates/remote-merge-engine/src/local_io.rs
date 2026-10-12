//! このマシン（ローカルの経路）の範囲の操作: パスの確認・走査・子の一覧。一つの項目への操作は
//! core の `machine_ops` にある

use std::path::Path;

use remote_merge_core::path_resolution::{self, ResolvedPath};
use remote_merge_core::relative_path::RelPath;
use remote_merge_core::scan::exclude::Exclude;
use remote_merge_core::scan::outcome::{Listing, ScanError, ScanRequest};
use remote_merge_core::scan::this_machine::MachineCollector;

use crate::scan_result::into_tree;
use crate::tree::{FileNode, FileTree};

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

/// 件数の上限で打ち切った走査の扱い。`fail_on_truncation` なら [`ScanTruncated`] の誤りにし、
/// そうでなければ警告を記録して続ける
pub fn check_truncation(max_entries: usize, fail_on_truncation: bool) -> anyhow::Result<()> {
    if fail_on_truncation {
        return Err(ScanTruncated { max_entries }.into());
    }
    tracing::warn!("Tree scan truncated at {max_entries} entries; continuing with a partial tree");
    Ok(())
}

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

/// root_dir の直下だけのツリー（下のディレクトリは読まない）
pub fn fetch_tree(root: &Path, exclude: &[String]) -> anyhow::Result<FileTree> {
    let mut tree = FileTree::new(root);
    tree.nodes = fetch_children(root, exclude, &[], &RelPath::root())?;
    tree.sort();
    Ok(tree)
}

/// root_dir の下をこのマシンで走査する。エージェントも同じ集め方と規則を使う
/// （docs/decision/records/2026-10-12-foundation-2.md の C6）
pub fn scan(root: &Path, exclude: &[String], request: &ScanRequest) -> Result<Listing, ScanError> {
    let mut collector = MachineCollector::new(root.to_path_buf());
    remote_merge_core::scan::drive::scan(&mut collector, &Exclude::new(exclude), request)
}

/// このマシンで走査してツリーにする。件数の上限で打ち切った一覧は `fail` なら誤りにする
pub fn scan_tree(
    root: &Path,
    exclude: &[String],
    request: &ScanRequest,
    fail: bool,
) -> anyhow::Result<FileTree> {
    into_tree(
        root,
        &root.display().to_string(),
        scan(root, exclude, request),
        request.max_entries,
        fail,
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

/// root_dir からの `path` の直下の子。ディレクトリでなければ空で、一覧を取れなければ誤り
pub fn fetch_children(
    root: &Path,
    exclude: &[String],
    include: &[String],
    path: &RelPath,
) -> anyhow::Result<Vec<FileNode>> {
    let nodes = match list_children(root, exclude, path)? {
        remote_merge_core::listing::ChildListing::Children(nodes) => nodes,
        remote_merge_core::listing::ChildListing::NotDirectory => Vec::new(),
        remote_merge_core::listing::ChildListing::Unlistable(reason) => anyhow::bail!(reason),
    };
    Ok(crate::filter::filter_children_by_include(
        nodes,
        path.as_str(),
        include,
    ))
}

/// root_dir からの `path` の直下の子を、三つの結果を区別して返す
pub fn list_children(
    root: &Path,
    exclude: &[String],
    path: &RelPath,
) -> anyhow::Result<remote_merge_core::listing::ChildListing> {
    remote_merge_core::listing::list_children(
        root,
        path,
        exclude,
        crate::config::DEFAULT_MAX_DIR_ENTRIES,
    )
}
