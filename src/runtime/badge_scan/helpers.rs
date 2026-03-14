//! バッジスキャン用の純粋関数ヘルパー。
//!
//! ツリーとキャッシュの情報からスキャン対象のファイルを収集し、
//! バッジの一括変更・復元を行う。

use std::path::Path;

use crate::app::cache::BoundedCache;
use crate::app::types::{Badge, FlatNode};
use crate::tree::FileTree;

/// ディレクトリ直下のファイルパスを収集する（非再帰）。
///
/// left_tree と right_tree をマージして、指定ディレクトリの直下にある
/// ファイル（ディレクトリではない）のパスを返す。
/// シンボリックリンクは除外する。
pub fn collect_direct_children_files(
    left_tree: &FileTree,
    right_tree: &FileTree,
    dir_path: &str,
) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();

    // left_tree から直下ファイルを収集
    collect_file_names_from_tree(left_tree, dir_path, &mut names);
    // right_tree から直下ファイルを収集
    collect_file_names_from_tree(right_tree, dir_path, &mut names);

    let prefix = if dir_path.is_empty() {
        String::new()
    } else {
        format!("{}/", dir_path)
    };

    names
        .into_iter()
        .map(|name| format!("{}{}", prefix, name))
        .collect()
}

/// ツリーノードから直下ファイル名を収集するヘルパー
fn collect_file_names_from_tree(
    tree: &FileTree,
    dir_path: &str,
    names: &mut std::collections::BTreeSet<String>,
) {
    let node = match tree.find_node(Path::new(dir_path)) {
        Some(n) => n,
        None => return,
    };
    if let Some(children) = &node.children {
        for child in children {
            if !child.is_dir() && !child.is_symlink() {
                names.insert(child.name.clone());
            }
        }
    }
}

/// キャッシュ済みパスを除外したパスリストを返す。
pub fn filter_uncached_paths(
    paths: &[String],
    left_cache: &BoundedCache<String>,
    right_cache: &BoundedCache<String>,
    left_binary_cache: &BoundedCache<crate::diff::binary::BinaryInfo>,
    right_binary_cache: &BoundedCache<crate::diff::binary::BinaryInfo>,
) -> Vec<String> {
    paths
        .iter()
        .filter(|p| {
            // 両方のキャッシュに存在する場合のみスキップ
            let left_cached = left_cache.contains_key(p) || left_binary_cache.contains_key(p);
            let right_cached = right_cache.contains_key(p) || right_binary_cache.contains_key(p);
            !(left_cached && right_cached)
        })
        .cloned()
        .collect()
}

/// 対象ファイルのバッジを `[?]` → `[..]` に変更する。
///
/// 対象パスに一致する `flat_nodes` のバッジが `Unchecked` の場合のみ変更する。
pub fn set_loading_badges(flat_nodes: &mut [FlatNode], target_paths: &[String]) {
    let path_set: std::collections::HashSet<&str> =
        target_paths.iter().map(|s| s.as_str()).collect();
    for node in flat_nodes.iter_mut() {
        if !node.is_dir && path_set.contains(node.path.as_str()) && node.badge == Badge::Unchecked {
            node.badge = Badge::Loading;
        }
    }
}

/// `[..]` のバッジを `[?]` に戻す（キャンセル時用）。
///
/// 指定ディレクトリの直下ファイルで `Loading` バッジのものを `Unchecked` に戻す。
pub fn revert_loading_badges(flat_nodes: &mut [FlatNode], dir_path: &str) {
    let prefix = format!("{}/", dir_path);
    for node in flat_nodes.iter_mut() {
        if !node.is_dir && node.badge == Badge::Loading && node.path.starts_with(&prefix) {
            // 直下のファイルのみ（サブディレクトリのファイルは除外）
            let rest = &node.path[prefix.len()..];
            if !rest.contains('/') {
                node.badge = Badge::Unchecked;
            }
        }
    }
}

/// バッジスキャンのファイル数上限
pub const BADGE_SCAN_MAX_FILES: usize = 100;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::cache::BoundedCache;
    use crate::diff::binary::BinaryInfo;
    use crate::tree::{FileNode, FileTree};
    use std::path::PathBuf;

    fn make_tree(nodes: Vec<FileNode>) -> FileTree {
        FileTree {
            root: PathBuf::from("/test"),
            nodes,
        }
    }

    fn make_flat_file(path: &str, badge: Badge) -> FlatNode {
        FlatNode {
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            depth: path.matches('/').count(),
            is_dir: false,
            is_symlink: false,
            expanded: false,
            badge,
            ref_only: false,
        }
    }

    // ── collect_direct_children_files ──

    #[test]
    fn collect_direct_children_files_returns_direct_files_only() {
        let left = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![
                FileNode::new_file("main.rs"),
                FileNode::new_dir_with_children("app", vec![FileNode::new_file("mod.rs")]),
            ],
        )]);
        let right = make_tree(vec![]);

        let result = collect_direct_children_files(&left, &right, "src");
        assert_eq!(result, vec!["src/main.rs"]);
    }

    #[test]
    fn collect_direct_children_files_empty_dir() {
        let left = make_tree(vec![FileNode::new_dir_with_children("src", vec![])]);
        let right = make_tree(vec![]);

        let result = collect_direct_children_files(&left, &right, "src");
        assert!(result.is_empty());
    }

    #[test]
    fn collect_direct_children_files_filters_cached() {
        let left = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs"), FileNode::new_file("b.rs")],
        )]);
        let right = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs"), FileNode::new_file("b.rs")],
        )]);

        let all = collect_direct_children_files(&left, &right, "src");
        assert_eq!(all.len(), 2);

        let mut left_cache: BoundedCache<String> = BoundedCache::new(100);
        let mut right_cache: BoundedCache<String> = BoundedCache::new(100);
        left_cache.insert("src/a.rs".to_string(), "content".to_string());
        right_cache.insert("src/a.rs".to_string(), "content".to_string());

        let left_bin: BoundedCache<BinaryInfo> = BoundedCache::new(100);
        let right_bin: BoundedCache<BinaryInfo> = BoundedCache::new(100);

        let filtered =
            filter_uncached_paths(&all, &left_cache, &right_cache, &left_bin, &right_bin);
        assert_eq!(filtered, vec!["src/b.rs"]);
    }

    #[test]
    fn collect_direct_children_files_left_only() {
        let left = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("left_only.rs")],
        )]);
        let right = make_tree(vec![FileNode::new_dir_with_children("src", vec![])]);

        let result = collect_direct_children_files(&left, &right, "src");
        assert_eq!(result, vec!["src/left_only.rs"]);
    }

    #[test]
    fn collect_direct_children_files_right_only() {
        let left = make_tree(vec![FileNode::new_dir_with_children("src", vec![])]);
        let right = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("right_only.rs")],
        )]);

        let result = collect_direct_children_files(&left, &right, "src");
        assert_eq!(result, vec!["src/right_only.rs"]);
    }

    #[test]
    fn collect_direct_children_files_merges_both_trees() {
        let left = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs")],
        )]);
        let right = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("b.rs")],
        )]);

        let result = collect_direct_children_files(&left, &right, "src");
        assert_eq!(result, vec!["src/a.rs", "src/b.rs"]);
    }

    #[test]
    fn collect_direct_children_files_excludes_symlinks() {
        let left = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![
                FileNode::new_file("a.rs"),
                FileNode::new_symlink("link", "target"),
            ],
        )]);
        let right = make_tree(vec![]);

        let result = collect_direct_children_files(&left, &right, "src");
        assert_eq!(result, vec!["src/a.rs"]);
    }

    // ── filter_uncached_paths ──

    #[test]
    fn filter_uncached_paths_all_cached() {
        let paths = vec!["a.rs".to_string(), "b.rs".to_string()];
        let mut left_cache: BoundedCache<String> = BoundedCache::new(100);
        let mut right_cache: BoundedCache<String> = BoundedCache::new(100);
        left_cache.insert("a.rs".to_string(), "x".to_string());
        right_cache.insert("a.rs".to_string(), "x".to_string());
        left_cache.insert("b.rs".to_string(), "y".to_string());
        right_cache.insert("b.rs".to_string(), "y".to_string());

        let left_bin: BoundedCache<BinaryInfo> = BoundedCache::new(100);
        let right_bin: BoundedCache<BinaryInfo> = BoundedCache::new(100);

        let result =
            filter_uncached_paths(&paths, &left_cache, &right_cache, &left_bin, &right_bin);
        assert!(result.is_empty());
    }

    #[test]
    fn filter_uncached_paths_none_cached() {
        let paths = vec!["a.rs".to_string()];
        let left_cache: BoundedCache<String> = BoundedCache::new(100);
        let right_cache: BoundedCache<String> = BoundedCache::new(100);
        let left_bin: BoundedCache<BinaryInfo> = BoundedCache::new(100);
        let right_bin: BoundedCache<BinaryInfo> = BoundedCache::new(100);

        let result =
            filter_uncached_paths(&paths, &left_cache, &right_cache, &left_bin, &right_bin);
        assert_eq!(result, vec!["a.rs"]);
    }

    #[test]
    fn filter_uncached_paths_partial_cache_not_skipped() {
        // left のみキャッシュ済み → スキップしない（両方必要）
        let paths = vec!["a.rs".to_string()];
        let mut left_cache: BoundedCache<String> = BoundedCache::new(100);
        left_cache.insert("a.rs".to_string(), "x".to_string());
        let right_cache: BoundedCache<String> = BoundedCache::new(100);
        let left_bin: BoundedCache<BinaryInfo> = BoundedCache::new(100);
        let right_bin: BoundedCache<BinaryInfo> = BoundedCache::new(100);

        let result =
            filter_uncached_paths(&paths, &left_cache, &right_cache, &left_bin, &right_bin);
        assert_eq!(result, vec!["a.rs"]);
    }

    // ── set_loading_badges ──

    #[test]
    fn set_loading_badges_changes_unchecked_to_loading() {
        let mut nodes = vec![
            make_flat_file("src/a.rs", Badge::Unchecked),
            make_flat_file("src/b.rs", Badge::Modified),
            make_flat_file("src/c.rs", Badge::Unchecked),
        ];
        set_loading_badges(
            &mut nodes,
            &["src/a.rs".to_string(), "src/c.rs".to_string()],
        );
        assert_eq!(nodes[0].badge, Badge::Loading);
        assert_eq!(nodes[1].badge, Badge::Modified); // 変更なし
        assert_eq!(nodes[2].badge, Badge::Loading);
    }

    #[test]
    fn set_loading_badges_does_not_change_non_unchecked() {
        let mut nodes = vec![make_flat_file("src/a.rs", Badge::Modified)];
        set_loading_badges(&mut nodes, &["src/a.rs".to_string()]);
        assert_eq!(nodes[0].badge, Badge::Modified);
    }

    // ── revert_loading_badges ──

    #[test]
    fn revert_loading_badges_reverts_loading_to_unchecked() {
        let mut nodes = vec![
            make_flat_file("src/a.rs", Badge::Loading),
            make_flat_file("src/b.rs", Badge::Modified),
            make_flat_file("src/c.rs", Badge::Loading),
        ];
        revert_loading_badges(&mut nodes, "src");
        assert_eq!(nodes[0].badge, Badge::Unchecked);
        assert_eq!(nodes[1].badge, Badge::Modified);
        assert_eq!(nodes[2].badge, Badge::Unchecked);
    }

    #[test]
    fn revert_loading_badges_does_not_affect_subdirs() {
        let mut nodes = vec![
            make_flat_file("src/a.rs", Badge::Loading),
            make_flat_file("src/sub/b.rs", Badge::Loading),
        ];
        revert_loading_badges(&mut nodes, "src");
        assert_eq!(nodes[0].badge, Badge::Unchecked); // 直下
        assert_eq!(nodes[1].badge, Badge::Loading); // サブディレクトリは変更なし
    }

    // ── cancel flag check logic ──

    #[test]
    fn cancel_flag_check() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let flag = AtomicBool::new(false);
        assert!(!flag.load(Ordering::Relaxed));
        flag.store(true, Ordering::Relaxed);
        assert!(flag.load(Ordering::Relaxed));
    }

    // ── BADGE_SCAN_MAX_FILES ──

    #[test]
    fn badge_scan_max_files_value() {
        assert_eq!(BADGE_SCAN_MAX_FILES, 100);
    }
}
