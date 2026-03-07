use std::os::unix::fs::MetadataExt;
use std::path::Path;

use anyhow::Context;
use chrono::{TimeZone, Utc};

use crate::filter;
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

    tree.nodes = scan_dir(root, exclude, "")
        .with_context(|| format!("Failed to scan local tree: {}", root.display()))?;
    tree.sort();

    Ok(tree)
}

/// ディレクトリ取得のデフォルト最大エントリ数
pub const MAX_DIR_ENTRIES: usize = 10_000;

/// 指定ディレクトリの直下エントリのみを取得する（1階層のみ）
///
/// サブディレクトリは children: None（未取得）で返す。
/// `parent_rel_path` はプロジェクトルートからの相対パス（例: `"config"`）。
/// パスパターン（`config/*.toml` など）のフィルタに使われる。
/// ルート直下の場合は `""` を渡す。
pub fn scan_dir(
    dir: &Path,
    exclude: &[String],
    parent_rel_path: &str,
) -> crate::error::Result<Vec<FileNode>> {
    scan_dir_with_limit(dir, exclude, parent_rel_path, MAX_DIR_ENTRIES).map(|(nodes, _)| nodes)
}

/// 指定ディレクトリの直下エントリを取得する（エントリ数制限付き）
///
/// 戻り値の bool は打ち切りが発生したかどうか
pub fn scan_dir_with_limit(
    dir: &Path,
    exclude: &[String],
    parent_rel_path: &str,
    max_entries: usize,
) -> crate::error::Result<(Vec<FileNode>, bool)> {
    let mut nodes = Vec::new();
    let mut truncated = false;

    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory: {}", dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().to_string();

        // 除外フィルター（セグメント + パスパターン両対応）
        let rel_path = if parent_rel_path.is_empty() {
            file_name.clone()
        } else {
            format!("{}/{}", parent_rel_path, file_name)
        };
        if filter::is_path_excluded(&rel_path, exclude) {
            continue;
        }

        if nodes.len() >= max_entries {
            truncated = true;
            tracing::warn!(
                "Entry count reached limit {}: {}",
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

/// ローカルディレクトリを再帰的に全走査する（変更ファイルフィルター用）
///
/// walkdir クレートを使用して全ファイルのメタデータを取得する。
/// 戻り値の bool は打ち切りが発生したかどうか。
pub fn scan_local_tree_recursive(
    root: &Path,
    exclude: &[String],
    max_entries: usize,
) -> crate::error::Result<(Vec<FileNode>, bool)> {
    use walkdir::WalkDir;

    if !root.exists() {
        anyhow::bail!(crate::error::AppError::PathNotFound {
            path: root.to_path_buf(),
        });
    }

    let mut flat_entries: Vec<(String, FileNode)> = Vec::new();
    let mut truncated = false;

    for entry in WalkDir::new(root)
        .min_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // filter_entry ではディレクトリの枝刈りも行える。
            // 相対パスが取れる場合はパスパターンも適用し、ディレクトリ配下を丸ごとスキップ。
            let rel = e
                .path()
                .strip_prefix(root)
                .unwrap_or(e.path())
                .to_string_lossy();
            if rel.is_empty() {
                return true;
            }
            !filter::is_path_excluded(&rel, exclude)
        })
    {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::debug!("walkdir error: {}", e);
                continue;
            }
        };

        if flat_entries.len() >= max_entries {
            truncated = true;
            tracing::warn!(
                "Recursive scan: entry count reached limit {}: {}",
                max_entries,
                root.display()
            );
            break;
        }

        let rel_path = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .to_string();

        if rel_path.is_empty() {
            continue;
        }

        let file_name = entry.file_name().to_string_lossy().to_string();
        let meta = entry.path().symlink_metadata()?;

        let mut node = if meta.is_symlink() {
            let target = std::fs::read_link(entry.path())
                .map(|t| t.to_string_lossy().to_string())
                .unwrap_or_else(|_| "???".to_string());
            FileNode::new_symlink(&file_name, target)
        } else if meta.is_dir() {
            let mut d = FileNode::new_dir(&file_name);
            d.children = Some(Vec::new()); // loaded 状態
            d
        } else {
            FileNode::new_file(&file_name)
        };

        apply_metadata(&mut node, &meta);
        flat_entries.push((rel_path, node));
    }

    // フラットリストから再帰ツリーを構築
    let tree = build_local_tree_from_flat(flat_entries);
    Ok((tree, truncated))
}

/// フラットなエントリリスト（相対パス付き）から再帰ツリーを構築する
fn build_local_tree_from_flat(entries: Vec<(String, FileNode)>) -> Vec<FileNode> {
    use std::collections::BTreeMap;

    fn insert_into_tree(
        tree: &mut BTreeMap<String, FileNode>,
        parts: &[&str],
        original_node: &FileNode,
    ) {
        if parts.is_empty() {
            return;
        }

        let name = parts[0];

        if parts.len() == 1 {
            if let Some(existing) = tree.get_mut(name) {
                existing.size = original_node.size.or(existing.size);
                existing.mtime = original_node.mtime.or(existing.mtime);
                existing.permissions = original_node.permissions.or(existing.permissions);
            } else {
                let mut node = original_node.clone();
                node.name = name.to_string();
                tree.insert(name.to_string(), node);
            }
        } else {
            let dir = tree.entry(name.to_string()).or_insert_with(|| {
                let mut d = FileNode::new_dir(name);
                d.children = Some(Vec::new());
                d
            });
            if dir.children.is_none() {
                dir.children = Some(Vec::new());
            }
            let children = dir.children.take().unwrap_or_default();
            let mut child_map: BTreeMap<String, FileNode> = BTreeMap::new();
            for child in children {
                child_map.insert(child.name.clone(), child);
            }
            insert_into_tree(&mut child_map, &parts[1..], original_node);
            let mut sorted: Vec<FileNode> = child_map.into_values().collect();
            sorted.sort_by(|a, b| match (a.is_dir(), b.is_dir()) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            });
            dir.children = Some(sorted);
        }
    }

    let mut root_map: BTreeMap<String, FileNode> = BTreeMap::new();

    for (rel_path, node) in &entries {
        let parts: Vec<&str> = rel_path.split('/').collect();
        insert_into_tree(&mut root_map, &parts, node);
    }

    let mut result: Vec<FileNode> = root_map.into_values().collect();
    result.sort_by(|a, b| match (a.is_dir(), b.is_dir()) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });
    result
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
        let children = scan_dir(&dir.path().join("subdir"), &[], "subdir").unwrap();
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

    #[test]
    fn test_scan_dir_path_pattern_exclude() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // config/settings.toml, config/other.json
        std::fs::create_dir(root.join("config")).unwrap();
        std::fs::write(root.join("config").join("settings.toml"), "").unwrap();
        std::fs::write(root.join("config").join("other.json"), "").unwrap();

        // パスパターン config/*.toml で settings.toml を除外
        let exclude = vec!["config/*.toml".to_string()];
        let children = scan_dir(&root.join("config"), &exclude, "config").unwrap();

        let names: Vec<&str> = children.iter().map(|n| n.name.as_str()).collect();
        assert!(
            !names.contains(&"settings.toml"),
            "settings.toml should be excluded by config/*.toml"
        );
        assert!(names.contains(&"other.json"), "other.json should remain");
    }

    #[test]
    fn test_scan_dir_segment_pattern_still_works() {
        let dir = create_test_tree();
        let exclude = vec!["*.log".to_string()];
        let nodes = scan_dir(dir.path(), &exclude, "").unwrap();

        let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        assert!(!names.contains(&"file2.log"));
        assert!(names.contains(&"file1.txt"));
    }

    #[test]
    fn test_scan_local_tree_recursive_path_pattern() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // vendor/legacy/old.rs, vendor/current/new.rs
        std::fs::create_dir_all(root.join("vendor/legacy")).unwrap();
        std::fs::create_dir_all(root.join("vendor/current")).unwrap();
        std::fs::write(root.join("vendor/legacy/old.rs"), "").unwrap();
        std::fs::write(root.join("vendor/current/new.rs"), "").unwrap();

        let exclude = vec!["vendor/legacy/**".to_string()];
        let (nodes, _) = scan_local_tree_recursive(root, &exclude, 50_000).unwrap();

        // vendor は残る
        let vendor = nodes.iter().find(|n| n.name == "vendor").unwrap();
        let vendor_children = vendor.children.as_ref().unwrap();

        // current は残り、legacy は除外（ディレクトリ自体が枝刈りされる）
        let child_names: Vec<&str> = vendor_children.iter().map(|n| n.name.as_str()).collect();
        assert!(child_names.contains(&"current"), "current should remain");
        assert!(
            !child_names.contains(&"legacy"),
            "legacy should be excluded by vendor/legacy/**"
        );
    }

    #[test]
    fn test_scan_local_tree_recursive() {
        let dir = create_test_tree();

        let (nodes, truncated) =
            scan_local_tree_recursive(dir.path(), &["node_modules".to_string()], 50_000).unwrap();

        assert!(!truncated);

        // ルート直下: file1.txt, file2.log, subdir, empty_dir
        assert!(nodes.iter().any(|n| n.name == "file1.txt"));
        assert!(nodes.iter().any(|n| n.name == "subdir"));

        // node_modules は除外されている
        assert!(!nodes.iter().any(|n| n.name == "node_modules"));

        // subdir は children が展開済み
        let subdir = nodes.iter().find(|n| n.name == "subdir").unwrap();
        assert!(subdir.is_loaded());
        let children = subdir.children.as_ref().unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "nested.txt");
    }

    #[test]
    fn test_scan_local_tree_recursive_max_entries() {
        let dir = create_test_tree();

        // 極端に小さい上限
        let (nodes, truncated) = scan_local_tree_recursive(dir.path(), &[], 2).unwrap();

        assert!(truncated);
        // ノード数は上限以下（ツリー構築で増える場合があるが、元のフラットエントリが2以下）
        let flat_count = count_all_nodes(&nodes);
        assert!(flat_count <= 3); // 2エントリ + 1中間ディレクトリ程度
    }

    fn count_all_nodes(nodes: &[FileNode]) -> usize {
        let mut count = nodes.len();
        for node in nodes {
            if let Some(children) = &node.children {
                count += count_all_nodes(children);
            }
        }
        count
    }

    #[test]
    fn test_scan_local_tree_recursive_nonexistent() {
        let result = scan_local_tree_recursive(Path::new("/nonexistent/path"), &[], 50_000);
        assert!(result.is_err());
    }
}
