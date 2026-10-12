//! SSH の経路の走査（集め方は `remote_merge_ssh::scan_collect`、規則は core の scan）

use remote_merge_core::scan::drive::scan;
use remote_merge_core::scan::exclude::Exclude;
use remote_merge_core::scan::outcome::ScanRequest;
use remote_merge_engine::scan_result::into_tree;
use remote_merge_ssh::scan_collect::SshCollector;

use super::core::CoreRuntime;
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
        let server_config = self
            .config
            .servers
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found in config", server_name))?;
        let root_path = server_config.root_dir.clone();
        let exclude = Exclude::new(&self.config.filter.exclude);
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
        let scanned = scan(&mut collector, &exclude, request);
        into_tree(
            &root_path,
            &super::side_io::remote_shown_root(server_name, &root_path),
            scanned,
            request.max_entries,
            fail_on_truncation,
            || super::side_io::remote_root_missing(server_name, &root_path),
        )
    }
}
