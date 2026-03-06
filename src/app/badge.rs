//! バッジ計算（ファイル差分状態の判定）。

use std::path::Path;

use crate::tree::find_node_in_slice;

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

        let local_presence = self.local_tree.find_node_or_unloaded(Path::new(path));
        let remote_presence = self.remote_tree.find_node_or_unloaded(Path::new(path));

        // 確実に片方のみと言えるときだけ LocalOnly/RemoteOnly
        let in_local = local_presence == NodePresence::Found;
        let in_remote = remote_presence == NodePresence::Found;
        let local_absent = local_presence == NodePresence::NotFound;
        let remote_absent = remote_presence == NodePresence::NotFound;

        match (in_local, in_remote) {
            (true, true) => {
                // シンボリックリンクの場合はリンク先パスで比較
                let local_node = self.local_tree.find_node(path);
                let remote_node = self.remote_tree.find_node(path);
                if let (Some(ln), Some(rn)) = (local_node, remote_node) {
                    if ln.is_symlink() || rn.is_symlink() {
                        return self.compute_symlink_badge(ln, rn);
                    }
                }

                // バイナリキャッシュに両方あれば SHA-256 で判定
                let local_bin = self.local_binary_cache.get(path);
                let remote_bin = self.remote_binary_cache.get(path);
                if let (Some(lb), Some(rb)) = (local_bin, remote_bin) {
                    return if lb.sha256 == rb.sha256 {
                        Badge::Equal
                    } else {
                        Badge::Modified
                    };
                }

                // テキストキャッシュに両方あれば diff で判定
                match (self.local_cache.get(path), self.remote_cache.get(path)) {
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
            (true, false) if remote_absent => Badge::LocalOnly,
            (false, true) if local_absent => Badge::RemoteOnly,
            _ => Badge::Unchecked,
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
            .local_tree
            .find_node(std::path::Path::new(path))
            .is_some();
        let in_remote = self
            .remote_tree
            .find_node(std::path::Path::new(path))
            .is_some();

        match (in_local, in_remote) {
            (true, false) => Badge::LocalOnly,
            (false, true) => Badge::RemoteOnly,
            (false, false) => Badge::Unchecked,
            (true, true) => {
                // ツリーから配下の全ファイルを列挙
                let all_files = super::merge_collect::collect_merge_files(
                    &self.local_tree,
                    &self.remote_tree,
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
                        Badge::Modified | Badge::LocalOnly | Badge::RemoteOnly | Badge::Error => {
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

    /// 走査結果を使ったバッジ計算（mtime + size ベース）
    pub fn compute_scan_badge(&self, path: &str, is_dir: bool) -> Badge {
        if is_dir {
            return self.compute_dir_badge(path);
        }

        // まずキャッシュベースの正確なバッジがあればそれを使う
        let cache_badge = self.compute_badge(path, false);
        if cache_badge != Badge::Unchecked {
            return cache_badge;
        }

        // 走査結果からメタデータ比較
        let local_node = self
            .scan_local_tree
            .as_ref()
            .and_then(|tree| find_node_in_slice(tree, path));
        let remote_node = self
            .scan_remote_tree
            .as_ref()
            .and_then(|tree| find_node_in_slice(tree, path));

        match (local_node, remote_node) {
            (Some(_), None) => Badge::LocalOnly,
            (None, Some(_)) => Badge::RemoteOnly,
            (Some(l), Some(r)) => {
                // mtime + size が一致なら Equal（推定）
                let size_match = l.size == r.size;
                let mtime_match = match (l.mtime, r.mtime) {
                    (Some(lt), Some(rt)) => lt == rt,
                    _ => false,
                };
                if size_match && mtime_match {
                    Badge::Equal
                } else {
                    Badge::Modified
                }
            }
            (None, None) => Badge::Unchecked,
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
