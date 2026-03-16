//! マージ対象ファイルの収集。
//!
//! `flat_nodes`（表示用）ではなく、ツリー構造から直接ファイルパスを再帰的に収集する。
//! ディレクトリの展開状態 (`expanded_dirs`) に依存しないため、
//! マージ走査後にツリーを展開せずにファイル一覧を取得できる。

use std::collections::HashSet;

use crate::app::cache::BoundedCache;
use crate::diff::binary::BinaryInfo;
use crate::tree::FileTree;

/// 指定ディレクトリ配下のファイルパスをローカル・リモート両ツリーから収集する。
///
/// 両ツリーの union を返す（片方にのみ存在するファイルも含む）。
/// パスは `dir_path/...` 形式の相対パス。
pub fn collect_merge_files(
    local_tree: &FileTree,
    remote_tree: &FileTree,
    dir_path: &str,
) -> Vec<String> {
    let mut files = Vec::new();
    collect_from_tree(local_tree, dir_path, &mut files);
    collect_from_tree(remote_tree, dir_path, &mut files);
    files
}

/// 指定ディレクトリ配下のファイルパスを3ツリー（left, right, ref）から収集する。
///
/// 3ツリーの union を返す。ref_tree が None の場合は 2-way と同等。
pub fn collect_merge_files_3way(
    local_tree: &FileTree,
    remote_tree: &FileTree,
    ref_tree: Option<&FileTree>,
    dir_path: &str,
) -> Vec<String> {
    let mut files = Vec::new();
    collect_from_tree(local_tree, dir_path, &mut files);
    collect_from_tree(remote_tree, dir_path, &mut files);
    if let Some(ref_t) = ref_tree {
        collect_from_tree(ref_t, dir_path, &mut files);
    }
    files
}

/// キャッシュキーから prefix マッチでファイルパスを追加収集する内部ヘルパー。
fn supplement_from_cache(
    files: &mut Vec<String>,
    dir_path: &str,
    left_cache: &BoundedCache<String>,
    right_cache: &BoundedCache<String>,
    left_binary_cache: &BoundedCache<BinaryInfo>,
    right_binary_cache: &BoundedCache<BinaryInfo>,
) {
    let prefix = format!("{}/", dir_path);
    let mut existing: HashSet<String> = files.iter().cloned().collect();

    for key in left_cache
        .keys()
        .chain(right_cache.keys())
        .chain(left_binary_cache.keys())
        .chain(right_binary_cache.keys())
    {
        if key.starts_with(&prefix) && existing.insert(key.clone()) {
            files.push(key.clone());
        }
    }
}

/// ツリー + キャッシュの union でファイルパスを収集する（2-way）。
#[allow(clippy::too_many_arguments)]
pub fn collect_merge_files_with_cache(
    local_tree: &FileTree,
    remote_tree: &FileTree,
    dir_path: &str,
    left_cache: &BoundedCache<String>,
    right_cache: &BoundedCache<String>,
    left_binary_cache: &BoundedCache<BinaryInfo>,
    right_binary_cache: &BoundedCache<BinaryInfo>,
) -> Vec<String> {
    let mut files = collect_merge_files(local_tree, remote_tree, dir_path);
    supplement_from_cache(
        &mut files,
        dir_path,
        left_cache,
        right_cache,
        left_binary_cache,
        right_binary_cache,
    );
    files
}

/// ツリー + キャッシュの union でファイルパスを収集する（3-way）。
#[allow(clippy::too_many_arguments)]
pub fn collect_merge_files_3way_with_cache(
    local_tree: &FileTree,
    remote_tree: &FileTree,
    ref_tree: Option<&FileTree>,
    dir_path: &str,
    left_cache: &BoundedCache<String>,
    right_cache: &BoundedCache<String>,
    left_binary_cache: &BoundedCache<BinaryInfo>,
    right_binary_cache: &BoundedCache<BinaryInfo>,
) -> Vec<String> {
    let mut files = collect_merge_files_3way(local_tree, remote_tree, ref_tree, dir_path);
    supplement_from_cache(
        &mut files,
        dir_path,
        left_cache,
        right_cache,
        left_binary_cache,
        right_binary_cache,
    );
    files
}

/// 単一ツリーからファイルパスを再帰的に収集する（重複除去付き）。
fn collect_from_tree(tree: &FileTree, dir_path: &str, files: &mut Vec<String>) {
    let node = match tree.find_node(std::path::Path::new(dir_path)) {
        Some(n) => n,
        None => return,
    };
    if !node.is_dir() {
        return;
    }
    collect_children_recursive(node, dir_path, files);
}

/// ノードの子を再帰的に走査してファイルパスを収集する。
fn collect_children_recursive(
    node: &crate::tree::FileNode,
    current_path: &str,
    files: &mut Vec<String>,
) {
    let children = match &node.children {
        Some(c) => c,
        None => return, // 未ロードディレクトリはスキップ
    };

    for child in children {
        let child_path = format!("{}/{}", current_path, child.name);
        if child.is_dir() {
            collect_children_recursive(child, &child_path, files);
        } else if !files.contains(&child_path) {
            files.push(child_path);
        }
    }
}

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

    #[test]
    fn test_empty_directory() {
        let local = make_tree(vec![FileNode::new_dir_with_children("src", vec![])]);
        let remote = make_tree(vec![FileNode::new_dir_with_children("src", vec![])]);
        let files = collect_merge_files(&local, &remote, "src");
        assert!(files.is_empty());
    }

    #[test]
    fn test_files_only() {
        let local = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs"), FileNode::new_file("b.rs")],
        )]);
        let remote = make_tree(vec![FileNode::new_dir_with_children("src", vec![])]);
        let files = collect_merge_files(&local, &remote, "src");
        assert_eq!(files, vec!["src/a.rs", "src/b.rs"]);
    }

    #[test]
    fn test_nested_directories() {
        let local = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_dir_with_children(
                "app",
                vec![FileNode::new_file("mod.rs"), FileNode::new_file("state.rs")],
            )],
        )]);
        let remote = make_tree(vec![FileNode::new_dir_with_children("src", vec![])]);
        let files = collect_merge_files(&local, &remote, "src");
        assert_eq!(files, vec!["src/app/mod.rs", "src/app/state.rs"]);
    }

    #[test]
    fn test_union_of_both_trees() {
        let local = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![
                FileNode::new_file("local_only.rs"),
                FileNode::new_file("common.rs"),
            ],
        )]);
        let remote = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![
                FileNode::new_file("remote_only.rs"),
                FileNode::new_file("common.rs"),
            ],
        )]);
        let files = collect_merge_files(&local, &remote, "src");
        assert_eq!(
            files,
            vec!["src/local_only.rs", "src/common.rs", "src/remote_only.rs"]
        );
    }

    #[test]
    fn test_one_side_only() {
        let local = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("only_local.rs")],
        )]);
        let remote = make_tree(vec![]); // リモートに src なし
        let files = collect_merge_files(&local, &remote, "src");
        assert_eq!(files, vec!["src/only_local.rs"]);
    }

    #[test]
    fn test_unloaded_subdirectory_skipped() {
        let local = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![
                FileNode::new_file("a.rs"),
                FileNode::new_dir("unloaded"), // children = None
            ],
        )]);
        let remote = make_tree(vec![FileNode::new_dir_with_children("src", vec![])]);
        let files = collect_merge_files(&local, &remote, "src");
        // unloaded ディレクトリはスキップされ、a.rs のみ
        assert_eq!(files, vec!["src/a.rs"]);
    }

    #[test]
    fn test_nonexistent_directory() {
        let local = make_tree(vec![]);
        let remote = make_tree(vec![]);
        let files = collect_merge_files(&local, &remote, "nonexistent");
        assert!(files.is_empty());
    }

    // ── collect_merge_files_3way ──

    #[test]
    fn test_3way_includes_ref_only_files() {
        let local = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs")],
        )]);
        let remote = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs")],
        )]);
        let ref_tree = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![
                FileNode::new_file("a.rs"),
                FileNode::new_file("staging_config.rs"),
            ],
        )]);
        let files = collect_merge_files_3way(&local, &remote, Some(&ref_tree), "src");
        assert!(files.contains(&"src/a.rs".to_string()));
        assert!(files.contains(&"src/staging_config.rs".to_string()));
    }

    #[test]
    fn test_3way_no_ref_same_as_2way() {
        let local = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs")],
        )]);
        let remote = make_tree(vec![FileNode::new_dir_with_children("src", vec![])]);
        let files_2way = collect_merge_files(&local, &remote, "src");
        let files_3way = collect_merge_files_3way(&local, &remote, None, "src");
        assert_eq!(files_2way, files_3way);
    }

    // ── supplement_from_cache / _with_cache テスト ──

    fn empty_cache() -> BoundedCache<String> {
        BoundedCache::new(100)
    }

    fn empty_binary_cache() -> BoundedCache<BinaryInfo> {
        BoundedCache::new(100)
    }

    #[test]
    fn test_with_cache_tree_only() {
        // ツリーからのみ収集可能 → 既存動作と同じ
        let local = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs")],
        )]);
        let remote = make_tree(vec![FileNode::new_dir_with_children("src", vec![])]);

        let files = collect_merge_files_with_cache(
            &local,
            &remote,
            "src",
            &empty_cache(),
            &empty_cache(),
            &empty_binary_cache(),
            &empty_binary_cache(),
        );
        assert_eq!(files, vec!["src/a.rs"]);
    }

    #[test]
    fn test_with_cache_supplements_missing_tree_files() {
        // ツリーにないがキャッシュにあるファイル → リストに追加される
        let local = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs")],
        )]);
        let remote = make_tree(vec![FileNode::new_dir_with_children("src", vec![])]);

        let mut right_cache = empty_cache();
        right_cache.insert("src/b.rs".to_string(), "content".to_string());

        let files = collect_merge_files_with_cache(
            &local,
            &remote,
            "src",
            &empty_cache(),
            &right_cache,
            &empty_binary_cache(),
            &empty_binary_cache(),
        );
        assert!(files.contains(&"src/a.rs".to_string()));
        assert!(files.contains(&"src/b.rs".to_string()));
    }

    #[test]
    fn test_with_cache_deduplication() {
        // 重複排除が正しく動作する（ツリー由来とキャッシュ由来の重複）
        let local = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs")],
        )]);
        let remote = make_tree(vec![FileNode::new_dir_with_children("src", vec![])]);

        let mut left_cache = empty_cache();
        left_cache.insert("src/a.rs".to_string(), "content".to_string());

        let files = collect_merge_files_with_cache(
            &local,
            &remote,
            "src",
            &left_cache,
            &empty_cache(),
            &empty_binary_cache(),
            &empty_binary_cache(),
        );
        // a.rs は1回だけ
        assert_eq!(files.iter().filter(|f| *f == "src/a.rs").count(), 1);
    }

    #[test]
    fn test_with_cache_empty_cache_same_as_tree_only() {
        // 空キャッシュ → ツリーのみの結果と一致
        let local = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs"), FileNode::new_file("b.rs")],
        )]);
        let remote = make_tree(vec![FileNode::new_dir_with_children("src", vec![])]);

        let tree_only = collect_merge_files(&local, &remote, "src");
        let with_cache = collect_merge_files_with_cache(
            &local,
            &remote,
            "src",
            &empty_cache(),
            &empty_cache(),
            &empty_binary_cache(),
            &empty_binary_cache(),
        );
        assert_eq!(tree_only, with_cache);
    }

    #[test]
    fn test_3way_with_cache_includes_cache_files() {
        // 3way 版: ref_tree にのみ存在 + キャッシュにのみ存在 → 両方リストに含まれる
        let local = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs")],
        )]);
        let remote = make_tree(vec![FileNode::new_dir_with_children("src", vec![])]);
        let ref_tree = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("ref_only.rs")],
        )]);

        let mut right_cache = empty_cache();
        right_cache.insert("src/cache_only.rs".to_string(), "content".to_string());

        let files = collect_merge_files_3way_with_cache(
            &local,
            &remote,
            Some(&ref_tree),
            "src",
            &empty_cache(),
            &right_cache,
            &empty_binary_cache(),
            &empty_binary_cache(),
        );
        assert!(files.contains(&"src/a.rs".to_string()));
        assert!(files.contains(&"src/ref_only.rs".to_string()));
        assert!(files.contains(&"src/cache_only.rs".to_string()));
    }

    #[test]
    fn test_with_cache_binary_cache_also_supplements() {
        // バイナリキャッシュからも補完される
        let local = make_tree(vec![FileNode::new_dir_with_children("img", vec![])]);
        let remote = make_tree(vec![FileNode::new_dir_with_children("img", vec![])]);

        let mut left_binary = empty_binary_cache();
        left_binary.insert(
            "img/photo.png".to_string(),
            BinaryInfo::from_bytes(b"\x89PNG"),
        );

        let files = collect_merge_files_with_cache(
            &local,
            &remote,
            "img",
            &empty_cache(),
            &empty_cache(),
            &left_binary,
            &empty_binary_cache(),
        );
        assert!(files.contains(&"img/photo.png".to_string()));
    }

    #[test]
    fn test_with_cache_ignores_unrelated_prefix() {
        // dir_path に一致しないキャッシュキーは含まれない
        let local = make_tree(vec![FileNode::new_dir_with_children("src", vec![])]);
        let remote = make_tree(vec![FileNode::new_dir_with_children("src", vec![])]);

        let mut left_cache = empty_cache();
        left_cache.insert("other/file.rs".to_string(), "content".to_string());
        left_cache.insert("src/valid.rs".to_string(), "content".to_string());

        let files = collect_merge_files_with_cache(
            &local,
            &remote,
            "src",
            &left_cache,
            &empty_cache(),
            &empty_binary_cache(),
            &empty_binary_cache(),
        );
        assert!(files.contains(&"src/valid.rs".to_string()));
        assert!(!files.contains(&"other/file.rs".to_string()));
    }

    // ── 結合テスト: ツリーにローカルのみ + キャッシュに両方 ──

    #[test]
    fn test_integration_tree_local_only_cache_both() {
        // シナリオ B: ツリーにはローカルファイルのみ、キャッシュに両方のコンテンツ
        // → collect_merge_files_with_cache が両方のファイルを返す
        let local = make_tree(vec![FileNode::new_dir_with_children(
            "app",
            vec![FileNode::new_dir_with_children(
                "controllers",
                vec![FileNode::new_file("home.php")],
            )],
        )]);
        // リモートツリーにはディレクトリが存在しない（中間ディレクトリ不在の状態を再現）
        let remote = make_tree(vec![]);

        let mut left_cache = empty_cache();
        left_cache.insert(
            "app/controllers/home.php".to_string(),
            "<?php // local".to_string(),
        );
        let mut right_cache = empty_cache();
        right_cache.insert(
            "app/controllers/home.php".to_string(),
            "<?php // remote".to_string(),
        );
        right_cache.insert(
            "app/controllers/about.php".to_string(),
            "<?php // remote only".to_string(),
        );

        let files = collect_merge_files_with_cache(
            &local,
            &remote,
            "app/controllers",
            &left_cache,
            &right_cache,
            &empty_binary_cache(),
            &empty_binary_cache(),
        );

        // ツリーからは home.php のみ、キャッシュから about.php が補完される
        assert!(files.contains(&"app/controllers/home.php".to_string()));
        assert!(files.contains(&"app/controllers/about.php".to_string()));
    }

    #[test]
    fn test_integration_deep_path_ensure_then_collect() {
        // シナリオ A + B の統合: ensure_path でツリーを修正 → collect_merge_files_with_cache
        let mut remote = make_tree(vec![FileNode::new_dir("ja")]);

        // ensure_path で中間ディレクトリを作成
        remote.ensure_path(std::path::Path::new("ja/Front/process/Common/pc"));
        // children をセット
        if let Some(node) = remote.find_node_mut(std::path::Path::new("ja/Front/process/Common/pc"))
        {
            node.children = Some(vec![
                FileNode::new_file("index.php"),
                FileNode::new_file("edit.php"),
            ]);
        }

        let local = make_tree(vec![FileNode::new_dir_with_children(
            "ja",
            vec![FileNode::new_dir_with_children(
                "Front",
                vec![FileNode::new_dir_with_children(
                    "process",
                    vec![FileNode::new_dir_with_children(
                        "Common",
                        vec![FileNode::new_dir_with_children(
                            "pc",
                            vec![
                                FileNode::new_file("index.php"),
                                FileNode::new_file("list.php"),
                            ],
                        )],
                    )],
                )],
            )],
        )]);

        let files = collect_merge_files_with_cache(
            &local,
            &remote,
            "ja/Front/process/Common/pc",
            &empty_cache(),
            &empty_cache(),
            &empty_binary_cache(),
            &empty_binary_cache(),
        );

        // 両ツリーの union: index.php, list.php (local), edit.php (remote)
        assert!(files.contains(&"ja/Front/process/Common/pc/index.php".to_string()));
        assert!(files.contains(&"ja/Front/process/Common/pc/list.php".to_string()));
        assert!(files.contains(&"ja/Front/process/Common/pc/edit.php".to_string()));
        assert_eq!(files.len(), 3);
    }
}
