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

    /// リモート側でバックアップを作成する（バッチ cp コマンド）。
    ///
    /// `rel_paths` の各ファイルについて、リモートの `.remote-merge-backup/` にコピー。
    /// 1回のSSH exec で全ファイルを処理する。
    ///
    /// side_io.rs の統一 API 経由でのみ使用する。外部からは `create_backups(side, paths)` を使うこと。
    pub(crate) fn create_remote_backups(
        &mut self,
        server_name: &str,
        rel_paths: &[String],
        session_id: &str,
    ) -> anyhow::Result<()> {
        if rel_paths.is_empty() {
            return Ok(());
        }

        let server_config = self.get_server_config(server_name)?;
        let remote_root = server_config.root_dir.to_string_lossy().to_string();

        let pairs: Vec<(String, String)> = rel_paths
            .iter()
            .filter_map(|rel| {
                let src = format!("{}/{}", remote_root.trim_end_matches('/'), rel);
                let dst = crate::backup::remote_backup_path(&remote_root, session_id, rel)?;
                Some((src, dst))
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
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;

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

        let quoted: Vec<String> = full_paths.iter().map(|p| format!("'{}'", p)).collect();
        let cmd = format!("stat -c '%Y %n' {} 2>/dev/null || true", quoted.join(" "));

        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;

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

    /// SSH exec でリモートバックアップセッション一覧を取得する。
    ///
    /// `.remote-merge-backup/` 配下のセッションディレクトリを `find` で列挙し、
    /// 各セッション内のファイルを収集して `BackupSession` のリストを返す。
    pub(crate) fn list_remote_backup_sessions_ssh(
        &mut self,
        server_name: &str,
    ) -> anyhow::Result<Vec<crate::service::types::BackupSession>> {
        use crate::service::types::BackupSession;
        use crate::ssh::tree_parser::shell_escape;

        let server_config = self.get_server_config(server_name)?;
        let remote_root = server_config.root_dir.to_string_lossy().to_string();
        let backup_dir = format!(
            "{}/{}",
            remote_root.trim_end_matches('/'),
            crate::backup::BACKUP_DIR_NAME,
        );

        // セッションディレクトリ一覧を取得（タイムスタンプ降順）
        let list_cmd = format!(
            "find {} -maxdepth 1 -mindepth 1 -type d 2>/dev/null | sort -r",
            shell_escape(&backup_dir),
        );
        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;
        let session_list_output = self.rt.block_on(client.exec(&list_cmd))?;

        let session_ids = parse_backup_session_dirs(&session_list_output);
        let mut sessions = Vec::new();

        for session_id in session_ids {
            // セッション内のファイル一覧とサイズを取得
            let session_path = format!("{}/{}", backup_dir, session_id);
            let file_cmd = format!(
                "find {} -type f -printf '%P\\t%s\\n' 2>/dev/null",
                shell_escape(&session_path),
            );
            let client = self
                .ssh_clients
                .get_mut(server_name)
                .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;
            let file_output = self.rt.block_on(client.exec(&file_cmd))?;

            let mut files = parse_backup_file_entries(&file_output);
            files.sort_by(|a, b| a.path.cmp(&b.path));

            sessions.push(BackupSession::new(session_id, files, false));
        }

        Ok(sessions)
    }

    /// SSH exec でリモートバックアップからファイルを復元する。
    ///
    /// `cp` コマンドを使ってバックアップファイルを元の場所に上書きコピーする。
    /// 個別ファイルのエラーは記録して続行する（部分成功に対応）。
    pub(crate) fn restore_remote_backup_ssh(
        &mut self,
        server_name: &str,
        session_id: &str,
        files: &[String],
        pre_session_id: &str,
        backup_enabled: bool,
    ) -> anyhow::Result<(
        Vec<crate::service::types::RollbackFileResult>,
        Vec<crate::service::types::RollbackFailure>,
    )> {
        use crate::service::types::{RollbackFailure, RollbackFileResult};
        use crate::ssh::tree_parser::shell_escape;

        // session_id のパストラバーサル防御
        if session_id.contains("..") || session_id.contains('/') || session_id.contains('\\') {
            anyhow::bail!("invalid session_id: contains path separator or traversal sequence");
        }

        let server_config = self.get_server_config(server_name)?;
        let remote_root = server_config.root_dir.to_string_lossy().to_string();
        let root_dir = remote_root.trim_end_matches('/').to_string();

        let mut restored = Vec::new();
        let mut failures = Vec::new();

        for file in files {
            // パストラバーサル検証
            let has_parent_dir = std::path::Path::new(file)
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir));
            if has_parent_dir {
                tracing::warn!("Skipping file with path traversal: {}", file);
                failures.push(RollbackFailure {
                    path: file.clone(),
                    error: "path traversal detected".to_string(),
                });
                continue;
            }

            let src = match crate::backup::remote_backup_path(&root_dir, session_id, file) {
                Some(p) => p,
                None => {
                    failures.push(RollbackFailure {
                        path: file.clone(),
                        error: "path traversal detected in file path".to_string(),
                    });
                    continue;
                }
            };
            let dst = format!("{}/{}", root_dir, file);

            // 親ディレクトリを作成してからコピー（cp 成否を出力で確認するため && echo OK || echo FAIL を使用）
            let parent_cmd = if let Some(parent) = std::path::Path::new(&dst).parent() {
                let parent_str = parent.to_string_lossy();
                if parent_str.is_empty() || parent_str == "/" {
                    String::new()
                } else {
                    format!("mkdir -p {} && ", shell_escape(parent_str.as_ref()))
                }
            } else {
                String::new()
            };

            let cmd = format!(
                "{}cp {} {} && echo 'CP_OK' || echo 'CP_FAIL'",
                parent_cmd,
                shell_escape(&src),
                shell_escape(&dst),
            );

            let client = self
                .ssh_clients
                .get_mut(server_name)
                .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;

            match self.rt.block_on(client.exec(&cmd)) {
                Ok(output) if output.contains("CP_OK") && !output.contains("CP_FAIL") => {
                    tracing::debug!("Remote restored: {} from session {}", file, session_id);
                    restored.push(RollbackFileResult {
                        path: file.clone(),
                        pre_rollback_backup: if backup_enabled {
                            Some(pre_session_id.to_string())
                        } else {
                            None
                        },
                    });
                }
                Ok(output) => {
                    tracing::warn!(
                        "cp command returned failure for {} (output: {:?})",
                        file,
                        output.trim()
                    );
                    failures.push(RollbackFailure {
                        path: file.clone(),
                        error: format!("cp failed on remote: {}", output.trim()),
                    });
                }
                Err(e) => {
                    tracing::warn!("Failed to restore {} remotely: {}", file, e);
                    failures.push(RollbackFailure {
                        path: file.clone(),
                        error: format!("ssh exec failed: {}", e),
                    });
                }
            }
        }

        Ok((restored, failures))
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

/// SSH `find` 出力からセッション一覧をパースする（純粋関数）。
///
/// 各行はセッションディレクトリのフルパス。最後のパスコンポーネントを
/// session_id として使用する。以下の条件を満たさないエントリはスキップ:
/// - パスの最後のコンポーネントが取得できないもの
/// - `..`, `/`, `\` を含む session_id（パストラバーサル防御）
/// - タイムスタンプ形式（`YYYYMMDD-HHMMSS`）でないもの
pub(crate) fn parse_backup_session_dirs(find_output: &str) -> Vec<String> {
    find_output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let session_id = std::path::Path::new(line)
                .file_name()
                .and_then(|n| n.to_str())?
                .to_string();

            // パストラバーサル防御
            if session_id.contains("..") || session_id.contains('/') || session_id.contains('\\') {
                tracing::warn!(
                    "Skipping session_id with suspicious characters: {:?}",
                    session_id
                );
                return None;
            }

            // タイムスタンプ形式の検証
            crate::backup::extract_timestamp(&session_id)?;
            Some(session_id)
        })
        .collect()
}

/// SSH `find -printf '%P\t%s\n'` 出力からファイルエントリをパースする（純粋関数）。
///
/// 各行はタブ区切りの `相対パス\tサイズ`。
/// パースできない行やパストラバーサルを含むパスはスキップする。
pub(crate) fn parse_backup_file_entries(
    find_output: &str,
) -> Vec<crate::service::types::BackupEntry> {
    find_output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let (rel_path, size_str) = line.split_once('\t')?;

            // パストラバーサル防御
            let has_traversal = std::path::Path::new(rel_path)
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir));
            if has_traversal {
                tracing::warn!("Skipping backup entry with path traversal: {:?}", rel_path);
                return None;
            }

            let size = size_str.trim().parse::<u64>().unwrap_or(0);
            Some(crate::service::types::BackupEntry {
                path: rel_path.to_string(),
                size,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_backup_session_dirs ──

    #[test]
    fn test_parse_session_dirs_empty() {
        assert!(parse_backup_session_dirs("").is_empty());
        assert!(parse_backup_session_dirs("   \n  \n  ").is_empty());
    }

    #[test]
    fn test_parse_session_dirs_normal() {
        let output = "/var/www/.remote-merge-backup/20240115-140000\n\
                      /var/www/.remote-merge-backup/20240116-100000\n";
        let sessions = parse_backup_session_dirs(output);
        assert_eq!(sessions, vec!["20240115-140000", "20240116-100000"]);
    }

    #[test]
    fn test_parse_session_dirs_rejects_non_timestamp() {
        let output = "/var/www/.remote-merge-backup/not-a-timestamp\n\
                      /var/www/.remote-merge-backup/20240115-140000\n\
                      /var/www/.remote-merge-backup/config.ts\n";
        let sessions = parse_backup_session_dirs(output);
        assert_eq!(sessions, vec!["20240115-140000"]);
    }

    #[test]
    fn test_parse_session_dirs_rejects_path_traversal() {
        // `..` を含む session_id は `file_name()` が返さないため通常は取得されないが、
        // 念のためシミュレートして防御コードを確認する
        let output = "/var/www/.remote-merge-backup/20240115-140000\n";
        let sessions = parse_backup_session_dirs(output);
        assert_eq!(sessions.len(), 1);
        // `..` が含まれるパスは file_name() で取得できないため None になる
        let output_traversal = "/var/www/.remote-merge-backup/../etc/passwd\n";
        let sessions_traversal = parse_backup_session_dirs(output_traversal);
        // file_name() は "passwd" を返すが extract_timestamp が None → スキップ
        assert!(sessions_traversal.is_empty());
    }

    #[test]
    fn test_parse_session_dirs_with_leading_trailing_whitespace() {
        let output = "  /var/www/.remote-merge-backup/20240115-140000  \n";
        let sessions = parse_backup_session_dirs(output);
        assert_eq!(sessions, vec!["20240115-140000"]);
    }

    // ── parse_backup_file_entries ──

    #[test]
    fn test_parse_file_entries_empty() {
        assert!(parse_backup_file_entries("").is_empty());
        assert!(parse_backup_file_entries("   \n").is_empty());
    }

    #[test]
    fn test_parse_file_entries_normal() {
        let output = "src/config.ts\t1234\nsrc/index.ts\t5678\n";
        let entries = parse_backup_file_entries(output);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "src/config.ts");
        assert_eq!(entries[0].size, 1234);
        assert_eq!(entries[1].path, "src/index.ts");
        assert_eq!(entries[1].size, 5678);
    }

    #[test]
    fn test_parse_file_entries_missing_tab() {
        // タブなし行はスキップ
        let output = "src/config.ts\t100\nno-tab-here\nsrc/b.ts\t200\n";
        let entries = parse_backup_file_entries(output);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "src/config.ts");
        assert_eq!(entries[1].path, "src/b.ts");
    }

    #[test]
    fn test_parse_file_entries_invalid_size() {
        // サイズが数字でない場合は 0 にフォールバック
        let output = "src/config.ts\tnot-a-number\n";
        let entries = parse_backup_file_entries(output);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].size, 0);
    }

    #[test]
    fn test_parse_file_entries_rejects_path_traversal() {
        let output = "../etc/passwd\t100\nsrc/config.ts\t200\n";
        let entries = parse_backup_file_entries(output);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "src/config.ts");
    }

    #[test]
    fn test_parse_file_entries_rejects_nested_traversal() {
        let output = "src/../../../etc/passwd\t100\nsrc/safe.ts\t50\n";
        let entries = parse_backup_file_entries(output);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "src/safe.ts");
    }
}
