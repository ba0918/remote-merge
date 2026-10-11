//! merge 実行前の検査と hunk merge の準備（純粋関数）。

use std::collections::HashSet;

use crate::diff::engine::{
    apply_selected_hunks_single_pass, compute_diff, is_binary, merge_hunks_in_display_hunks,
    DiffResult,
};
use crate::merge::executor::MergeDirection;
use crate::service::merge::find_symlink_target;
use crate::service::types::{FileStatus, FileStatusKind, HunkMergeInfo};
use crate::tree::{find_node_in_slice, FileTree};

/// ソース側にファイルが存在しない方向のマージを検出する
///
/// - `LeftToRight` + `RightOnly` = ソース(left)にファイルがない
/// - `RightToLeft` + `LeftOnly` = ソース(right)にファイルがない
pub fn check_source_exists(
    path: &str,
    direction: MergeDirection,
    statuses: &[FileStatus],
) -> anyhow::Result<()> {
    let status = statuses.iter().find(|s| s.path == path);
    let source_missing = matches!(
        (direction, status.map(|s| &s.status)),
        (MergeDirection::LeftToRight, Some(FileStatusKind::RightOnly))
            | (MergeDirection::RightToLeft, Some(FileStatusKind::LeftOnly))
    );
    if source_missing {
        let source_name = match direction {
            MergeDirection::LeftToRight => "left (source)",
            MergeDirection::RightToLeft => "right (source)",
        };
        anyhow::bail!(
            "File '{}' does not exist on {} side. Cannot merge a non-existent source file.",
            path,
            source_name
        );
    }
    Ok(())
}

/// 比べたときに symlink だった書き込み先が、今も同じリンク先を指す symlink かを確かめる
/// （REQ-merge-033）。
///
/// `current` は今の書き込み先が symlink ならそのリンク先の文字列で、symlink でないかないなら None。
/// 比べたときに symlink でなかった書き込み先には当てない
pub fn check_symlink_destination_unchanged(
    path: &str,
    target_tree: &FileTree,
    current: Option<&str>,
) -> anyhow::Result<()> {
    let Some(compared) = find_symlink_target(target_tree, path) else {
        return Ok(());
    };
    if current != Some(compared.as_str()) {
        anyhow::bail!("destination content changed since comparison: {path}");
    }
    Ok(())
}

/// hunk merge 対象ファイルのバリデーション（純粋関数）。
///
/// - symlink はエラー（hunk merge はテキスト専用）
///
/// バイナリ判定はファイル内容の読み込みが必要なため、ここでは行わない
/// （呼び出し元で内容取得後にチェックする）。
pub fn validate_hunk_merge_target(
    path: &str,
    source_tree: &FileTree,
    target_tree: &FileTree,
) -> anyhow::Result<()> {
    // symlink チェック（ソース側・ターゲット側どちらか）
    let source_node = find_node_in_slice(&source_tree.nodes, path);
    let target_node = find_node_in_slice(&target_tree.nodes, path);
    if source_node.is_some_and(|n| n.is_symlink()) || target_node.is_some_and(|n| n.is_symlink()) {
        anyhow::bail!("Hunk merge is not supported for symlink files: '{}'", path);
    }

    Ok(())
}

pub enum HunkMergePreparation {
    Equal,
    Modified {
        merged_text: String,
        hunk_info: HunkMergeInfo,
    },
}

pub fn prepare_hunk_merge(
    path: &str,
    source_bytes: &[u8],
    target_bytes: &[u8],
    direction: MergeDirection,
    hunk_indices: &[usize],
) -> anyhow::Result<HunkMergePreparation> {
    // バイナリチェック
    if is_binary(source_bytes) || is_binary(target_bytes) {
        anyhow::bail!("Hunk merge is not supported for binary files: '{}'", path);
    }

    let source_text = String::from_utf8_lossy(source_bytes);
    let target_text = String::from_utf8_lossy(target_bytes);

    // diff 計算
    // hunk merge の方向: left の内容を right に反映する（LeftToRight の場合）
    // compute_diff(old=left, new=right) → LeftToRight は「right テキストに left の変更を取り込む」
    let diff = compute_diff(&source_text, &target_text);

    match &diff {
        DiffResult::Equal => Ok(HunkMergePreparation::Equal),
        DiffResult::Binary { .. } => {
            anyhow::bail!("Hunk merge is not supported for binary files: '{}'", path);
        }
        DiffResult::SymlinkDiff { .. } => {
            anyhow::bail!("Hunk merge is not supported for symlink files: '{}'", path);
        }
        DiffResult::Modified {
            hunks,
            merge_hunks,
            lines,
            ..
        } => {
            // 番号は diff --format json と同じ表示用ハンク（コンテキスト3行）で数える
            let total = hunks.len();
            let hunk_dir = direction.to_hunk_direction();

            // インデックスの範囲チェック + 重複排除を1パスで実行
            let mut unique_indices = HashSet::with_capacity(hunk_indices.len());
            for &idx in hunk_indices {
                if idx >= total {
                    anyhow::bail!(
                        "Hunk index {} is out of range (total hunks: {})",
                        idx,
                        total
                    );
                }
                unique_indices.insert(idx);
            }

            let selected_merge_hunks =
                merge_hunks_in_display_hunks(hunks, merge_hunks, &unique_indices);
            let merged_text = if selected_merge_hunks.len() >= merge_hunks.len() {
                // 全 hunk → ソーステキストをそのまま使用
                source_text.to_string()
            } else {
                // 部分適用 → single-pass
                let trailing_nl = target_text.ends_with('\n');
                apply_selected_hunks_single_pass(
                    lines,
                    merge_hunks,
                    &selected_merge_hunks,
                    hunk_dir,
                    trailing_nl,
                )
            };

            let hunk_info = HunkMergeInfo {
                hunks_applied: hunk_indices.to_vec(),
                hunks_total: total,
                direction: direction.as_str().to_string(),
            };

            Ok(HunkMergePreparation::Modified {
                merged_text,
                hunk_info,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::tree::FileNode;

    fn tree_with(node: FileNode) -> FileTree {
        let mut tree = FileTree::new(PathBuf::from("/tmp"));
        tree.nodes.push(node);
        tree
    }

    #[test]
    fn a_destination_symlink_still_pointing_at_the_compared_target_passes() {
        let tree = tree_with(FileNode::new_symlink("link", "b.txt"));
        assert!(check_symlink_destination_unchanged("link", &tree, Some("b.txt")).is_ok());
    }

    #[test]
    fn a_destination_symlink_that_changed_since_comparison_fails() {
        let tree = tree_with(FileNode::new_symlink("link", "b.txt"));
        // リンク先が変わった・symlink でなくなった・消えた
        for current in [Some("c.txt"), None] {
            assert!(
                check_symlink_destination_unchanged("link", &tree, current).is_err(),
                "{current:?}"
            );
        }
    }

    #[test]
    fn a_destination_that_was_not_a_symlink_at_comparison_is_not_checked() {
        let file = tree_with(FileNode::new_file("link"));
        let empty = FileTree::new(PathBuf::from("/tmp"));
        for tree in [&file, &empty] {
            assert!(check_symlink_destination_unchanged("link", tree, Some("x")).is_ok());
            assert!(check_symlink_destination_unchanged("link", tree, None).is_ok());
        }
    }
}
