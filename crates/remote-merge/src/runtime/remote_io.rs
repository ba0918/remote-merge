//! リモートファイルの読み書き操作（内部 API）。
//!
//! Side ベースの統一 API (`side_io.rs`) から Remote 分岐で呼ばれる。
//! 外部からは `read_file_bytes(side, path, force)` 等の統一 API を使用すること。

use std::collections::HashMap;

use anyhow::Context as _;

use crate::merge::executor;

use super::core::CoreRuntime;
use remote_merge_core::relative_path::{EntryPath, RelPath};

// ── CoreRuntime にリモートI/Oを実装 ──
//
// Side::Remote 分岐の内部実装。side_io.rs が唯一の呼び出し元。

impl CoreRuntime {
    /// リモートファイルに書き込む（自動再接続なし — safety のため）
    ///
    /// side_io.rs の統一 API 経由でのみ使用する。外部からは `write_file(side, path, content)` を使うこと。
    pub(crate) fn write_remote_file(
        &mut self,
        server_name: &str,
        rel_path: &EntryPath,
        content: &str,
    ) -> anyhow::Result<()> {
        let full_path = self.resolve_remote_path(server_name, rel_path.as_rel())?;

        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;

        match self.rt.block_on(client.write_file(&full_path, content)) {
            Ok(()) => {
                tracing::info!(
                    "Remote file written: server={}, path={}, size={}",
                    server_name,
                    rel_path.as_typed(),
                    content.len()
                );
                Ok(())
            }
            Err(e) => {
                tracing::error!(
                    "Remote file write failed: server={}, path={}, error={}",
                    server_name,
                    rel_path.as_typed(),
                    e
                );
                Err(e).with_context(|| format!("write remote file: {}", rel_path.as_typed()))
            }
        }
    }

    /// リモートファイルにバイト列を書き込む（バイナリファイル対応）
    ///
    /// 自動再接続なし（safety のため）。
    /// side_io.rs の統一 API 経由でのみ使用する。
    pub(crate) fn write_remote_file_bytes(
        &mut self,
        server_name: &str,
        rel_path: &EntryPath,
        content: &[u8],
    ) -> anyhow::Result<()> {
        let full_path = self.resolve_remote_path(server_name, rel_path.as_rel())?;

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
                    rel_path.as_typed(),
                    content.len()
                );
                Ok(())
            }
            Err(e) => {
                tracing::error!(
                    "Remote file write (bytes) failed: server={}, path={}, error={}",
                    server_name,
                    rel_path.as_typed(),
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
        rel_path: &EntryPath,
        mode: u32,
    ) -> anyhow::Result<()> {
        let rel = rel_path.as_str();
        remote_merge_core::operations::check_mode(rel, mode)?;
        let full_path = self.resolve_remote_path(server_name, rel_path.as_rel())?;

        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;

        self.rt
            .block_on(client.chmod_file(&full_path, mode))
            .map_err(|error| {
                let message =
                    format!("failed to change permissions of {rel:?} to {mode:o}: {error}");
                error.context(message)
            })
    }

    /// リモートファイルの mtime をバッチ取得する。
    ///
    /// `stat -c '%Y %n'` で一括取得し、`(rel_path, Option<DateTime<Utc>>)` のリストで返す。
    ///
    /// side_io.rs の統一 API 経由でのみ使用する。外部からは `stat_files(side, paths)` を使うこと。
    pub(crate) fn stat_remote_files(
        &mut self,
        server_name: &str,
        rel_paths: &[EntryPath],
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
        let mut results: Vec<(String, Option<chrono::DateTime<chrono::Utc>>)> = rel_paths
            .iter()
            .map(|p| (p.as_typed().to_string(), None))
            .collect();

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

    /// リモートでファイルまたはシンボリックリンクを削除する。symlink はリンク先ではなくリンクを消す。
    ///
    /// 比べた後に消えていたパスは、削除の失敗として Err を返す。
    ///
    /// side_io.rs の統一 API 経由でのみ使用する。外部からは `remove_file(side, path)` を使うこと。
    pub(crate) fn remove_remote_file(
        &mut self,
        server_name: &str,
        rel_path: &EntryPath,
    ) -> anyhow::Result<()> {
        let rel = rel_path.as_str();
        let full_path = self.resolve_remote_path(server_name, rel_path.as_rel())?;
        let cmd = crate::ssh::tree_parser::build_remove_command(&full_path);
        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;
        self.rt
            .block_on(crate::ssh::exec_strict(client, &cmd))
            .map_err(|error| {
                let message = format!("failed to remove {rel:?}: {error}");
                error.context(message)
            })?;
        Ok(())
    }

    /// リモートでシンボリックリンクを作成/更新する。親ディレクトリがなければ作る。
    ///
    /// side_io.rs の統一 API 経由でのみ使用する。外部からは `create_symlink(side, path, target)` を使うこと。
    pub(crate) fn create_remote_symlink(
        &mut self,
        server_name: &str,
        link_rel_path: &EntryPath,
        target: &str,
    ) -> anyhow::Result<()> {
        let full_path = self.resolve_remote_path(server_name, link_rel_path.as_rel())?;
        let cmd = crate::ssh::tree_parser::build_create_symlink_command(target, &full_path);
        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;
        self.rt.block_on(crate::ssh::exec_strict(client, &cmd))?;
        Ok(())
    }

    /// リモートディレクトリの子を SSH で一覧し、三つの結果を区別して返す。`exclude` を当て、子を
    /// `max_entries` 件で打ち切る
    pub(crate) fn list_remote_children_excluding(
        &mut self,
        server_name: &str,
        rel_path: &RelPath,
        exclude: &[String],
        max_entries: usize,
    ) -> anyhow::Result<remote_merge_core::listing::ChildListing> {
        let rel = rel_path.as_str();
        let full_path = self.resolve_remote_path(server_name, rel_path)?;
        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;
        self.rt.block_on(client.list_children_with_limit(
            full_path.trim_end_matches('/'),
            exclude,
            rel,
            max_entries,
        ))
    }

    /// リモートの `rel_path` の項目そのもの（最後の要素のリンクは辿らない）が symlink ならそのリンク先を、
    /// SSH でその項目だけを読んで返す
    pub(crate) fn ssh_entry_link_target(
        &mut self,
        server_name: &str,
        rel_path: &EntryPath,
    ) -> anyhow::Result<Option<String>> {
        let rel = rel_path.as_str();
        let full_path = self.resolve_remote_path(server_name, rel_path.as_rel())?;
        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;
        self.rt.block_on(client.entry_link_target(&full_path, rel))
    }

    // ── private helpers ──

    fn resolve_remote_path(&self, server_name: &str, rel_path: &RelPath) -> anyhow::Result<String> {
        let server_config = self.get_server_config(server_name)?;
        let remote_root = server_config.root_dir.to_string_lossy().to_string();
        Ok(executor::remote_full_path(&remote_root, rel_path))
    }

    fn resolve_remote_paths(
        &self,
        server_name: &str,
        rel_paths: &[EntryPath],
    ) -> anyhow::Result<Vec<String>> {
        let server_config = self.get_server_config(server_name)?;
        let remote_root = server_config.root_dir.to_string_lossy().to_string();
        Ok(rel_paths
            .iter()
            .map(|rel| executor::remote_full_path(&remote_root, rel.as_rel()))
            .collect())
    }
}

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
