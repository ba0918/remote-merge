//! リモートファイルの読み書き操作。

use std::collections::HashMap;

use crate::merge::executor;

use super::TuiRuntime;

impl TuiRuntime {
    /// リモートファイル内容を取得する（接続エラー時に1回自動再接続）
    pub fn read_remote_file(
        &mut self,
        server_name: &str,
        rel_path: &str,
    ) -> anyhow::Result<String> {
        let full_path = self.resolve_remote_path(server_name, rel_path)?;

        match self.read_file_inner(&full_path) {
            Ok(content) => Ok(content),
            Err(e) if crate::error::is_connection_error(&e) => {
                tracing::info!(
                    "Read failed (connection error), auto-reconnecting: {}",
                    rel_path
                );
                self.try_reconnect(server_name)?;
                self.read_file_inner(&full_path)
            }
            Err(e) => Err(e),
        }
    }

    /// 複数のリモートファイルをバッチ読み込みする（接続エラー時に1回自動再接続）
    pub fn read_remote_files_batch(
        &mut self,
        server_name: &str,
        rel_paths: &[String],
    ) -> anyhow::Result<HashMap<String, String>> {
        if rel_paths.is_empty() {
            return Ok(HashMap::new());
        }

        let full_paths = self.resolve_remote_paths(server_name, rel_paths)?;

        match self.read_files_batch_inner(&full_paths) {
            Ok(batch_result) => Ok(Self::map_to_rel_paths(rel_paths, &full_paths, batch_result)),
            Err(e) if crate::error::is_connection_error(&e) => {
                tracing::info!("Batch read failed (connection error), auto-reconnecting");
                self.try_reconnect(server_name)?;
                let batch_result = self.read_files_batch_inner(&full_paths)?;
                Ok(Self::map_to_rel_paths(rel_paths, &full_paths, batch_result))
            }
            Err(e) => Err(e),
        }
    }

    /// リモートファイルに書き込む（自動再接続なし — safety のため）
    pub fn write_remote_file(
        &mut self,
        server_name: &str,
        rel_path: &str,
        content: &str,
    ) -> anyhow::Result<()> {
        let full_path = self.resolve_remote_path(server_name, rel_path)?;

        let client = self
            .ssh_client
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("SSH not connected"))?;

        self.rt.block_on(client.write_file(&full_path, content))
    }

    // ── private helpers ──

    fn resolve_remote_path(&self, server_name: &str, rel_path: &str) -> anyhow::Result<String> {
        let server_config = self.get_server_config(server_name)?;
        let remote_root = server_config.root_dir.to_string_lossy().to_string();
        executor::validate_remote_path(&remote_root, rel_path)
    }

    fn resolve_remote_paths(
        &self,
        server_name: &str,
        rel_paths: &[String],
    ) -> anyhow::Result<Vec<String>> {
        let server_config = self.get_server_config(server_name)?;
        let remote_root = server_config.root_dir.to_string_lossy().to_string();
        Ok(rel_paths
            .iter()
            .map(|rel| {
                format!(
                    "{}/{}",
                    remote_root.trim_end_matches('/'),
                    rel.trim_start_matches('/')
                )
            })
            .collect())
    }

    fn read_file_inner(&mut self, full_path: &str) -> anyhow::Result<String> {
        let client = self
            .ssh_client
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("SSH not connected"))?;
        self.rt.block_on(client.read_file(full_path))
    }

    fn read_files_batch_inner(
        &mut self,
        full_paths: &[String],
    ) -> anyhow::Result<HashMap<String, String>> {
        let client = self
            .ssh_client
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("SSH not connected"))?;
        self.rt.block_on(client.read_files_batch(full_paths))
    }

    fn map_to_rel_paths(
        rel_paths: &[String],
        full_paths: &[String],
        batch_result: HashMap<String, String>,
    ) -> HashMap<String, String> {
        let mut result = HashMap::new();
        for (i, rel_path) in rel_paths.iter().enumerate() {
            if let Some(content) = batch_result.get(&full_paths[i]) {
                result.insert(rel_path.clone(), content.clone());
            }
        }
        result
    }
}
