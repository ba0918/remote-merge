//! 走査結果を AppState に反映する。
//!
//! MergeScanResult → AppState の変換ロジック。

use std::path::Path;

use crate::app::{AppState, MergeScanResult};

/// 走査結果を AppState に反映する。
///
/// ref キャッシュが含まれている場合、state.ref_cache にも反映する。
pub fn apply_merge_scan_result(state: &mut AppState, result: MergeScanResult) {
    // ツリー更新（中間ディレクトリを自動作成してから children をセット）
    for (path, children) in result.local_tree_updates {
        state.left_tree.ensure_path(Path::new(&path));
        if let Some(node) = state.left_tree.find_node_mut(Path::new(&path)) {
            node.children = Some(children);
            node.sort_children();
        }
    }
    for (path, children) in result.remote_tree_updates {
        state.right_tree.ensure_path(Path::new(&path));
        if let Some(node) = state.right_tree.find_node_mut(Path::new(&path)) {
            node.children = Some(children);
            node.sort_children();
        }
    }

    // NOTE: expanded_dirs には追加しない（ツリー表示の展開状態を変えない）

    // キャッシュ反映（走査結果は新規SSH接続で取得した最新データなので上書き）
    for (path, content) in result.local_cache {
        state.left_cache.insert(path, content);
    }
    for (path, content) in result.remote_cache {
        state.right_cache.insert(path, content);
    }
    for (path, info) in result.local_binary_cache {
        state.left_binary_cache.insert(path, info);
    }
    for (path, info) in result.remote_binary_cache {
        state.right_binary_cache.insert(path, info);
    }

    // ref キャッシュ反映 + コンフリクト検出
    for (path, content) in result.ref_cache {
        // コンフリクト情報を計算してキャッシュ
        if let (Some(left), Some(right)) =
            (state.left_cache.get(&path), state.right_cache.get(&path))
        {
            let info = crate::diff::conflict::detect_conflicts(Some(&content), left, right);
            if !info.is_empty() {
                state.conflict_cache.insert(path.clone(), info);
            } else {
                state.conflict_cache.remove(&path);
            }
        }
        state.ref_cache.insert(path, content);
    }
    for (path, info) in result.ref_binary_cache {
        state.ref_binary_cache.insert(path, info);
    }

    // エラーパス
    state.error_paths.extend(result.error_paths);

    // flat_nodes を再構築
    state.rebuild_flat_nodes();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Side;
    use crate::tree::{FileNode, FileTree};
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;

    fn make_tree(nodes: Vec<FileNode>) -> FileTree {
        FileTree {
            root: PathBuf::from("/test"),
            nodes,
        }
    }

    fn make_state() -> AppState {
        AppState::new(
            make_tree(vec![FileNode::new_file("a.txt")]),
            make_tree(vec![FileNode::new_file("a.txt")]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        )
    }

    fn make_empty_result() -> MergeScanResult {
        MergeScanResult {
            local_cache: HashMap::new(),
            remote_cache: HashMap::new(),
            local_binary_cache: HashMap::new(),
            remote_binary_cache: HashMap::new(),
            ref_cache: HashMap::new(),
            ref_binary_cache: HashMap::new(),
            local_tree_updates: vec![],
            remote_tree_updates: vec![],
            error_paths: HashSet::new(),
        }
    }

    #[test]
    fn apply_ref_cache_populates_state() {
        let mut state = make_state();
        assert!(state.ref_cache.is_empty());

        let mut result = make_empty_result();
        result
            .ref_cache
            .insert("a.txt".to_string(), "ref content".to_string());

        apply_merge_scan_result(&mut state, result);

        assert_eq!(state.ref_cache.get("a.txt").unwrap(), "ref content");
    }

    #[test]
    fn apply_ref_binary_cache_populates_state() {
        let mut state = make_state();
        assert!(state.ref_binary_cache.is_empty());

        let mut result = make_empty_result();
        let info = crate::diff::binary::BinaryInfo::from_bytes(b"\x00\x01\x02\x03");
        result.ref_binary_cache.insert("img.png".to_string(), info);

        apply_merge_scan_result(&mut state, result);

        assert!(state.ref_binary_cache.get("img.png").is_some());
    }

    #[test]
    fn apply_empty_ref_cache_leaves_state_unchanged() {
        let mut state = make_state();
        state
            .ref_cache
            .insert("existing.txt".to_string(), "old".to_string());

        let result = make_empty_result();
        apply_merge_scan_result(&mut state, result);

        // 既存の ref_cache は維持される
        assert_eq!(state.ref_cache.get("existing.txt").unwrap(), "old");
    }

    #[test]
    fn apply_ref_cache_merges_with_existing() {
        let mut state = make_state();
        state
            .ref_cache
            .insert("old.txt".to_string(), "old content".to_string());

        let mut result = make_empty_result();
        result
            .ref_cache
            .insert("new.txt".to_string(), "new content".to_string());

        apply_merge_scan_result(&mut state, result);

        assert_eq!(state.ref_cache.get("old.txt").unwrap(), "old content");
        assert_eq!(state.ref_cache.get("new.txt").unwrap(), "new content");
    }

    #[test]
    fn apply_remote_tree_updates_with_missing_intermediates() {
        // リモートツリーに "a" のみ存在、remote_tree_updates に "a/b/c" がある
        let mut state = AppState::new(
            make_tree(vec![]),
            make_tree(vec![FileNode::new_dir("a")]), // a は未ロード
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );

        let mut result = make_empty_result();
        result
            .remote_tree_updates
            .push(("a/b/c".to_string(), vec![FileNode::new_file("file.rs")]));

        apply_merge_scan_result(&mut state, result);

        // ensure_path により中間ディレクトリが作成され、children がセットされる
        let node = state
            .right_tree
            .find_node(std::path::Path::new("a/b/c"))
            .expect("a/b/c should exist after ensure_path");
        assert!(node.is_dir());
        assert!(node.is_loaded());
        let children = node.children.as_ref().unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "file.rs");
    }

    #[test]
    fn apply_local_tree_updates_with_missing_intermediates() {
        // ローカルツリーに "a" のみ存在
        let mut state = AppState::new(
            make_tree(vec![FileNode::new_dir("a")]),
            make_tree(vec![]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );

        let mut result = make_empty_result();
        result
            .local_tree_updates
            .push(("a/b".to_string(), vec![FileNode::new_file("mod.rs")]));

        apply_merge_scan_result(&mut state, result);

        let node = state
            .left_tree
            .find_node(std::path::Path::new("a/b"))
            .expect("a/b should exist");
        assert_eq!(node.children.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn apply_tree_updates_all_intermediates_exist() {
        // 全中間ディレクトリが既に存在する場合 → 既存動作と同じ
        let mut state = AppState::new(
            make_tree(vec![]),
            make_tree(vec![FileNode::new_dir_with_children(
                "a",
                vec![FileNode::new_dir("b")],
            )]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );

        let mut result = make_empty_result();
        result
            .remote_tree_updates
            .push(("a/b".to_string(), vec![FileNode::new_file("existing.rs")]));

        apply_merge_scan_result(&mut state, result);

        let node = state
            .right_tree
            .find_node(std::path::Path::new("a/b"))
            .unwrap();
        assert_eq!(node.children.as_ref().unwrap().len(), 1);
        assert_eq!(node.children.as_ref().unwrap()[0].name, "existing.rs");
    }

    #[test]
    fn apply_multiple_level_updates_order_independent() {
        // "a/b/c" と "a/b/c/d" の順で更新 → 順序に関わらず正しくツリーが構築される
        let mut state = AppState::new(
            make_tree(vec![]),
            make_tree(vec![FileNode::new_dir("a")]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );

        let mut result = make_empty_result();
        result.remote_tree_updates.push((
            "a/b/c".to_string(),
            vec![FileNode::new_file("file1.rs"), FileNode::new_dir("d")],
        ));
        result
            .remote_tree_updates
            .push(("a/b/c/d".to_string(), vec![FileNode::new_file("file2.rs")]));

        apply_merge_scan_result(&mut state, result);

        assert!(state
            .right_tree
            .find_node(std::path::Path::new("a/b/c"))
            .is_some());
        let d_node = state
            .right_tree
            .find_node(std::path::Path::new("a/b/c/d"))
            .unwrap();
        assert_eq!(d_node.children.as_ref().unwrap().len(), 1);
        assert_eq!(d_node.children.as_ref().unwrap()[0].name, "file2.rs");
    }
}
