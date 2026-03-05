//! バッジ計算（ファイル差分状態の判定）。

use std::path::Path;

use crate::tree::find_node_in_slice;

use super::types::{Badge, MergedNode};
use super::AppState;

impl AppState {
    /// ローカル/リモートのツリーを比較してバッジを決定する
    pub fn compute_badge(&self, path: &str, is_dir: bool) -> Badge {
        if is_dir {
            return Badge::Unchecked;
        }

        // エラーパスの場合は Error バッジ
        if self.error_paths.contains(path) {
            return Badge::Error;
        }

        let in_local = self.local_tree.find_node(Path::new(path)).is_some();
        let in_remote = self.remote_tree.find_node(Path::new(path)).is_some();

        match (in_local, in_remote) {
            (true, false) => Badge::LocalOnly,
            (false, true) => Badge::RemoteOnly,
            (true, true) => {
                // キャッシュに両方あれば diff で判定
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
            (false, false) => Badge::Unchecked,
        }
    }

    /// 走査結果を使ったバッジ計算（mtime + size ベース）
    pub fn compute_scan_badge(&self, path: &str, is_dir: bool) -> Badge {
        if is_dir {
            return Badge::Unchecked;
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
