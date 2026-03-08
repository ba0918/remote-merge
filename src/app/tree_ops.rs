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
    ///
    /// diff filter モード時はスキャン結果のフルツリーを使用する。
    /// 通常モードでは初期取得ツリー（遅延ロード）を使用する。
    /// ref_tree が設定されている場合は 3-way マージし、ref_only フラグを付与する。
    fn merge_tree_nodes(&self) -> Vec<MergedNode> {
        let ref_nodes = self.ref_tree.as_ref().map(|t| t.nodes.as_slice());

        if self.diff_filter_mode {
            let left = self
                .scan_left_tree
                .as_deref()
                .unwrap_or(&self.left_tree.nodes);
            let right = self
                .scan_right_tree
                .as_deref()
                .unwrap_or(&self.right_tree.nodes);
            merge_node_lists_3way(left, right, ref_nodes)
        } else {
            merge_node_lists_3way(&self.left_tree.nodes, &self.right_tree.nodes, ref_nodes)
        }
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
            let b = self.compute_badge(&path, node.is_dir);
            // スキャン済みで Unchecked ならスキャン結果をフォールバック
            if b == Badge::Unchecked && self.scan_statuses.is_some() {
                self.compute_scan_badge(&path, node.is_dir)
            } else {
                b
            }
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
        if self.search_state.has_query() {
            let query_lower = self.search_state.query.to_lowercase();
            if !node.is_dir {
                if !super::search::name_matches(&node.name, &query_lower) {
                    return;
                }
            } else if !super::search::dir_has_search_matches(node, &self.search_state.query) {
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
            ref_only: node.ref_only,
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
    merge_node_lists_3way(local, remote, None)
}

/// 3つの FileNode リスト（left, right, ref）をマージして MergedNode リストを返す。
///
/// ref にのみ存在するノードは `ref_only: true` でマーク。
/// ref が None の場合は 2-way マージと同等。
pub fn merge_node_lists_3way(
    local: &[FileNode],
    remote: &[FileNode],
    ref_nodes: Option<&[FileNode]>,
) -> Vec<MergedNode> {
    let mut map: BTreeMap<String, MergedNode> = BTreeMap::new();
    // left/right に存在する名前を記録（ref_only 判定用）
    let mut lr_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    for node in local {
        lr_names.insert(node.name.clone());
        let entry = map.entry(node.name.clone()).or_insert_with(|| MergedNode {
            name: node.name.clone(),
            is_dir: node.is_dir(),
            is_symlink: node.is_symlink(),
            children: Vec::new(),
            ref_only: false,
        });
        if node.is_dir() {
            entry.is_dir = true;
            if let Some(children) = &node.children {
                entry.children = merge_node_lists_3way(children, &[], None);
            }
        }
    }

    for node in remote {
        lr_names.insert(node.name.clone());
        let entry = map.entry(node.name.clone()).or_insert_with(|| MergedNode {
            name: node.name.clone(),
            is_dir: node.is_dir(),
            is_symlink: node.is_symlink(),
            children: Vec::new(),
            ref_only: false,
        });
        if node.is_dir() {
            entry.is_dir = true;
            if let Some(children) = &node.children {
                let existing = std::mem::take(&mut entry.children);
                entry.children = merge_merged_with_file_nodes(existing, children);
            }
        }
    }

    // ref ツリーのノードをマージ（ref_only フラグ付き）
    if let Some(ref_nodes) = ref_nodes {
        for node in ref_nodes {
            let is_ref_only = !lr_names.contains(&node.name);
            let entry = map.entry(node.name.clone()).or_insert_with(|| MergedNode {
                name: node.name.clone(),
                is_dir: node.is_dir(),
                is_symlink: node.is_symlink(),
                children: Vec::new(),
                ref_only: is_ref_only,
            });
            // ディレクトリの場合は子ノードも再帰マージ
            if node.is_dir() {
                entry.is_dir = true;
                if let Some(children) = &node.children {
                    let existing = std::mem::take(&mut entry.children);
                    // 既存の子に ref の子をマージ（ref_only 判定含む）
                    entry.children = merge_merged_with_ref_nodes(existing, children);
                }
            }
        }
    }

    let mut result: Vec<MergedNode> = map.into_values().collect();
    sort_merged_nodes(&mut result);
    result
}

/// MergedNode リストと FileNode リストをマージ（right 側の追加用）
///
/// ディレクトリの場合は children を再帰的にマージする。
/// これにより、right にしかないファイルも正しく MergedNode に含まれる。
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
            ref_only: false,
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

/// MergedNode リストと ref FileNode リストをマージ（ref_only 判定付き）
fn merge_merged_with_ref_nodes(merged: Vec<MergedNode>, ref_nodes: &[FileNode]) -> Vec<MergedNode> {
    let mut map: BTreeMap<String, MergedNode> = BTreeMap::new();
    let existing_names: std::collections::HashSet<String> =
        merged.iter().map(|m| m.name.clone()).collect();

    for m in merged {
        map.insert(m.name.clone(), m);
    }

    for node in ref_nodes {
        let is_ref_only = !existing_names.contains(&node.name);
        let entry = map.entry(node.name.clone()).or_insert_with(|| MergedNode {
            name: node.name.clone(),
            is_dir: node.is_dir(),
            is_symlink: node.is_symlink(),
            children: Vec::new(),
            ref_only: is_ref_only,
        });
        if node.is_dir() {
            entry.is_dir = true;
            if let Some(children) = &node.children {
                let existing = std::mem::take(&mut entry.children);
                entry.children = merge_merged_with_ref_nodes(existing, children);
            }
        }
    }

    let mut result: Vec<MergedNode> = map.into_values().collect();
    sort_merged_nodes(&mut result);
    result
}

#[cfg(test)]
mod tests {
    use crate::app::{AppState, Side};
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
            Side::Local,
            Side::Remote("test".to_string()),
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
            Side::Local,
            Side::Remote("test".to_string()),
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
            Side::Local,
            Side::Remote("test".to_string()),
            "default",
        );

        state.rebuild_flat_nodes();
        assert_eq!(state.flat_nodes.len(), 2);
    }

    // ── 3-way merge テスト ──

    #[test]
    fn test_3way_merge_ref_only_file() {
        use super::merge_node_lists_3way;

        let local = vec![FileNode::new_file("a.rs")];
        let remote = vec![FileNode::new_file("b.rs")];
        let ref_nodes = vec![FileNode::new_file("c.rs")];

        let merged = merge_node_lists_3way(&local, &remote, Some(&ref_nodes));
        assert_eq!(merged.len(), 3);

        let c_node = merged.iter().find(|n| n.name == "c.rs").unwrap();
        assert!(c_node.ref_only, "c.rs should be ref_only");

        let a_node = merged.iter().find(|n| n.name == "a.rs").unwrap();
        assert!(!a_node.ref_only, "a.rs should not be ref_only");

        let b_node = merged.iter().find(|n| n.name == "b.rs").unwrap();
        assert!(!b_node.ref_only, "b.rs should not be ref_only");
    }

    #[test]
    fn test_3way_merge_ref_has_same_file() {
        use super::merge_node_lists_3way;

        let local = vec![FileNode::new_file("common.rs")];
        let remote = vec![FileNode::new_file("common.rs")];
        let ref_nodes = vec![FileNode::new_file("common.rs")];

        let merged = merge_node_lists_3way(&local, &remote, Some(&ref_nodes));
        assert_eq!(merged.len(), 1);
        assert!(!merged[0].ref_only, "common.rs exists in left/right");
    }

    #[test]
    fn test_3way_merge_no_ref() {
        use super::merge_node_lists_3way;

        let local = vec![FileNode::new_file("a.rs")];
        let remote = vec![FileNode::new_file("b.rs")];

        let merged = merge_node_lists_3way(&local, &remote, None);
        assert_eq!(merged.len(), 2);
        assert!(merged.iter().all(|n| !n.ref_only));
    }

    #[test]
    fn test_3way_merge_ref_only_in_subdir() {
        use super::merge_node_lists_3way;

        let local = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs")],
        )];
        let remote = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs")],
        )];
        let ref_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![
                FileNode::new_file("a.rs"),
                FileNode::new_file("staging_config.rs"),
            ],
        )];

        let merged = merge_node_lists_3way(&local, &remote, Some(&ref_nodes));
        assert_eq!(merged.len(), 1);
        let src = &merged[0];
        assert!(!src.ref_only, "src dir is in all trees");
        assert_eq!(src.children.len(), 2);

        let staging = src
            .children
            .iter()
            .find(|c| c.name == "staging_config.rs")
            .unwrap();
        assert!(staging.ref_only, "staging_config.rs should be ref_only");

        let a = src.children.iter().find(|c| c.name == "a.rs").unwrap();
        assert!(!a.ref_only, "a.rs should not be ref_only");
    }

    #[test]
    fn test_rebuild_flat_nodes_includes_ref_only() {
        let mut state = AppState::new(
            make_tree(vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_file("a.rs")],
            )]),
            make_tree(vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_file("a.rs")],
            )]),
            Side::Local,
            Side::Remote("develop".to_string()),
            "default",
        );
        state.set_reference(
            Side::Remote("staging".to_string()),
            make_tree(vec![FileNode::new_dir_with_children(
                "src",
                vec![
                    FileNode::new_file("a.rs"),
                    FileNode::new_file("staging_config.rs"),
                ],
            )]),
        );

        // src を展開
        state.expanded_dirs.insert("src".to_string());
        state.rebuild_flat_nodes();

        let names: Vec<(&str, bool)> = state
            .flat_nodes
            .iter()
            .map(|n| (n.name.as_str(), n.ref_only))
            .collect();
        assert!(
            names.contains(&("staging_config.rs", true)),
            "staging_config.rs should appear as ref_only, got: {:?}",
            names
        );
        assert!(
            names.contains(&("a.rs", false)),
            "a.rs should appear as not ref_only"
        );
    }

    /// ref_tree が未展開（children: None）のとき、3way マージで誤った ref_only にならないこと。
    ///
    /// 問題のシナリオ:
    /// - left/right の src は展開済み（children あり）
    /// - ref の src は未展開（children: None）
    ///   → ref の src.children が None なので merge_merged_with_ref_nodes がスキップされ、
    ///   ref に存在するファイルが ref_only として現れない（正しい動作）。
    ///   ただし left/right にある src/a.rs が [3-] になるのは ref が未展開だから判定不能で、
    ///   これはバッジ計算側（compute_ref_file_badge）で ref_tree.find_node が None を返すことで
    ///   正しく None（バッジ非表示）になる。
    #[test]
    fn test_3way_merge_ref_unexpanded_dir_no_false_ref_only() {
        use super::merge_node_lists_3way;

        // left/right: src ディレクトリが展開済み
        let local = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs"), FileNode::new_file("b.rs")],
        )];
        let remote = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs"), FileNode::new_file("b.rs")],
        )];
        // ref: src ディレクトリが未展開（children: None）
        let ref_nodes = vec![FileNode::new_dir("src")];

        let merged = merge_node_lists_3way(&local, &remote, Some(&ref_nodes));
        assert_eq!(merged.len(), 1);
        let src = &merged[0];
        assert!(!src.ref_only, "src exists in all trees");

        // ref の src が未展開（children: None）なので、
        // merge_merged_with_ref_nodes は呼ばれない。
        // left/right の子はそのまま残る（ref_only: false）
        assert_eq!(src.children.len(), 2);
        for child in &src.children {
            assert!(
                !child.ref_only,
                "{} should NOT be ref_only (ref dir is unexpanded)",
                child.name
            );
        }
    }

    /// ref_tree が展開済みのとき、3way マージで正しく ref_only が判定されること。
    #[test]
    fn test_3way_merge_ref_expanded_correct_badges() {
        use super::merge_node_lists_3way;

        let local = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs")],
        )];
        let remote = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs")],
        )];
        // ref: src が展開済み、ref にだけある ref_only.rs を含む
        let ref_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![
                FileNode::new_file("a.rs"),
                FileNode::new_file("ref_only.rs"),
            ],
        )];

        let merged = merge_node_lists_3way(&local, &remote, Some(&ref_nodes));
        let src = &merged[0];
        assert_eq!(src.children.len(), 2);

        let a = src.children.iter().find(|c| c.name == "a.rs").unwrap();
        assert!(!a.ref_only, "a.rs exists in all three");

        let ref_only = src
            .children
            .iter()
            .find(|c| c.name == "ref_only.rs")
            .unwrap();
        assert!(ref_only.ref_only, "ref_only.rs only in ref");
    }

    /// 全3ツリーが同じ深さで展開されているとき、rebuild_flat_nodes で正しい ref_only が出ること。
    /// これは load_ref_children による同期ロード後の期待状態をシミュレートする。
    #[test]
    fn test_rebuild_flat_nodes_3way_all_expanded_consistent() {
        let mut state = AppState::new(
            make_tree(vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_file("main.rs")],
            )]),
            make_tree(vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_file("main.rs")],
            )]),
            Side::Remote("develop".to_string()),
            Side::Remote("staging".to_string()),
            "default",
        );
        // ref も同じ深さで展開済み
        state.set_reference(
            Side::Local,
            make_tree(vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_file("main.rs")],
            )]),
        );

        state.expanded_dirs.insert("src".to_string());
        state.rebuild_flat_nodes();

        // main.rs は全3ツリーに存在 → ref_only: false
        let main_node = state
            .flat_nodes
            .iter()
            .find(|n| n.name == "main.rs")
            .expect("main.rs should be in flat_nodes");
        assert!(!main_node.ref_only, "main.rs exists in all three trees");
    }

    /// develop → staging シナリオ: right にしかないファイルが展開後に見えるか
    #[test]
    fn test_right_only_file_visible_after_expand() {
        // left(develop): src/app に a.rs のみ
        // right(staging): src/app に a.rs + staging_config.rs
        // ref(local): なし
        let mut state = AppState::new(
            make_tree(vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_dir_with_children(
                    "app",
                    vec![FileNode::new_file("a.rs")],
                )],
            )]),
            make_tree(vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_dir_with_children(
                    "app",
                    vec![
                        FileNode::new_file("a.rs"),
                        FileNode::new_file("staging_config.rs"),
                    ],
                )],
            )]),
            Side::Remote("develop".to_string()),
            Side::Remote("staging".to_string()),
            "default",
        );

        // src と src/app を展開
        state.expanded_dirs.insert("src".to_string());
        state.expanded_dirs.insert("src/app".to_string());
        state.rebuild_flat_nodes();

        let names: Vec<&str> = state.flat_nodes.iter().map(|n| n.name.as_str()).collect();
        assert!(
            names.contains(&"staging_config.rs"),
            "staging_config.rs should be visible after expanding src/app, got: {:?}",
            names
        );
    }
}
