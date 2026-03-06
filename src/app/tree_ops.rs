//! ツリー操作（フラット化、マージ、展開/折りたたみ）。

use std::collections::BTreeMap;

use crate::tree::FileNode;

use super::types::{Badge, FlatNode, MergedNode};
use super::AppState;

/// MergedNode をディレクトリ優先・名前順でソートする
fn sort_merged_nodes(nodes: &mut [MergedNode]) {
    nodes.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });
}

impl AppState {
    /// ディレクトリの展開/折りたたみを切り替える
    pub fn toggle_expand(&mut self) {
        if let Some(node) = self.flat_nodes.get(self.tree_cursor) {
            if node.is_dir {
                let path = node.path.clone();
                if self.expanded_dirs.contains(&path) {
                    self.expanded_dirs.remove(&path);
                } else {
                    self.expanded_dirs.insert(path);
                }
                self.rebuild_flat_nodes();
            }
        }
    }

    /// ファイルツリーをフラット化して flat_nodes を再構築する
    pub fn rebuild_flat_nodes(&mut self) {
        let mut nodes = Vec::new();
        let merged = self.merge_tree_nodes();
        for node in &merged {
            self.flatten_node(node, "", 0, &mut nodes);
        }
        self.flat_nodes = nodes;
        // カーソル位置を範囲内に収める
        if self.tree_cursor >= self.flat_nodes.len() && !self.flat_nodes.is_empty() {
            self.tree_cursor = self.flat_nodes.len() - 1;
        }
    }

    /// ローカルとリモートのルートノードをマージ（和集合）する
    fn merge_tree_nodes(&self) -> Vec<MergedNode> {
        merge_node_lists(&self.local_tree.nodes, &self.remote_tree.nodes)
    }

    /// 再帰的にフラット化する
    fn flatten_node(
        &self,
        node: &MergedNode,
        parent_path: &str,
        depth: usize,
        out: &mut Vec<FlatNode>,
    ) {
        let path = if parent_path.is_empty() {
            node.name.clone()
        } else {
            format!("{}/{}", parent_path, node.name)
        };

        let expanded = self.expanded_dirs.contains(&path);
        let badge = if self.diff_filter_mode {
            self.compute_scan_badge(&path, node.is_dir)
        } else {
            self.compute_badge(&path, node.is_dir)
        };

        // フィルターモード時: Equal ファイルをスキップ
        if self.diff_filter_mode && !node.is_dir && badge == Badge::Equal {
            return;
        }

        // フィルターモード時: ディレクトリは配下に差分があるかチェック
        if self.diff_filter_mode && node.is_dir && !self.dir_has_diff_children(node, &path) {
            return;
        }

        // 検索フィルタリング: クエリにマッチしないファイルをスキップ
        let search_query = &self.search_state.query;
        if self.search_state.has_query() {
            if !node.is_dir {
                let query_lower = search_query.to_lowercase();
                if !node.name.to_lowercase().contains(&query_lower) {
                    return;
                }
            } else if !super::search::dir_has_search_matches(node, search_query) {
                return;
            }
        }

        out.push(FlatNode {
            path: path.clone(),
            name: node.name.clone(),
            depth,
            is_dir: node.is_dir,
            is_symlink: node.is_symlink,
            expanded,
            badge,
        });

        // 検索中はマッチする子孫がいるディレクトリを自動展開する
        let force_expand = self.search_state.has_query();
        if node.is_dir && (expanded || force_expand) {
            for child in &node.children {
                self.flatten_node(child, &path, depth + 1, out);
            }
        }
    }
}

/// 2つの FileNode リストをマージして MergedNode リストを返す
pub fn merge_node_lists(local: &[FileNode], remote: &[FileNode]) -> Vec<MergedNode> {
    let mut map: BTreeMap<String, MergedNode> = BTreeMap::new();

    for node in local {
        let entry = map.entry(node.name.clone()).or_insert_with(|| MergedNode {
            name: node.name.clone(),
            is_dir: node.is_dir(),
            is_symlink: node.is_symlink(),
            children: Vec::new(),
        });
        if node.is_dir() {
            entry.is_dir = true;
            if let Some(children) = &node.children {
                entry.children = merge_node_lists(children, &[]);
            }
        }
    }

    for node in remote {
        let entry = map.entry(node.name.clone()).or_insert_with(|| MergedNode {
            name: node.name.clone(),
            is_dir: node.is_dir(),
            is_symlink: node.is_symlink(),
            children: Vec::new(),
        });
        if node.is_dir() {
            entry.is_dir = true;
            if let Some(children) = &node.children {
                let existing = std::mem::take(&mut entry.children);
                entry.children = merge_merged_with_file_nodes(existing, children);
            }
        }
    }

    let mut result: Vec<MergedNode> = map.into_values().collect();
    sort_merged_nodes(&mut result);
    result
}

/// MergedNode リストと FileNode リストをマージ
fn merge_merged_with_file_nodes(
    merged: Vec<MergedNode>,
    file_nodes: &[FileNode],
) -> Vec<MergedNode> {
    let mut map: BTreeMap<String, MergedNode> = BTreeMap::new();

    for m in merged {
        map.insert(m.name.clone(), m);
    }

    for node in file_nodes {
        let entry = map.entry(node.name.clone()).or_insert_with(|| MergedNode {
            name: node.name.clone(),
            is_dir: node.is_dir(),
            is_symlink: node.is_symlink(),
            children: Vec::new(),
        });
        if node.is_dir() {
            entry.is_dir = true;
        }
    }

    let mut result: Vec<MergedNode> = map.into_values().collect();
    sort_merged_nodes(&mut result);
    result
}

#[cfg(test)]
mod tests {
    use crate::app::AppState;
    use crate::tree::{FileNode, FileTree};
    use std::path::PathBuf;

    fn make_tree(nodes: Vec<FileNode>) -> FileTree {
        FileTree {
            root: PathBuf::from("/test"),
            nodes,
        }
    }

    #[test]
    fn test_search_auto_expands_directories() {
        // サブディレクトリ src/app/search.rs を持つツリー
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_dir_with_children(
                "app",
                vec![FileNode::new_file("search.rs")],
            )],
        )];
        let mut state = AppState::new(
            make_tree(local_nodes),
            make_tree(vec![]),
            "test".to_string(),
            "default",
        );

        // 展開せずに検索 → search.rs がフラットノードに含まれるべき
        state.search_state.query = "search".to_string();
        state.rebuild_flat_nodes();

        let names: Vec<&str> = state.flat_nodes.iter().map(|n| n.name.as_str()).collect();
        assert!(
            names.contains(&"search.rs"),
            "Search should auto-expand and find nested files, got: {:?}",
            names
        );
        assert!(names.contains(&"src"));
        assert!(names.contains(&"app"));
    }

    #[test]
    fn test_search_filter_hides_non_matching() {
        let local_nodes = vec![
            FileNode::new_file("main.rs"),
            FileNode::new_file("lib.rs"),
            FileNode::new_file("utils.rs"),
        ];
        let mut state = AppState::new(
            make_tree(local_nodes),
            make_tree(vec![]),
            "test".to_string(),
            "default",
        );

        state.search_state.query = "main".to_string();
        state.rebuild_flat_nodes();

        assert_eq!(state.flat_nodes.len(), 1);
        assert_eq!(state.flat_nodes[0].name, "main.rs");
    }

    #[test]
    fn test_no_search_shows_all() {
        let local_nodes = vec![FileNode::new_file("main.rs"), FileNode::new_file("lib.rs")];
        let mut state = AppState::new(
            make_tree(local_nodes),
            make_tree(vec![]),
            "test".to_string(),
            "default",
        );

        state.rebuild_flat_nodes();
        assert_eq!(state.flat_nodes.len(), 2);
    }
}
