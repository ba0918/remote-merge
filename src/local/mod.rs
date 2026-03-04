use std::os::unix::fs::MetadataExt;
use std::path::Path;

use anyhow::Context;
use chrono::{TimeZone, Utc};

use crate::tree::{FileNode, FileTree};

/// ローカルファイルシステムからディレクトリツリーを取得する
///
/// `depth` = 1 でルート直下のみ取得（遅延読み込みの基盤）
pub fn scan_local_tree(root: &Path, exclude: &[String]) -> crate::error::Result<FileTree> {
    let mut tree = FileTree::new(root);

    if !root.exists() {
        anyhow::bail!(crate::error::AppError::PathNotFound {
            path: root.to_path_buf(),
        });
    }

    tree.nodes = scan_dir(root, exclude)
        .with_context(|| format!("ローカルツリーの取得に失敗: {}", root.display()))?;
    tree.sort();

    Ok(tree)
}

/// ディレクトリ取得のデフォルト最大エントリ数
pub const MAX_DIR_ENTRIES: usize = 10_000;

/// 指定ディレクトリの直下エントリのみを取得する（1階層のみ）
///
/// サブディレクトリは children: None（未取得）で返す
pub fn scan_dir(dir: &Path, exclude: &[String]) -> crate::error::Result<Vec<FileNode>> {
    scan_dir_with_limit(dir, exclude, MAX_DIR_ENTRIES).map(|(nodes, _)| nodes)
}

/// 指定ディレクトリの直下エントリを取得する（エントリ数制限付き）
///
/// 戻り値の bool は打ち切りが発生したかどうか
pub fn scan_dir_with_limit(
    dir: &Path,
    exclude: &[String],
    max_entries: usize,
) -> crate::error::Result<(Vec<FileNode>, bool)> {
    let mut nodes = Vec::new();
    let mut truncated = false;

    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("ディレクトリの読み込みに失敗: {}", dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().to_string();

        // 除外フィルター
        if should_exclude(&file_name, exclude) {
            continue;
        }

        if nodes.len() >= max_entries {
            truncated = true;
            tracing::warn!(
                "エントリ数が上限 {} に達しました: {}",
                max_entries,
                dir.display()
            );
            break;
        }

        // シンボリックリンクかどうかの判定は symlink_metadata を使う
        let symlink_meta = entry.path().symlink_metadata()?;

        let node = if symlink_meta.is_symlink() {
            // シンボリックリンク: リンク先を読み取る
            let target = std::fs::read_link(entry.path())
                .map(|t| t.to_string_lossy().to_string())
                .unwrap_or_else(|_| "???".to_string());
            let mut node = FileNode::new_symlink(&file_name, target);
            apply_metadata(&mut node, &symlink_meta);
            node
        } else if symlink_meta.is_dir() {
            // ディレクトリ: children は None（遅延読み込み）
            let mut node = FileNode::new_dir(&file_name);
            apply_metadata(&mut node, &symlink_meta);
            node
        } else {
            // 通常ファイル
            let mut node = FileNode::new_file(&file_name);
            apply_metadata(&mut node, &symlink_meta);
            node
        };

        nodes.push(node);
    }

    Ok((nodes, truncated))
}

/// メタデータをノードに適用
fn apply_metadata(node: &mut FileNode, meta: &std::fs::Metadata) {
    node.size = Some(meta.len());
    node.permissions = Some(meta.mode());

    // mtime を DateTime<Utc> に変換
    if let Ok(mtime) = meta.modified() {
        if let Ok(duration) = mtime.duration_since(std::time::UNIX_EPOCH) {
            node.mtime = Utc.timestamp_opt(duration.as_secs() as i64, 0).single();
        }
    }
}

/// ファイル名が除外パターンにマッチするか
fn should_exclude(name: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        if glob_match::glob_match(pattern, name) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::NodeKind;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    fn create_test_tree() -> TempDir {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // ファイル作成
        std::fs::write(root.join("file1.txt"), "hello").unwrap();
        std::fs::write(root.join("file2.log"), "log content").unwrap();

        // サブディレクトリ
        std::fs::create_dir(root.join("subdir")).unwrap();
        std::fs::write(root.join("subdir").join("nested.txt"), "nested").unwrap();

        // 空ディレクトリ
        std::fs::create_dir(root.join("empty_dir")).unwrap();

        // node_modules（除外対象）
        std::fs::create_dir(root.join("node_modules")).unwrap();
        std::fs::write(root.join("node_modules").join("pkg.json"), "{}").unwrap();

        dir
    }

    #[test]
    fn test_scan_local_tree() {
        let dir = create_test_tree();
        let tree = scan_local_tree(dir.path(), &[]).unwrap();

        assert_eq!(tree.root, dir.path());
        // ルート直下: file1.txt, file2.log, subdir, empty_dir, node_modules
        assert_eq!(tree.nodes.len(), 5);
    }

    #[test]
    fn test_exclude_filter() {
        let dir = create_test_tree();
        let exclude = vec!["node_modules".to_string(), "*.log".to_string()];
        let tree = scan_local_tree(dir.path(), &exclude).unwrap();

        let names: Vec<&str> = tree.nodes.iter().map(|n| n.name.as_str()).collect();
        assert!(!names.contains(&"node_modules"));
        assert!(!names.contains(&"file2.log"));
        assert!(names.contains(&"file1.txt"));
    }

    #[test]
    fn test_empty_dir() {
        let dir = create_test_tree();
        let tree = scan_local_tree(dir.path(), &[]).unwrap();

        let empty = tree.nodes.iter().find(|n| n.name == "empty_dir").unwrap();
        assert!(empty.is_dir());
        // 1階層のみスキャンなので children は None（未取得）
        assert!(!empty.is_loaded());
    }

    #[test]
    fn test_lazy_loading() {
        let dir = create_test_tree();
        let tree = scan_local_tree(dir.path(), &[]).unwrap();

        // サブディレクトリの children は None（遅延読み込み）
        let subdir = tree.nodes.iter().find(|n| n.name == "subdir").unwrap();
        assert!(subdir.is_dir());
        assert!(!subdir.is_loaded());

        // 展開時: scan_dir で子ノードを取得
        let children = scan_dir(&dir.path().join("subdir"), &[]).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "nested.txt");
    }

    #[test]
    fn test_symlink_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        std::fs::write(root.join("target.txt"), "content").unwrap();
        symlink(root.join("target.txt"), root.join("link.txt")).unwrap();

        let tree = scan_local_tree(root, &[]).unwrap();
        let link = tree.nodes.iter().find(|n| n.name == "link.txt").unwrap();
        assert!(link.is_symlink());

        if let NodeKind::Symlink { ref target } = link.kind {
            assert!(target.contains("target.txt"));
        } else {
            panic!("Expected Symlink");
        }
    }

    #[test]
    fn test_metadata() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("test.txt"), "hello world").unwrap();

        let tree = scan_local_tree(dir.path(), &[]).unwrap();
        let file = tree.nodes.iter().find(|n| n.name == "test.txt").unwrap();

        assert_eq!(file.size, Some(11)); // "hello world" = 11 bytes
        assert!(file.mtime.is_some());
        assert!(file.permissions.is_some());
    }

    #[test]
    fn test_nonexistent_path() {
        let result = scan_local_tree(Path::new("/nonexistent/path"), &[]);
        assert!(result.is_err());
    }
}
