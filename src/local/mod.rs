#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

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

    #[cfg(unix)]
    {
        node.permissions = Some(meta.mode());
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        node.permissions = None;
    }

    // mtime を DateTime<Utc> に変換
    if let Ok(mtime) = meta.modified() {
        if let Ok(duration) = mtime.duration_since(std::time::UNIX_EPOCH) {
            node.mtime = Utc.timestamp_opt(duration.as_secs() as i64, 0).single();
        }
    }
}

/// include パスからスキャンルートを解決する。
///
/// - `include_paths` が空の場合は `root` 自体を返す
/// - 各 include パスを `root` に結合して正規化し、`root` 配下であることを確認する
/// - 存在しないパスは警告ログを出してスキップ
/// - シンボリックリンクが `root` 外を指す場合は拒否
pub fn resolve_scan_roots(root: &Path, include_paths: &[String]) -> Vec<PathBuf> {
    if include_paths.is_empty() {
        return match root.canonicalize() {
            Ok(p) => vec![p],
            Err(_) => vec![root.to_path_buf()],
        };
    }

    let canonical_root = match root.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("Cannot canonicalize root path {}: {}", root.display(), e);
            return vec![root.to_path_buf()];
        }
    };

    let mut scan_roots: Vec<PathBuf> = Vec::new();

    for include_path in include_paths {
        let joined = root.join(include_path);
        match joined.canonicalize() {
            Ok(canonical) => {
                // root 配下であることを確認（シンボリックリンク脱出防止）
                if !canonical.starts_with(&canonical_root) {
                    tracing::warn!(
                        "Include path escapes root directory: {} -> {}",
                        include_path,
                        canonical.display()
                    );
                    continue;
                }
                if !scan_roots.contains(&canonical) {
                    scan_roots.push(canonical);
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Include path does not exist, skipping: {} ({})",
                    include_path,
                    e
                );
            }
        }
    }

    // 祖先パスが既にリストにある場合、子孫パスを除去する
    // 例: ["/root/vendor", "/root/vendor/current"] → ["/root/vendor"]
    scan_roots.sort();
    scan_roots.dedup();
    let filtered: Vec<PathBuf> = scan_roots
        .iter()
        .filter(|path| {
            !scan_roots
                .iter()
                .any(|other| other != *path && path.starts_with(other))
        })
        .cloned()
        .collect();

    filtered
}

/// ローカルディレクトリを再帰的に全走査する（変更ファイルフィルター用）
///
/// walkdir クレートを使用して全ファイルのメタデータを取得する。
/// 戻り値の bool は打ち切りが発生したかどうか。
///
/// `include` が空でない場合、`resolve_scan_roots` でスキャンルートを絞り込み、
/// 各ルート配下のみを走査する。`max_entries` は全ルート横断の累計カウント。
pub fn scan_local_tree_recursive(
    root: &Path,
    exclude: &[String],
    max_entries: usize,
) -> crate::error::Result<(Vec<FileNode>, bool)> {
    scan_local_tree_recursive_with_include(root, exclude, &[], max_entries)
}

/// include フィルター付きのローカル再帰スキャン
///
/// - `include` が空: root 全体をスキャン（従来動作）
/// - `include` が非空: `resolve_scan_roots` で得たディレクトリのみスキャン
/// - `max_entries` は全スキャンルート横断の累計カウント
/// - 結果のパスは `root` からの相対パス
pub fn scan_local_tree_recursive_with_include(
    root: &Path,
    exclude: &[String],
    include: &[String],
    max_entries: usize,
) -> crate::error::Result<(Vec<FileNode>, bool)> {
    if !root.exists() {
        anyhow::bail!(crate::error::AppError::PathNotFound {
            path: root.to_path_buf(),
        });
    }

    let scan_roots = resolve_scan_roots(root, include);

    // strip_prefix の失敗を防ぐため、original_root も正規化する
    // （scan_roots は resolve_scan_roots 内で canonicalize 済み）
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());

    let mut flat_entries: Vec<(String, FileNode)> = Vec::new();
    let mut truncated = false;

    for scan_root in &scan_roots {
        if truncated {
            break;
        }
        let (entries, trunc) = walk_single_root(
            scan_root,
            &canonical_root,
            exclude,
            max_entries - flat_entries.len(),
        )?;
        flat_entries.extend(entries);
        if trunc {
            truncated = true;
        }
    }

    // フラットリストから再帰ツリーを構築
    let tree = build_local_tree_from_flat(flat_entries);
    Ok((tree, truncated))
}

/// 単一のスキャンルートを WalkDir で走査し、フラットエントリリストを返す。
///
/// パスは `original_root` からの相対パスとして格納される。
/// `remaining` は残りエントリ数上限。
fn walk_single_root(
    scan_root: &Path,
    original_root: &Path,
    exclude: &[String],
    remaining: usize,
) -> crate::error::Result<(Vec<(String, FileNode)>, bool)> {
    use walkdir::WalkDir;

    let mut flat_entries: Vec<(String, FileNode)> = Vec::new();
    let mut truncated = false;

    for entry in WalkDir::new(scan_root)
        .min_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // filter_entry ではディレクトリの枝刈りも行える。
            // 相対パスが取れる場合はパスパターンも適用し、ディレクトリ配下を丸ごとスキップ。
            let rel = e
                .path()
                .strip_prefix(original_root)
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

        if flat_entries.len() >= remaining {
            truncated = true;
            tracing::warn!(
                "Recursive scan: entry count reached limit: {}",
                scan_root.display()
            );
            break;
        }

        let rel_path = entry
            .path()
            .strip_prefix(original_root)
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

    Ok((flat_entries, truncated))
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
    use serial_test::serial;
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
        let (nodes, _) =
            scan_local_tree_recursive(root, &exclude, crate::config::DEFAULT_MAX_SCAN_ENTRIES)
                .unwrap();

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

        let (nodes, truncated) = scan_local_tree_recursive(
            dir.path(),
            &["node_modules".to_string()],
            crate::config::DEFAULT_MAX_SCAN_ENTRIES,
        )
        .unwrap();

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
        let result = scan_local_tree_recursive(
            Path::new("/nonexistent/path"),
            &[],
            crate::config::DEFAULT_MAX_SCAN_ENTRIES,
        );
        assert!(result.is_err());
    }

    // ── resolve_scan_roots ──

    #[test]
    fn test_resolve_scan_roots_empty_includes() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let result = resolve_scan_roots(root, &[]);
        // canonicalize() が適用されるので、正規化済みパスが返る
        assert_eq!(result, vec![root.canonicalize().unwrap()]);
    }

    #[test]
    #[serial]
    fn test_resolve_scan_roots_relative_path_canonicalized() {
        // 相対パスの root が canonicalize() で絶対パスに正規化されることを確認
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        // TempDir 内にサブディレクトリを作成し、相対パスで参照する
        let subdir = root.join("project");
        std::fs::create_dir(&subdir).unwrap();

        // 相対パスを構築するために CWD を root に変更してテスト
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(root).unwrap();

        let relative_root = Path::new("project");
        let result = resolve_scan_roots(relative_root, &[]);

        // 元の CWD を復元
        std::env::set_current_dir(&original_cwd).unwrap();

        // 結果は絶対パスであること
        assert_eq!(result.len(), 1);
        assert!(
            result[0].is_absolute(),
            "resolve_scan_roots should return absolute path for relative root, got: {:?}",
            result[0]
        );
        // パスコンポーネントに "." や ".." が含まれないこと
        for component in result[0].components() {
            if let std::path::Component::CurDir | std::path::Component::ParentDir = component {
                panic!(
                    "Canonicalized path should not contain . or .. components: {:?}",
                    result[0]
                );
            }
        }
    }

    #[test]
    fn test_resolve_scan_roots_with_existing_dirs() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let sub_a = root.join("a");
        let sub_b = root.join("b");
        std::fs::create_dir(&sub_a).unwrap();
        std::fs::create_dir(&sub_b).unwrap();

        let include = vec!["a".to_string(), "b".to_string()];
        let result = resolve_scan_roots(root, &include);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&sub_a.canonicalize().unwrap()));
        assert!(result.contains(&sub_b.canonicalize().unwrap()));
    }

    #[test]
    fn test_resolve_scan_roots_nonexistent_skipped() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let sub_a = root.join("a");
        std::fs::create_dir(&sub_a).unwrap();

        let include = vec!["a".to_string(), "nonexistent".to_string()];
        let result = resolve_scan_roots(root, &include);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], sub_a.canonicalize().unwrap());
    }

    #[test]
    fn test_resolve_scan_roots_dedup() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let sub_a = root.join("a");
        std::fs::create_dir(&sub_a).unwrap();

        let include = vec!["a".to_string(), "a".to_string()];
        let result = resolve_scan_roots(root, &include);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_resolve_scan_roots_symlink_escape() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("project");
        std::fs::create_dir(&root).unwrap();

        let outside = tmp.path().join("outside");
        std::fs::create_dir(&outside).unwrap();

        // project/escape -> ../outside（root 外を指すシンボリックリンク）
        symlink(&outside, root.join("escape")).unwrap();
        let include = vec!["escape".to_string()];
        let result = resolve_scan_roots(&root, &include);
        assert!(
            result.is_empty(),
            "Symlink escaping root should be rejected"
        );
    }

    #[test]
    fn test_resolve_scan_roots_overlapping_paths_deduplicated() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let vendor = root.join("vendor");
        let vendor_current = root.join("vendor/current");
        std::fs::create_dir_all(&vendor_current).unwrap();

        // vendor と vendor/current の両方を include → vendor だけ残る
        let include = vec!["vendor".to_string(), "vendor/current".to_string()];
        let result = resolve_scan_roots(root, &include);
        assert_eq!(result.len(), 1, "Descendant path should be removed");
        assert_eq!(result[0], vendor.canonicalize().unwrap());
    }

    #[test]
    fn test_resolve_scan_roots_non_overlapping_preserved() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("docs")).unwrap();

        // 重複しないパスは両方残る
        let include = vec!["src".to_string(), "docs".to_string()];
        let result = resolve_scan_roots(root, &include);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_resolve_scan_roots_all_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let include = vec!["no1".to_string(), "no2".to_string()];
        let result = resolve_scan_roots(tmp.path(), &include);
        assert!(result.is_empty());
    }

    // ── scan_local_tree_recursive_with_include ──

    /// テスト用ツリー構造:
    /// root/
    ///   src/
    ///     main.rs
    ///     lib.rs
    ///   docs/
    ///     readme.txt
    ///   vendor/
    ///     legacy/
    ///       old.js
    ///     current/
    ///       new.js
    ///   top.txt
    fn create_include_test_tree() -> TempDir {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(root.join("src/lib.rs"), "// lib").unwrap();
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join("docs/readme.txt"), "readme").unwrap();
        std::fs::create_dir_all(root.join("vendor/legacy")).unwrap();
        std::fs::create_dir_all(root.join("vendor/current")).unwrap();
        std::fs::write(root.join("vendor/legacy/old.js"), "").unwrap();
        std::fs::write(root.join("vendor/current/new.js"), "").unwrap();
        std::fs::write(root.join("top.txt"), "top").unwrap();
        dir
    }

    #[test]
    fn test_include_scans_only_specified_subdirs() {
        let dir = create_include_test_tree();
        let include = vec!["src".to_string()];
        let (nodes, truncated) = scan_local_tree_recursive_with_include(
            dir.path(),
            &[],
            &include,
            crate::config::DEFAULT_MAX_SCAN_ENTRIES,
        )
        .unwrap();

        assert!(!truncated);
        // src ディレクトリだけが含まれる
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "src");
        let children = nodes[0].children.as_ref().unwrap();
        let names: Vec<&str> = children.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"main.rs"));
        assert!(names.contains(&"lib.rs"));
    }

    #[test]
    fn test_include_multiple_paths_merged() {
        let dir = create_include_test_tree();
        let include = vec!["src".to_string(), "docs".to_string()];
        let (nodes, truncated) = scan_local_tree_recursive_with_include(
            dir.path(),
            &[],
            &include,
            crate::config::DEFAULT_MAX_SCAN_ENTRIES,
        )
        .unwrap();

        assert!(!truncated);
        let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"src"));
        assert!(names.contains(&"docs"));
        // top.txt や vendor は含まれない
        assert!(!names.contains(&"top.txt"));
        assert!(!names.contains(&"vendor"));
    }

    #[test]
    fn test_include_with_exclude_combined() {
        let dir = create_include_test_tree();
        // vendor 配下を include しつつ legacy を exclude
        let include = vec!["vendor".to_string()];
        let exclude = vec!["vendor/legacy/**".to_string()];
        let (nodes, truncated) = scan_local_tree_recursive_with_include(
            dir.path(),
            &exclude,
            &include,
            crate::config::DEFAULT_MAX_SCAN_ENTRIES,
        )
        .unwrap();

        assert!(!truncated);
        let vendor = nodes.iter().find(|n| n.name == "vendor").unwrap();
        let vendor_children = vendor.children.as_ref().unwrap();
        let child_names: Vec<&str> = vendor_children.iter().map(|n| n.name.as_str()).collect();
        assert!(child_names.contains(&"current"));
        assert!(
            !child_names.contains(&"legacy"),
            "legacy should be excluded"
        );
    }

    #[test]
    fn test_include_nonexistent_path_skipped() {
        let dir = create_include_test_tree();
        // nonexistent は存在しない → スキップ、src だけスキャン
        let include = vec!["nonexistent".to_string(), "src".to_string()];
        let (nodes, _) = scan_local_tree_recursive_with_include(
            dir.path(),
            &[],
            &include,
            crate::config::DEFAULT_MAX_SCAN_ENTRIES,
        )
        .unwrap();

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "src");
    }

    #[test]
    fn test_include_max_entries_cumulative() {
        let dir = create_include_test_tree();
        // src (2 files) + docs (1 file) = 3 ファイル + 中間ディレクトリ
        // max_entries=2 で打ち切り確認（累計カウント）
        let include = vec!["src".to_string(), "docs".to_string()];
        let (_, truncated) =
            scan_local_tree_recursive_with_include(dir.path(), &[], &include, 2).unwrap();

        assert!(
            truncated,
            "Should truncate when cumulative entries exceed max"
        );
    }

    #[test]
    fn test_include_empty_scans_all() {
        let dir = create_include_test_tree();
        // include 空 → 従来通り全スキャン
        let (nodes_all, _) = scan_local_tree_recursive_with_include(
            dir.path(),
            &[],
            &[],
            crate::config::DEFAULT_MAX_SCAN_ENTRIES,
        )
        .unwrap();

        let (nodes_legacy, _) =
            scan_local_tree_recursive(dir.path(), &[], crate::config::DEFAULT_MAX_SCAN_ENTRIES)
                .unwrap();

        // 同じ結果になるはず
        assert_eq!(count_all_nodes(&nodes_all), count_all_nodes(&nodes_legacy));
    }

    // ── 相対パス root_dir でのスキャンテスト ──

    #[test]
    #[serial]
    fn test_scan_local_tree_recursive_with_relative_root() {
        // 相対パスの root_dir でスキャンしてもツリーに "." が混入しないことを確認
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("project/src")).unwrap();
        std::fs::write(root.join("project/src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(root.join("project/top.txt"), "hello").unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(root).unwrap();

        let relative_root = Path::new("project");
        let result = scan_local_tree_recursive_with_include(
            relative_root,
            &[],
            &[],
            crate::config::DEFAULT_MAX_SCAN_ENTRIES,
        );

        std::env::set_current_dir(&original_cwd).unwrap();

        let (nodes, _) = result.unwrap();

        // "." ディレクトリが存在しないこと
        assert!(
            !nodes.iter().any(|n| n.name == "."),
            "Tree should not contain '.' directory, got: {:?}",
            nodes.iter().map(|n| &n.name).collect::<Vec<_>>()
        );

        // "project" ディレクトリが存在しないこと（root からの相対パスであるべき）
        assert!(
            !nodes.iter().any(|n| n.name == "project"),
            "Tree should not contain 'project' as root-level entry"
        );

        // 実際の内容が正しく含まれること
        let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"src"), "Should contain 'src' directory");
        assert!(names.contains(&"top.txt"), "Should contain 'top.txt'");
    }

    #[test]
    #[serial]
    fn test_scan_local_tree_recursive_relative_root_no_project_path_leak() {
        // スキャン結果のパスにプロジェクトルート構造が含まれないことを確認
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        // 深めのネスト構造: base/deep/nested/app/file.php
        std::fs::create_dir_all(root.join("base/deep/nested/app")).unwrap();
        std::fs::write(root.join("base/deep/nested/app/file.php"), "<?php").unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(root).unwrap();

        // "./base/deep/nested" という相対パスで root を指定
        let relative_root = Path::new("./base/deep/nested");
        let result = scan_local_tree_recursive_with_include(
            relative_root,
            &[],
            &[],
            crate::config::DEFAULT_MAX_SCAN_ENTRIES,
        );

        std::env::set_current_dir(&original_cwd).unwrap();

        let (nodes, _) = result.unwrap();

        // ツリーのルートに "base", "deep", "nested", "." が存在しないこと
        for bad_name in &[".", "base", "deep", "nested"] {
            assert!(
                !nodes.iter().any(|n| n.name == *bad_name),
                "Tree should not contain '{}' as root-level entry, got: {:?}",
                bad_name,
                nodes.iter().map(|n| &n.name).collect::<Vec<_>>()
            );
        }

        // "app" ディレクトリが正しくルートレベルに存在すること
        assert!(
            nodes.iter().any(|n| n.name == "app"),
            "Should contain 'app' directory at root level"
        );

        // app/file.php が存在すること
        let app = nodes.iter().find(|n| n.name == "app").unwrap();
        let app_children = app.children.as_ref().unwrap();
        assert!(app_children.iter().any(|n| n.name == "file.php"));
    }

    #[test]
    #[serial]
    fn test_scan_local_tree_recursive_relative_root_strip_prefix_safety() {
        // strip_prefix 結果にフルパスや ".." セグメントが含まれないことを検証
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("myapp/controllers")).unwrap();
        std::fs::write(root.join("myapp/controllers/home.rs"), "// home").unwrap();
        std::fs::write(root.join("myapp/main.rs"), "fn main() {}").unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(root).unwrap();

        let relative_root = Path::new("./myapp");
        let result = scan_local_tree_recursive_with_include(
            relative_root,
            &[],
            &[],
            crate::config::DEFAULT_MAX_SCAN_ENTRIES,
        );

        std::env::set_current_dir(&original_cwd).unwrap();

        let (nodes, _) = result.unwrap();

        // 全ノードを再帰的に走査して不正なパスセグメントがないことを確認
        fn assert_no_bad_segments(nodes: &[FileNode]) {
            for node in nodes {
                assert!(
                    !node.name.contains(".."),
                    "Node name should not contain '..': {}",
                    node.name
                );
                assert!(
                    !node.name.starts_with('/'),
                    "Node name should not be absolute path: {}",
                    node.name
                );
                assert!(
                    node.name != ".",
                    "Node name should not be '.': {}",
                    node.name
                );
                if let Some(children) = &node.children {
                    assert_no_bad_segments(children);
                }
            }
        }

        assert_no_bad_segments(&nodes);

        // 正しいツリー構造であること
        let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"controllers"));
        assert!(names.contains(&"main.rs"));
    }
}
