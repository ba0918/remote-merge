//! status と merge の --ref の印（docs/ir/cli/status-targets.md の TBL-cli-005）。
//!
//! 左右と参照先のツリーと中身の読み込みの結果から、ファイルごとの印と印の集計を求める。

use std::collections::HashMap;
use std::path::Path;

use crate::three_way;
use crate::tree::{kinds_agree, FileTree};

use super::content_reads::ContentReads;
use super::types::FileStatus;

/// 3-way バッジを計算する。
///
/// `three_way::compute_file_comparison()` を内部で呼び出す。中身で決まるのは左右と参照先の
/// 三つともに同じ種類であるファイルだけで、そのとき三つのどれかを読めなければバッジを作らない（読めなかった
/// 中身どうしを同じとみなさない）。ほかのファイルは存在だけで決め、中身の読み込みに左右されない
/// （docs/ir/cli/status-targets.md の TBL-cli-005）。
pub fn compute_ref_badges(
    files: &[FileStatus],
    left_tree: &FileTree,
    right_tree: &FileTree,
    ref_tree: &FileTree,
    left_contents: &ContentReads,
    right_contents: &ContentReads,
    ref_contents: &ContentReads,
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for file in files {
        let path = file.path.as_str();
        let left_exists = left_tree.find_node(Path::new(path)).is_some();
        let right_exists = right_tree.find_node(Path::new(path)).is_some();
        let ref_exists = ref_tree.find_node(Path::new(path)).is_some();
        let trees = [left_tree, right_tree, ref_tree];
        let (left_eq_right, left_eq_ref) = if three_kinds_agree(path, trees) == Some(true) {
            let (Some(left), Some(right), Some(base)) = (
                left_contents.content(path),
                right_contents.content(path),
                ref_contents.content(path),
            ) else {
                continue;
            };
            (left == right, left == base)
        } else {
            // 中身を使わない組み合わせ。三つの種類がそろわなければ中身は同じでありえない
            (false, false)
        };
        let badge = three_way::compute_file_comparison(
            left_exists,
            right_exists,
            ref_exists,
            left_eq_right,
            left_eq_ref,
        );
        let badge_str = match badge {
            three_way::FileComparison::AllEqual => "all_equal",
            three_way::FileComparison::Differs => "differs",
            three_way::FileComparison::ExistsOnlyInRef => "exists_only_in_ref",
            three_way::FileComparison::MissingInRef => "missing_in_ref",
        };
        map.insert(file.path.clone(), badge_str.to_string());
    }
    map
}

/// 三つのツリーのすべてにあるパスについて、三つの種類（通常ファイル・ディレクトリ・末尾の symlink）が
/// そろうかどうか。どれかにないときは None
fn three_kinds_agree(path: &str, trees: [&FileTree; 3]) -> Option<bool> {
    let [first, second, third] = trees.map(|tree| tree.find_node(Path::new(path)));
    Some(kinds_agree([first?, second?, third?]))
}

/// `files` のうち、バッジを決めるのに中身が要る、左右と参照先の三つともに同じ種類であるパス
/// （docs/ir/cli/status-targets.md の TBL-cli-005）
pub fn ref_badge_content_paths(
    files: &[FileStatus],
    left_tree: &FileTree,
    right_tree: &FileTree,
    ref_tree: &FileTree,
) -> Vec<String> {
    files
        .iter()
        .map(|file| file.path.as_str())
        .filter(|path| three_kinds_agree(path, [left_tree, right_tree, ref_tree]) == Some(true))
        .map(str::to_string)
        .collect()
}

/// ref バッジの集計を返す。
pub fn compute_ref_summary(ref_badges: &HashMap<String, String>) -> (usize, usize, usize) {
    let mut differs = 0;
    let mut ref_only = 0;
    let mut ref_missing = 0;
    for badge in ref_badges.values() {
        match badge.as_str() {
            "differs" => differs += 1,
            "exists_only_in_ref" => ref_only += 1,
            "missing_in_ref" => ref_missing += 1,
            _ => {} // all_equal — no count
        }
    }
    (differs, ref_only, ref_missing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::types::FileStatusKind;
    use crate::tree::FileNode;
    use std::path::PathBuf;

    fn make_tree(nodes: Vec<FileNode>) -> FileTree {
        FileTree {
            root: PathBuf::from("/test"),
            nodes,
            unreadable_names: Vec::new(),
        }
    }

    fn modified(path: &str) -> FileStatus {
        FileStatus {
            path: path.into(),
            status: FileStatusKind::Modified,
            hunks: None,
            ref_badge: None,
        }
    }

    fn reads(entries: &[(&str, &[u8])]) -> ContentReads {
        let mut reads = ContentReads::default();
        for (path, content) in entries {
            reads.record_read(*path, content.to_vec());
        }
        reads
    }

    #[test]
    fn test_compute_ref_badges_all_equal() {
        let files = vec![modified("a.rs")];
        let tree = make_tree(vec![FileNode::new_file("a.rs")]);
        let content = reads(&[("a.rs", b"content")]);

        let badges = compute_ref_badges(&files, &tree, &tree, &tree, &content, &content, &content);
        assert_eq!(badges.get("a.rs").unwrap(), "all_equal");
    }

    #[test]
    fn test_compute_ref_badges_differs() {
        let files = vec![modified("a.rs")];
        let tree = make_tree(vec![FileNode::new_file("a.rs")]);

        let badges = compute_ref_badges(
            &files,
            &tree,
            &tree,
            &tree,
            &reads(&[("a.rs", b"left_content")]),
            &reads(&[("a.rs", b"right_content")]),
            &reads(&[("a.rs", b"ref_content")]),
        );
        assert_eq!(badges.get("a.rs").unwrap(), "differs");
    }

    #[test]
    fn test_compute_ref_badges_missing_in_ref() {
        let files = vec![modified("a.rs")];
        let tree = make_tree(vec![FileNode::new_file("a.rs")]);
        let ref_tree = make_tree(vec![]);
        let content = reads(&[("a.rs", b"content")]);

        let badges = compute_ref_badges(
            &files,
            &tree,
            &tree,
            &ref_tree,
            &content,
            &content,
            &ContentReads::default(),
        );
        assert_eq!(badges.get("a.rs").unwrap(), "missing_in_ref");
    }

    #[test]
    fn test_compute_ref_summary() {
        let mut badges = HashMap::new();
        badges.insert("a.rs".into(), "differs".into());
        badges.insert("b.rs".into(), "differs".into());
        badges.insert("c.rs".into(), "exists_only_in_ref".into());
        badges.insert("d.rs".into(), "all_equal".into());
        badges.insert("e.rs".into(), "missing_in_ref".into());

        let (differs, ref_only, ref_missing) = compute_ref_summary(&badges);
        assert_eq!(differs, 2);
        assert_eq!(ref_only, 1);
        assert_eq!(ref_missing, 1);
    }

    #[test]
    fn test_compute_ref_badges_empty_ref_tree() {
        let mut left_only = modified("b.rs");
        left_only.status = FileStatusKind::LeftOnly;
        let files = vec![modified("a.rs"), left_only];
        let left = make_tree(vec![FileNode::new_file("a.rs"), FileNode::new_file("b.rs")]);
        let right = make_tree(vec![FileNode::new_file("a.rs")]);
        let ref_tree = make_tree(vec![]);

        let badges = compute_ref_badges(
            &files,
            &left,
            &right,
            &ref_tree,
            &reads(&[("a.rs", b"left"), ("b.rs", b"left")]),
            &reads(&[("a.rs", b"right")]),
            &ContentReads::default(),
        );
        assert_eq!(badges.get("a.rs").unwrap(), "missing_in_ref");
    }

    // 読めなかった中身どうしを同じとみなして all_equal にしない
    #[test]
    fn no_badge_is_made_for_a_file_whose_content_could_not_be_read_on_a_side_that_has_it() {
        let files = vec![modified("a.rs")];
        let tree = make_tree(vec![FileNode::new_file("a.rs")]);
        let content = reads(&[("a.rs", b"content")]);
        let mut failed = ContentReads::default();
        failed.record_failure("a.rs", "denied");

        let badges = compute_ref_badges(&files, &tree, &tree, &tree, &content, &content, &failed);
        assert!(!badges.contains_key("a.rs"), "{badges:?}");
        let badges = compute_ref_badges(&files, &tree, &tree, &tree, &failed, &failed, &failed);
        assert!(badges.is_empty(), "{badges:?}");
    }

    // 存在だけで決まるバッジは、中身の読み込みの失敗に左右されない
    #[test]
    fn a_badge_decided_by_existence_alone_ignores_a_failed_read() {
        let mut left_only = modified("o.rs");
        left_only.status = FileStatusKind::LeftOnly;
        let files = vec![modified("m.rs"), left_only];
        let left = make_tree(vec![FileNode::new_file("m.rs"), FileNode::new_file("o.rs")]);
        let right = make_tree(vec![FileNode::new_file("m.rs")]);
        let ref_tree = make_tree(vec![FileNode::new_file("o.rs")]);
        let mut failed = ContentReads::default();
        failed.record_failure("m.rs", "denied");
        failed.record_failure("o.rs", "denied");

        let badges = compute_ref_badges(
            &files,
            &left,
            &right,
            &ref_tree,
            &failed,
            &ContentReads::default(),
            &ContentReads::default(),
        );

        assert_eq!(
            badges.get("m.rs").map(String::as_str),
            Some("missing_in_ref")
        );
        assert_eq!(badges.get("o.rs").map(String::as_str), Some("differs"));
    }

    #[test]
    fn only_files_on_all_three_sides_need_their_contents_for_a_badge() {
        let files = vec![modified("all.rs"), modified("two.rs"), modified("one.rs")];
        let left = make_tree(vec![
            FileNode::new_file("all.rs"),
            FileNode::new_file("two.rs"),
            FileNode::new_file("one.rs"),
        ]);
        let right = make_tree(vec![
            FileNode::new_file("all.rs"),
            FileNode::new_file("two.rs"),
        ]);
        let ref_tree = make_tree(vec![
            FileNode::new_file("all.rs"),
            FileNode::new_file("one.rs"),
        ]);

        assert_eq!(
            ref_badge_content_paths(&files, &left, &right, &ref_tree),
            vec!["all.rs"]
        );
    }

    // 三つの種類がそろわないパスは中身を読まずに "differs" とする（TBL-cli-005）
    #[test]
    fn a_path_whose_three_kinds_differ_differs_without_its_contents() {
        let files = vec![modified("dir_ref"), modified("dir_right"), modified("link")];
        let left = make_tree(vec![
            FileNode::new_file("dir_ref"),
            FileNode::new_file("dir_right"),
            FileNode::new_file("link"),
        ]);
        let right = make_tree(vec![
            FileNode::new_file("dir_ref"),
            FileNode::new_dir("dir_right"),
            FileNode::new_file("link"),
        ]);
        let ref_tree = make_tree(vec![
            FileNode::new_dir("dir_ref"),
            FileNode::new_file("dir_right"),
            FileNode::new_symlink("link", "target"),
        ]);
        let none = ContentReads::default();

        assert!(ref_badge_content_paths(&files, &left, &right, &ref_tree).is_empty());
        let badges = compute_ref_badges(&files, &left, &right, &ref_tree, &none, &none, &none);
        for path in ["dir_ref", "dir_right", "link"] {
            assert_eq!(
                badges.get(path).map(String::as_str),
                Some("differs"),
                "{path}"
            );
        }
    }
}
