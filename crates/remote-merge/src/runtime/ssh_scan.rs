//! SSH の経路の走査と子の一覧（集め方は `remote_merge_ssh::scan_collect`、規則は core の scan）

use std::path::PathBuf;

use remote_merge_core::relative_path::RelPath;
use remote_merge_core::scan::children::ChildListing;
use remote_merge_core::scan::drive::scan;
use remote_merge_core::scan::exclude::Exclude;
use remote_merge_core::scan::outcome::{Listing, ScanError, ScanRequest, ScanScope};
use remote_merge_engine::child_listing::into_child_listing;
use remote_merge_engine::scan_result::{into_listing, into_tree};
use remote_merge_ssh::scan_collect::SshCollector;

use super::core::CoreRuntime;
use super::side_io::{remote_root_missing, remote_shown_root};
use crate::tree::FileTree;

impl CoreRuntime {
    /// SSH の経路の集め方と走査の規則で走査する。走査の時間切れは接続の設定の timeout_sec から
    /// 決める（REQ-scan-013）
    pub(crate) fn scan_remote_tree(
        &mut self,
        server_name: &str,
        request: &ScanRequest,
        fail_on_truncation: bool,
    ) -> anyhow::Result<FileTree> {
        let exclude = self.config.filter.exclude.clone();
        let (root, scanned) = self.scan_over_ssh(server_name, request, &exclude)?;
        into_tree(
            &root,
            &remote_shown_root(server_name, &root),
            scanned,
            request.max_entries,
            fail_on_truncation,
            || remote_root_missing(server_name, &root),
        )
    }

    /// SSH の経路の集め方と走査の規則で走査した一覧
    pub(crate) fn scan_remote_listing(
        &mut self,
        server_name: &str,
        request: &ScanRequest,
    ) -> anyhow::Result<Listing> {
        let exclude = self.config.filter.exclude.clone();
        let (root, scanned) = self.scan_over_ssh(server_name, request, &exclude)?;
        into_listing(&remote_shown_root(server_name, &root), scanned, || {
            remote_root_missing(server_name, &root)
        })
    }

    /// SSH の経路で、走査の集め方を深さ一つにしたもので子の一覧を取る（D6）
    pub(crate) fn list_children_over_ssh(
        &mut self,
        server_name: &str,
        path: &RelPath,
        exclude: &[String],
        max_entries: usize,
    ) -> anyhow::Result<ChildListing> {
        let request = ScanRequest {
            scope: ScanScope::Children(path.as_str().to_string()),
            max_entries,
        };
        let (root, scanned) = self.scan_over_ssh(server_name, &request, exclude)?;
        into_child_listing(
            &root,
            &remote_shown_root(server_name, &root),
            path.as_str(),
            scanned,
            max_entries,
            || remote_root_missing(server_name, &root),
        )
    }

    /// SSH の集め方で走査する。返す値は root_dir と走査の結果
    fn scan_over_ssh(
        &mut self,
        server_name: &str,
        request: &ScanRequest,
        exclude: &[String],
    ) -> anyhow::Result<(PathBuf, Result<Listing, ScanError>)> {
        let server_config = self
            .config
            .servers
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found in config", server_name))?;
        let root_path = server_config.root_dir.clone();
        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;
        let mut collector = SshCollector {
            client,
            runtime: &self.rt,
            root: root_path.to_string_lossy().into_owned().into_bytes(),
            timeout_sec: self.config.ssh.timeout_sec,
        };
        let scanned = scan(&mut collector, &Exclude::new(exclude), request);
        Ok((root_path, scanned))
    }
}
