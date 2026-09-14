//! リモートファイルの読み書き操作（内部 API）。
//!
//! Side ベースの統一 API (`side_io.rs`) から Remote 分岐で呼ばれる。
//! 外部からは `read_file(side, path)` 等の統一 API を使用すること。

use std::collections::HashMap;

use crate::merge::executor;

use super::core::CoreRuntime;

// ── CoreRuntime にリモートI/Oを実装 ──
//
// Side::Remote 分岐の内部実装。side_io.rs が唯一の呼び出し元。

impl CoreRuntime {
    pub(crate) fn inspect_remote_path(
        &mut self,
        server_name: &str,
        rel_path: &str,
    ) -> anyhow::Result<super::target_io::TargetPath> {
        let full_path = self.resolve_remote_path(server_name, rel_path)?;
        let command = super::remote_path::build_inspect_path_command(&full_path);
        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {server_name}"))?;
        let output = self.rt.block_on(client.exec_strict(&command))?;
        super::remote_path::parse_inspect_path_output(&output)
    }

    /// リモートファイル内容を取得する（接続エラー時に1回自動再接続）
    ///
    /// side_io.rs の統一 API 経由でのみ使用する。外部からは `read_file(side, path)` を使うこと。
    pub(crate) fn read_remote_file(
        &mut self,
        server_name: &str,
        rel_path: &str,
    ) -> anyhow::Result<String> {
        let full_path = self.resolve_remote_path(server_name, rel_path)?;

        match self.read_file_inner(server_name, &full_path) {
            Ok(content) => Ok(content),
            Err(e) if crate::error::is_connection_error(&e) => {
                tracing::info!(
                    "Read failed (connection error), auto-reconnecting: {}",
                    rel_path
                );
                self.try_reconnect(server_name)?;
                self.read_file_inner(server_name, &full_path)
            }
            Err(e) => Err(e),
        }
    }

    /// 複数のリモートファイルをバッチ読み込みする（接続エラー時に1回自動再接続）
    ///
    /// side_io.rs の統一 API 経由でのみ使用する。外部からは `read_files_batch(side, paths)` を使うこと。
    pub(crate) fn read_remote_files_batch(
        &mut self,
        server_name: &str,
        rel_paths: &[String],
    ) -> anyhow::Result<HashMap<String, String>> {
        if rel_paths.is_empty() {
            return Ok(HashMap::new());
        }

        let full_paths = self.resolve_remote_paths(server_name, rel_paths)?;

        match self.read_files_batch_inner(server_name, &full_paths) {
            Ok(batch_result) => Ok(Self::map_to_rel_paths(rel_paths, &full_paths, batch_result)),
            Err(e) if crate::error::is_connection_error(&e) => {
                tracing::info!("Batch read failed (connection error), auto-reconnecting");
                self.try_reconnect(server_name)?;
                let batch_result = self.read_files_batch_inner(server_name, &full_paths)?;
                Ok(Self::map_to_rel_paths(rel_paths, &full_paths, batch_result))
            }
            Err(e) => Err(e),
        }
    }

    /// 複数のリモートファイルをバイト列としてバッチ読み込みする（接続エラー時に1回自動再接続）。
    ///
    /// SSH の `read_files_batch_bytes()` を使い、チャンク分割されたバッチ読み込みを行う。
    /// side_io.rs の統一 API 経由でのみ使用する。
    pub(crate) fn read_remote_files_batch_bytes(
        &mut self,
        server_name: &str,
        rel_paths: &[String],
    ) -> anyhow::Result<HashMap<String, Vec<u8>>> {
        if rel_paths.is_empty() {
            return Ok(HashMap::new());
        }

        let full_paths = self.resolve_remote_paths(server_name, rel_paths)?;

        match self.read_files_batch_bytes_inner(server_name, &full_paths) {
            Ok(batch_result) => Ok(Self::map_to_rel_paths_bytes(
                rel_paths,
                &full_paths,
                batch_result,
            )),
            Err(e) if crate::error::is_connection_error(&e) => {
                tracing::info!("Batch read bytes failed (connection error), auto-reconnecting");
                self.try_reconnect(server_name)?;
                let batch_result = self.read_files_batch_bytes_inner(server_name, &full_paths)?;
                Ok(Self::map_to_rel_paths_bytes(
                    rel_paths,
                    &full_paths,
                    batch_result,
                ))
            }
            Err(e) => Err(e),
        }
    }

    /// リモートファイルに書き込む（自動再接続なし — safety のため）
    ///
    /// side_io.rs の統一 API 経由でのみ使用する。外部からは `write_file(side, path, content)` を使うこと。
    pub(crate) fn write_remote_file(
        &mut self,
        server_name: &str,
        rel_path: &str,
        content: &str,
    ) -> anyhow::Result<()> {
        let full_path = self.resolve_remote_path(server_name, rel_path)?;

        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;

        match self.rt.block_on(client.write_file(&full_path, content)) {
            Ok(()) => {
                tracing::info!(
                    "Remote file written: server={}, path={}, size={}",
                    server_name,
                    rel_path,
                    content.len()
                );
                Ok(())
            }
            Err(e) => {
                tracing::error!(
                    "Remote file write failed: server={}, path={}, error={}",
                    server_name,
                    rel_path,
                    e
                );
                Err(e)
            }
        }
    }

    /// リモートファイルをバイト列として読み込む（バイナリファイル対応）
    ///
    /// 接続エラー時に1回自動再接続を試みる。
    /// side_io.rs の統一 API 経由でのみ使用する。
    pub(crate) fn read_remote_file_bytes(
        &mut self,
        server_name: &str,
        rel_path: &str,
        force: bool,
    ) -> anyhow::Result<Vec<u8>> {
        let full_path = self.resolve_remote_path(server_name, rel_path)?;

        match self.read_file_bytes_inner(server_name, &full_path) {
            Ok(bytes) => {
                validate_file_size(&bytes, rel_path, force)?;
                Ok(bytes)
            }
            Err(e) if crate::error::is_connection_error(&e) => {
                tracing::info!(
                    "Read bytes failed (connection error), auto-reconnecting: {}",
                    rel_path
                );
                self.try_reconnect(server_name)?;
                let bytes = self.read_file_bytes_inner(server_name, &full_path)?;
                validate_file_size(&bytes, rel_path, force)?;
                Ok(bytes)
            }
            Err(e) => Err(e),
        }
    }

    /// リモートファイルにバイト列を書き込む（バイナリファイル対応）
    ///
    /// 自動再接続なし（safety のため）。
    /// side_io.rs の統一 API 経由でのみ使用する。
    pub(crate) fn write_remote_file_bytes(
        &mut self,
        server_name: &str,
        rel_path: &str,
        content: &[u8],
    ) -> anyhow::Result<()> {
        let full_path = self.resolve_remote_path(server_name, rel_path)?;

        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;

        match self
            .rt
            .block_on(client.write_file_bytes(&full_path, content))
        {
            Ok(()) => {
                tracing::info!(
                    "Remote file written (bytes): server={}, path={}, size={}",
                    server_name,
                    rel_path,
                    content.len()
                );
                Ok(())
            }
            Err(e) => {
                tracing::error!(
                    "Remote file write (bytes) failed: server={}, path={}, error={}",
                    server_name,
                    rel_path,
                    e
                );
                Err(e)
            }
        }
    }

    /// リモートファイルのパーミッションを変更する。
    ///
    /// side_io.rs の統一 API 経由でのみ使用する。外部からは `chmod_file(side, path, mode)` を使うこと。
    pub(crate) fn chmod_remote_file(
        &mut self,
        server_name: &str,
        rel_path: &str,
        mode: u32,
    ) -> anyhow::Result<()> {
        let full_path = self.resolve_remote_path(server_name, rel_path)?;

        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;

        self.rt.block_on(client.chmod_file(&full_path, mode))
    }

    /// リモートファイルの mtime をバッチ取得する。
    ///
    /// `stat -c '%Y %n'` で一括取得し、`(rel_path, Option<DateTime<Utc>>)` のリストで返す。
    ///
    /// side_io.rs の統一 API 経由でのみ使用する。外部からは `stat_files(side, paths)` を使うこと。
    pub(crate) fn stat_remote_files(
        &mut self,
        server_name: &str,
        rel_paths: &[String],
    ) -> anyhow::Result<Vec<(String, Option<chrono::DateTime<chrono::Utc>>)>> {
        if rel_paths.is_empty() {
            return Ok(vec![]);
        }

        let full_paths = self.resolve_remote_paths(server_name, rel_paths)?;

        let quoted: Vec<String> = full_paths
            .iter()
            .map(|p| crate::ssh::tree_parser::shell_escape(p))
            .collect();
        let cmd = format!("stat -c '%Y %n' {} 2>/dev/null || true", quoted.join(" "));

        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;

        let output = self.rt.block_on(client.exec(&cmd))?;

        // パース: 各行 "1705312800 /var/www/src/config.ts"
        let mut results: Vec<(String, Option<chrono::DateTime<chrono::Utc>>)> =
            rel_paths.iter().map(|p| (p.clone(), None)).collect();

        // O(n) ルックアップ用 HashMap (Step 3-3: O(n*m) → O(n) に改善)
        let path_index: HashMap<&str, usize> = full_paths
            .iter()
            .enumerate()
            .map(|(i, p)| (p.as_str(), i))
            .collect();

        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some((ts_str, path)) = line.split_once(' ') {
                if let Ok(epoch) = ts_str.parse::<i64>() {
                    let dt = chrono::DateTime::from_timestamp(epoch, 0);
                    if let Some(&idx) = path_index.get(path) {
                        results[idx].1 = dt;
                    }
                }
            }
        }

        Ok(results)
    }

    /// リモートでファイルまたはシンボリックリンクを削除する（rm -f）。
    ///
    /// `rm -f` を使用する理由: ファイルが存在しない場合にもエラーにならず、
    /// かつ symlink 自体を（参照先ではなく）削除できるため。
    ///
    /// side_io.rs の統一 API 経由でのみ使用する。外部からは `remove_file(side, path)` を使うこと。
    pub(crate) fn remove_remote_file(
        &mut self,
        server_name: &str,
        rel_path: &str,
    ) -> anyhow::Result<()> {
        let full_path = self.resolve_remote_path(server_name, rel_path)?;
        let cmd = format!(
            "rm -f {}",
            crate::ssh::tree_parser::shell_escape(&full_path)
        );
        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;
        self.rt.block_on(client.exec(&cmd))?;
        Ok(())
    }

    /// リモートでシンボリックリンクを作成/更新する（ln -sfn）。
    ///
    /// side_io.rs の統一 API 経由でのみ使用する。外部からは `create_symlink(side, path, target)` を使うこと。
    pub(crate) fn create_remote_symlink(
        &mut self,
        server_name: &str,
        link_rel_path: &str,
        target: &str,
    ) -> anyhow::Result<()> {
        let full_path = self.resolve_remote_path(server_name, link_rel_path)?;

        let cmd = format!(
            "ln -sfn {} {}",
            crate::ssh::tree_parser::shell_escape(target),
            crate::ssh::tree_parser::shell_escape(&full_path),
        );

        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;

        self.rt.block_on(client.exec(&cmd))?;
        Ok(())
    }

    /// リモートディレクトリの子ノードを取得する
    ///
    /// side_io.rs の統一 API 経由でのみ使用する。外部からは `fetch_children(side, path)` を使うこと。
    pub(crate) fn fetch_remote_children(
        &mut self,
        server_name: &str,
        dir_rel_path: &str,
    ) -> anyhow::Result<Vec<crate::tree::FileNode>> {
        let server_config = self.get_server_config(server_name)?;
        let root_dir = server_config.root_dir.to_string_lossy().to_string();
        let sub_dir = format!(
            "{}/{}",
            root_dir.trim_end_matches('/'),
            dir_rel_path.trim_start_matches('/')
        );
        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;
        let nodes = self.rt.block_on(client.list_dir(
            &sub_dir,
            &self.config.filter.exclude,
            dir_rel_path,
        ))?;
        Ok(nodes)
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

    fn read_file_bytes_inner(
        &mut self,
        server_name: &str,
        full_path: &str,
    ) -> anyhow::Result<Vec<u8>> {
        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;
        self.rt.block_on(client.read_file_bytes(full_path))
    }

    fn read_file_inner(&mut self, server_name: &str, full_path: &str) -> anyhow::Result<String> {
        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;
        self.rt.block_on(client.read_file(full_path))
    }

    fn read_files_batch_inner(
        &mut self,
        server_name: &str,
        full_paths: &[String],
    ) -> anyhow::Result<HashMap<String, String>> {
        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;
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

    fn read_files_batch_bytes_inner(
        &mut self,
        server_name: &str,
        full_paths: &[String],
    ) -> anyhow::Result<HashMap<String, Vec<u8>>> {
        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;
        self.rt.block_on(client.read_files_batch_bytes(full_paths))
    }

    fn map_to_rel_paths_bytes(
        rel_paths: &[String],
        full_paths: &[String],
        mut batch_result: HashMap<String, Vec<u8>>,
    ) -> HashMap<String, Vec<u8>> {
        let mut result = HashMap::with_capacity(batch_result.len());
        for (i, rel_path) in rel_paths.iter().enumerate() {
            if let Some(content) = batch_result.remove(&full_paths[i]) {
                result.insert(rel_path.clone(), content);
            }
        }
        result
    }
}

/// バイナリファイルのサイズ制限チェック（100MB）
///
/// `force` が true の場合はスキップする。
fn validate_file_size(bytes: &[u8], rel_path: &str, force: bool) -> anyhow::Result<()> {
    use crate::merge::executor::MAX_BINARY_FILE_SIZE;
    if !force && bytes.len() > MAX_BINARY_FILE_SIZE {
        anyhow::bail!(
            "File too large ({} bytes > {} bytes limit): {}. Use --force to override.",
            bytes.len(),
            MAX_BINARY_FILE_SIZE,
            rel_path
        );
    }
    Ok(())
}

// NOTE: 旧 remote-only TuiRuntime デリゲート（read_remote_file, write_remote_file 等）は
// Phase F で削除済み。Side ベースの統一 API（side_io.rs）を使用すること。

// ── バックアップパース純粋関数 ──

#[cfg(test)]
mod tests {
    use super::*;

    /// shell_escape がシングルクォート含むパスを正しくエスケープすることを検証。
    /// stat_remote_files で format!("'{}'", p) → shell_escape(p) に修正した安全性テスト。
    #[test]
    fn test_stat_quoting_uses_shell_escape_for_single_quotes() {
        let paths = [
            "/var/www/it's a file.txt".to_string(),
            "/var/www/normal.txt".to_string(),
        ];
        let quoted: Vec<String> = paths
            .iter()
            .map(|p| crate::ssh::tree_parser::shell_escape(p))
            .collect();
        // シングルクォート含むパスが正しくエスケープされること
        assert_eq!(quoted[0], "'/var/www/it'\\''s a file.txt'");
        assert_eq!(quoted[1], "'/var/www/normal.txt'");

        // 生成されるコマンドが安全であること
        let cmd = format!("stat -c '%Y %n' {} 2>/dev/null || true", quoted.join(" "));
        assert!(cmd.contains("'\\''"));
        assert!(!cmd.contains("it's")); // 未エスケープのシングルクォートが含まれないこと
    }

    /// stat 出力パースで HashMap ベースのルックアップが正しく動作することを検証。
    #[test]
    fn test_stat_parse_hashmap_lookup() {
        let full_paths = [
            "/var/www/a.txt".to_string(),
            "/var/www/b.txt".to_string(),
            "/var/www/c.txt".to_string(),
        ];
        let rel_paths = [
            "a.txt".to_string(),
            "b.txt".to_string(),
            "c.txt".to_string(),
        ];

        let mut results: Vec<(String, Option<chrono::DateTime<chrono::Utc>>)> =
            rel_paths.iter().map(|p| (p.clone(), None)).collect();

        let path_index: HashMap<&str, usize> = full_paths
            .iter()
            .enumerate()
            .map(|(i, p)| (p.as_str(), i))
            .collect();

        // 順序が入れ替わった出力をシミュレート
        let output = "1705312800 /var/www/c.txt\n1705312900 /var/www/a.txt\n";

        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some((ts_str, path)) = line.split_once(' ') {
                if let Ok(epoch) = ts_str.parse::<i64>() {
                    let dt = chrono::DateTime::from_timestamp(epoch, 0);
                    if let Some(&idx) = path_index.get(path) {
                        results[idx].1 = dt;
                    }
                }
            }
        }

        // a.txt と c.txt は mtime が設定される
        assert!(results[0].1.is_some(), "a.txt should have mtime");
        assert!(results[1].1.is_none(), "b.txt should have no mtime");
        assert!(results[2].1.is_some(), "c.txt should have mtime");
        // 正しいタイムスタンプが設定されること
        assert_eq!(results[0].1.unwrap().timestamp(), 1705312900);
        assert_eq!(results[2].1.unwrap().timestamp(), 1705312800);
    }
}
