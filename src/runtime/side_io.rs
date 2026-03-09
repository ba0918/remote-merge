//! Side ベースの統一 I/O API。
//!
//! `Side::Local` と `Side::Remote(name)` を透過的に扱い、
//! swap 後に right=local になっても同じ API でアクセスできるようにする。

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};

use crate::app::Side;
use crate::local;
use crate::merge::executor;
use crate::tree::{FileNode, FileTree};

use super::core::CoreRuntime;
use super::TuiRuntime;

// ── CoreRuntime に Side ベース統一 I/O を実装 ──

impl CoreRuntime {
    // ── 読み込み ──

    /// Side に基づいてファイルを読み込む
    pub fn read_file(&mut self, side: &Side, rel_path: &str) -> anyhow::Result<String> {
        match side {
            Side::Local => {
                // バリデーションは executor::read_local_file 内部で行われる
                executor::read_local_file(&self.config.local.root_dir, rel_path)
            }
            Side::Remote(name) => self.read_remote_file(name, rel_path),
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
                    // バリデーションは executor::read_local_file 内部で行われる
                    let content = executor::read_local_file(&self.config.local.root_dir, rel_path)?;
                    result.insert(rel_path.clone(), content);
                }
                Ok(result)
            }
            Side::Remote(name) => self.read_remote_files_batch(name, rel_paths),
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
            Side::Remote(name) => self.read_remote_file_bytes(name, rel_path, force),
        }
    }

    // ── 書き込み ──

    /// Side に基づいてファイルを書き込む
    pub fn write_file(&mut self, side: &Side, rel_path: &str, content: &str) -> anyhow::Result<()> {
        match side {
            Side::Local => {
                // バリデーションは executor::write_local_file 内部で行われる
                executor::write_local_file(&self.config.local.root_dir, rel_path, content)
            }
            Side::Remote(name) => self.write_remote_file(name, rel_path, content),
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
            Side::Remote(name) => self.write_remote_file_bytes(name, rel_path, content),
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
                // パストラバーサルチェック
                for rel_path in rel_paths {
                    let full = root.join(rel_path);
                    executor::validate_path_within_root(root, &full)?;
                }
                stat_local_files(root, rel_paths)
            }
            Side::Remote(name) => self.stat_remote_files(name, rel_paths),
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
            Side::Remote(name) => self.chmod_remote_file(name, rel_path, mode),
        }
    }

    // ── バックアップ ──

    /// Side に基づいてバックアップを作成する
    pub fn create_backups(&mut self, side: &Side, rel_paths: &[String]) -> anyhow::Result<()> {
        match side {
            Side::Local => {
                let root = &self.config.local.root_dir;
                // パストラバーサルチェック
                for rel_path in rel_paths {
                    let full = root.join(rel_path);
                    executor::validate_path_within_root(root, &full)?;
                }
                create_local_backups(root, rel_paths)?;
                Ok(())
            }
            Side::Remote(name) => self.create_remote_backups(name, rel_paths),
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
            Side::Remote(name) => self.create_remote_symlink(name, rel_path, target),
        }
    }

    // ── ツリー ──

    /// Side に基づいてファイルツリーを取得する（1階層のみ）
    pub fn fetch_tree(&mut self, side: &Side) -> anyhow::Result<FileTree> {
        match side {
            Side::Local => {
                local::scan_local_tree(&self.config.local.root_dir, &self.config.filter.exclude)
            }
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
            Side::Remote(name) => self.fetch_remote_tree_recursive(name, max_entries),
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
    fn test_write_file_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);

        rt.write_file(&Side::Local, "a/b/c/deep.txt", "deep content")
            .unwrap();

        let content = std::fs::read_to_string(tmp.path().join("a/b/c/deep.txt")).unwrap();
        assert_eq!(content, "deep content");
    }
}
