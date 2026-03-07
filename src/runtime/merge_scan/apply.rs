//! 走査結果を AppState に反映する。
//!
//! MergeScanResult → AppState の変換ロジック。

use std::path::Path;

use crate::app::{AppState, MergeScanResult};

/// 走査結果を AppState に反映する
pub fn apply_merge_scan_result(state: &mut AppState, result: MergeScanResult) {
    // ツリー更新
    for (path, children) in result.local_tree_updates {
        if let Some(node) = state.left_tree.find_node_mut(Path::new(&path)) {
            node.children = Some(children);
            node.sort_children();
        }
    }
    for (path, children) in result.remote_tree_updates {
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

    // エラーパス
    state.error_paths.extend(result.error_paths);

    // flat_nodes を再構築
    state.rebuild_flat_nodes();
}
