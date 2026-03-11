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
    pub fn create_backups(&mut self, side: &Side, rel_paths: &[String]) -> anyhow::Result<()> {
        match side {
            Side::Local => {
                let root = &self.config.local.root_dir;
                for rel_path in rel_paths {
                    let full = root.join(rel_path);
                    executor::validate_path_within_root(root, &full)?;
                }
                create_local_backups(root, rel_paths)?;
                Ok(())
            }
            Side::Remote(name) => {
                if let Some(result) = self.try_agent_backup(name, rel_paths) {
                    return result;
                }
                self.create_remote_backups(name, rel_paths)
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
    ) -> Option<anyhow::Result<()>> {
        let full_paths = self.resolve_agent_paths(server_name, rel_paths)?;
        let remote_root = self
            .config
            .servers
            .get(server_name)
            .map(|s| s.root_dir.to_string_lossy().to_string())?;
        let backup_dir = format!(
            "{}/{}",
            remote_root.trim_end_matches('/'),
            crate::backup::BACKUP_DIR_NAME
        );
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

/// ローカルファイルのバックアップを作成する
fn create_local_backups(root_dir: &Path, rel_paths: &[String]) -> anyhow::Result<()> {
    let backup_dir = root_dir.join(crate::backup::BACKUP_DIR_NAME);
    for rel_path in rel_paths {
        crate::backup::create_local_backup(root_dir, rel_path, &backup_dir)?;
    }
    Ok(())
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

    pub fn create_backups(&mut self, side: &Side, rel_paths: &[String]) -> anyhow::Result<()> {
        self.core.create_backups(side, rel_paths)
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
}
