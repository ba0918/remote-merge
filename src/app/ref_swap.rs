//! right ↔ ref サーバスワップのドメインロジック。
//!
//! `X` キーで right と reference サーバを交換する。
//! キャッシュを in-memory swap するだけなので接続コストはゼロ。

use super::side::comparison_label;
use super::AppState;

impl AppState {
    /// right ↔ ref のサーバ・ツリー・キャッシュを交換する。
    ///
    /// - `ref_source` が None なら何もしない（early return）
    /// - undo_stack はペアが変わるためクリア
    /// - current_diff は None にリセット
    /// - showing_ref_diff は false にリセット
    /// - flat_nodes を再構築
    pub fn swap_right_ref(&mut self) {
        let Some(ref_src) = self.ref_source.take() else {
            return;
        };

        // source 交換
        let old_right_source = std::mem::replace(&mut self.right_source, ref_src);
        self.ref_source = Some(old_right_source);

        // tree 交換
        let old_right_tree = std::mem::replace(
            &mut self.right_tree,
            self.ref_tree.take().unwrap_or_default(),
        );
        self.ref_tree = Some(old_right_tree);

        // テキストキャッシュ交換
        std::mem::swap(&mut self.right_cache, &mut self.ref_cache);

        // バイナリキャッシュ交換
        std::mem::swap(&mut self.right_binary_cache, &mut self.ref_binary_cache);

        // undo_stack クリア（ペアが変わるため）
        self.undo_stack.clear();

        // diff 状態リセット
        self.current_diff = None;
        self.showing_ref_diff = false;

        // flat_nodes 再構築
        self.rebuild_flat_nodes();

        // ステータスメッセージ更新
        let label = comparison_label(&self.left_source, &self.right_source);
        let ref_name = self
            .ref_source
            .as_ref()
            .map(|s| s.display_name())
            .unwrap_or("none");
        self.status_message = format!("Swapped: {} | ref: {}", label, ref_name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Side;
    use crate::tree::{FileNode, FileTree};
    use std::path::PathBuf;

    fn make_tree(nodes: Vec<FileNode>) -> FileTree {
        FileTree {
            root: PathBuf::from("/test"),
            nodes,
        }
    }

    fn make_state_with_ref() -> AppState {
        let mut state = AppState::new(
            make_tree(vec![FileNode::new_file("a.rs")]),
            make_tree(vec![FileNode::new_file("b.rs")]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state.set_reference(
            Side::Remote("staging".to_string()),
            make_tree(vec![FileNode::new_file("c.rs")]),
        );
        state
    }

    #[test]
    fn swap_exchanges_sources() {
        let mut state = make_state_with_ref();
        state.swap_right_ref();
        assert_eq!(state.right_source, Side::Remote("staging".to_string()));
        assert_eq!(state.ref_source, Some(Side::Remote("develop".to_string())));
    }

    #[test]
    fn swap_exchanges_trees() {
        let mut state = make_state_with_ref();
        // right は b.rs, ref は c.rs
        state.swap_right_ref();
        // swap 後: right に c.rs, ref に b.rs
        let right_names: Vec<&str> = state
            .right_tree
            .nodes
            .iter()
            .map(|n| n.name.as_str())
            .collect();
        assert!(
            right_names.contains(&"c.rs"),
            "right should have c.rs after swap"
        );
        let ref_names: Vec<&str> = state
            .ref_tree
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .map(|n| n.name.as_str())
            .collect();
        assert!(
            ref_names.contains(&"b.rs"),
            "ref should have b.rs after swap"
        );
    }

    #[test]
    fn swap_exchanges_caches() {
        let mut state = make_state_with_ref();
        state
            .right_cache
            .insert("b.rs".to_string(), "right_content".to_string());
        state
            .ref_cache
            .insert("c.rs".to_string(), "ref_content".to_string());

        state.swap_right_ref();

        assert_eq!(
            state.right_cache.get("c.rs"),
            Some(&"ref_content".to_string())
        );
        assert_eq!(
            state.ref_cache.get("b.rs"),
            Some(&"right_content".to_string())
        );
    }

    #[test]
    fn swap_clears_undo_stack() {
        let mut state = make_state_with_ref();
        state.undo_stack.push_back(crate::app::CacheSnapshot {
            local_content: String::new(),
            remote_content: String::new(),
            diff: None,
        });
        state.swap_right_ref();
        assert!(state.undo_stack.is_empty());
    }

    #[test]
    fn swap_clears_current_diff() {
        let mut state = make_state_with_ref();
        state.current_diff = Some(crate::diff::engine::DiffResult::Equal);
        state.swap_right_ref();
        assert!(state.current_diff.is_none());
    }

    #[test]
    fn swap_updates_right_source() {
        let mut state = make_state_with_ref();
        assert_eq!(state.right_source.display_name(), "develop");
        state.swap_right_ref();
        assert_eq!(state.right_source.display_name(), "staging");
    }

    #[test]
    fn swap_resets_showing_ref_diff() {
        let mut state = make_state_with_ref();
        state.showing_ref_diff = true;
        state.swap_right_ref();
        assert!(!state.showing_ref_diff);
    }

    #[test]
    fn swap_noop_when_no_ref() {
        let mut state = AppState::new(
            make_tree(vec![FileNode::new_file("a.rs")]),
            make_tree(vec![FileNode::new_file("b.rs")]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        // ref なし
        let old_right = state.right_source.clone();
        state.swap_right_ref();
        assert_eq!(state.right_source, old_right);
        assert!(state.ref_source.is_none());
    }

    #[test]
    fn swap_twice_restores_original() {
        let mut state = make_state_with_ref();
        let orig_right = state.right_source.clone();
        let orig_ref = state.ref_source.clone();
        state.swap_right_ref();
        state.swap_right_ref();
        assert_eq!(state.right_source, orig_right);
        assert_eq!(state.ref_source, orig_ref);
    }

    #[test]
    fn swap_with_ref_tree_none() {
        let mut state = AppState::new(
            make_tree(vec![]),
            make_tree(vec![FileNode::new_file("b.rs")]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        // ref_source はあるが ref_tree は None
        state.ref_source = Some(Side::Remote("staging".to_string()));
        state.ref_tree = None;

        state.swap_right_ref();
        assert_eq!(state.right_source, Side::Remote("staging".to_string()));
        // right_tree は空（ref_tree が None だったので default）
        assert!(state.right_tree.nodes.is_empty());
        // ref_tree は旧 right_tree（b.rs）
        let ref_tree = state.ref_tree.as_ref().unwrap();
        assert!(!ref_tree.nodes.is_empty());
    }
}
