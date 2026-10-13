//! エージェントの経路の範囲の読み取り（子の一覧・走査）。どちらもローカルと同じ集め方と規則の
//! 走査をエージェントに頼む

use remote_merge_core::relative_path::RelPath;
use remote_merge_core::scan::children::ChildListing;
use remote_merge_core::scan::outcome::{Listing, ScanError, ScanRequest, ScanScope};
use remote_merge_engine::child_listing::into_child_listing;
use remote_merge_engine::scan_result::{into_listing, into_tree};

use super::with_agent;
use crate::runtime::core::CoreRuntime;
use crate::runtime::side_io::{remote_root_missing, remote_shown_root};
use crate::tree::FileTree;

/// エージェントで、走査の集め方を深さ一つにしたもので子の一覧を取る（D6）
pub(crate) fn list_children(
    rt: &mut CoreRuntime,
    server_name: &str,
    path: &RelPath,
    exclude: &[String],
    max_entries: usize,
) -> anyhow::Result<ChildListing> {
    let root_dir = rt.get_server_config(server_name)?.root_dir.clone();
    let request = ScanRequest {
        scope: ScanScope::Children(path.as_str().to_string()),
        max_entries,
    };
    let scanned = with_agent(rt, server_name, |agent| agent.list_tree(&request, exclude))?;
    into_child_listing(
        &root_dir,
        &remote_shown_root(server_name, &root_dir),
        path.as_str(),
        scanned.map_err(ScanError::Failure),
        max_entries,
        || remote_root_missing(server_name, &root_dir),
    )
}

/// エージェントで、ローカルと同じ集め方と規則で走査した一覧
pub(crate) fn scan_listing(
    rt: &mut CoreRuntime,
    server_name: &str,
    request: &ScanRequest,
) -> anyhow::Result<Listing> {
    let root_dir = rt.get_server_config(server_name)?.root_dir.clone();
    let exclude = rt.config.filter.exclude.clone();
    let scanned = with_agent(rt, server_name, |agent| agent.list_tree(request, &exclude))?;
    into_listing(
        &remote_shown_root(server_name, &root_dir),
        scanned.map_err(ScanError::Failure),
        || remote_root_missing(server_name, &root_dir),
    )
}

/// エージェントで、ローカルと同じ集め方と規則で走査する。走査を止めた誤り（root_dir がない・
/// 読めないディレクトリなど）はエージェントの故障ではなく、走査の誤りとして返す
pub(crate) fn scan(
    rt: &mut CoreRuntime,
    server_name: &str,
    request: &ScanRequest,
    fail_on_truncation: bool,
) -> anyhow::Result<FileTree> {
    let root_dir = rt.get_server_config(server_name)?.root_dir.clone();
    let exclude = rt.config.filter.exclude.clone();
    let scanned = with_agent(rt, server_name, |agent| agent.list_tree(request, &exclude))?;
    into_tree(
        &root_dir,
        &remote_shown_root(server_name, &root_dir),
        scanned.map_err(ScanError::Failure),
        request.max_entries,
        fail_on_truncation,
        || remote_root_missing(server_name, &root_dir),
    )
}
