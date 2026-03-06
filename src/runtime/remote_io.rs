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

    /// リモート側でバックアップを作成する（バッチ cp コマンド）。
    ///
    /// `rel_paths` の各ファイルについて、リモートの `.remote-merge-backup/` にコピー。
    /// 1回のSSH exec で全ファイルを処理する。
    pub fn create_remote_backups(
        &mut self,
        server_name: &str,
        rel_paths: &[String],
    ) -> anyhow::Result<()> {
        if rel_paths.is_empty() {
            return Ok(());
        }

        let server_config = self.get_server_config(server_name)?;
        let remote_root = server_config.root_dir.to_string_lossy().to_string();
        let ts = crate::backup::backup_timestamp();

        let pairs: Vec<(String, String)> = rel_paths
            .iter()
            .map(|rel| {
                let src = format!("{}/{}", remote_root.trim_end_matches('/'), rel,);
                let dst = crate::backup::remote_backup_path(&remote_root, rel, &ts);
                (src, dst)
            })
            .collect();

        let pair_refs: Vec<(&str, &str)> = pairs
            .iter()
            .map(|(s, d)| (s.as_str(), d.as_str()))
            .collect();

        let cmd = crate::backup::build_batch_backup_command(&pair_refs);
        if cmd.is_empty() {
            return Ok(());
        }

        let client = self
            .ssh_client
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("SSH not connected"))?;

        // バックアップ失敗は警告だけでマージを止めない
        match self.rt.block_on(client.exec(&cmd)) {
            Ok(_) => {
                tracing::info!(
                    "Remote backups created: {} files in {}",
                    rel_paths.len(),
                    remote_root
                );
                Ok(())
            }
            Err(e) => {
                tracing::warn!("Remote backup failed (continuing merge): {}", e);
                Err(e)
            }
        }
    }

    /// リモートファイルの mtime をバッチ取得する。
    ///
    /// `stat -c '%Y %n'` で一括取得し、`(rel_path, Option<DateTime<Utc>>)` のリストで返す。
    pub fn stat_remote_files(
        &mut self,
        server_name: &str,
        rel_paths: &[String],
    ) -> anyhow::Result<Vec<(String, Option<chrono::DateTime<chrono::Utc>>)>> {
        if rel_paths.is_empty() {
            return Ok(vec![]);
        }

        let full_paths = self.resolve_remote_paths(server_name, rel_paths)?;

        let quoted: Vec<String> = full_paths.iter().map(|p| format!("'{}'", p)).collect();
        let cmd = format!("stat -c '%Y %n' {} 2>/dev/null || true", quoted.join(" "));

        let client = self
            .ssh_client
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("SSH not connected"))?;

        let output = self.rt.block_on(client.exec(&cmd))?;

        // パース: 各行 "1705312800 /var/www/src/config.ts"
        let mut results: Vec<(String, Option<chrono::DateTime<chrono::Utc>>)> =
            rel_paths.iter().map(|p| (p.clone(), None)).collect();

        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some((ts_str, path)) = line.split_once(' ') {
                if let Ok(epoch) = ts_str.parse::<i64>() {
                    let dt = chrono::DateTime::from_timestamp(epoch, 0);
                    // full_path → rel_path のマッピング
                    for (i, full) in full_paths.iter().enumerate() {
                        if path == full {
                            results[i].1 = dt;
                            break;
                        }
                    }
                }
            }
        }

        Ok(results)
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
        rel_paths
            .iter()
            .map(|rel| executor::validate_remote_path(&remote_root, rel))
            .collect()
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
        mut batch_result: HashMap<String, String>,
    ) -> HashMap<String, String> {
        let mut result = HashMap::with_capacity(batch_result.len());
        for (i, rel_path) in rel_paths.iter().enumerate() {
            if let Some(content) = batch_result.remove(&full_paths[i]) {
                result.insert(rel_path.clone(), content);
            }
        }
        result
    }
}
