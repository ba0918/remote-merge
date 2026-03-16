//! Side ベースの統一 I/O API。
//!
//! `Side::Local` と `Side::Remote(name)` を透過的に扱い、
//! swap 後に right=local になっても同じ API でアクセスできるようにする。

use std::collections::HashMap;
use std::io;
use std::path::Path;

use chrono::{DateTime, Utc};

use crate::agent::protocol::FileReadResult;
use crate::app::Side;
use crate::local;
use crate::merge::executor;
use crate::tree::{FileNode, FileTree};

use super::core::{AgentUnavailableReason, BoxedAgentClient, CoreRuntime};
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
                // SSH fallback: バッチ読み込み（チャンク分割対応）
                self.check_sudo_fallback(name)?;
                self.read_remote_files_batch_bytes(name, rel_paths)
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
    ///
    /// `fail_on_truncation` が true の場合、max_entries で切り捨てが発生するとエラーを返す。
    /// false の場合は warn ログのみで Ok を返す（従来動作）。
    pub fn fetch_tree_recursive(
        &mut self,
        side: &Side,
        max_entries: usize,
        fail_on_truncation: bool,
    ) -> anyhow::Result<FileTree> {
        match side {
            Side::Local => {
                let root = &self.config.local.root_dir;
                let exclude = &self.config.filter.exclude;
                let include = &self.config.filter.include;
                let (nodes, truncated) = local::scan_local_tree_recursive_with_include(
                    root,
                    exclude,
                    include,
                    max_entries,
                )?;
                if truncated {
                    check_truncation(max_entries, fail_on_truncation)?;
                }
                let mut tree = FileTree::new(root);
                tree.nodes = nodes;
                tree.sort();
                Ok(tree)
            }
            Side::Remote(name) => {
                // Agent の ListTree はフルツリー走査に最適
                if let Some(tree) =
                    self.try_agent_fetch_tree_recursive(name, max_entries, fail_on_truncation)
                {
                    return tree;
                }
                self.check_sudo_fallback(name)?;
                self.fetch_remote_tree_recursive(name, max_entries, fail_on_truncation)
            }
        }
    }

    /// 指定サブパス配下のみツリーを取得する。
    /// ルート全体のスキャンを回避し、ディレクトリ指定時のパフォーマンスを改善する。
    ///
    /// - ローカル: scan_local_tree_recursive() に root_dir + subpath を渡す
    /// - リモート Agent: list_tree に subpath を渡す
    /// - リモート SSH: find -P <root_dir>/<subpath> でサブツリーのみ走査
    /// - 存在しないサブパス → 空ツリー（エラーではない）
    /// - truncation は fail_on_truncation に従う
    pub fn fetch_tree_for_subpath(
        &mut self,
        side: &Side,
        subpath: &str,
        max_entries: usize,
        fail_on_truncation: bool,
    ) -> anyhow::Result<FileTree> {
        // サブパスの正規化: 末尾スラッシュを除去
        let subpath = subpath.trim_end_matches('/');

        // path traversal チェック
        let has_traversal = subpath.split('/').any(|component| component == "..");
        if has_traversal {
            anyhow::bail!("path traversal not allowed: {}", subpath);
        }

        match side {
            Side::Local => {
                let root = &self.config.local.root_dir;
                let scan_root = root.join(subpath);

                // 存在しないディレクトリ → 空ツリー
                if !scan_root.exists() || !scan_root.is_dir() {
                    return Ok(FileTree::new(root));
                }

                let exclude = &self.config.filter.exclude;
                let (scanned_nodes, truncated) =
                    local::scan_local_tree_recursive(&scan_root, exclude, max_entries)?;
                if truncated {
                    check_truncation(max_entries, fail_on_truncation)?;
                }

                // スキャン結果を subpath 配下のツリーとして root_dir からの相対に変換
                let mut tree = FileTree::new(root);
                tree.nodes = wrap_nodes_in_subpath(subpath, scanned_nodes);
                tree.sort();
                Ok(tree)
            }
            Side::Remote(name) => {
                // Agent を優先
                if let Some(tree) = self.try_agent_fetch_tree_for_subpath(
                    name,
                    subpath,
                    max_entries,
                    fail_on_truncation,
                ) {
                    return tree;
                }
                self.check_sudo_fallback(name)?;
                self.fetch_remote_tree_for_subpath(name, subpath, max_entries, fail_on_truncation)
            }
        }
    }

    /// Side に基づいてディレクトリの子ノードを取得する
    pub fn fetch_children(
        &mut self,
        side: &Side,
        dir_rel_path: &str,
    ) -> anyhow::Result<Vec<FileNode>> {
        let nodes = match side {
            Side::Local => {
                let root = &self.config.local.root_dir;
                let dir = root.join(dir_rel_path);
                local::scan_dir(&dir, &self.config.filter.exclude, dir_rel_path)?
            }
            // fetch_children は1階層のみ — Agent は再帰走査なのでここでは SSH を使用
            Side::Remote(name) => self.fetch_remote_children(name, dir_rel_path)?,
        };

        // include フィルターを適用（各 child の完全相対パスで判定）
        let include = &self.config.filter.include;
        if include.is_empty() {
            return Ok(nodes);
        }
        let filtered = nodes
            .into_iter()
            .filter(|node| {
                let child_path = if dir_rel_path.is_empty() {
                    node.name.clone()
                } else {
                    format!("{}/{}", dir_rel_path, node.name)
                };
                crate::filter::is_path_included(&child_path, include)
            })
            .collect();
        Ok(filtered)
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
        let rel = rel_path.to_string();
        let agent_result = self.with_agent(server_name, "read_file", |agent| {
            agent.read_files(&[rel], 0)
        });
        // with_agent の結果 → FileReadResult の後処理（純粋関数）
        flatten_agent_read_result(agent_result, extract_single_file_as_string)
    }

    /// Agent 経由で複数ファイルをバッチ読み込む
    fn try_agent_read_files_batch(
        &mut self,
        server_name: &str,
        rel_paths: &[String],
    ) -> Option<anyhow::Result<HashMap<String, String>>> {
        let paths = rel_paths.to_vec();
        let owned_rel_paths: Vec<String> = rel_paths.to_vec();
        let agent_result = self.with_agent(server_name, "read_files_batch", |agent| {
            agent.read_files(&paths, 0)
        });
        flatten_agent_read_result(agent_result, |results| {
            extract_batch_files_as_string(results, &owned_rel_paths)
        })
    }

    /// Agent 経由でバイト列を読み込む
    fn try_agent_read_file_bytes(
        &mut self,
        server_name: &str,
        rel_path: &str,
    ) -> Option<anyhow::Result<Vec<u8>>> {
        let rel = rel_path.to_string();
        let agent_result = self.with_agent(server_name, "read_file_bytes", |agent| {
            agent.read_files(&[rel], 0)
        });
        flatten_agent_read_result(agent_result, extract_single_file_as_bytes)
    }

    /// Agent 経由で複数ファイルのバイト列をバッチ読み込む（チャンク分割対応）
    ///
    /// パスを `AGENT_BATCH_MAX_PATHS` 件ごとにチャンク分割して Agent に送る。
    /// チャンク途中でエラーが発生した場合は Agent を無効化して None を返す
    /// （呼び出し元が SSH フォールバックで全件リトライ）。
    fn try_agent_read_files_bytes_batch(
        &mut self,
        server_name: &str,
        rel_paths: &[String],
    ) -> Option<anyhow::Result<HashMap<String, Vec<u8>>>> {
        use crate::ssh::batch_read::AGENT_BATCH_MAX_PATHS;

        // サーバー設定の存在確認（Agent が有効かどうか）
        self.config.servers.get(server_name)?;
        let agent_arc = self.agent_clients.get(server_name)?.clone();

        let mut agent = match agent_arc.lock() {
            Ok(guard) => guard,
            Err(_) => {
                self.invalidate_agent(server_name);
                return None;
            }
        };

        let mut merged: HashMap<String, Vec<u8>> = HashMap::with_capacity(rel_paths.len());

        // パス数ベースのチャンク分割
        for chunk in rel_paths.chunks(AGENT_BATCH_MAX_PATHS) {
            let paths: Vec<String> = chunk.to_vec();
            let result = agent.read_files(&paths, 0);

            match result {
                Ok(results) => {
                    match extract_batch_files_as_bytes(results, chunk) {
                        Some(Ok(batch)) => merged.extend(batch),
                        Some(Err(e)) => {
                            // 変換エラー → 伝播
                            drop(agent);
                            return Some(Err(e));
                        }
                        None => {
                            // FileReadResult::Error → SSH フォールバック
                            drop(agent);
                            self.invalidate_agent(server_name);
                            return None;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Agent read_files_bytes_batch failed for {} (chunk), falling back to SSH: {}",
                        server_name,
                        e
                    );
                    drop(agent);
                    self.invalidate_agent(server_name);
                    return None;
                }
            }
        }

        drop(agent);
        Some(Ok(merged))
    }

    /// Agent 経由でファイルを書き込む
    fn try_agent_write_file(
        &mut self,
        server_name: &str,
        rel_path: &str,
        content: &[u8],
        is_binary: bool,
    ) -> Option<anyhow::Result<()>> {
        self.with_agent(server_name, "write_file", |agent| {
            agent.write_file(rel_path, content, is_binary)
        })
    }

    /// Agent 経由で stat を取得する
    #[allow(clippy::type_complexity)]
    fn try_agent_stat_files(
        &mut self,
        server_name: &str,
        rel_paths: &[String],
    ) -> Option<anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>>> {
        let paths = rel_paths.to_vec();
        let owned_rel_paths: Vec<String> = rel_paths.to_vec();
        let agent_result =
            self.with_agent(server_name, "stat_files", |agent| agent.stat_files(&paths));
        // with_agent の結果に対して後処理を適用
        match agent_result {
            Some(Ok(stats)) => transform_stat_results(stats, &owned_rel_paths),
            Some(Err(e)) => Some(Err(e)),
            None => None,
        }
    }

    /// Agent 経由でバックアップを作成する
    fn try_agent_backup(
        &mut self,
        server_name: &str,
        rel_paths: &[String],
        session_id: &str,
    ) -> Option<anyhow::Result<()>> {
        let backup_dir = crate::backup::agent_backup_session_dir(session_id)?;
        let paths: Vec<String> = rel_paths.to_vec();
        self.with_agent(server_name, "backup", |agent| {
            agent.backup(&paths, &backup_dir)
        })
    }

    /// Agent 経由でシンボリックリンクを作成する
    fn try_agent_symlink(
        &mut self,
        server_name: &str,
        rel_path: &str,
        target: &str,
    ) -> Option<anyhow::Result<()>> {
        self.with_agent(server_name, "symlink", |agent| {
            agent.symlink(rel_path, target)
        })
    }

    /// Agent 経由でバックアップセッション一覧を取得する
    fn try_agent_list_backup_sessions(
        &mut self,
        server_name: &str,
    ) -> Option<anyhow::Result<Vec<crate::service::types::BackupSession>>> {
        let backup_dir = crate::backup::BACKUP_DIR_NAME.to_string();
        self.with_agent(server_name, "list_backups", |agent| {
            agent.list_backups(&backup_dir)
        })
        .map(|r| r.map(convert_agent_backup_sessions))
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
        let backup_dir = crate::backup::BACKUP_DIR_NAME.to_string();
        let pre_session_id = pre_session_id.to_string();
        self.with_agent(server_name, "restore_backup", |agent| {
            agent.restore_backup(&backup_dir, session_id, files, "")
        })
        .map(|r| {
            r.map(|agent_results| {
                convert_agent_restore_results(agent_results, &pre_session_id, backup_enabled)
            })
        })
    }

    /// Agent 経由でツリーを再帰取得する
    fn try_agent_fetch_tree_recursive(
        &mut self,
        server_name: &str,
        max_entries: usize,
        fail_on_truncation: bool,
    ) -> Option<anyhow::Result<FileTree>> {
        let root_dir = self
            .config
            .servers
            .get(server_name)
            .map(|s| s.root_dir.clone())?;
        let exclude = self.config.filter.exclude.clone();
        let include = self.config.filter.include.clone();
        self.with_agent(server_name, "list_tree", |agent| {
            agent.list_tree("", &exclude, &include, max_entries)
        })
        .map(|r| {
            r.and_then(|(entries, truncated)| {
                if truncated {
                    check_truncation(max_entries, fail_on_truncation)?;
                }
                let nodes = crate::agent::tree_scan::convert_agent_entries_to_nodes(&entries);
                let mut tree = FileTree::new(&root_dir);
                tree.nodes = nodes;
                tree.sort();
                Ok(tree)
            })
        })
    }

    /// Agent 経由でサブパス配下のツリーを取得する
    fn try_agent_fetch_tree_for_subpath(
        &mut self,
        server_name: &str,
        subpath: &str,
        max_entries: usize,
        fail_on_truncation: bool,
    ) -> Option<anyhow::Result<FileTree>> {
        let root_dir = self
            .config
            .servers
            .get(server_name)
            .map(|s| s.root_dir.clone())?;
        let exclude = self.config.filter.exclude.clone();
        // サブパス走査では include フィルターは適用しない
        // （既に特定サブディレクトリを直接指定しているため）
        let subpath_owned = subpath.to_string();
        self.with_agent(server_name, "list_tree", |agent| {
            agent.list_tree(&subpath_owned, &exclude, &[], max_entries)
        })
        .map(|r| {
            r.and_then(|(entries, truncated)| {
                if truncated {
                    check_truncation(max_entries, fail_on_truncation)?;
                }
                let nodes = crate::agent::tree_scan::convert_agent_entries_to_nodes(&entries);
                let mut tree = FileTree::new(&root_dir);
                // Agent の list_tree は subpath をルートとして走査するため、
                // 結果を subpath 配下にラップして root_dir からの相対パスにする
                tree.nodes = wrap_nodes_in_subpath(&subpath_owned, nodes);
                tree.sort();
                Ok(tree)
            })
        })
    }

    /// Agent のロック取得 + 操作実行 + エラーハンドリングの共通ヘルパー
    ///
    /// 成功: Some(Ok(T))
    /// Agent 未設定/サーバ未設定: None
    /// Agent lock 失敗/操作エラー: invalidate + None（SSH フォールバック）
    ///
    /// エラーの種類に応じて invalidate 判定を行う:
    /// - BrokenPipe / ConnectionReset / ConnectionAborted → 致命的: OperationFailed を記録して invalidate
    /// - 不明エラー → 安全側: OperationFailed を記録して invalidate
    /// - それ以外の io::Error → 非致命的: warn のみ。invalidate しない
    fn with_agent<T, F>(
        &mut self,
        server_name: &str,
        op_name: &str,
        f: F,
    ) -> Option<anyhow::Result<T>>
    where
        F: FnOnce(&mut BoxedAgentClient) -> anyhow::Result<T>,
    {
        self.config.servers.get(server_name)?;
        let agent_arc = self.agent_clients.get(server_name)?.clone();
        let mut agent = match agent_arc.lock() {
            Ok(guard) => guard,
            Err(_) => {
                self.agent_unavailable.insert(
                    server_name.to_string(),
                    AgentUnavailableReason::OperationFailed,
                );
                self.invalidate_agent(server_name);
                return None;
            }
        };
        let result = f(&mut agent);
        drop(agent);
        match result {
            Ok(val) => Some(Ok(val)),
            Err(e) => {
                // エラー種別を純粋関数で判定
                let should_invalidate = should_invalidate_agent_error(&e);

                if should_invalidate {
                    tracing::warn!("Agent invalidated for {}: {}", server_name, e);
                    // invalidate_agent() 内で agent_unavailable を設定する
                    // （sudo=true の場合は SudoInvalidated、それ以外は OperationFailed）
                    self.invalidate_agent(server_name);
                } else {
                    tracing::warn!(
                        "Agent {} temporary error for {}, falling back to SSH: {}",
                        op_name,
                        server_name,
                        e
                    );
                }
                None
            }
        }
    }

    // ── Agent パス解決ヘルパー ──
}

// ── Agent エラー判定関数 ──

/// Agent の操作エラーが接続を invalidate すべきかを判定する純粋関数。
///
/// anyhow のエラーチェーン全体を探索し、`io::Error` を検出する。
///
/// 判定基準:
/// - `io::Error` で BrokenPipe / ConnectionReset / ConnectionAborted / UnexpectedEof → true（致命的接続エラー）
/// - `io::Error` でそれ以外 → false（一時的エラー、invalidate 不要）
/// - チェーン内に `io::Error` なし → true（安全側: 不明エラーは invalidate する）
pub(crate) fn should_invalidate_agent_error(e: &anyhow::Error) -> bool {
    for cause in e.chain() {
        if let Some(io_err) = cause.downcast_ref::<io::Error>() {
            return matches!(
                io_err.kind(),
                io::ErrorKind::BrokenPipe
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::UnexpectedEof
            );
        }
    }
    // チェーン内に io::Error なし → 安全側に倒して invalidate
    true
}

// ── Agent read 結果変換ヘルパー（純粋関数） ──

/// `with_agent` の結果（`Option<Result<Vec<FileReadResult>>>`）に対して
/// 変換関数を適用し、最終結果を `Option<Result<T>>` として返す。
///
/// - `None` → そのまま `None`（Agent 未接続/エラー）
/// - `Some(Err(e))` → `Some(Err(e))`（Agent 操作エラー）
/// - `Some(Ok(results))` → `transform(results)` を適用
fn flatten_agent_read_result<T, F>(
    agent_result: Option<anyhow::Result<Vec<FileReadResult>>>,
    transform: F,
) -> Option<anyhow::Result<T>>
where
    F: FnOnce(Vec<FileReadResult>) -> Option<anyhow::Result<T>>,
{
    match agent_result {
        Some(Ok(results)) => transform(results),
        Some(Err(e)) => Some(Err(e)),
        None => None,
    }
}

/// 単一ファイルの `FileReadResult` を `String` に変換する。
///
/// - `FileReadResult::Ok` → `Some(Ok(String))` （UTF-8 変換エラーは `Some(Err)` で伝播）
/// - `FileReadResult::Error` / 結果なし → `None`（SSH フォールバック）
fn extract_single_file_as_string(results: Vec<FileReadResult>) -> Option<anyhow::Result<String>> {
    let first = results.into_iter().next()?;
    match first {
        FileReadResult::Ok { content, .. } => Some(String::from_utf8(content).map_err(Into::into)),
        FileReadResult::Error { .. } => None,
    }
}

/// 単一ファイルの `FileReadResult` をバイト列として取得する。
///
/// - `FileReadResult::Ok` → `Some(Ok(Vec<u8>))`
/// - `FileReadResult::Error` / 結果なし → `None`（SSH フォールバック）
fn extract_single_file_as_bytes(results: Vec<FileReadResult>) -> Option<anyhow::Result<Vec<u8>>> {
    let first = results.into_iter().next()?;
    match first {
        FileReadResult::Ok { content, .. } => Some(Ok(content)),
        FileReadResult::Error { .. } => None,
    }
}

/// 複数ファイルの `FileReadResult` を `HashMap<String, String>` に変換する。
///
/// - 全ファイル成功 → `Some(Ok(HashMap))`
/// - UTF-8 変換エラー → `Some(Err)`
/// - `FileReadResult::Error` → `None`（SSH フォールバック）
fn extract_batch_files_as_string(
    results: Vec<FileReadResult>,
    rel_paths: &[String],
) -> Option<anyhow::Result<HashMap<String, String>>> {
    let mut map = HashMap::with_capacity(results.len());
    for (i, result) in results.into_iter().enumerate() {
        match result {
            FileReadResult::Ok { content, .. } => match String::from_utf8(content) {
                Ok(s) => {
                    if i < rel_paths.len() {
                        map.insert(rel_paths[i].clone(), s);
                    }
                }
                Err(e) => return Some(Err(e.into())),
            },
            FileReadResult::Error { .. } => return None,
        }
    }
    Some(Ok(map))
}

/// 複数ファイルの `FileReadResult` をバイト列の `HashMap` に変換する。
///
/// - 全ファイル成功 → `Some(Ok(HashMap))`
/// - `FileReadResult::Error` → `None`（SSH フォールバック）
fn extract_batch_files_as_bytes(
    results: Vec<FileReadResult>,
    rel_paths: &[String],
) -> Option<anyhow::Result<HashMap<String, Vec<u8>>>> {
    let mut map = HashMap::with_capacity(results.len());
    for (i, result) in results.into_iter().enumerate() {
        match result {
            FileReadResult::Ok { content, .. } => {
                if i < rel_paths.len() {
                    map.insert(rel_paths[i].clone(), content);
                }
            }
            FileReadResult::Error { .. } => return None,
        }
    }
    Some(Ok(map))
}

/// Agent stat 結果を `(path, Option<DateTime>)` のベクタに変換する。
///
/// - 結果数がパス数と一致 → `Some(Ok(Vec))`
/// - 結果数不一致 → `None`（SSH フォールバック）
#[allow(clippy::type_complexity)]
fn transform_stat_results(
    stats: Vec<crate::agent::protocol::AgentFileStat>,
    rel_paths: &[String],
) -> Option<anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>>> {
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

// ── truncation 判定関数 ──

/// ツリースキャンの truncation を検査する。
///
/// `fail_on_truncation` が true ならエラーを返し、false なら warn ログのみで Ok を返す。
pub(crate) fn check_truncation(max_entries: usize, fail_on_truncation: bool) -> anyhow::Result<()> {
    if fail_on_truncation {
        anyhow::bail!(
            "Tree scan truncated at {} entries. Results may be incomplete. \
             Use --max-entries <value> to increase the limit, \
             or set max_scan_entries in config, \
             or specify file paths instead of scanning all.",
            max_entries
        );
    }
    tracing::warn!(
        "Tree scan truncated at {} entries. Results may be incomplete.",
        max_entries
    );
    Ok(())
}

// ── subpath ツリー構築ヘルパー ──

/// スキャン結果のノード群を subpath の階層構造でラップする。
///
/// 例: subpath="app/controllers", nodes=[file_0.php, file_1.php]
/// → [app/ → [controllers/ → [file_0.php, file_1.php]]]
///
/// これにより、返却パスが root_dir からの相対パスになる。
pub(crate) fn wrap_nodes_in_subpath(subpath: &str, nodes: Vec<FileNode>) -> Vec<FileNode> {
    if subpath.is_empty() {
        return nodes;
    }

    let parts: Vec<&str> = subpath.split('/').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return nodes;
    }

    // 最も深い部分から外側に向かってラップしていく
    let mut current = nodes;
    for part in parts.iter().rev() {
        let dir = FileNode::new_dir_with_children(*part, current);
        current = vec![dir];
    }
    current
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
#[cfg(unix)]
fn chmod_local_file(full_path: &Path, mode: u32) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let perms = std::fs::Permissions::from_mode(mode);
    std::fs::set_permissions(full_path, perms)?;
    Ok(())
}

/// Windows ではパーミッション変更は no-op
#[cfg(not(unix))]
fn chmod_local_file(_full_path: &Path, _mode: u32) -> anyhow::Result<()> {
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

    #[cfg(unix)]
    std::os::unix::fs::symlink(target, full_path)?;
    #[cfg(not(unix))]
    {
        // Windows では symlink 作成は未サポート
        anyhow::bail!(
            "Symlink creation is not supported on this platform: {} -> {}",
            full_path.display(),
            target
        );
    }
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
        fail_on_truncation: bool,
    ) -> anyhow::Result<FileTree> {
        self.core
            .fetch_tree_recursive(side, max_entries, fail_on_truncation)
    }

    pub fn fetch_tree_for_subpath(
        &mut self,
        side: &Side,
        subpath: &str,
        max_entries: usize,
        fail_on_truncation: bool,
    ) -> anyhow::Result<FileTree> {
        self.core
            .fetch_tree_for_subpath(side, subpath, max_entries, fail_on_truncation)
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
        let tree = rt.fetch_tree_recursive(&Side::Local, 10000, false).unwrap();

        assert_eq!(tree.root, tmp.path());
        // ノードが2つ以上あること（root.txt, sub/ の少なくとも2つ）
        assert!(tree.nodes.len() >= 2);
    }

    #[test]
    fn test_fetch_tree_recursive_remote_not_connected() {
        let mut rt = CoreRuntime::new_for_test();
        let result =
            rt.fetch_tree_recursive(&Side::Remote("nonexistent".to_string()), 10000, false);
        assert!(result.is_err());
    }

    // ── check_truncation テスト ──

    #[test]
    fn test_check_truncation_fail_on_truncation_returns_error() {
        let result = check_truncation(50_000, true);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("Tree scan truncated at 50000 entries"),
            "expected truncation message, got: {msg}"
        );
        assert!(
            msg.contains("Use --max-entries <value> to increase the limit"),
            "expected guidance about --max-entries, got: {msg}"
        );
    }

    #[test]
    fn test_check_truncation_no_fail_returns_ok() {
        // fail_on_truncation = false → Ok（warn ログのみ）
        let result = check_truncation(50_000, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_truncation_error_message_contains_max_entries() {
        let result = check_truncation(12345, true);
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("12345"),
            "error message should contain max_entries value, got: {msg}"
        );
    }

    #[test]
    fn test_check_truncation_zero_max_entries() {
        // max_entries=0 の境界値: fail_on_truncation=true → エラー
        let result = check_truncation(0, true);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("truncated at 0 entries"),
            "expected '0 entries' in message, got: {msg}"
        );
    }

    #[test]
    fn test_check_truncation_one_max_entry() {
        // max_entries=1 の境界値（最小値）: fail_on_truncation=true → エラー
        let result = check_truncation(1, true);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("truncated at 1 entries"),
            "expected '1 entries' in message, got: {msg}"
        );

        // fail_on_truncation=false → Ok
        let result = check_truncation(1, false);
        assert!(result.is_ok());
    }

    // ── fetch_tree_recursive truncation テスト ──

    #[test]
    fn test_fetch_tree_recursive_local_truncated_fail_on_truncation() {
        // max_entries=1 にして truncation を発生させる
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "a").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "b").unwrap();
        std::fs::write(tmp.path().join("c.txt"), "c").unwrap();

        let mut rt = create_test_runtime(&tmp);
        // fail_on_truncation = true → truncation 時にエラー
        let result = rt.fetch_tree_recursive(&Side::Local, 1, true);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Tree scan truncated"));
        assert!(msg.contains("Use --max-entries"));
    }

    #[test]
    fn test_fetch_tree_recursive_local_truncated_no_fail() {
        // max_entries=1 にして truncation を発生させる
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "a").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "b").unwrap();
        std::fs::write(tmp.path().join("c.txt"), "c").unwrap();

        let mut rt = create_test_runtime(&tmp);
        // fail_on_truncation = false → truncation 時も Ok
        let result = rt.fetch_tree_recursive(&Side::Local, 1, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_fetch_tree_recursive_local_no_truncation_returns_ok_regardless() {
        // ファイル数 < max_entries → truncation なし → どちらのフラグでも Ok
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "a").unwrap();

        let mut rt = create_test_runtime(&tmp);
        // fail_on_truncation = true でも truncation が起きなければ Ok
        let result = rt.fetch_tree_recursive(&Side::Local, 10000, true);
        assert!(result.is_ok());
        // fail_on_truncation = false でも当然 Ok
        let result = rt.fetch_tree_recursive(&Side::Local, 10000, false);
        assert!(result.is_ok());
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

    // ── wrap_nodes_in_subpath テスト ──

    #[test]
    fn test_wrap_nodes_in_subpath_empty_subpath() {
        let nodes = vec![FileNode::new_file("file.txt")];
        let result = wrap_nodes_in_subpath("", nodes.clone());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "file.txt");
    }

    #[test]
    fn test_wrap_nodes_in_subpath_single_level() {
        let nodes = vec![FileNode::new_file("file.txt")];
        let result = wrap_nodes_in_subpath("app", nodes);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "app");
        let children = result[0].children.as_ref().unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "file.txt");
    }

    #[test]
    fn test_wrap_nodes_in_subpath_multi_level() {
        let nodes = vec![
            FileNode::new_file("file_0.php"),
            FileNode::new_file("file_1.php"),
        ];
        let result = wrap_nodes_in_subpath("app/controllers", nodes);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "app");
        let app_children = result[0].children.as_ref().unwrap();
        assert_eq!(app_children.len(), 1);
        assert_eq!(app_children[0].name, "controllers");
        let ctrl_children = app_children[0].children.as_ref().unwrap();
        assert_eq!(ctrl_children.len(), 2);
        assert_eq!(ctrl_children[0].name, "file_0.php");
        assert_eq!(ctrl_children[1].name, "file_1.php");
    }

    #[test]
    fn test_wrap_nodes_in_subpath_empty_nodes() {
        let result = wrap_nodes_in_subpath("app/controllers", vec![]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "app");
        let app_children = result[0].children.as_ref().unwrap();
        assert_eq!(app_children.len(), 1);
        assert_eq!(app_children[0].name, "controllers");
        let ctrl_children = app_children[0].children.as_ref().unwrap();
        assert!(ctrl_children.is_empty());
    }

    // ── fetch_tree_for_subpath テスト ──

    #[test]
    fn test_fetch_tree_for_subpath_local_scans_only_subdirectory() {
        let tmp = TempDir::new().unwrap();
        // root 直下にファイルを作成
        std::fs::write(tmp.path().join("root.txt"), "root").unwrap();
        // サブディレクトリにファイルを作成
        let sub = tmp.path().join("app").join("controllers");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("file_0.php"), "<?php").unwrap();
        std::fs::write(sub.join("file_1.php"), "<?php").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let tree = rt
            .fetch_tree_for_subpath(&Side::Local, "app/controllers", 10000, false)
            .unwrap();

        assert_eq!(tree.root, tmp.path());
        // ツリーには app ノードのみが含まれる（root.txt は含まれない）
        assert_eq!(tree.nodes.len(), 1);
        assert_eq!(tree.nodes[0].name, "app");
        let app_children = tree.nodes[0].children.as_ref().unwrap();
        assert_eq!(app_children.len(), 1);
        assert_eq!(app_children[0].name, "controllers");
        let ctrl_children = app_children[0].children.as_ref().unwrap();
        assert_eq!(ctrl_children.len(), 2);
        let names: Vec<&str> = ctrl_children.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"file_0.php"));
        assert!(names.contains(&"file_1.php"));
    }

    #[test]
    fn test_fetch_tree_for_subpath_local_nonexistent_returns_empty() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("root.txt"), "root").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let tree = rt
            .fetch_tree_for_subpath(&Side::Local, "nonexistent/path", 10000, false)
            .unwrap();

        assert_eq!(tree.root, tmp.path());
        assert!(tree.nodes.is_empty());
    }

    #[test]
    fn test_fetch_tree_for_subpath_local_paths_relative_to_root() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("src").join("main");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("app.rs"), "fn main() {}").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let tree = rt
            .fetch_tree_for_subpath(&Side::Local, "src/main", 10000, false)
            .unwrap();

        // root は root_dir
        assert_eq!(tree.root, tmp.path());
        // ツリー構造: src/ → main/ → app.rs
        assert_eq!(tree.nodes.len(), 1);
        assert_eq!(tree.nodes[0].name, "src");
        let src_children = tree.nodes[0].children.as_ref().unwrap();
        assert_eq!(src_children[0].name, "main");
        let main_children = src_children[0].children.as_ref().unwrap();
        assert_eq!(main_children[0].name, "app.rs");
    }

    #[test]
    fn test_fetch_tree_for_subpath_local_truncation_fail() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("dir");
        std::fs::create_dir_all(&sub).unwrap();
        // 3ファイル作成して max_entries=1 で truncation を発生させる
        std::fs::write(sub.join("a.txt"), "a").unwrap();
        std::fs::write(sub.join("b.txt"), "b").unwrap();
        std::fs::write(sub.join("c.txt"), "c").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let result = rt.fetch_tree_for_subpath(&Side::Local, "dir", 1, true);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Tree scan truncated"));
    }

    #[test]
    fn test_fetch_tree_for_subpath_local_truncation_no_fail() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("dir");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("a.txt"), "a").unwrap();
        std::fs::write(sub.join("b.txt"), "b").unwrap();
        std::fs::write(sub.join("c.txt"), "c").unwrap();

        let mut rt = create_test_runtime(&tmp);
        // fail_on_truncation=false → truncation が発生しても Ok
        let result = rt.fetch_tree_for_subpath(&Side::Local, "dir", 1, false);
        assert!(result.is_ok());
        let tree = result.unwrap();
        // truncation でも部分結果が返る
        assert!(!tree.nodes.is_empty());
    }

    #[test]
    fn test_fetch_tree_for_subpath_trailing_slash_stripped() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("app");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("test.txt"), "content").unwrap();

        let mut rt = create_test_runtime(&tmp);
        // 末尾スラッシュがあっても正常動作する
        let tree = rt
            .fetch_tree_for_subpath(&Side::Local, "app/", 10000, false)
            .unwrap();

        assert_eq!(tree.nodes.len(), 1);
        assert_eq!(tree.nodes[0].name, "app");
    }

    #[test]
    fn test_fetch_tree_for_subpath_path_traversal_rejected() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);
        let result = rt.fetch_tree_for_subpath(&Side::Local, "../outside", 10000, false);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("path traversal not allowed"));
    }

    #[test]
    fn test_fetch_tree_for_subpath_file_as_subpath_returns_empty() {
        // subpath がファイルを指す場合は空ツリー（ディレクトリではない）
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("file.txt"), "content").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let tree = rt
            .fetch_tree_for_subpath(&Side::Local, "file.txt", 10000, false)
            .unwrap();

        assert!(tree.nodes.is_empty());
    }

    // ── should_invalidate_agent_error テスト ──

    #[test]
    fn test_should_invalidate_broken_pipe() {
        // BrokenPipe → invalidate すべき
        let io_err = io::Error::new(io::ErrorKind::BrokenPipe, "broken pipe");
        let e = anyhow::Error::from(io_err);
        assert!(should_invalidate_agent_error(&e));
    }

    #[test]
    fn test_should_invalidate_connection_reset() {
        // ConnectionReset → invalidate すべき
        let io_err = io::Error::new(io::ErrorKind::ConnectionReset, "connection reset");
        let e = anyhow::Error::from(io_err);
        assert!(should_invalidate_agent_error(&e));
    }

    #[test]
    fn test_should_invalidate_connection_aborted() {
        // ConnectionAborted → invalidate すべき
        let io_err = io::Error::new(io::ErrorKind::ConnectionAborted, "connection aborted");
        let e = anyhow::Error::from(io_err);
        assert!(should_invalidate_agent_error(&e));
    }

    #[test]
    fn test_should_not_invalidate_non_fatal_io_error() {
        // WouldBlock など → invalidate しない（一時的エラー）
        let io_err = io::Error::new(io::ErrorKind::WouldBlock, "would block");
        let e = anyhow::Error::from(io_err);
        assert!(!should_invalidate_agent_error(&e));
    }

    #[test]
    fn test_should_invalidate_unknown_error() {
        // io::Error 以外の不明エラー → 安全側で invalidate
        let e = anyhow::anyhow!("some unknown error");
        assert!(should_invalidate_agent_error(&e));
    }

    #[test]
    fn test_should_invalidate_anyhow_wrapped_non_io_error() {
        // anyhow でラップした文字列エラー → 不明扱いで invalidate
        let e = anyhow::Error::msg("protocol error: unexpected frame");
        assert!(should_invalidate_agent_error(&e));
    }

    #[test]
    fn test_should_invalidate_unexpected_eof() {
        // UnexpectedEof → invalidate すべき（Agent プロセスのクラッシュ）
        let io_err = io::Error::new(io::ErrorKind::UnexpectedEof, "unexpected eof");
        let e = anyhow::Error::from(io_err);
        assert!(should_invalidate_agent_error(&e));
    }

    #[test]
    fn test_should_invalidate_chained_io_error() {
        // anyhow チェーンの内部に io::Error がある場合でも検出できること
        let io_err = io::Error::new(io::ErrorKind::BrokenPipe, "broken pipe");
        let inner = anyhow::Error::from(io_err);
        let outer = inner.context("agent operation failed");
        assert!(should_invalidate_agent_error(&outer));
    }

    #[test]
    fn test_should_not_invalidate_chained_non_fatal_io_error() {
        // チェーン内部の非致命的 io::Error は invalidate しない
        let io_err = io::Error::new(io::ErrorKind::TimedOut, "timed out");
        let inner = anyhow::Error::from(io_err);
        let outer = inner.context("agent operation timed out");
        assert!(!should_invalidate_agent_error(&outer));
    }

    // ── with_agent のキャッシュ動作（間接テスト）──

    #[test]
    fn test_write_file_remote_no_agent_no_ssh_returns_error() {
        // Agent なし + SSH 未接続 のリモートサーバーへの書き込み → エラー
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);
        rt.config.servers.insert(
            "develop".to_string(),
            crate::config::ServerConfig {
                host: "10.0.0.1".to_string(),
                port: 22,
                user: "deploy".to_string(),
                auth: crate::config::AuthMethod::Key,
                key: None,
                root_dir: tmp.path().to_path_buf(),
                ssh_options: None,
                sudo: false,
                file_permissions: None,
                dir_permissions: None,
            },
        );
        // Agent なし → try_agent_write_file は None → check_sudo_fallback 通過 → SSH 未接続でエラー
        let result = rt.write_file(&Side::Remote("develop".to_string()), "test.txt", "content");
        assert!(result.is_err());
    }

    // ── extract_single_file_as_string テスト ──

    #[test]
    fn test_extract_single_file_as_string_ok() {
        let results = vec![FileReadResult::Ok {
            path: "test.txt".to_string(),
            content: b"hello world".to_vec(),
            more_to_follow: false,
        }];
        let result = extract_single_file_as_string(results);
        assert!(result.is_some());
        assert_eq!(result.unwrap().unwrap(), "hello world");
    }

    #[test]
    fn test_extract_single_file_as_string_error_returns_none() {
        let results = vec![FileReadResult::Error {
            path: "test.txt".to_string(),
            message: "not found".to_string(),
        }];
        let result = extract_single_file_as_string(results);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_single_file_as_string_empty_returns_none() {
        let result = extract_single_file_as_string(vec![]);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_single_file_as_string_invalid_utf8() {
        let results = vec![FileReadResult::Ok {
            path: "bad.txt".to_string(),
            content: vec![0xFF, 0xFE, 0x00, 0x80],
            more_to_follow: false,
        }];
        let result = extract_single_file_as_string(results);
        assert!(result.is_some());
        assert!(result.unwrap().is_err()); // UTF-8 変換エラー
    }

    // ── extract_single_file_as_bytes テスト ──

    #[test]
    fn test_extract_single_file_as_bytes_ok() {
        let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let results = vec![FileReadResult::Ok {
            path: "data.bin".to_string(),
            content: data.clone(),
            more_to_follow: false,
        }];
        let result = extract_single_file_as_bytes(results);
        assert_eq!(result.unwrap().unwrap(), data);
    }

    #[test]
    fn test_extract_single_file_as_bytes_error_returns_none() {
        let results = vec![FileReadResult::Error {
            path: "data.bin".to_string(),
            message: "permission denied".to_string(),
        }];
        assert!(extract_single_file_as_bytes(results).is_none());
    }

    // ── extract_batch_files_as_string テスト ──

    #[test]
    fn test_extract_batch_files_as_string_all_ok() {
        let results = vec![
            FileReadResult::Ok {
                path: "a.txt".to_string(),
                content: b"aaa".to_vec(),
                more_to_follow: false,
            },
            FileReadResult::Ok {
                path: "b.txt".to_string(),
                content: b"bbb".to_vec(),
                more_to_follow: false,
            },
        ];
        let paths = vec!["a.txt".to_string(), "b.txt".to_string()];
        let result = extract_batch_files_as_string(results, &paths);
        let map = result.unwrap().unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(map["a.txt"], "aaa");
        assert_eq!(map["b.txt"], "bbb");
    }

    #[test]
    fn test_extract_batch_files_as_string_with_error_returns_none() {
        let results = vec![
            FileReadResult::Ok {
                path: "a.txt".to_string(),
                content: b"aaa".to_vec(),
                more_to_follow: false,
            },
            FileReadResult::Error {
                path: "b.txt".to_string(),
                message: "not found".to_string(),
            },
        ];
        let paths = vec!["a.txt".to_string(), "b.txt".to_string()];
        assert!(extract_batch_files_as_string(results, &paths).is_none());
    }

    #[test]
    fn test_extract_batch_files_as_string_utf8_error() {
        let results = vec![FileReadResult::Ok {
            path: "bad.txt".to_string(),
            content: vec![0xFF, 0xFE],
            more_to_follow: false,
        }];
        let paths = vec!["bad.txt".to_string()];
        let result = extract_batch_files_as_string(results, &paths);
        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    // ── extract_batch_files_as_bytes テスト ──

    #[test]
    fn test_extract_batch_files_as_bytes_all_ok() {
        let results = vec![
            FileReadResult::Ok {
                path: "a.bin".to_string(),
                content: vec![0x01],
                more_to_follow: false,
            },
            FileReadResult::Ok {
                path: "b.bin".to_string(),
                content: vec![0x02],
                more_to_follow: false,
            },
        ];
        let paths = vec!["a.bin".to_string(), "b.bin".to_string()];
        let map = extract_batch_files_as_bytes(results, &paths)
            .unwrap()
            .unwrap();
        assert_eq!(map["a.bin"], vec![0x01]);
        assert_eq!(map["b.bin"], vec![0x02]);
    }

    #[test]
    fn test_extract_batch_files_as_bytes_error_returns_none() {
        let results = vec![FileReadResult::Error {
            path: "a.bin".to_string(),
            message: "read failed".to_string(),
        }];
        let paths = vec!["a.bin".to_string()];
        assert!(extract_batch_files_as_bytes(results, &paths).is_none());
    }

    // ── flatten_agent_read_result テスト ──

    #[test]
    fn test_flatten_agent_read_result_none_passthrough() {
        let result: Option<anyhow::Result<String>> =
            flatten_agent_read_result(None, extract_single_file_as_string);
        assert!(result.is_none());
    }

    #[test]
    fn test_flatten_agent_read_result_some_err_passthrough() {
        let err = Some(Err(anyhow::anyhow!("agent error")));
        let result: Option<anyhow::Result<String>> =
            flatten_agent_read_result(err, extract_single_file_as_string);
        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    #[test]
    fn test_flatten_agent_read_result_some_ok_transforms() {
        let ok = Some(Ok(vec![FileReadResult::Ok {
            path: "x.txt".to_string(),
            content: b"content".to_vec(),
            more_to_follow: false,
        }]));
        let result: Option<anyhow::Result<String>> =
            flatten_agent_read_result(ok, extract_single_file_as_string);
        assert_eq!(result.unwrap().unwrap(), "content");
    }

    // ── transform_stat_results テスト ──

    #[test]
    fn test_transform_stat_results_matching_count() {
        use crate::agent::protocol::AgentFileStat;
        let stats = vec![
            AgentFileStat {
                path: "a.txt".to_string(),
                mtime_secs: 1000000,
                mtime_nanos: 0,
                size: 100,
                permissions: 0o644,
            },
            AgentFileStat {
                path: "b.txt".to_string(),
                mtime_secs: 2000000,
                mtime_nanos: 0,
                size: 200,
                permissions: 0o755,
            },
        ];
        let paths = vec!["a.txt".to_string(), "b.txt".to_string()];
        let result = transform_stat_results(stats, &paths);
        let vec = result.unwrap().unwrap();
        assert_eq!(vec.len(), 2);
        assert_eq!(vec[0].0, "a.txt");
        assert!(vec[0].1.is_some());
        assert_eq!(vec[1].0, "b.txt");
        assert!(vec[1].1.is_some());
    }

    #[test]
    fn test_transform_stat_results_count_mismatch_returns_none() {
        use crate::agent::protocol::AgentFileStat;
        let stats = vec![AgentFileStat {
            path: "a.txt".to_string(),
            mtime_secs: 1000000,
            mtime_nanos: 0,
            size: 100,
            permissions: 0o644,
        }];
        // 2パスに対して1結果 → None
        let paths = vec!["a.txt".to_string(), "b.txt".to_string()];
        assert!(transform_stat_results(stats, &paths).is_none());
    }

    #[test]
    fn test_transform_stat_results_empty() {
        let result = transform_stat_results(vec![], &[]);
        let vec = result.unwrap().unwrap();
        assert!(vec.is_empty());
    }

    // ── fetch_children include フィルター ──

    #[test]
    fn test_fetch_children_local_no_include_returns_all() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("parent");
        std::fs::create_dir(&sub).unwrap();
        std::fs::create_dir(sub.join("alpha")).unwrap();
        std::fs::create_dir(sub.join("beta")).unwrap();
        std::fs::write(sub.join("file.txt"), "x").unwrap();

        let mut rt = create_test_runtime(&tmp);
        // include 空 → 全 children 表示
        let children = rt.fetch_children(&Side::Local, "parent").unwrap();
        assert_eq!(children.len(), 3);
    }

    #[test]
    fn test_fetch_children_local_include_filters_children() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("parent");
        std::fs::create_dir(&sub).unwrap();
        std::fs::create_dir(sub.join("alpha")).unwrap();
        std::fs::create_dir(sub.join("beta")).unwrap();
        std::fs::write(sub.join("file.txt"), "x").unwrap();

        let mut rt = create_test_runtime(&tmp);
        // include = ["parent/alpha"] → parent 展開時は alpha のみ表示
        rt.config.filter.include = vec!["parent/alpha".to_string()];
        let children = rt.fetch_children(&Side::Local, "parent").unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "alpha");
    }

    #[test]
    fn test_fetch_children_local_include_descendant_shows_all() {
        let tmp = TempDir::new().unwrap();
        // include = ["src"] で "src" 配下を展開すると全 children が表示される
        let src = tmp.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("main.rs"), "fn main(){}").unwrap();
        std::fs::write(src.join("lib.rs"), "").unwrap();

        let mut rt = create_test_runtime(&tmp);
        rt.config.filter.include = vec!["src".to_string()];
        let children = rt.fetch_children(&Side::Local, "src").unwrap();
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn test_fetch_children_root_with_include() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::create_dir(tmp.path().join("docs")).unwrap();
        std::fs::create_dir(tmp.path().join("tests")).unwrap();

        let mut rt = create_test_runtime(&tmp);
        rt.config.filter.include = vec!["src".to_string()];
        // ルート直下（dir_rel_path = ""）の展開で include フィルタ
        let children = rt.fetch_children(&Side::Local, "").unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "src");
    }
}
