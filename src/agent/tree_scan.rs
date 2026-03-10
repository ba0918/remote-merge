//! リモートサーバー上でディレクトリツリーを再帰走査し、
//! `AgentFileEntry` のチャンクを生成するモジュール。
//!
//! `std::fs` を使用（tokio 不使用）。シンボリックリンクはフォローせず、
//! ターゲットパスのみを記録する。

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Result;

use super::protocol::{AgentFileEntry, FileKind};
use crate::filter;

/// ツリー走査のオプション
#[derive(Debug)]
pub struct ScanOptions {
    pub root: PathBuf,
    pub exclude: Vec<String>,
    pub max_entries: usize,
    pub chunk_size: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            exclude: Vec::new(),
            max_entries: usize::MAX,
            chunk_size: 1000,
        }
    }
}

/// 走査結果のチャンク。
/// 注意: 各チャンク内のエントリはパスでソート済みだが、
/// チャンク境界をまたぐグローバルなソート順は保証しない。
#[derive(Debug, PartialEq)]
pub struct ScanChunk {
    pub entries: Vec<AgentFileEntry>,
    pub is_last: bool,
    pub total_scanned: usize,
}

/// ディレクトリツリーを再帰走査し、チャンク単位で結果を返すイテレータ
pub fn scan_tree(options: &ScanOptions) -> impl Iterator<Item = Result<ScanChunk>> + '_ {
    ScanIterator::new(options)
}

// ---------------------------------------------------------------------------
// Internal iterator
// ---------------------------------------------------------------------------

struct ScanIterator<'a> {
    options: &'a ScanOptions,
    /// 未処理のディレクトリスタック
    dir_stack: Vec<PathBuf>,
    /// 現在読み込み中のディレクトリイテレータ
    current_read_dir: Option<std::fs::ReadDir>,
    /// 現在のチャンクに蓄積中のエントリ
    buffer: Vec<AgentFileEntry>,
    /// 走査済みエントリ総数
    total_scanned: usize,
    /// 走査完了フラグ
    finished: bool,
}

impl<'a> ScanIterator<'a> {
    fn new(options: &'a ScanOptions) -> Self {
        Self {
            options,
            dir_stack: vec![options.root.clone()],
            current_read_dir: None,
            buffer: Vec::new(),
            total_scanned: 0,
            finished: false,
        }
    }

    /// 次のエントリを1つ取得して処理する。
    /// ディレクトリスタックと current_read_dir を使って状態を維持する。
    /// None を返したら走査終了。
    fn advance_one(&mut self) -> Option<()> {
        loop {
            // current_read_dir を一旦 take して借用を分離する
            if let Some(mut rd) = self.current_read_dir.take() {
                let result = Self::next_valid_path(&mut rd, &self.options.exclude);
                match result {
                    Some(path) => {
                        // イテレータを戻す
                        self.current_read_dir = Some(rd);
                        if self.process_entry(&path) {
                            return Some(());
                        }
                        // 除外された場合は再度ループ
                        continue;
                    }
                    None => {
                        // このディレクトリは読み終わった（current_read_dir は take 済み）
                    }
                }
            }

            // 次のディレクトリをスタックからポップ
            let dir = self.dir_stack.pop()?;
            match std::fs::read_dir(&dir) {
                Ok(rd) => self.current_read_dir = Some(rd),
                Err(e) => {
                    tracing::warn!("cannot read directory {}: {e}", dir.display());
                    continue;
                }
            }
        }
    }

    /// ReadDir から次の有効なパスを取得する（セグメント除外を適用）
    fn next_valid_path(rd: &mut std::fs::ReadDir, exclude: &[String]) -> Option<PathBuf> {
        for entry_result in rd.by_ref() {
            let entry = match entry_result {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("error reading directory entry: {e}");
                    continue;
                }
            };
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if filter::should_exclude(&name_str, exclude) {
                continue;
            }
            return Some(entry.path());
        }
        None
    }

    /// 1エントリを処理してバッファに追加する。
    /// 追加した場合 true、除外等でスキップした場合 false を返す。
    fn process_entry(&mut self, path: &Path) -> bool {
        let meta = match path.symlink_metadata() {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("cannot read metadata for {}: {e}", path.display());
                return false;
            }
        };

        let rel = match path.strip_prefix(&self.options.root) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => return false,
        };

        // 除外判定
        if filter::is_path_excluded(&rel, &self.options.exclude) {
            return false;
        }

        let file_type = meta.file_type();
        let (kind, symlink_target) = if file_type.is_symlink() {
            let target = std::fs::read_link(path)
                .ok()
                .map(|t| t.to_string_lossy().replace('\\', "/"));
            (FileKind::Symlink, target)
        } else if file_type.is_dir() {
            self.dir_stack.push(path.to_path_buf());
            (FileKind::Directory, None)
        } else {
            (FileKind::File, None)
        };

        let size = if kind == FileKind::File {
            meta.len()
        } else {
            0
        };

        let (mtime_secs, mtime_nanos) = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| (d.as_secs() as i64, d.subsec_nanos()))
            .unwrap_or((0, 0));

        let permissions = get_permissions(&meta, &kind);

        self.buffer.push(AgentFileEntry {
            path: rel,
            kind,
            size,
            mtime_secs,
            mtime_nanos,
            permissions,
            symlink_target,
        });
        self.total_scanned += 1;
        true
    }

    /// バッファの中身をソートしてチャンクとして切り出す
    fn flush_chunk(&mut self, is_last: bool) -> ScanChunk {
        self.buffer.sort_by(|a, b| a.path.cmp(&b.path));
        ScanChunk {
            entries: std::mem::take(&mut self.buffer),
            is_last,
            total_scanned: self.total_scanned,
        }
    }
}

impl Iterator for ScanIterator<'_> {
    type Item = Result<ScanChunk>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        loop {
            // max_entries に達したら終了
            if self.total_scanned >= self.options.max_entries {
                self.finished = true;
                return Some(Ok(self.flush_chunk(true)));
            }

            // 次のエントリを取得
            if self.advance_one().is_none() {
                // 走査終了
                self.finished = true;
                return Some(Ok(self.flush_chunk(true)));
            }

            // チャンクサイズに達したら中間チャンクを返す
            if self.buffer.len() >= self.options.chunk_size {
                return Some(Ok(self.flush_chunk(false)));
            }
        }
    }
}

/// メタデータからパーミッションビットを取得する。
/// Unix 以外では File=0o644, Directory=0o755 のデフォルト値を返す。
#[cfg(unix)]
fn get_permissions(meta: &std::fs::Metadata, _kind: &FileKind) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode()
}

#[cfg(not(unix))]
fn get_permissions(_meta: &std::fs::Metadata, kind: &FileKind) -> u32 {
    match kind {
        FileKind::Directory => 0o755,
        _ => 0o644,
    }
}

// ---------------------------------------------------------------------------
// Conversion: AgentFileEntry → FileNode
// ---------------------------------------------------------------------------

/// Agent から受信した `AgentFileEntry` のリストを `FileNode` のフラットリストに変換する。
///
/// エントリのパスは相対パス（"dir/file.txt" 形式）を想定する。
/// 結果はフラット（1階層）で返され、ツリー構造の構築は呼び出し元の責務。
pub fn convert_agent_entries_to_nodes(entries: &[AgentFileEntry]) -> Vec<crate::tree::FileNode> {
    use crate::tree::{FileNode, NodeKind};
    use chrono::DateTime;

    entries
        .iter()
        .map(|entry| {
            let kind = match entry.kind {
                FileKind::File => NodeKind::File,
                FileKind::Directory => NodeKind::Directory,
                FileKind::Symlink => NodeKind::Symlink {
                    target: entry.symlink_target.clone().unwrap_or_default(),
                },
            };

            let mtime = DateTime::from_timestamp(entry.mtime_secs, entry.mtime_nanos);

            // パスの最後のセグメントを name にする
            let name = entry
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&entry.path)
                .to_string();

            FileNode {
                name,
                kind,
                size: Some(entry.size),
                mtime,
                permissions: Some(entry.permissions),
                children: None, // 遅延読み込み（ディレクトリ含む）
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// テスト用のディレクトリ構造を作成するヘルパー
    fn create_test_tree() -> TempDir {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // ファイル
        fs::write(root.join("file1.txt"), "hello").unwrap();
        fs::write(root.join("file2.rs"), "fn main() {}").unwrap();

        // サブディレクトリ + ファイル
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub/nested.txt"), "nested").unwrap();

        // 深いネスト
        fs::create_dir_all(root.join("sub/deep")).unwrap();
        fs::write(root.join("sub/deep/leaf.txt"), "leaf").unwrap();

        dir
    }

    #[test]
    fn basic_scan() {
        let dir = create_test_tree();
        let options = ScanOptions {
            root: dir.path().to_path_buf(),
            chunk_size: 1000,
            ..Default::default()
        };

        let chunks: Vec<_> = scan_tree(&options).collect::<Result<Vec<_>>>().unwrap();
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].is_last);

        let all_paths: Vec<&str> = chunks[0].entries.iter().map(|e| e.path.as_str()).collect();
        assert!(all_paths.contains(&"file1.txt"));
        assert!(all_paths.contains(&"file2.rs"));
        assert!(all_paths.contains(&"sub"));
        assert!(all_paths.contains(&"sub/nested.txt"));
        assert!(all_paths.contains(&"sub/deep"));
        assert!(all_paths.contains(&"sub/deep/leaf.txt"));
    }

    #[test]
    fn paths_are_relative_forward_slash() {
        let dir = create_test_tree();
        let options = ScanOptions {
            root: dir.path().to_path_buf(),
            chunk_size: 1000,
            ..Default::default()
        };

        let chunks: Vec<_> = scan_tree(&options).collect::<Result<Vec<_>>>().unwrap();
        for entry in &chunks[0].entries {
            assert!(
                !entry.path.starts_with('/'),
                "path should be relative: {}",
                entry.path
            );
            assert!(
                !entry.path.contains('\\'),
                "path should use forward slashes: {}",
                entry.path
            );
        }
    }

    #[test]
    fn exclude_segment_pattern() {
        let dir = create_test_tree();
        // node_modules 風のディレクトリを追加
        fs::create_dir(dir.path().join("node_modules")).unwrap();
        fs::write(dir.path().join("node_modules/pkg.js"), "js").unwrap();

        let options = ScanOptions {
            root: dir.path().to_path_buf(),
            exclude: vec!["node_modules".to_string()],
            chunk_size: 1000,
            ..Default::default()
        };

        let chunks: Vec<_> = scan_tree(&options).collect::<Result<Vec<_>>>().unwrap();
        let all_paths: Vec<&str> = chunks[0].entries.iter().map(|e| e.path.as_str()).collect();
        assert!(!all_paths.iter().any(|p| p.contains("node_modules")));
    }

    #[test]
    fn exclude_path_pattern() {
        let dir = create_test_tree();
        let options = ScanOptions {
            root: dir.path().to_path_buf(),
            exclude: vec!["sub/deep/**".to_string()],
            chunk_size: 1000,
            ..Default::default()
        };

        let chunks: Vec<_> = scan_tree(&options).collect::<Result<Vec<_>>>().unwrap();
        let all_paths: Vec<&str> = chunks
            .iter()
            .flat_map(|c| c.entries.iter().map(|e| e.path.as_str()))
            .collect();
        // sub/deep 自体と配下が除外される
        assert!(!all_paths.contains(&"sub/deep/leaf.txt"));
        // sub/nested.txt は残る
        assert!(all_paths.contains(&"sub/nested.txt"));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_handling() {
        let dir = create_test_tree();
        let root = dir.path();
        std::os::unix::fs::symlink("file1.txt", root.join("link")).unwrap();

        let options = ScanOptions {
            root: root.to_path_buf(),
            chunk_size: 1000,
            ..Default::default()
        };

        let chunks: Vec<_> = scan_tree(&options).collect::<Result<Vec<_>>>().unwrap();
        let link_entry = chunks[0]
            .entries
            .iter()
            .find(|e| e.path == "link")
            .expect("symlink entry should exist");

        assert_eq!(link_entry.kind, FileKind::Symlink);
        assert_eq!(link_entry.size, 0);
        assert_eq!(link_entry.symlink_target.as_deref(), Some("file1.txt"));
    }

    #[test]
    fn max_entries_respected() {
        let dir = create_test_tree();
        let options = ScanOptions {
            root: dir.path().to_path_buf(),
            max_entries: 2,
            chunk_size: 1000,
            ..Default::default()
        };

        let chunks: Vec<_> = scan_tree(&options).collect::<Result<Vec<_>>>().unwrap();
        let total: usize = chunks.iter().map(|c| c.entries.len()).sum();
        assert!(total <= 2, "should have at most 2 entries, got {total}");
        assert!(chunks.last().unwrap().is_last);
    }

    #[test]
    fn chunking_works() {
        let dir = create_test_tree();
        // create_test_tree: file1.txt, file2.rs, sub, sub/nested.txt, sub/deep, sub/deep/leaf.txt = 6 entries
        let options = ScanOptions {
            root: dir.path().to_path_buf(),
            chunk_size: 2,
            ..Default::default()
        };

        let chunks: Vec<_> = scan_tree(&options).collect::<Result<Vec<_>>>().unwrap();
        // chunk_size=2 で 6 エントリ → 3チャンク（2+2+2）または 4チャンク等
        assert!(
            chunks.len() >= 2,
            "expected multiple chunks, got {}",
            chunks.len()
        );
        // 最後のチャンク以外は is_last=false
        for (i, chunk) in chunks.iter().enumerate() {
            if i < chunks.len() - 1 {
                assert!(!chunk.is_last, "intermediate chunk should not be last");
            }
        }
        assert!(chunks.last().unwrap().is_last);

        // 全エントリが揃っていること
        let total: usize = chunks.iter().map(|c| c.entries.len()).sum();
        assert_eq!(total, 6);
    }

    #[test]
    fn empty_directory() {
        let dir = TempDir::new().unwrap();
        let options = ScanOptions {
            root: dir.path().to_path_buf(),
            chunk_size: 1000,
            ..Default::default()
        };

        let chunks: Vec<_> = scan_tree(&options).collect::<Result<Vec<_>>>().unwrap();
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].entries.is_empty());
        assert!(chunks[0].is_last);
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_entry_skipped() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(root.join("ok.txt"), "ok").unwrap();
        fs::create_dir(root.join("noperm")).unwrap();
        fs::write(root.join("noperm/secret.txt"), "secret").unwrap();
        // ディレクトリのパーミッションを 0o000 に設定（読めなくする）
        fs::set_permissions(root.join("noperm"), fs::Permissions::from_mode(0o000)).unwrap();

        let options = ScanOptions {
            root: root.to_path_buf(),
            chunk_size: 1000,
            ..Default::default()
        };

        let chunks: Vec<_> = scan_tree(&options).collect::<Result<Vec<_>>>().unwrap();
        let all_paths: Vec<&str> = chunks
            .iter()
            .flat_map(|c| c.entries.iter().map(|e| e.path.as_str()))
            .collect();

        // ok.txt は含まれる
        assert!(all_paths.contains(&"ok.txt"));
        // secret.txt は読めないので含まれない
        assert!(!all_paths.contains(&"noperm/secret.txt"));

        // クリーンアップ: パーミッション戻す
        fs::set_permissions(root.join("noperm"), fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn entries_sorted_within_chunk() {
        let dir = create_test_tree();
        let options = ScanOptions {
            root: dir.path().to_path_buf(),
            chunk_size: 1000,
            ..Default::default()
        };

        let chunks: Vec<_> = scan_tree(&options).collect::<Result<Vec<_>>>().unwrap();
        for chunk in &chunks {
            let paths: Vec<&str> = chunk.entries.iter().map(|e| e.path.as_str()).collect();
            let mut sorted = paths.clone();
            sorted.sort();
            assert_eq!(paths, sorted, "entries should be sorted by path");
        }
    }

    #[test]
    fn file_metadata_populated() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.txt"), "12345").unwrap();

        let options = ScanOptions {
            root: dir.path().to_path_buf(),
            chunk_size: 1000,
            ..Default::default()
        };

        let chunks: Vec<_> = scan_tree(&options).collect::<Result<Vec<_>>>().unwrap();
        let entry = &chunks[0].entries[0];

        assert_eq!(entry.path, "test.txt");
        assert_eq!(entry.kind, FileKind::File);
        assert_eq!(entry.size, 5);
        assert!(entry.mtime_secs > 0, "mtime should be set");
        assert!(entry.permissions > 0, "permissions should be set");
        assert!(entry.symlink_target.is_none());
    }

    // ── convert_agent_entries_to_nodes テスト ──

    #[test]
    fn convert_empty_entries() {
        let nodes = convert_agent_entries_to_nodes(&[]);
        assert!(nodes.is_empty());
    }

    #[test]
    fn convert_file_entry() {
        use crate::tree::NodeKind;

        let entries = vec![AgentFileEntry {
            path: "src/main.rs".to_string(),
            kind: FileKind::File,
            size: 1024,
            mtime_secs: 1700000000,
            mtime_nanos: 500,
            permissions: 0o644,
            symlink_target: None,
        }];

        let nodes = convert_agent_entries_to_nodes(&entries);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "main.rs");
        assert!(matches!(nodes[0].kind, NodeKind::File));
        assert_eq!(nodes[0].size, Some(1024));
        assert!(nodes[0].mtime.is_some());
        assert_eq!(nodes[0].permissions, Some(0o644));
    }

    #[test]
    fn convert_directory_entry() {
        use crate::tree::NodeKind;

        let entries = vec![AgentFileEntry {
            path: "src".to_string(),
            kind: FileKind::Directory,
            size: 0,
            mtime_secs: 1700000000,
            mtime_nanos: 0,
            permissions: 0o755,
            symlink_target: None,
        }];

        let nodes = convert_agent_entries_to_nodes(&entries);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "src");
        assert!(matches!(nodes[0].kind, NodeKind::Directory));
    }

    #[test]
    fn convert_symlink_entry() {
        use crate::tree::NodeKind;

        let entries = vec![AgentFileEntry {
            path: "current".to_string(),
            kind: FileKind::Symlink,
            size: 0,
            mtime_secs: 1700000000,
            mtime_nanos: 0,
            permissions: 0o777,
            symlink_target: Some("releases/v2".to_string()),
        }];

        let nodes = convert_agent_entries_to_nodes(&entries);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "current");
        match &nodes[0].kind {
            NodeKind::Symlink { target } => assert_eq!(target, "releases/v2"),
            other => panic!("expected Symlink, got {:?}", other),
        }
    }

    #[test]
    fn convert_mixed_entries() {
        let entries = vec![
            AgentFileEntry {
                path: "file.txt".to_string(),
                kind: FileKind::File,
                size: 100,
                mtime_secs: 1700000000,
                mtime_nanos: 0,
                permissions: 0o644,
                symlink_target: None,
            },
            AgentFileEntry {
                path: "dir".to_string(),
                kind: FileKind::Directory,
                size: 0,
                mtime_secs: 1700000000,
                mtime_nanos: 0,
                permissions: 0o755,
                symlink_target: None,
            },
            AgentFileEntry {
                path: "dir/nested.txt".to_string(),
                kind: FileKind::File,
                size: 50,
                mtime_secs: 1700000000,
                mtime_nanos: 0,
                permissions: 0o644,
                symlink_target: None,
            },
        ];

        let nodes = convert_agent_entries_to_nodes(&entries);
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].name, "file.txt");
        assert_eq!(nodes[1].name, "dir");
        assert_eq!(nodes[2].name, "nested.txt"); // パスの最後のセグメント
    }
}
