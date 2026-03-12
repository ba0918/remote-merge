//! Side ベースの統一 I/O API。
//!
//! `Side::Local` と `Side::Remote(name)` を透過的に扱い、
//! swap 後に right=local になっても同じ API でアクセスできるようにする。

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};

use crate::agent::protocol::FileReadResult;
use crate::app::Side;
use crate::local;
use crate::merge::executor;
use crate::tree::{FileNode, FileTree};

use super::core::CoreRuntime;
use super::TuiRuntime;

// ── CoreRuntime に Side ベース統一 I/O を実装 ──
//
// Remote ブランチでは Agent を優先的に使用し、失敗時は SSH にフォールバックする。
// Agent が無い場合は直接 SSH パスを通る。

impl CoreRuntime {
    // ── 読み込み ──

    /// Side に基づいてファイルを読み込む
    pub fn read_file(&mut self, side: &Side, rel_path: &str) -> anyhow::Result<String> {
        match side {
            Side::Local => executor::read_local_file(&self.config.local.root_dir, rel_path),
            Side::Remote(name) => {
                if let Some(content) = self.try_agent_read_file(name, rel_path) {
                    return content;
                }
                self.check_sudo_fallback(name)?;
                self.read_remote_file(name, rel_path)
            }
        }
    }

    /// Side に基づいて複数ファイルをバッチ読み込みする
    pub fn read_files_batch(
        &mut self,
        side: &Side,
        rel_paths: &[String],
    ) -> anyhow::Result<HashMap<String, String>> {
        match side {
            Side::Local => {
                let mut result = HashMap::with_capacity(rel_paths.len());
                for rel_path in rel_paths {
                    let content = executor::read_local_file(&self.config.local.root_dir, rel_path)?;
                    result.insert(rel_path.clone(), content);
                }
                Ok(result)
            }
            Side::Remote(name) => {
                if let Some(batch) = self.try_agent_read_files_batch(name, rel_paths) {
                    return batch;
                }
                self.check_sudo_fallback(name)?;
                self.read_remote_files_batch(name, rel_paths)
            }
        }
    }

    // ── バイト列読み込み ──

    /// Side に基づいてバイト列を読み込む（バイナリファイル対応）
    pub fn read_file_bytes(
        &mut self,
        side: &Side,
        rel_path: &str,
        force: bool,
    ) -> anyhow::Result<Vec<u8>> {
        match side {
            Side::Local => {
                executor::read_local_file_bytes(&self.config.local.root_dir, rel_path, force)
            }
            Side::Remote(name) => {
                if let Some(bytes) = self.try_agent_read_file_bytes(name, rel_path) {
                    return bytes;
                }
                self.check_sudo_fallback(name)?;
                self.read_remote_file_bytes(name, rel_path, force)
            }
        }
    }

    /// Side に基づいて複数ファイルのバイト列をバッチ読み込みする
    ///
    /// `read_files_batch` と同じ strict セマンティクス:
    /// ファイル単位の読み込みエラーは即座に `Err` として伝播する。
    /// エラートレラントな読み込みが必要な場合は `cli::tolerant_io::fetch_contents_tolerant` を使うこと。
    pub fn read_files_bytes_batch(
        &mut self,
        side: &Side,
        rel_paths: &[String],
    ) -> anyhow::Result<HashMap<String, Vec<u8>>> {
        match side {
            Side::Local => {
                let mut result = HashMap::with_capacity(rel_paths.len());
                for rel_path in rel_paths {
                    let bytes = executor::read_local_file_bytes(
                        &self.config.local.root_dir,
                        rel_path,
                        false,
                    )?;
                    result.insert(rel_path.clone(), bytes);
                }
                Ok(result)
            }
            Side::Remote(name) => {
                // Agent バッチを試行
                if let Some(batch_result) = self.try_agent_read_files_bytes_batch(name, rel_paths) {
                    return batch_result;
                }
                // SSH fallback: 1ファイルずつ
                self.check_sudo_fallback(name)?;
                let mut result = HashMap::with_capacity(rel_paths.len());
                for rel_path in rel_paths {
                    let bytes = self.read_remote_file_bytes(name, rel_path, false)?;
                    result.insert(rel_path.clone(), bytes);
                }
                Ok(result)
            }
        }
    }

    // ── 書き込み ──

    /// Side に基づいてファイルを書き込む
    pub fn write_file(&mut self, side: &Side, rel_path: &str, content: &str) -> anyhow::Result<()> {
        match side {
            Side::Local => {
                executor::write_local_file(&self.config.local.root_dir, rel_path, content)
            }
            Side::Remote(name) => {
                if let Some(result) =
                    self.try_agent_write_file(name, rel_path, content.as_bytes(), false)
                {
                    return result;
                }
                self.check_sudo_fallback(name)?;
                self.write_remote_file(name, rel_path, content)
            }
        }
    }

    /// Side に基づいてバイト列を書き込む（バイナリファイル対応）
    pub fn write_file_bytes(
        &mut self,
        side: &Side,
        rel_path: &str,
        content: &[u8],
    ) -> anyhow::Result<()> {
        match side {
            Side::Local => {
                executor::write_local_file_bytes(&self.config.local.root_dir, rel_path, content)
            }
            Side::Remote(name) => {
                if let Some(result) = self.try_agent_write_file(name, rel_path, content, true) {
                    return result;
                }
                self.check_sudo_fallback(name)?;
                self.write_remote_file_bytes(name, rel_path, content)
            }
        }
    }

    // ── メタデータ ──

    /// Side に基づいてファイルの mtime を取得する
    pub fn stat_files(
        &mut self,
        side: &Side,
        rel_paths: &[String],
    ) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>> {
        match side {
            Side::Local => {
                let root = &self.config.local.root_dir;
                for rel_path in rel_paths {
                    let full = root.join(rel_path);
                    executor::validate_path_within_root(root, &full)?;
                }
                stat_local_files(root, rel_paths)
            }
            Side::Remote(name) => {
                if let Some(stats) = self.try_agent_stat_files(name, rel_paths) {
                    return stats;
                }
                self.check_sudo_fallback(name)?;
                self.stat_remote_files(name, rel_paths)
            }
        }
    }

    /// Side に基づいてファイルのパーミッションを変更する
    pub fn chmod_file(&mut self, side: &Side, rel_path: &str, mode: u32) -> anyhow::Result<()> {
        match side {
            Side::Local => {
                let root = &self.config.local.root_dir;
                let full = root.join(rel_path);
                let normalized = executor::validate_path_within_root(root, &full)?;
                chmod_local_file(&normalized, mode)
            }
            // Agent プロトコルに chmod は未定義のため、常に SSH を使用
            Side::Remote(name) => self.chmod_remote_file(name, rel_path, mode),
        }
    }

    // ── バックアップ ──

    /// Side に基づいてバックアップを作成する
    ///
    /// `session_id` はマージ単位で1度だけ生成されたタイムスタンプ。
    /// 全ファイルが同一セッションディレクトリに格納される。
    pub fn create_backups(
        &mut self,
        side: &Side,
        rel_paths: &[String],
        session_id: &str,
    ) -> anyhow::Result<()> {
        match side {
            Side::Local => {
                let root = &self.config.local.root_dir;
                for rel_path in rel_paths {
                    let full = root.join(rel_path);
                    executor::validate_path_within_root(root, &full)?;
                }
                create_local_backups(root, rel_paths, session_id)?;
                Ok(())
            }
            Side::Remote(name) => {
                if let Some(result) = self.try_agent_backup(name, rel_paths, session_id) {
                    return result;
                }
                self.check_sudo_fallback(name)?;
                self.create_remote_backups(name, rel_paths, session_id)
            }
        }
    }

    // ── バックアップ一覧・復元 ──

    /// バックアップセッション一覧を取得する。
    /// Local: ローカルの backup_dir を走査
    /// Remote: 現状は未対応（Agent 対応後に差し替え）
    pub fn list_backup_sessions(
        &mut self,
        side: &Side,
    ) -> anyhow::Result<Vec<crate::service::types::BackupSession>> {
        use crate::backup;
        use crate::service::types::{BackupEntry, BackupSession};

        match side {
            Side::Local => {
                let backup_dir = self.config.local.root_dir.join(backup::BACKUP_DIR_NAME);
                let local_sessions = backup::list_local_sessions(&backup_dir)?;
                Ok(local_sessions
                    .into_iter()
                    .map(|s| {
                        let files: Vec<BackupEntry> = s
                            .files
                            .iter()
                            .map(|path| {
                                let full =
                                    backup::session_backup_path(&backup_dir, &s.session_id, path);
                                let size = std::fs::metadata(&full).map(|m| m.len()).unwrap_or(0);
                                BackupEntry {
                                    path: path.clone(),
                                    size,
                                }
                            })
                            .collect();
                        BackupSession::new(s.session_id, files, false)
                    })
                    .collect())
            }
            Side::Remote(name) => {
                let name = name.clone();
                // Agent 経由で取得を試みる
                if let Some(result) = self.try_agent_list_backup_sessions(&name) {
                    return result.map(|mut sessions| {
                        crate::service::rollback::mark_expired(
                            &mut sessions,
                            self.config.backup.retention_days,
                            chrono::Utc::now(),
                        );
                        sessions
                    });
                }
                // SSH フォールバック
                self.check_sudo_fallback(&name)?;
                let mut sessions = self.list_remote_backup_sessions_ssh(&name)?;
                crate::service::rollback::mark_expired(
                    &mut sessions,
                    self.config.backup.retention_days,
                    chrono::Utc::now(),
                );
                Ok(sessions)
            }
        }
    }

    /// バックアップからファイルを復元する。
    /// 復元前に現在のファイルを自動バックアップ（pre-rollback backup）。
    /// 成功ファイルと失敗ファイルの両方を返す。
    pub fn restore_backup(
        &mut self,
        side: &Side,
        session_id: &str,
        files: &[String],
    ) -> anyhow::Result<(
        Vec<crate::service::types::RollbackFileResult>,
        Vec<crate::service::types::RollbackFailure>,
    )> {
        use crate::backup;

        // 復元前バックアップ（pre-rollback backup）
        let pre_session_id = backup::backup_timestamp();
        if self.config.backup.enabled && !files.is_empty() {
            // 存在するファイルのみバックアップ（復元先にファイルがない場合はスキップ）
            let existing: Vec<String> = files
                .iter()
                .filter(|f| match side {
                    Side::Local => self.config.local.root_dir.join(f).exists(),
                    Side::Remote(_) => true, // リモートは存在確認が困難なので常にバックアップ試行
                })
                .cloned()
                .collect();
            if !existing.is_empty() {
                if let Err(e) = self.create_backups(side, &existing, &pre_session_id) {
                    tracing::warn!("Pre-rollback backup failed (continuing): {}", e);
                }
            }
        }

        // session_id のフォーマット検証
        if backup::extract_timestamp(session_id).is_none() {
            anyhow::bail!("Invalid session_id format: {}", session_id);
        }

        match side {
            Side::Local => {
                let root = self.config.local.root_dir.clone();
                let backup_dir = root.join(backup::BACKUP_DIR_NAME);
                let backup_enabled = self.config.backup.enabled;
                let result = restore_local_files(
                    &root,
                    &backup_dir,
                    session_id,
                    files,
                    backup_enabled,
                    &pre_session_id,
                )?;
                Ok((result.restored, result.failures))
            }
            Side::Remote(name) => {
                let name = name.clone();
                let backup_enabled = self.config.backup.enabled;
                // Agent 経由で復元を試みる
                if let Some(result) = self.try_agent_restore_backup(
                    &name,
                    session_id,
                    files,
                    &pre_session_id,
                    backup_enabled,
                ) {
                    return result;
                }
                // SSH フォールバック
                self.check_sudo_fallback(&name)?;
                self.restore_remote_backup_ssh(
                    &name,
                    session_id,
                    files,
                    &pre_session_id,
                    backup_enabled,
                )
            }
        }
    }

    // ── 削除 ──

    /// Side に基づいてファイルまたはシンボリックリンクを削除する
    pub fn remove_file(&mut self, side: &Side, rel_path: &str) -> anyhow::Result<()> {
        match side {
            Side::Local => {
                let full = self.config.local.root_dir.join(rel_path);
                let normalized =
                    executor::validate_path_within_root(&self.config.local.root_dir, &full)?;
                remove_local_file(&normalized)
            }
            // Agent プロトコルに remove は未定義のため、常に SSH を使用
            Side::Remote(name) => self.remove_remote_file(name, rel_path),
        }
    }

    // ── シンボリックリンク ──

    /// Side に基づいてシンボリックリンクを作成する
    pub fn create_symlink(
        &mut self,
        side: &Side,
        rel_path: &str,
        target: &str,
    ) -> anyhow::Result<()> {
        match side {
            Side::Local => {
                let root = &self.config.local.root_dir;
                let full = root.join(rel_path);
                let normalized = executor::validate_path_within_root(root, &full)?;
                create_local_symlink(&normalized, target)
            }
            Side::Remote(name) => {
                if let Some(result) = self.try_agent_symlink(name, rel_path, target) {
                    return result;
                }
                self.check_sudo_fallback(name)?;
                self.create_remote_symlink(name, rel_path, target)
            }
        }
    }

    // ── ツリー ──

    /// Side に基づいてファイルツリーを取得する（1階層のみ）
    pub fn fetch_tree(&mut self, side: &Side) -> anyhow::Result<FileTree> {
        match side {
            Side::Local => {
                local::scan_local_tree(&self.config.local.root_dir, &self.config.filter.exclude)
            }
            // fetch_tree は1階層のみ — Agent の ListTree は再帰的なので不適合。
            // SSH exec (find) を使用する。
            Side::Remote(name) => self.fetch_remote_tree(name),
        }
    }

    /// Side に基づいてファイルツリーを再帰取得する
    pub fn fetch_tree_recursive(
        &mut self,
        side: &Side,
        max_entries: usize,
    ) -> anyhow::Result<FileTree> {
        match side {
            Side::Local => {
                let root = &self.config.local.root_dir;
                let exclude = &self.config.filter.exclude;
                let (nodes, truncated) =
                    local::scan_local_tree_recursive(root, exclude, max_entries)?;
                if truncated {
                    tracing::warn!("Local tree scan truncated at {} entries", max_entries);
                }
                let mut tree = FileTree::new(root);
                tree.nodes = nodes;
                tree.sort();
                Ok(tree)
            }
            Side::Remote(name) => {
                // Agent の ListTree はフルツリー走査に最適
                if let Some(tree) = self.try_agent_fetch_tree_recursive(name, max_entries) {
                    return tree;
                }
                self.check_sudo_fallback(name)?;
                self.fetch_remote_tree_recursive(name, max_entries)
            }
        }
    }

    /// Side に基づいてディレクトリの子ノードを取得する
    pub fn fetch_children(
        &mut self,
        side: &Side,
        dir_rel_path: &str,
    ) -> anyhow::Result<Vec<FileNode>> {
        match side {
            Side::Local => {
                let root = &self.config.local.root_dir;
                let dir = root.join(dir_rel_path);
                let nodes = local::scan_dir(&dir, &self.config.filter.exclude, dir_rel_path)?;
                Ok(nodes)
            }
            // fetch_children は1階層のみ — Agent は再帰走査なのでここでは SSH を使用
            Side::Remote(name) => self.fetch_remote_children(name, dir_rel_path),
        }
    }

    // ── 接続 ──

    /// リモートの場合のみ接続する（ローカルは何もしない）
    pub fn connect_if_remote(&mut self, side: &Side) -> anyhow::Result<()> {
        match side {
            Side::Local => Ok(()),
            Side::Remote(name) => self.connect(name),
        }
    }

    /// リモートの場合のみ切断する（ローカルは何もしない）
    pub fn disconnect_if_remote(&mut self, side: &Side) {
        if let Side::Remote(name) = side {
            self.disconnect(name);
        }
    }

    /// Side が利用可能かどうか（ローカルは常に true）
    pub fn is_side_available(&self, side: &Side) -> bool {
        match side {
            Side::Local => true,
            Side::Remote(name) => self.has_client(name),
        }
    }
}

// ── Agent 経由 I/O ヘルパー ──
//
// 各メソッドは Agent が接続されている場合にのみ操作を試みる。
// - 成功 → Some(Ok(result))
// - Agent エラー → Agent を無効化して None を返す（呼び出し元が SSH にフォールバック）
// - Agent 未接続 → None
//
// `&mut self` の借用の衝突を避けるため、agent_clients の操作は一時変数を介して行う。

impl CoreRuntime {
    /// Agent 経由で単一ファイルを読み込む
    fn try_agent_read_file(
        &mut self,
        server_name: &str,
        rel_path: &str,
    ) -> Option<anyhow::Result<String>> {
        let full_path = self.resolve_agent_path(server_name, rel_path)?;
        let agent_arc = self.agent_clients.get(server_name)?.clone();

        let mut agent = match agent_arc.lock() {
            Ok(guard) => guard,
            Err(_) => {
                self.invalidate_agent(server_name);
                return None;
            }
        };
        let result = agent.read_files(&[full_path], 0);
        drop(agent);

        match result {
            Ok(results) => {
                let first: Option<FileReadResult> = results.into_iter().next();
                if let Some(FileReadResult::Ok { content, .. }) = first {
                    Some(String::from_utf8(content).map_err(Into::into))
                } else {
                    // FileReadResult::Error — Agent は生きているがファイル読み込み失敗
                    None
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Agent read_file failed for {}, falling back to SSH: {}",
                    server_name,
                    e
                );
                self.invalidate_agent(server_name);
                None
            }
        }
    }

    /// Agent 経由で複数ファイルをバッチ読み込む
    fn try_agent_read_files_batch(
        &mut self,
        server_name: &str,
        rel_paths: &[String],
    ) -> Option<anyhow::Result<HashMap<String, String>>> {
        let full_paths = self.resolve_agent_paths(server_name, rel_paths)?;
        let agent_arc = self.agent_clients.get(server_name)?.clone();

        let mut agent = match agent_arc.lock() {
            Ok(guard) => guard,
            Err(_) => {
                self.invalidate_agent(server_name);
                return None;
            }
        };
        let result = agent.read_files(&full_paths, 0);
        drop(agent);

        match result {
            Ok(results) => {
                let mut map: HashMap<String, String> = HashMap::with_capacity(results.len());
                for (i, result) in results.into_iter().enumerate() {
                    match result {
                        FileReadResult::Ok { content, .. } => match String::from_utf8(content) {
                            Ok(s) => {
                                map.insert(rel_paths[i].clone(), s);
                            }
                            Err(e) => return Some(Err(e.into())),
                        },
                        FileReadResult::Error { .. } => {
                            // ファイル単位のエラー → SSH フォールバックに委ねる
                            return None;
                        }
                    }
                }
                Some(Ok(map))
            }
            Err(e) => {
                tracing::warn!(
                    "Agent read_files_batch failed for {}, falling back to SSH: {}",
                    server_name,
                    e
                );
                self.invalidate_agent(server_name);
                None
            }
        }
    }

    /// Agent 経由でバイト列を読み込む
    fn try_agent_read_file_bytes(
        &mut self,
        server_name: &str,
        rel_path: &str,
    ) -> Option<anyhow::Result<Vec<u8>>> {
        let full_path = self.resolve_agent_path(server_name, rel_path)?;
        let agent_arc = self.agent_clients.get(server_name)?.clone();

        let mut agent = match agent_arc.lock() {
            Ok(guard) => guard,
            Err(_) => {
                self.invalidate_agent(server_name);
                return None;
            }
        };
        let result = agent.read_files(&[full_path], 0);
        drop(agent);

        match result {
            Ok(results) => {
                let first: Option<FileReadResult> = results.into_iter().next();
                if let Some(FileReadResult::Ok { content, .. }) = first {
                    Some(Ok(content))
                } else {
                    None
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Agent read_file_bytes failed for {}, falling back to SSH: {}",
                    server_name,
                    e
                );
                self.invalidate_agent(server_name);
                None
            }
        }
    }

    /// Agent 経由で複数ファイルのバイト列をバッチ読み込む
    fn try_agent_read_files_bytes_batch(
        &mut self,
        server_name: &str,
        rel_paths: &[String],
    ) -> Option<anyhow::Result<HashMap<String, Vec<u8>>>> {
        let full_paths = self.resolve_agent_paths(server_name, rel_paths)?;
        let agent_arc = self.agent_clients.get(server_name)?.clone();

        let mut agent = match agent_arc.lock() {
            Ok(guard) => guard,
            Err(_) => {
                self.invalidate_agent(server_name);
                return None;
            }
        };
        let result = agent.read_files(&full_paths, 0);
        drop(agent);

        match result {
            Ok(results) => {
                let mut map: HashMap<String, Vec<u8>> = HashMap::with_capacity(results.len());
                for (i, file_result) in results.into_iter().enumerate() {
                    match file_result {
                        FileReadResult::Ok { content, .. } => {
                            if i < rel_paths.len() {
                                map.insert(rel_paths[i].clone(), content);
                            }
                        }
                        FileReadResult::Error { .. } => {
                            // ファイル単位のエラー → SSH フォールバックに委ねる
                            return None;
                        }
                    }
                }
                Some(Ok(map))
            }
            Err(e) => {
                tracing::warn!(
                    "Agent read_files_bytes_batch failed for {}, falling back to SSH: {}",
                    server_name,
                    e
                );
                self.invalidate_agent(server_name);
                None
            }
        }
    }

    /// Agent 経由でファイルを書き込む
    fn try_agent_write_file(
        &mut self,
        server_name: &str,
        rel_path: &str,
        content: &[u8],
        is_binary: bool,
    ) -> Option<anyhow::Result<()>> {
        let full_path = self.resolve_agent_path(server_name, rel_path)?;
        let agent_arc = self.agent_clients.get(server_name)?.clone();

        let mut agent = match agent_arc.lock() {
            Ok(guard) => guard,
            Err(_) => {
                self.invalidate_agent(server_name);
                return None;
            }
        };
        let result = agent.write_file(&full_path, content, is_binary);
        drop(agent);

        match result {
            Ok(()) => Some(Ok(())),
            Err(e) => {
                tracing::warn!(
                    "Agent write_file failed for {}, falling back to SSH: {}",
                    server_name,
                    e
                );
                self.invalidate_agent(server_name);
                None
            }
        }
    }

    /// Agent 経由で stat を取得する
    #[allow(clippy::type_complexity)]
    fn try_agent_stat_files(
        &mut self,
        server_name: &str,
        rel_paths: &[String],
    ) -> Option<anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>>> {
        let full_paths = self.resolve_agent_paths(server_name, rel_paths)?;
        let agent_arc = self.agent_clients.get(server_name)?.clone();

        let mut agent = match agent_arc.lock() {
            Ok(guard) => guard,
            Err(_) => {
                self.invalidate_agent(server_name);
                return None;
            }
        };
        let result = agent.stat_files(&full_paths);
        drop(agent);

        match result {
            Ok(stats) => {
                if stats.len() != rel_paths.len() {
                    tracing::warn!(
                        "Agent stat_files returned {} results for {} paths, falling back to SSH",
                        stats.len(),
                        rel_paths.len()
                    );
                    return None;
                }
                let results: Vec<(String, Option<DateTime<Utc>>)> = rel_paths
                    .iter()
                    .enumerate()
                    .map(|(i, rel)| {
                        let mtime = stats
                            .get(i)
                            .and_then(|s| DateTime::from_timestamp(s.mtime_secs, s.mtime_nanos));
                        (rel.clone(), mtime)
                    })
                    .collect();
                Some(Ok(results))
            }
            Err(e) => {
                tracing::warn!(
                    "Agent stat_files failed for {}, falling back to SSH: {}",
                    server_name,
                    e
                );
                self.invalidate_agent(server_name);
                None
            }
        }
    }

    /// Agent 経由でバックアップを作成する
    fn try_agent_backup(
        &mut self,
        server_name: &str,
        rel_paths: &[String],
        session_id: &str,
    ) -> Option<anyhow::Result<()>> {
        let full_paths = self.resolve_agent_paths(server_name, rel_paths)?;
        let backup_dir = crate::backup::agent_backup_session_dir(session_id)?;
        let agent_arc = self.agent_clients.get(server_name)?.clone();

        let mut agent = match agent_arc.lock() {
            Ok(guard) => guard,
            Err(_) => {
                self.invalidate_agent(server_name);
                return None;
            }
        };
        let result = agent.backup(&full_paths, &backup_dir);
        drop(agent);

        match result {
            Ok(()) => Some(Ok(())),
            Err(e) => {
                tracing::warn!(
                    "Agent backup failed for {}, falling back to SSH: {}",
                    server_name,
                    e
                );
                self.invalidate_agent(server_name);
                None
            }
        }
    }

    /// Agent 経由でシンボリックリンクを作成する
    fn try_agent_symlink(
        &mut self,
        server_name: &str,
        rel_path: &str,
        target: &str,
    ) -> Option<anyhow::Result<()>> {
        let full_path = self.resolve_agent_path(server_name, rel_path)?;
        let agent_arc = self.agent_clients.get(server_name)?.clone();

        let mut agent = match agent_arc.lock() {
            Ok(guard) => guard,
            Err(_) => {
                self.invalidate_agent(server_name);
                return None;
            }
        };
        let result = agent.symlink(&full_path, target);
        drop(agent);

        match result {
            Ok(()) => Some(Ok(())),
            Err(e) => {
                tracing::warn!(
                    "Agent symlink failed for {}, falling back to SSH: {}",
                    server_name,
                    e
                );
                self.invalidate_agent(server_name);
                None
            }
        }
    }

    /// Agent 経由でバックアップセッション一覧を取得する
    fn try_agent_list_backup_sessions(
        &mut self,
        server_name: &str,
    ) -> Option<anyhow::Result<Vec<crate::service::types::BackupSession>>> {
        // Agent には root_dir からの相対パスを渡す（絶対パスは拒否される）
        let backup_dir = crate::backup::BACKUP_DIR_NAME.to_string();
        let agent_arc = self.agent_clients.get(server_name)?.clone();

        let mut agent = match agent_arc.lock() {
            Ok(guard) => guard,
            Err(_) => {
                self.invalidate_agent(server_name);
                return None;
            }
        };
        let result = agent.list_backups(&backup_dir);
        drop(agent);

        match result {
            Ok(agent_sessions) => {
                let sessions = convert_agent_backup_sessions(agent_sessions);
                Some(Ok(sessions))
            }
            Err(e) => {
                tracing::warn!(
                    "Agent list_backups failed for {}, falling back to SSH: {}",
                    server_name,
                    e
                );
                self.invalidate_agent(server_name);
                None
            }
        }
    }

    /// Agent 経由でバックアップからファイルを復元する
    fn try_agent_restore_backup(
        &mut self,
        server_name: &str,
        session_id: &str,
        files: &[String],
        pre_session_id: &str,
        backup_enabled: bool,
    ) -> Option<
        anyhow::Result<(
            Vec<crate::service::types::RollbackFileResult>,
            Vec<crate::service::types::RollbackFailure>,
        )>,
    > {
        // Agent には root_dir からの相対パスを渡す（絶対パスは拒否される）
        // root_dir パラメータは Agent 側で無視され、Agent 起動時の --root が使用される
        let backup_dir = crate::backup::BACKUP_DIR_NAME.to_string();
        let agent_arc = self.agent_clients.get(server_name)?.clone();

        let mut agent = match agent_arc.lock() {
            Ok(guard) => guard,
            Err(_) => {
                self.invalidate_agent(server_name);
                return None;
            }
        };
        let result = agent.restore_backup(&backup_dir, session_id, files, "");
        drop(agent);

        match result {
            Ok(agent_results) => {
                let (restored, failures) =
                    convert_agent_restore_results(agent_results, pre_session_id, backup_enabled);
                Some(Ok((restored, failures)))
            }
            Err(e) => {
                tracing::warn!(
                    "Agent restore_backup failed for {}, falling back to SSH: {}",
                    server_name,
                    e
                );
                self.invalidate_agent(server_name);
                None
            }
        }
    }

    /// Agent 経由でツリーを再帰取得する
    fn try_agent_fetch_tree_recursive(
        &mut self,
        server_name: &str,
        max_entries: usize,
    ) -> Option<anyhow::Result<FileTree>> {
        let root_dir = self
            .config
            .servers
            .get(server_name)
            .map(|s| s.root_dir.clone())?;
        let exclude = self.config.filter.exclude.clone();
        let agent_arc = self.agent_clients.get(server_name)?.clone();

        let mut agent = match agent_arc.lock() {
            Ok(guard) => guard,
            Err(_) => {
                self.invalidate_agent(server_name);
                return None;
            }
        };
        // Agent は --root で起動時にルートディレクトリ設定済み。
        // 空文字列を渡すとルート全体を走査する。
        let result = agent.list_tree("", &exclude, max_entries);
        drop(agent);

        match result {
            Ok(entries) => {
                let nodes = crate::agent::tree_scan::convert_agent_entries_to_nodes(&entries);
                let mut tree = FileTree::new(&root_dir);
                tree.nodes = nodes;
                tree.sort();
                Some(Ok(tree))
            }
            Err(e) => {
                tracing::warn!(
                    "Agent list_tree failed for {}, falling back to SSH: {}",
                    server_name,
                    e
                );
                self.invalidate_agent(server_name);
                None
            }
        }
    }

    // ── Agent パス解決ヘルパー ──

    /// Agent 向けにサーバの root_dir + rel_path をフルパスに解決する。
    /// サーバ設定が存在しない場合は None を返す。
    fn resolve_agent_path(&self, server_name: &str, rel_path: &str) -> Option<String> {
        let server_config = self.config.servers.get(server_name)?;
        let remote_root = server_config.root_dir.to_string_lossy();
        Some(format!(
            "{}/{}",
            remote_root.trim_end_matches('/'),
            rel_path.trim_start_matches('/')
        ))
    }

    /// Agent 向けに複数の rel_path をフルパスに解決する
    fn resolve_agent_paths(&self, server_name: &str, rel_paths: &[String]) -> Option<Vec<String>> {
        let server_config = self.config.servers.get(server_name)?;
        let remote_root = server_config.root_dir.to_string_lossy();
        let root = remote_root.trim_end_matches('/');
        Some(
            rel_paths
                .iter()
                .map(|rel| format!("{}/{}", root, rel.trim_start_matches('/')))
                .collect(),
        )
    }
}

// ── Agent 変換ヘルパー（純粋関数） ──

/// `AgentBackupSession` のリストを `BackupSession` のリストに変換する。
fn convert_agent_backup_sessions(
    agent_sessions: Vec<crate::agent::protocol::AgentBackupSession>,
) -> Vec<crate::service::types::BackupSession> {
    agent_sessions
        .into_iter()
        .map(|s| {
            let files: Vec<crate::service::types::BackupEntry> = s
                .files
                .into_iter()
                .map(|f| crate::service::types::BackupEntry {
                    path: f.path,
                    size: f.size,
                })
                .collect();
            crate::service::types::BackupSession::new(s.session_id, files, false)
        })
        .collect()
}

/// `AgentRestoreFileResult` のリストを `(Vec<RollbackFileResult>, Vec<RollbackFailure>)` に変換する。
fn convert_agent_restore_results(
    agent_results: Vec<crate::agent::protocol::AgentRestoreFileResult>,
    pre_session_id: &str,
    backup_enabled: bool,
) -> (
    Vec<crate::service::types::RollbackFileResult>,
    Vec<crate::service::types::RollbackFailure>,
) {
    let mut restored = Vec::new();
    let mut failures = Vec::new();

    for r in agent_results {
        if r.success {
            restored.push(crate::service::types::RollbackFileResult {
                path: r.path,
                pre_rollback_backup: if backup_enabled {
                    Some(pre_session_id.to_string())
                } else {
                    None
                },
            });
        } else {
            failures.push(crate::service::types::RollbackFailure {
                path: r.path,
                error: r.error.unwrap_or_else(|| "unknown error".to_string()),
            });
        }
    }

    (restored, failures)
}

// ── ローカル I/O ヘルパー（純粋関数） ──

/// ローカルファイルの mtime をバッチ取得する
fn stat_local_files(
    root_dir: &Path,
    rel_paths: &[String],
) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>> {
    use chrono::TimeZone;

    let mut results = Vec::with_capacity(rel_paths.len());
    for rel_path in rel_paths {
        let full = root_dir.join(rel_path);
        let mtime = std::fs::metadata(&full)
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|mtime| mtime.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|dur| Utc.timestamp_opt(dur.as_secs() as i64, 0).single());
        results.push((rel_path.clone(), mtime));
    }
    Ok(results)
}

/// ローカルファイルのパーミッションを変更する
fn chmod_local_file(full_path: &Path, mode: u32) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let perms = std::fs::Permissions::from_mode(mode);
    std::fs::set_permissions(full_path, perms)?;
    Ok(())
}

/// ローカルファイルのバックアップをセッションディレクトリに作成する
fn create_local_backups(
    root_dir: &Path,
    rel_paths: &[String],
    session_id: &str,
) -> anyhow::Result<()> {
    let backup_dir = root_dir.join(crate::backup::BACKUP_DIR_NAME);
    for rel_path in rel_paths {
        crate::backup::create_local_backup(root_dir, &backup_dir, session_id, rel_path)?;
    }
    Ok(())
}

/// ローカルバックアップからファイルを復元する。
/// Component レベルのパストラバーサル検証 + canonicalize による復元先検証を行う。
/// 個別ファイルの失敗は記録して続行する（部分成功に対応）。
/// ローカル復元の結果（成功 + 失敗を両方含む）
struct LocalRestoreResult {
    restored: Vec<crate::service::types::RollbackFileResult>,
    failures: Vec<crate::service::types::RollbackFailure>,
}

fn restore_local_files(
    root: &Path,
    backup_dir: &Path,
    session_id: &str,
    files: &[String],
    backup_enabled: bool,
    pre_session_id: &str,
) -> anyhow::Result<LocalRestoreResult> {
    use crate::backup;
    use crate::service::types::{RollbackFailure, RollbackFileResult};

    let mut restored = Vec::new();
    let mut failures = Vec::new();

    for file in files {
        // パストラバーサル検証（Component レベルで ParentDir を拒否）
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

        let source = backup::session_backup_path(backup_dir, session_id, file);
        let dest = root.join(file);

        // 親ディレクトリを作成してから canonicalize で復元先検証
        if let Some(parent) = dest.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!("Failed to create dir for {}: {}", file, e);
                failures.push(RollbackFailure {
                    path: file.clone(),
                    error: format!("failed to create directory: {}", e),
                });
                continue;
            }
            // canonicalize ベースの復元先検証
            if let (Ok(parent_canon), Ok(root_canon)) = (parent.canonicalize(), root.canonicalize())
            {
                if !parent_canon.starts_with(&root_canon) {
                    tracing::warn!("Skipping file outside root: {}", file);
                    failures.push(RollbackFailure {
                        path: file.clone(),
                        error: "restore path is outside project root".to_string(),
                    });
                    continue;
                }
            }
        }

        if !source.exists() {
            tracing::warn!("Backup file not found, skipping: {}", file);
            failures.push(RollbackFailure {
                path: file.clone(),
                error: "backup file not found".to_string(),
            });
            continue;
        }

        match std::fs::copy(&source, &dest) {
            Ok(_) => {
                tracing::debug!("Restored: {} from session {}", file, session_id);
                restored.push(RollbackFileResult {
                    path: file.clone(),
                    pre_rollback_backup: if backup_enabled {
                        Some(pre_session_id.to_string())
                    } else {
                        None
                    },
                });
            }
            Err(e) => {
                tracing::warn!("Failed to restore {}: {}", file, e);
                failures.push(RollbackFailure {
                    path: file.clone(),
                    error: format!("copy failed: {}", e),
                });
            }
        }
    }

    Ok(LocalRestoreResult { restored, failures })
}

/// ローカルファイルまたはシンボリックリンクを削除する
fn remove_local_file(full_path: &Path) -> anyhow::Result<()> {
    use anyhow::Context;
    std::fs::remove_file(full_path)
        .with_context(|| format!("Failed to remove file: {}", full_path.display()))
}

/// ローカルにシンボリックリンクを作成する（既存リンクは削除してから作成）
fn create_local_symlink(full_path: &Path, target: &str) -> anyhow::Result<()> {
    // 既存のファイル/リンクがあれば削除
    if full_path.exists() || full_path.symlink_metadata().is_ok() {
        std::fs::remove_file(full_path)?;
    }

    // 親ディレクトリを作成
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::os::unix::fs::symlink(target, full_path)?;
    Ok(())
}

// ── TuiRuntime デリゲート ──

impl TuiRuntime {
    pub fn read_file(&mut self, side: &Side, rel_path: &str) -> anyhow::Result<String> {
        self.core.read_file(side, rel_path)
    }

    pub fn read_files_batch(
        &mut self,
        side: &Side,
        rel_paths: &[String],
    ) -> anyhow::Result<HashMap<String, String>> {
        self.core.read_files_batch(side, rel_paths)
    }

    pub fn read_files_bytes_batch(
        &mut self,
        side: &Side,
        rel_paths: &[String],
    ) -> anyhow::Result<HashMap<String, Vec<u8>>> {
        self.core.read_files_bytes_batch(side, rel_paths)
    }

    pub fn read_file_bytes(
        &mut self,
        side: &Side,
        rel_path: &str,
        force: bool,
    ) -> anyhow::Result<Vec<u8>> {
        self.core.read_file_bytes(side, rel_path, force)
    }

    pub fn write_file(&mut self, side: &Side, rel_path: &str, content: &str) -> anyhow::Result<()> {
        self.core.write_file(side, rel_path, content)
    }

    pub fn write_file_bytes(
        &mut self,
        side: &Side,
        rel_path: &str,
        content: &[u8],
    ) -> anyhow::Result<()> {
        self.core.write_file_bytes(side, rel_path, content)
    }

    pub fn stat_files(
        &mut self,
        side: &Side,
        rel_paths: &[String],
    ) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>> {
        self.core.stat_files(side, rel_paths)
    }

    pub fn chmod_file(&mut self, side: &Side, rel_path: &str, mode: u32) -> anyhow::Result<()> {
        self.core.chmod_file(side, rel_path, mode)
    }

    pub fn create_backups(
        &mut self,
        side: &Side,
        rel_paths: &[String],
        session_id: &str,
    ) -> anyhow::Result<()> {
        self.core.create_backups(side, rel_paths, session_id)
    }

    pub fn remove_file(&mut self, side: &Side, rel_path: &str) -> anyhow::Result<()> {
        self.core.remove_file(side, rel_path)
    }

    pub fn create_symlink(
        &mut self,
        side: &Side,
        rel_path: &str,
        target: &str,
    ) -> anyhow::Result<()> {
        self.core.create_symlink(side, rel_path, target)
    }

    pub fn fetch_tree(&mut self, side: &Side) -> anyhow::Result<FileTree> {
        self.core.fetch_tree(side)
    }

    pub fn fetch_tree_recursive(
        &mut self,
        side: &Side,
        max_entries: usize,
    ) -> anyhow::Result<FileTree> {
        self.core.fetch_tree_recursive(side, max_entries)
    }

    pub fn fetch_children(
        &mut self,
        side: &Side,
        dir_rel_path: &str,
    ) -> anyhow::Result<Vec<FileNode>> {
        self.core.fetch_children(side, dir_rel_path)
    }

    pub fn connect_if_remote(&mut self, side: &Side) -> anyhow::Result<()> {
        self.core.connect_if_remote(side)
    }

    pub fn disconnect_if_remote(&mut self, side: &Side) {
        self.core.disconnect_if_remote(side);
    }

    pub fn is_side_available(&self, side: &Side) -> bool {
        self.core.is_side_available(side)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::core::CoreRuntime;
    use tempfile::TempDir;

    /// テスト用の CoreRuntime を tempdir をルートにして作成する
    fn create_test_runtime(tmp: &TempDir) -> CoreRuntime {
        let mut rt = CoreRuntime::new_for_test();
        rt.config.local.root_dir = tmp.path().to_path_buf();
        rt
    }

    #[test]
    fn test_read_file_local() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("test.txt"), "hello local").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let content = rt.read_file(&Side::Local, "test.txt").unwrap();
        assert_eq!(content, "hello local");
    }

    #[test]
    fn test_write_file_local() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);

        rt.write_file(&Side::Local, "output.txt", "written content")
            .unwrap();

        let content = std::fs::read_to_string(tmp.path().join("output.txt")).unwrap();
        assert_eq!(content, "written content");
    }

    #[test]
    fn test_stat_files_local() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "aaa").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let paths = vec!["a.txt".to_string(), "nonexistent.txt".to_string()];
        let results = rt.stat_files(&Side::Local, &paths).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "a.txt");
        assert!(results[0].1.is_some());
        assert_eq!(results[1].0, "nonexistent.txt");
        assert!(results[1].1.is_none());
    }

    #[test]
    fn test_is_side_available_local() {
        let rt = CoreRuntime::new_for_test();
        assert!(rt.is_side_available(&Side::Local));
    }

    #[test]
    fn test_is_side_available_remote_nonexistent() {
        let rt = CoreRuntime::new_for_test();
        assert!(!rt.is_side_available(&Side::Remote("nonexistent".to_string())));
    }

    #[test]
    fn test_connect_if_remote_local() {
        let mut rt = CoreRuntime::new_for_test();
        assert!(rt.connect_if_remote(&Side::Local).is_ok());
    }

    #[test]
    fn test_fetch_tree_local() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("file1.txt"), "content").unwrap();
        std::fs::create_dir(tmp.path().join("subdir")).unwrap();

        let mut rt = create_test_runtime(&tmp);
        let tree = rt.fetch_tree(&Side::Local).unwrap();

        assert_eq!(tree.root, tmp.path());
        assert!(tree.nodes.iter().any(|n| n.name == "file1.txt"));
        assert!(tree.nodes.iter().any(|n| n.name == "subdir"));
    }

    #[test]
    fn test_path_traversal_read() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("safe.txt"), "ok").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let result = rt.read_file(&Side::Local, "../../../etc/passwd");
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("Path escapes root_dir")
                || err.contains("Path traversal")
                || err.contains("path not found"),
            "Unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_path_traversal_write() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);

        let result = rt.write_file(&Side::Local, "../outside/file.txt", "malicious");
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("Path escapes root_dir")
                || err.contains("Path traversal")
                || err.contains("path not found"),
            "Unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_read_file_remote_nonexistent_server() {
        let mut rt = CoreRuntime::new_for_test();
        let result = rt.read_file(&Side::Remote("nonexistent".to_string()), "file.txt");
        assert!(result.is_err());
    }

    #[test]
    fn test_fetch_children_local() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("mydir");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("child1.txt"), "c1").unwrap();
        std::fs::write(sub.join("child2.txt"), "c2").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let children = rt.fetch_children(&Side::Local, "mydir").unwrap();

        assert_eq!(children.len(), 2);
        let names: Vec<&str> = children.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"child1.txt"));
        assert!(names.contains(&"child2.txt"));
    }

    #[test]
    fn test_read_files_batch_local() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "aaa").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "bbb").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let paths = vec!["a.txt".to_string(), "b.txt".to_string()];
        let batch = rt.read_files_batch(&Side::Local, &paths).unwrap();

        assert_eq!(batch.len(), 2);
        assert_eq!(batch["a.txt"], "aaa");
        assert_eq!(batch["b.txt"], "bbb");
    }

    #[test]
    fn test_chmod_file_local() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("script.sh"), "#!/bin/bash").unwrap();

        let mut rt = create_test_runtime(&tmp);
        rt.chmod_file(&Side::Local, "script.sh", 0o755).unwrap();

        let meta = std::fs::metadata(tmp.path().join("script.sh")).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o755);
    }

    #[test]
    fn test_create_symlink_local() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("target.txt"), "target content").unwrap();

        let mut rt = create_test_runtime(&tmp);
        rt.create_symlink(&Side::Local, "link.txt", "target.txt")
            .unwrap();

        let link_path = tmp.path().join("link.txt");
        assert!(link_path.symlink_metadata().unwrap().is_symlink());
        let target = std::fs::read_link(&link_path).unwrap();
        assert_eq!(target.to_string_lossy(), "target.txt");
    }

    #[test]
    fn test_read_file_bytes_local() {
        let tmp = TempDir::new().unwrap();
        let binary = vec![0x00, 0x01, 0xFF, 0xFE];
        std::fs::write(tmp.path().join("data.bin"), &binary).unwrap();

        let mut rt = create_test_runtime(&tmp);
        let result = rt.read_file_bytes(&Side::Local, "data.bin", false).unwrap();
        assert_eq!(result, binary);
    }

    #[test]
    fn test_write_file_bytes_local() {
        let tmp = TempDir::new().unwrap();
        let binary = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00];

        let mut rt = create_test_runtime(&tmp);
        rt.write_file_bytes(&Side::Local, "out.bin", &binary)
            .unwrap();

        let written = std::fs::read(tmp.path().join("out.bin")).unwrap();
        assert_eq!(written, binary);
    }

    #[test]
    fn test_bytes_roundtrip_via_side_io() {
        use sha2::{Digest, Sha256};

        let tmp = TempDir::new().unwrap();
        let data: Vec<u8> = (0..=255).collect();

        let mut rt = create_test_runtime(&tmp);
        rt.write_file_bytes(&Side::Local, "roundtrip.bin", &data)
            .unwrap();
        let read_back = rt
            .read_file_bytes(&Side::Local, "roundtrip.bin", false)
            .unwrap();

        assert_eq!(Sha256::digest(&data), Sha256::digest(&read_back));
    }

    #[test]
    fn test_disconnect_if_remote_local_noop() {
        let mut rt = CoreRuntime::new_for_test();
        // ローカルの場合は何もしないことを確認（パニックしない）
        rt.disconnect_if_remote(&Side::Local);
    }

    #[test]
    fn test_remove_file_local_regular_file() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("to_remove.txt");
        std::fs::write(&file_path, "will be removed").unwrap();
        assert!(file_path.exists());

        let mut rt = create_test_runtime(&tmp);
        rt.remove_file(&Side::Local, "to_remove.txt").unwrap();
        assert!(!file_path.exists());
    }

    #[test]
    fn test_remove_file_local_symlink() {
        let tmp = TempDir::new().unwrap();
        let target_path = tmp.path().join("target.txt");
        std::fs::write(&target_path, "target content").unwrap();

        let link_path = tmp.path().join("link.txt");
        std::os::unix::fs::symlink("target.txt", &link_path).unwrap();
        assert!(link_path.symlink_metadata().is_ok());

        let mut rt = create_test_runtime(&tmp);
        rt.remove_file(&Side::Local, "link.txt").unwrap();

        // シンボリックリンクが削除されていること
        assert!(!link_path.exists());
        assert!(link_path.symlink_metadata().is_err());
        // リンク先は残っていること
        assert!(target_path.exists());
    }

    #[test]
    fn test_remove_file_path_traversal_rejected() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);

        let result = rt.remove_file(&Side::Local, "../outside.txt");
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("Path escapes root_dir")
                || err.contains("Path traversal")
                || err.contains("path not found"),
            "Unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_read_files_bytes_batch_local() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.bin"), vec![0x00, 0x01, 0x02]).unwrap();
        std::fs::write(tmp.path().join("b.txt"), "hello").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let paths = vec!["a.bin".to_string(), "b.txt".to_string()];
        let batch = rt.read_files_bytes_batch(&Side::Local, &paths).unwrap();

        assert_eq!(batch.len(), 2);
        assert_eq!(batch["a.bin"], vec![0x00, 0x01, 0x02]);
        assert_eq!(batch["b.txt"], b"hello".to_vec());
    }

    #[test]
    fn test_read_files_bytes_batch_strict_error() {
        // strict セマンティクス: 存在しないファイルが含まれると Err を返す
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("exists.txt"), "content").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let paths = vec!["exists.txt".to_string(), "missing.txt".to_string()];
        let result = rt.read_files_bytes_batch(&Side::Local, &paths);

        assert!(result.is_err());
    }

    #[test]
    fn test_read_files_bytes_batch_empty() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);
        let paths: Vec<String> = vec![];
        let batch = rt.read_files_bytes_batch(&Side::Local, &paths).unwrap();

        assert!(batch.is_empty());
    }

    #[test]
    fn test_write_file_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);

        rt.write_file(&Side::Local, "a/b/c/deep.txt", "deep content")
            .unwrap();

        let content = std::fs::read_to_string(tmp.path().join("a/b/c/deep.txt")).unwrap();
        assert_eq!(content, "deep content");
    }

    // ── convert_agent_backup_sessions のテスト ──

    #[test]
    fn test_convert_agent_backup_sessions_empty() {
        let result = convert_agent_backup_sessions(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_convert_agent_backup_sessions_single_session_with_files() {
        use crate::agent::protocol::{AgentBackupFile, AgentBackupSession};

        let agent_sessions = vec![AgentBackupSession {
            session_id: "session-001".to_string(),
            files: vec![
                AgentBackupFile {
                    path: "src/main.rs".to_string(),
                    size: 1024,
                },
                AgentBackupFile {
                    path: "src/lib.rs".to_string(),
                    size: 2048,
                },
            ],
        }];

        let result = convert_agent_backup_sessions(agent_sessions);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].session_id, "session-001");
        assert_eq!(result[0].files.len(), 2);
        assert_eq!(result[0].files[0].path, "src/main.rs");
        assert_eq!(result[0].files[0].size, 1024);
        assert_eq!(result[0].files[1].path, "src/lib.rs");
        assert_eq!(result[0].files[1].size, 2048);
    }

    #[test]
    fn test_convert_agent_backup_sessions_multiple_sessions() {
        use crate::agent::protocol::{AgentBackupFile, AgentBackupSession};

        let agent_sessions = vec![
            AgentBackupSession {
                session_id: "session-A".to_string(),
                files: vec![AgentBackupFile {
                    path: "a.txt".to_string(),
                    size: 100,
                }],
            },
            AgentBackupSession {
                session_id: "session-B".to_string(),
                files: vec![
                    AgentBackupFile {
                        path: "b1.txt".to_string(),
                        size: 200,
                    },
                    AgentBackupFile {
                        path: "b2.txt".to_string(),
                        size: 300,
                    },
                ],
            },
        ];

        let result = convert_agent_backup_sessions(agent_sessions);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].session_id, "session-A");
        assert_eq!(result[0].files.len(), 1);
        assert_eq!(result[1].session_id, "session-B");
        assert_eq!(result[1].files.len(), 2);
    }

    #[test]
    fn test_convert_agent_backup_sessions_expired_is_false() {
        // mark_expired は別途呼ばれるため、変換直後は expired = false でなければならない
        use crate::agent::protocol::{AgentBackupFile, AgentBackupSession};

        let agent_sessions = vec![AgentBackupSession {
            session_id: "session-X".to_string(),
            files: vec![AgentBackupFile {
                path: "x.txt".to_string(),
                size: 42,
            }],
        }];

        let result = convert_agent_backup_sessions(agent_sessions);

        assert!(!result[0].expired);
    }

    // ── convert_agent_restore_results のテスト ──

    #[test]
    fn test_convert_agent_restore_results_all_success() {
        use crate::agent::protocol::AgentRestoreFileResult;

        let agent_results = vec![
            AgentRestoreFileResult {
                path: "file1.txt".to_string(),
                success: true,
                error: None,
            },
            AgentRestoreFileResult {
                path: "file2.txt".to_string(),
                success: true,
                error: None,
            },
        ];

        let (restored, failures) =
            convert_agent_restore_results(agent_results, "pre-session-001", false);

        assert_eq!(restored.len(), 2);
        assert!(failures.is_empty());
        assert_eq!(restored[0].path, "file1.txt");
        assert_eq!(restored[1].path, "file2.txt");
    }

    #[test]
    fn test_convert_agent_restore_results_all_failure() {
        use crate::agent::protocol::AgentRestoreFileResult;

        let agent_results = vec![
            AgentRestoreFileResult {
                path: "bad1.txt".to_string(),
                success: false,
                error: Some("permission denied".to_string()),
            },
            AgentRestoreFileResult {
                path: "bad2.txt".to_string(),
                success: false,
                error: Some("file not found".to_string()),
            },
        ];

        let (restored, failures) =
            convert_agent_restore_results(agent_results, "pre-session-001", false);

        assert!(restored.is_empty());
        assert_eq!(failures.len(), 2);
        assert_eq!(failures[0].path, "bad1.txt");
        assert_eq!(failures[0].error, "permission denied");
        assert_eq!(failures[1].path, "bad2.txt");
        assert_eq!(failures[1].error, "file not found");
    }

    #[test]
    fn test_convert_agent_restore_results_mixed() {
        use crate::agent::protocol::AgentRestoreFileResult;

        let agent_results = vec![
            AgentRestoreFileResult {
                path: "ok.txt".to_string(),
                success: true,
                error: None,
            },
            AgentRestoreFileResult {
                path: "fail.txt".to_string(),
                success: false,
                error: Some("disk full".to_string()),
            },
            AgentRestoreFileResult {
                path: "ok2.txt".to_string(),
                success: true,
                error: None,
            },
        ];

        let (restored, failures) =
            convert_agent_restore_results(agent_results, "pre-session-002", false);

        assert_eq!(restored.len(), 2);
        assert_eq!(failures.len(), 1);
        assert_eq!(restored[0].path, "ok.txt");
        assert_eq!(restored[1].path, "ok2.txt");
        assert_eq!(failures[0].path, "fail.txt");
        assert_eq!(failures[0].error, "disk full");
    }

    #[test]
    fn test_convert_agent_restore_results_with_pre_rollback_backup() {
        use crate::agent::protocol::AgentRestoreFileResult;

        let agent_results = vec![AgentRestoreFileResult {
            path: "src/app.rs".to_string(),
            success: true,
            error: None,
        }];

        let (restored, _) =
            convert_agent_restore_results(agent_results, "pre-backup-session-42", true);

        assert_eq!(restored.len(), 1);
        assert_eq!(
            restored[0].pre_rollback_backup,
            Some("pre-backup-session-42".to_string())
        );
    }

    #[test]
    fn test_convert_agent_restore_results_without_pre_rollback_backup() {
        use crate::agent::protocol::AgentRestoreFileResult;

        let agent_results = vec![AgentRestoreFileResult {
            path: "src/app.rs".to_string(),
            success: true,
            error: None,
        }];

        // backup_enabled = false → pre_rollback_backup は None
        let (restored, _) =
            convert_agent_restore_results(agent_results, "pre-backup-session-99", false);

        assert_eq!(restored.len(), 1);
        assert!(restored[0].pre_rollback_backup.is_none());
    }

    #[test]
    fn test_convert_agent_restore_results_failure_with_no_error_message() {
        use crate::agent::protocol::AgentRestoreFileResult;

        // error フィールドが None でも "unknown error" にフォールバックされる
        let agent_results = vec![AgentRestoreFileResult {
            path: "mystery.txt".to_string(),
            success: false,
            error: None,
        }];

        let (restored, failures) =
            convert_agent_restore_results(agent_results, "pre-session-000", false);

        assert!(restored.is_empty());
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].error, "unknown error");
    }

    // ── resolve_agent_path / resolve_agent_paths テスト ──

    /// テスト用にサーバー設定を追加した CoreRuntime を作成する
    fn runtime_with_server(name: &str, root: &str) -> CoreRuntime {
        let mut rt = CoreRuntime::new_for_test();
        rt.config.servers.insert(
            name.to_string(),
            crate::config::ServerConfig {
                host: "10.0.0.1".to_string(),
                port: 22,
                user: "deploy".to_string(),
                auth: crate::config::AuthMethod::Key,
                key: None,
                root_dir: std::path::PathBuf::from(root),
                ssh_options: None,
                sudo: false,
                file_permissions: None,
                dir_permissions: None,
            },
        );
        rt
    }

    #[test]
    fn test_resolve_agent_path_basic() {
        let rt = runtime_with_server("develop", "/var/www/app");
        let result = rt.resolve_agent_path("develop", "src/main.rs");
        assert_eq!(result, Some("/var/www/app/src/main.rs".to_string()));
    }

    #[test]
    fn test_resolve_agent_path_trailing_slash_in_root() {
        let rt = runtime_with_server("develop", "/var/www/app/");
        let result = rt.resolve_agent_path("develop", "src/main.rs");
        assert_eq!(result, Some("/var/www/app/src/main.rs".to_string()));
    }

    #[test]
    fn test_resolve_agent_path_leading_slash_in_rel_path() {
        let rt = runtime_with_server("develop", "/var/www/app");
        let result = rt.resolve_agent_path("develop", "/src/main.rs");
        assert_eq!(result, Some("/var/www/app/src/main.rs".to_string()));
    }

    #[test]
    fn test_resolve_agent_path_both_slashes() {
        let rt = runtime_with_server("develop", "/var/www/app/");
        let result = rt.resolve_agent_path("develop", "/src/main.rs");
        assert_eq!(result, Some("/var/www/app/src/main.rs".to_string()));
    }

    #[test]
    fn test_resolve_agent_path_empty_rel_path() {
        let rt = runtime_with_server("develop", "/var/www/app");
        let result = rt.resolve_agent_path("develop", "");
        assert_eq!(result, Some("/var/www/app/".to_string()));
    }

    #[test]
    fn test_resolve_agent_path_unknown_server() {
        let rt = CoreRuntime::new_for_test();
        let result = rt.resolve_agent_path("nonexistent", "file.txt");
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_agent_paths_basic() {
        let rt = runtime_with_server("develop", "/var/www/app");
        let paths = vec!["src/main.rs".to_string(), "Cargo.toml".to_string()];
        let result = rt.resolve_agent_paths("develop", &paths).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "/var/www/app/src/main.rs");
        assert_eq!(result[1], "/var/www/app/Cargo.toml");
    }

    #[test]
    fn test_resolve_agent_paths_empty() {
        let rt = runtime_with_server("develop", "/var/www/app");
        let paths: Vec<String> = vec![];
        let result = rt.resolve_agent_paths("develop", &paths).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_resolve_agent_paths_unknown_server() {
        let rt = CoreRuntime::new_for_test();
        let paths = vec!["file.txt".to_string()];
        let result = rt.resolve_agent_paths("nonexistent", &paths);
        assert!(result.is_none());
    }

    // ── stat_local_files テスト ──

    #[test]
    fn test_stat_local_files_empty_paths() {
        let tmp = TempDir::new().unwrap();
        let results = stat_local_files(tmp.path(), &[]).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_stat_local_files_existing_file_has_mtime() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "content").unwrap();
        let paths = vec!["a.txt".to_string()];
        let results = stat_local_files(tmp.path(), &paths).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "a.txt");
        assert!(results[0].1.is_some());
    }

    #[test]
    fn test_stat_local_files_missing_file_returns_none() {
        let tmp = TempDir::new().unwrap();
        let paths = vec!["nonexistent.txt".to_string()];
        let results = stat_local_files(tmp.path(), &paths).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_none());
    }

    #[test]
    fn test_stat_local_files_mixed() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("exists.txt"), "data").unwrap();
        let paths = vec![
            "exists.txt".to_string(),
            "missing.txt".to_string(),
            "also_missing.txt".to_string(),
        ];
        let results = stat_local_files(tmp.path(), &paths).unwrap();
        assert_eq!(results.len(), 3);
        assert!(results[0].1.is_some());
        assert!(results[1].1.is_none());
        assert!(results[2].1.is_none());
    }

    // ── chmod_local_file テスト ──

    #[test]
    fn test_chmod_local_file_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let result = chmod_local_file(&tmp.path().join("nonexistent.txt"), 0o644);
        assert!(result.is_err());
    }

    // ── remove_local_file テスト ──

    #[test]
    fn test_remove_local_file_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let result = remove_local_file(&tmp.path().join("nonexistent.txt"));
        assert!(result.is_err());
    }

    // ── create_local_symlink テスト ──

    #[test]
    fn test_create_local_symlink_replaces_existing_symlink() {
        let tmp = TempDir::new().unwrap();
        let link_path = tmp.path().join("link.txt");

        // 最初のシンボリックリンクを作成
        std::os::unix::fs::symlink("target1.txt", &link_path).unwrap();
        assert_eq!(
            std::fs::read_link(&link_path).unwrap().to_string_lossy(),
            "target1.txt"
        );

        // 別のターゲットで上書き
        create_local_symlink(&link_path, "target2.txt").unwrap();
        assert_eq!(
            std::fs::read_link(&link_path).unwrap().to_string_lossy(),
            "target2.txt"
        );
    }

    #[test]
    fn test_create_local_symlink_replaces_regular_file() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("file.txt");
        std::fs::write(&file_path, "regular content").unwrap();

        create_local_symlink(&file_path, "new_target").unwrap();
        assert!(file_path.symlink_metadata().unwrap().is_symlink());
        assert_eq!(
            std::fs::read_link(&file_path).unwrap().to_string_lossy(),
            "new_target"
        );
    }

    #[test]
    fn test_create_local_symlink_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let link_path = tmp.path().join("a").join("b").join("link.txt");

        create_local_symlink(&link_path, "target").unwrap();
        assert!(link_path.symlink_metadata().unwrap().is_symlink());
    }

    // ── restore_local_files テスト ──

    #[test]
    fn test_restore_local_files_path_traversal_rejected() {
        let tmp = TempDir::new().unwrap();
        let backup_dir = tmp.path().join(".remote-merge-backup");
        let files = vec!["../../../etc/passwd".to_string()];

        let result =
            restore_local_files(tmp.path(), &backup_dir, "session-001", &files, false, "pre")
                .unwrap();
        assert!(result.restored.is_empty());
        assert_eq!(result.failures.len(), 1);
        assert!(result.failures[0].error.contains("path traversal"));
    }

    #[test]
    fn test_restore_local_files_missing_backup_file() {
        let tmp = TempDir::new().unwrap();
        let backup_dir = tmp.path().join(".remote-merge-backup");
        // バックアップディレクトリは存在するがファイルが無い
        std::fs::create_dir_all(backup_dir.join("session-001")).unwrap();

        let files = vec!["missing.txt".to_string()];
        let result =
            restore_local_files(tmp.path(), &backup_dir, "session-001", &files, false, "pre")
                .unwrap();
        assert!(result.restored.is_empty());
        assert_eq!(result.failures.len(), 1);
        assert!(result.failures[0].error.contains("backup file not found"));
    }

    #[test]
    fn test_restore_local_files_success() {
        let tmp = TempDir::new().unwrap();
        let backup_dir = tmp.path().join(".remote-merge-backup");
        let session_dir = backup_dir.join("session-001");
        std::fs::create_dir_all(&session_dir).unwrap();

        // バックアップファイルを作成
        std::fs::write(session_dir.join("file.txt"), "backup content").unwrap();

        let files = vec!["file.txt".to_string()];
        let result = restore_local_files(
            tmp.path(),
            &backup_dir,
            "session-001",
            &files,
            true,
            "pre-session",
        )
        .unwrap();
        assert_eq!(result.restored.len(), 1);
        assert!(result.failures.is_empty());
        assert_eq!(result.restored[0].path, "file.txt");
        assert_eq!(
            result.restored[0].pre_rollback_backup,
            Some("pre-session".to_string())
        );

        // 復元先にファイルが存在すること
        let restored_content = std::fs::read_to_string(tmp.path().join("file.txt")).unwrap();
        assert_eq!(restored_content, "backup content");
    }

    #[test]
    fn test_restore_local_files_backup_disabled() {
        let tmp = TempDir::new().unwrap();
        let backup_dir = tmp.path().join(".remote-merge-backup");
        let session_dir = backup_dir.join("session-001");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(session_dir.join("file.txt"), "content").unwrap();

        let files = vec!["file.txt".to_string()];
        let result = restore_local_files(
            tmp.path(),
            &backup_dir,
            "session-001",
            &files,
            false, // backup_enabled = false
            "pre-session",
        )
        .unwrap();
        assert_eq!(result.restored.len(), 1);
        // backup_enabled が false なので pre_rollback_backup は None
        assert!(result.restored[0].pre_rollback_backup.is_none());
    }

    #[test]
    fn test_restore_local_files_mixed_success_and_failure() {
        let tmp = TempDir::new().unwrap();
        let backup_dir = tmp.path().join(".remote-merge-backup");
        let session_dir = backup_dir.join("session-001");
        std::fs::create_dir_all(&session_dir).unwrap();

        // file1.txt のバックアップは存在する
        std::fs::write(session_dir.join("file1.txt"), "backup1").unwrap();
        // file2.txt のバックアップは存在しない

        let files = vec!["file1.txt".to_string(), "file2.txt".to_string()];
        let result =
            restore_local_files(tmp.path(), &backup_dir, "session-001", &files, false, "pre")
                .unwrap();
        assert_eq!(result.restored.len(), 1);
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.restored[0].path, "file1.txt");
        assert_eq!(result.failures[0].path, "file2.txt");
    }

    #[test]
    fn test_restore_local_files_empty_list() {
        let tmp = TempDir::new().unwrap();
        let backup_dir = tmp.path().join(".remote-merge-backup");
        let files: Vec<String> = vec![];

        let result =
            restore_local_files(tmp.path(), &backup_dir, "session-001", &files, false, "pre")
                .unwrap();
        assert!(result.restored.is_empty());
        assert!(result.failures.is_empty());
    }

    #[test]
    fn test_restore_local_files_nested_path() {
        let tmp = TempDir::new().unwrap();
        let backup_dir = tmp.path().join(".remote-merge-backup");
        let session_dir = backup_dir.join("session-001");
        // ネストしたディレクトリ構造のバックアップ
        std::fs::create_dir_all(session_dir.join("src/deep")).unwrap();
        std::fs::write(session_dir.join("src/deep/nested.rs"), "fn main() {}").unwrap();

        let files = vec!["src/deep/nested.rs".to_string()];
        let result =
            restore_local_files(tmp.path(), &backup_dir, "session-001", &files, false, "pre")
                .unwrap();
        assert_eq!(result.restored.len(), 1);
        let content = std::fs::read_to_string(tmp.path().join("src/deep/nested.rs")).unwrap();
        assert_eq!(content, "fn main() {}");
    }

    // ── connect_if_remote / disconnect_if_remote テスト ──

    #[test]
    fn test_connect_if_remote_unknown_remote_server() {
        let mut rt = CoreRuntime::new_for_test();
        let result = rt.connect_if_remote(&Side::Remote("nonexistent".to_string()));
        assert!(result.is_err());
    }

    #[test]
    fn test_disconnect_if_remote_remote_noop_when_not_connected() {
        let mut rt = CoreRuntime::new_for_test();
        // パニックしないことを確認
        rt.disconnect_if_remote(&Side::Remote("nonexistent".to_string()));
    }

    // ── fetch_tree_recursive ローカルテスト ──

    #[test]
    fn test_fetch_tree_recursive_local() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("root.txt"), "root").unwrap();
        let sub = tmp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("child.txt"), "child").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let tree = rt.fetch_tree_recursive(&Side::Local, 10000).unwrap();

        assert_eq!(tree.root, tmp.path());
        // ノードが2つ以上あること（root.txt, sub/ の少なくとも2つ）
        assert!(tree.nodes.len() >= 2);
    }

    #[test]
    fn test_fetch_tree_recursive_remote_not_connected() {
        let mut rt = CoreRuntime::new_for_test();
        let result = rt.fetch_tree_recursive(&Side::Remote("nonexistent".to_string()), 10000);
        assert!(result.is_err());
    }

    // ── create_local_backups テスト ──

    #[test]
    fn test_create_local_backups_success() {
        let tmp = TempDir::new().unwrap();
        // バックアップ対象ファイルを作成
        std::fs::write(tmp.path().join("a.txt"), "aaa").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "bbb").unwrap();

        let paths = vec!["a.txt".to_string(), "b.txt".to_string()];
        create_local_backups(tmp.path(), &paths, "session-001").unwrap();

        // バックアップディレクトリが作成されていること
        let backup_dir = tmp.path().join(crate::backup::BACKUP_DIR_NAME);
        assert!(backup_dir.exists());

        // バックアップファイルが正しい内容でコピーされていること
        let session_dir = backup_dir.join("session-001");
        assert!(session_dir.exists(), "session directory should be created");
        let backup_a = session_dir.join("a.txt");
        let backup_b = session_dir.join("b.txt");
        assert!(backup_a.exists(), "a.txt should be backed up");
        assert!(backup_b.exists(), "b.txt should be backed up");
        assert_eq!(std::fs::read_to_string(&backup_a).unwrap(), "aaa");
        assert_eq!(std::fs::read_to_string(&backup_b).unwrap(), "bbb");
    }

    // ── write_file_bytes_local 追加エッジケース ──

    #[test]
    fn test_write_file_bytes_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);

        let binary = vec![0xFF, 0xFE, 0xFD];
        rt.write_file_bytes(&Side::Local, "deep/nested/dir/file.bin", &binary)
            .unwrap();

        let written = std::fs::read(tmp.path().join("deep/nested/dir/file.bin")).unwrap();
        assert_eq!(written, binary);
    }

    // ── convert_agent_restore_results 追加テスト ──

    #[test]
    fn test_convert_agent_restore_results_empty() {
        let (restored, failures) = convert_agent_restore_results(vec![], "pre", false);
        assert!(restored.is_empty());
        assert!(failures.is_empty());
    }

    // ── read_file_bytes ローカルエッジケース ──

    #[test]
    fn test_read_file_bytes_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);
        let result = rt.read_file_bytes(&Side::Local, "nonexistent.bin", false);
        assert!(result.is_err());
    }

    // ── chmod パストラバーサル ──

    #[test]
    fn test_chmod_file_path_traversal_rejected() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);
        let result = rt.chmod_file(&Side::Local, "../outside.sh", 0o755);
        assert!(result.is_err());
    }

    // ── stat_files パストラバーサル ──

    #[test]
    fn test_stat_files_path_traversal_rejected() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);
        let paths = vec!["../../../etc/passwd".to_string()];
        let result = rt.stat_files(&Side::Local, &paths);
        assert!(result.is_err());
    }

    // ── is_side_available remote with server but no SSH ──

    #[test]
    fn test_is_side_available_remote_with_config_but_no_ssh() {
        let rt = runtime_with_server("develop", "/var/www");
        // サーバー設定はあるが SSH 未接続 → false
        assert!(!rt.is_side_available(&Side::Remote("develop".to_string())));
    }
}
