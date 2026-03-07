//! バッジ計算（ファイル差分状態の判定）。

use std::path::Path;

use crate::service::types::FileStatusKind;

use super::types::{Badge, MergedNode};
use super::AppState;

impl AppState {
    /// ローカル/リモートのツリーを比較してバッジを決定する
    pub fn compute_badge(&self, path: &str, is_dir: bool) -> Badge {
        if is_dir {
            return self.compute_dir_badge(path);
        }

        // エラーパスの場合は Error バッジ
        if self.error_paths.contains(path) {
            return Badge::Error;
        }

        use crate::tree::NodePresence;

        let local_presence = self.left_tree.find_node_or_unloaded(Path::new(path));
        let remote_presence = self.right_tree.find_node_or_unloaded(Path::new(path));

        // 確実に片方のみと言えるときだけ LocalOnly/RemoteOnly
        let in_local = local_presence == NodePresence::Found;
        let in_remote = remote_presence == NodePresence::Found;
        let local_absent = local_presence == NodePresence::NotFound;
        let remote_absent = remote_presence == NodePresence::NotFound;

        match (in_local, in_remote) {
            (true, true) => {
                // シンボリックリンクの場合はリンク先パスで比較
                let local_node = self.left_tree.find_node(path);
                let remote_node = self.right_tree.find_node(path);
                if let (Some(ln), Some(rn)) = (local_node, remote_node) {
                    if ln.is_symlink() || rn.is_symlink() {
                        return self.compute_symlink_badge(ln, rn);
                    }
                }

                // バイナリキャッシュに両方あれば SHA-256 で判定
                let local_bin = self.left_binary_cache.get(path);
                let remote_bin = self.right_binary_cache.get(path);
                if let (Some(lb), Some(rb)) = (local_bin, remote_bin) {
                    return if lb.is_same_content(rb) {
                        Badge::Equal
                    } else {
                        Badge::Modified
                    };
                }

                // テキストキャッシュに両方あれば diff で判定
                match (self.left_cache.get(path), self.right_cache.get(path)) {
                    (Some(l), Some(r)) => {
                        if l == r {
                            Badge::Equal
                        } else {
                            Badge::Modified
                        }
                    }
                    _ => Badge::Unchecked,
                }
            }
            (true, false) if remote_absent => Badge::LeftOnly,
            (false, true) if local_absent => Badge::RightOnly,
            _ => {
                // ツリー上で片方が Unloaded でも、キャッシュに両方あればコンテンツで判定。
                // 検索時にリモートツリーが未展開でも、コンテンツ読み込み済みなら正しいバッジを返す。
                match (self.left_cache.get(path), self.right_cache.get(path)) {
                    (Some(l), Some(r)) => {
                        if l == r {
                            Badge::Equal
                        } else {
                            Badge::Modified
                        }
                    }
                    (Some(_), None) if remote_absent => Badge::LeftOnly,
                    (None, Some(_)) if local_absent => Badge::RightOnly,
                    _ => Badge::Unchecked,
                }
            }
        }
    }

    /// ディレクトリのバッジを配下ファイルの状態から計算する。
    ///
    /// ルール：
    /// - `[M]` — 配下に1つでも差分ファイルがあれば確定（早期return）
    /// - `[=]` — 配下の全ファイルがキャッシュ済みかつ全部Equal
    /// - `[?]` — 未確認ファイルが残っている、またはキャッシュが空
    fn compute_dir_badge(&self, path: &str) -> Badge {
        let in_local = self
            .left_tree
            .find_node(std::path::Path::new(path))
            .is_some();
        let in_remote = self
            .right_tree
            .find_node(std::path::Path::new(path))
            .is_some();

        match (in_local, in_remote) {
            (true, false) => Badge::LeftOnly,
            (false, true) => Badge::RightOnly,
            (false, false) => Badge::Unchecked,
            (true, true) => {
                // ツリーから配下の全ファイルを列挙
                let all_files = super::merge_collect::collect_merge_files(
                    &self.left_tree,
                    &self.right_tree,
                    path,
                );

                if all_files.is_empty() {
                    return Badge::Unchecked;
                }

                // 各ファイルのバッジを集計
                let mut all_checked = true;
                for file_path in &all_files {
                    let badge = self.compute_badge(file_path, false);
                    match badge {
                        Badge::Modified | Badge::LeftOnly | Badge::RightOnly | Badge::Error => {
                            return Badge::Modified;
                        }
                        Badge::Unchecked => {
                            all_checked = false;
                        }
                        Badge::Equal | Badge::Loading => {}
                    }
                }

                if all_checked {
                    Badge::Equal
                } else {
                    Badge::Unchecked
                }
            }
        }
    }

    /// シンボリックリンクのバッジを計算する。
    /// 両方symlink → ターゲット比較、片方のみ → Modified。
    fn compute_symlink_badge(
        &self,
        local_node: &crate::tree::FileNode,
        remote_node: &crate::tree::FileNode,
    ) -> Badge {
        use crate::tree::NodeKind;
        match (&local_node.kind, &remote_node.kind) {
            (NodeKind::Symlink { target: lt }, NodeKind::Symlink { target: rt }) => {
                if lt == rt {
                    Badge::Equal
                } else {
                    Badge::Modified
                }
            }
            // 片方だけsymlink → 型が異なるので Modified
            _ => Badge::Modified,
        }
    }

    /// 走査結果を使ったバッジ計算（CLI と共通の差分ステータスを使用）
    ///
    /// `scan_statuses`（`service::status::compute_status_from_trees` で計算）から
    /// ルックアップする。CLI の status コマンドと同一の検出ロジックを使うことで、
    /// TUI と CLI で検出結果が一致する。
    pub fn compute_scan_badge(&self, path: &str, is_dir: bool) -> Badge {
        if is_dir {
            return self.compute_scan_dir_badge(path);
        }

        // まずキャッシュベースの正確なバッジがあればそれを使う
        let cache_badge = self.compute_badge(path, false);
        if cache_badge != Badge::Unchecked {
            return cache_badge;
        }

        // scan_statuses から差分ステータスをルックアップ
        self.badge_from_scan_statuses(path)
    }

    /// scan_statuses からファイルのバッジを引く
    fn badge_from_scan_statuses(&self, path: &str) -> Badge {
        if let Some(statuses) = &self.scan_statuses {
            if let Some(status) = statuses.get(path) {
                return match status {
                    FileStatusKind::Equal => Badge::Equal,
                    FileStatusKind::Modified => Badge::Modified,
                    FileStatusKind::LeftOnly => Badge::LeftOnly,
                    FileStatusKind::RightOnly => Badge::RightOnly,
                };
            }
        }
        Badge::Unchecked
    }

    /// スキャン結果を使ったディレクトリバッジ計算。
    ///
    /// `scan_statuses` の全エントリのうち、このディレクトリ配下のパスを集計する。
    /// キャッシュベースの `compute_dir_badge` と違い、スキャン済みの全ファイルを
    /// カバーできるため、未展開ディレクトリでも正確なバッジを返せる。
    fn compute_scan_dir_badge(&self, path: &str) -> Badge {
        let statuses = match &self.scan_statuses {
            Some(s) => s,
            None => return self.compute_dir_badge(path),
        };

        let prefix = format!("{}/", path);
        let mut has_children = false;
        let mut all_equal = true;

        for (file_path, status) in statuses {
            if !file_path.starts_with(&prefix) {
                continue;
            }
            has_children = true;

            // キャッシュがあればキャッシュを優先（ユーザーが diff を開いた後の更新を反映）
            let badge = {
                let cache_b = self.compute_badge(file_path, false);
                if cache_b != Badge::Unchecked {
                    cache_b
                } else {
                    match status {
                        FileStatusKind::Equal => Badge::Equal,
                        FileStatusKind::Modified => Badge::Modified,
                        FileStatusKind::LeftOnly => Badge::LeftOnly,
                        FileStatusKind::RightOnly => Badge::RightOnly,
                    }
                }
            };

            match badge {
                Badge::Modified | Badge::LeftOnly | Badge::RightOnly | Badge::Error => {
                    return Badge::Modified;
                }
                Badge::Equal => {}
                Badge::Unchecked | Badge::Loading => {
                    all_equal = false;
                }
            }
        }

        if !has_children {
            return self.compute_dir_badge(path);
        }

        if all_equal {
            Badge::Equal
        } else {
            Badge::Unchecked
        }
    }

    /// ディレクトリ配下に差分のある子ノードが存在するか（再帰チェック）
    pub fn dir_has_diff_children(&self, node: &MergedNode, parent_path: &str) -> bool {
        for child in &node.children {
            let child_path = format!("{}/{}", parent_path, child.name);
            let badge = if self.diff_filter_mode {
                self.compute_scan_badge(&child_path, child.is_dir)
            } else {
                self.compute_badge(&child_path, child.is_dir)
            };

            if child.is_dir {
                if self.dir_has_diff_children(child, &child_path) {
                    return true;
                }
            } else if badge != Badge::Equal {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Side;
    use crate::tree::{FileNode, FileTree};
    use std::path::PathBuf;

    fn make_test_tree(nodes: Vec<FileNode>) -> FileTree {
        FileTree {
            root: PathBuf::from("/test"),
            nodes,
        }
    }

    // ── ファイルバッジ ──

    #[test]
    fn test_badge_unchecked_when_no_cache() {
        let state = AppState::new(
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        assert_eq!(state.compute_badge("a.txt", false), Badge::Unchecked);
    }

    #[test]
    fn test_badge_equal_when_same_content() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state
            .left_cache
            .insert("a.txt".to_string(), "same".to_string());
        state
            .right_cache
            .insert("a.txt".to_string(), "same".to_string());
        assert_eq!(state.compute_badge("a.txt", false), Badge::Equal);
    }

    #[test]
    fn test_badge_modified_when_different_content() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state
            .left_cache
            .insert("a.txt".to_string(), "old".to_string());
        state
            .right_cache
            .insert("a.txt".to_string(), "new".to_string());
        assert_eq!(state.compute_badge("a.txt", false), Badge::Modified);
    }

    #[test]
    fn test_badge_local_only_not_error_when_remote_missing() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("local_only.rs")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children("src", vec![])];
        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state
            .left_cache
            .insert("src/local_only.rs".to_string(), "content".to_string());
        assert_eq!(
            state.compute_badge("src/local_only.rs", false),
            Badge::LeftOnly
        );
    }

    #[test]
    fn test_badge_remote_only_not_error_when_local_missing() {
        let local_nodes = vec![FileNode::new_dir_with_children("src", vec![])];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("remote_only.rs")],
        )];
        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state
            .right_cache
            .insert("src/remote_only.rs".to_string(), "content".to_string());
        assert_eq!(
            state.compute_badge("src/remote_only.rs", false),
            Badge::RightOnly
        );
    }

    #[test]
    fn test_badge_error_only_when_both_fail() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_file("broken.rs")]),
            make_test_tree(vec![FileNode::new_file("broken.rs")]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state.error_paths.insert("broken.rs".to_string());
        assert_eq!(state.compute_badge("broken.rs", false), Badge::Error);
    }

    #[test]
    fn test_error_paths_cleared_after_state_reset() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state.error_paths.insert("a.txt".to_string());
        assert_eq!(state.compute_badge("a.txt", false), Badge::Error);
        state.clear_cache();
        assert_ne!(state.compute_badge("a.txt", false), Badge::Error);
    }

    // ── リモートツリー未ロード時のバッジ ──

    #[test]
    fn test_badge_equal_when_remote_tree_unloaded_but_cache_exists() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("scan.rs")],
        )];
        let remote_nodes = vec![FileNode::new_dir("src")];
        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state
            .left_cache
            .insert("src/scan.rs".to_string(), "content".to_string());
        state
            .right_cache
            .insert("src/scan.rs".to_string(), "content".to_string());
        assert_eq!(state.compute_badge("src/scan.rs", false), Badge::Equal);
    }

    #[test]
    fn test_badge_modified_when_remote_tree_unloaded_but_cache_differs() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("scan.rs")],
        )];
        let remote_nodes = vec![FileNode::new_dir("src")];
        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state
            .left_cache
            .insert("src/scan.rs".to_string(), "old".to_string());
        state
            .right_cache
            .insert("src/scan.rs".to_string(), "new".to_string());
        assert_eq!(state.compute_badge("src/scan.rs", false), Badge::Modified);
    }

    // ── ディレクトリバッジ ──

    #[test]
    fn test_dir_badge_equal_when_all_children_equal() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts"), FileNode::new_file("b.ts")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts"), FileNode::new_file("b.ts")],
        )];
        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state
            .left_cache
            .insert("src/a.ts".to_string(), "same".to_string());
        state
            .right_cache
            .insert("src/a.ts".to_string(), "same".to_string());
        state
            .left_cache
            .insert("src/b.ts".to_string(), "same".to_string());
        state
            .right_cache
            .insert("src/b.ts".to_string(), "same".to_string());
        assert_eq!(state.compute_badge("src", true), Badge::Equal);
    }

    #[test]
    fn test_dir_badge_modified_when_child_differs() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts")],
        )];
        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state
            .left_cache
            .insert("src/a.ts".to_string(), "old".to_string());
        state
            .right_cache
            .insert("src/a.ts".to_string(), "new".to_string());
        assert_eq!(state.compute_badge("src", true), Badge::Modified);
    }

    #[test]
    fn test_dir_badge_local_only() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts")],
        )];
        let state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(vec![]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        assert_eq!(state.compute_badge("src", true), Badge::LeftOnly);
    }

    #[test]
    fn test_dir_badge_unchecked_when_no_cache() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts")],
        )];
        let state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        assert_eq!(state.compute_badge("src", true), Badge::Unchecked);
    }

    #[test]
    fn test_dir_badge_unchecked_when_partial_cache() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts"), FileNode::new_file("b.ts")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts"), FileNode::new_file("b.ts")],
        )];
        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state
            .left_cache
            .insert("src/a.ts".to_string(), "same".to_string());
        state
            .right_cache
            .insert("src/a.ts".to_string(), "same".to_string());
        assert_eq!(state.compute_badge("src", true), Badge::Unchecked);
    }

    #[test]
    fn test_dir_badge_modified_even_with_unchecked_siblings() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts"), FileNode::new_file("b.ts")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts"), FileNode::new_file("b.ts")],
        )];
        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state
            .left_cache
            .insert("src/a.ts".to_string(), "old".to_string());
        state
            .right_cache
            .insert("src/a.ts".to_string(), "new".to_string());
        assert_eq!(state.compute_badge("src", true), Badge::Modified);
    }

    // ── シンボリックリンクバッジ ──

    #[test]
    fn test_symlink_badge_equal_when_same_target() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_symlink("link", "../README.md")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_symlink("link", "../README.md")],
        )];
        let state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        assert_eq!(state.compute_badge("src/link", false), Badge::Equal);
    }

    #[test]
    fn test_symlink_badge_modified_when_different_target() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_symlink("link", "../README.md")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_symlink("link", "../OTHER.md")],
        )];
        let state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        assert_eq!(state.compute_badge("src/link", false), Badge::Modified);
    }

    #[test]
    fn test_symlink_badge_modified_when_mixed_types() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_symlink("file", "target")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("file")],
        )];
        let state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        assert_eq!(state.compute_badge("src/file", false), Badge::Modified);
    }

    // ── バイナリキャッシュバッジ ──

    #[test]
    fn test_binary_cache_badge_equal() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("logo.png")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("logo.png")],
        )];
        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        let info = crate::diff::binary::BinaryInfo {
            size: 100,
            sha256: "abc123".to_string(),
        };
        state
            .left_binary_cache
            .insert("src/logo.png".to_string(), info.clone());
        state
            .right_binary_cache
            .insert("src/logo.png".to_string(), info);
        assert_eq!(state.compute_badge("src/logo.png", false), Badge::Equal);
    }

    #[test]
    fn test_binary_cache_badge_modified() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("logo.png")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("logo.png")],
        )];
        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state.left_binary_cache.insert(
            "src/logo.png".to_string(),
            crate::diff::binary::BinaryInfo {
                size: 100,
                sha256: "abc".to_string(),
            },
        );
        state.right_binary_cache.insert(
            "src/logo.png".to_string(),
            crate::diff::binary::BinaryInfo {
                size: 200,
                sha256: "def".to_string(),
            },
        );
        assert_eq!(state.compute_badge("src/logo.png", false), Badge::Modified);
    }

    // ── compute_scan_badge ──

    #[test]
    fn test_compute_scan_badge_without_scan_statuses_returns_unchecked() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state.scan_statuses = None;
        assert_eq!(state.compute_scan_badge("a.txt", false), Badge::Unchecked);
    }

    #[test]
    fn test_compute_scan_badge_prefers_content_cache() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state
            .left_cache
            .insert("a.txt".to_string(), "same".to_string());
        state
            .right_cache
            .insert("a.txt".to_string(), "same".to_string());
        // scan_statuses ではModifiedだが、キャッシュが優先される
        state.scan_statuses = Some(std::collections::HashMap::from([(
            "a.txt".to_string(),
            crate::service::types::FileStatusKind::Modified,
        )]));
        assert_eq!(state.compute_scan_badge("a.txt", false), Badge::Equal);
    }

    #[test]
    fn test_compute_scan_badge_uses_statuses_when_no_cache() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        // キャッシュなしで scan_statuses から判定
        state.scan_statuses = Some(std::collections::HashMap::from([(
            "a.txt".to_string(),
            crate::service::types::FileStatusKind::Modified,
        )]));
        assert_eq!(state.compute_scan_badge("a.txt", false), Badge::Modified);
    }

    #[test]
    fn test_compute_scan_badge_equal_from_statuses() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state.scan_statuses = Some(std::collections::HashMap::from([(
            "a.txt".to_string(),
            crate::service::types::FileStatusKind::Equal,
        )]));
        assert_eq!(state.compute_scan_badge("a.txt", false), Badge::Equal);
    }

    #[test]
    fn test_compute_scan_badge_left_only_from_statuses() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            make_test_tree(vec![]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state.scan_statuses = Some(std::collections::HashMap::from([(
            "a.txt".to_string(),
            crate::service::types::FileStatusKind::LeftOnly,
        )]));
        assert_eq!(state.compute_scan_badge("a.txt", false), Badge::LeftOnly);
    }

    // ── compute_scan_dir_badge ──

    #[test]
    fn test_scan_dir_badge_modified_when_child_modified() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_dir_with_children(
                "config",
                vec![FileNode::new_file("settings.toml")],
            )]),
            make_test_tree(vec![FileNode::new_dir_with_children(
                "config",
                vec![FileNode::new_file("settings.toml")],
            )]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state.scan_statuses = Some(std::collections::HashMap::from([(
            "config/settings.toml".to_string(),
            FileStatusKind::Modified,
        )]));
        assert_eq!(state.compute_scan_badge("config", true), Badge::Modified);
    }

    #[test]
    fn test_scan_dir_badge_equal_when_all_children_equal() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_file("a.rs"), FileNode::new_file("b.rs")],
            )]),
            make_test_tree(vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_file("a.rs"), FileNode::new_file("b.rs")],
            )]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state.scan_statuses = Some(std::collections::HashMap::from([
            ("src/a.rs".to_string(), FileStatusKind::Equal),
            ("src/b.rs".to_string(), FileStatusKind::Equal),
        ]));
        assert_eq!(state.compute_scan_badge("src", true), Badge::Equal);
    }

    #[test]
    fn test_scan_dir_badge_falls_back_without_statuses() {
        let state = AppState::new(
            make_test_tree(vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_file("a.rs")],
            )]),
            make_test_tree(vec![]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        // scan_statuses なし → compute_dir_badge にフォールバック
        assert_eq!(state.compute_scan_badge("src", true), Badge::LeftOnly);
    }
}
