//! 中身の読み込みの結果。パスごとに、読めた中身か読めなかった理由を持つ。
//!
//! 中身を読む呼び出し元（diff・merge・sync・status）はどれもこの型で結果を受け取り、失敗を
//! 捨てず、読めなかったパスを読み直してその失敗を消さない。読み直すと、経路の状態（エージェントの
//! 無効化や再接続）で別の結果になりうる（docs/decision/records/2026-10-05-size-limit.md の A24）。
//! 読めた中身を後の段でもう一度読むことは妨げない（同じ記録の A28）。

use std::collections::HashMap;
use std::path::Path;

use crate::tree::{FileTree, NodePresence};

use super::types::MergeFailure;

/// パスごとの中身の読み込みの結果
#[derive(Debug, Default, Clone)]
pub struct ContentReads {
    results: HashMap<String, Result<Vec<u8>, String>>,
}

impl ContentReads {
    pub fn record_read(&mut self, path: impl Into<String>, content: Vec<u8>) {
        self.results.insert(path.into(), Ok(content));
    }

    pub fn record_failure(&mut self, path: impl Into<String>, reason: impl Into<String>) {
        self.results.insert(path.into(), Err(reason.into()));
    }

    /// 読んだパスの結果。尋ねていないパスは None
    pub fn get(&self, path: &str) -> Option<Result<&[u8], &str>> {
        self.results
            .get(path)
            .map(|result| result.as_deref().map_err(String::as_str))
    }

    pub fn content(&self, path: &str) -> Option<&[u8]> {
        self.get(path).and_then(Result::ok)
    }

    pub fn failure(&self, path: &str) -> Option<&str> {
        self.get(path).and_then(Result::err)
    }

    pub fn contains(&self, path: &str) -> bool {
        self.results.contains_key(path)
    }

    /// `path` の結果を取り出す。尋ねていないパスは None
    pub fn take(&mut self, path: &str) -> Option<Result<Vec<u8>, String>> {
        self.results.remove(path)
    }

    /// 読めた中身と、読めなかったパスごとの失敗（理由を error に持つ）に分ける
    pub fn into_parts(self) -> (HashMap<String, Vec<u8>>, Vec<MergeFailure>) {
        let mut contents = HashMap::new();
        let mut failures = Vec::new();
        for (path, result) in self.results {
            match result {
                Ok(content) => {
                    contents.insert(path, content);
                }
                Err(error) => failures.push(MergeFailure { path, error }),
            }
        }
        failures.sort_by(|left, right| left.path.cmp(&right.path));
        (contents, failures)
    }
}

/// 左右とも中身を読めたパスの組と、片側でも読めなかったパスの失敗
pub struct VerifiedComparisons {
    pub pairs: HashMap<String, (Vec<u8>, Vec<u8>)>,
    pub failures: Vec<MergeFailure>,
}

pub fn verified_content_pairs(
    paths: &[String],
    left: &ContentReads,
    right: &ContentReads,
) -> VerifiedComparisons {
    let mut pairs = HashMap::new();
    let mut failures = Vec::new();
    for path in paths {
        match (left.content(path), right.content(path)) {
            (Some(left_bytes), Some(right_bytes)) => {
                pairs.insert(path.clone(), (left_bytes.to_vec(), right_bytes.to_vec()));
            }
            (left_content, right_content) => {
                let reason = |reads: &ContentReads| {
                    reads
                        .failure(path)
                        .unwrap_or("no content returned")
                        .to_string()
                };
                let mut reasons = Vec::new();
                if left_content.is_none() {
                    reasons.push(format!("left: {}", reason(left)));
                }
                if right_content.is_none() {
                    reasons.push(format!("right: {}", reason(right)));
                }
                failures.push(MergeFailure {
                    path: path.clone(),
                    error: format!("read failed: {}", reasons.join("; ")),
                });
            }
        }
    }
    VerifiedComparisons { pairs, failures }
}

/// `paths` のうち、`tree` でないと分かっていないもの（未読み込みのディレクトリの下も含む）。
/// ないと分かっている側は読まない（読めば「ない」が読めなかったことになる）ので、読む前に
/// これで絞る
pub fn not_absent_in(paths: &[String], tree: &FileTree) -> Vec<String> {
    paths
        .iter()
        .filter(|path| !is_absent(tree, path))
        .cloned()
        .collect()
}

fn is_absent(tree: &FileTree, path: &str) -> bool {
    tree.find_node_or_unloaded(Path::new(path)) == NodePresence::NotFound
}

/// 三者のどれでも、ないと分かっていない側は中身を読めたか（ない側は読まないので読めたかを
/// 問わない）。参照先との関係は、これが成り立つファイルにだけ決める
pub fn three_way_contents_read(path: &str, sides: [(&FileTree, &ContentReads); 3]) -> bool {
    sides
        .into_iter()
        .all(|(tree, reads)| is_absent(tree, path) || reads.content(path).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::FileNode;

    fn tree_with(paths: &[&str]) -> FileTree {
        let mut tree = FileTree::new("/root");
        tree.nodes = paths.iter().map(|path| FileNode::new_file(*path)).collect();
        tree
    }

    fn reads(entries: &[(&str, Result<&[u8], &str>)]) -> ContentReads {
        let mut reads = ContentReads::default();
        for (path, result) in entries {
            match result {
                Ok(content) => reads.record_read(*path, content.to_vec()),
                Err(reason) => reads.record_failure(*path, *reason),
            }
        }
        reads
    }

    #[test]
    fn a_read_keeps_its_content_and_a_failure_keeps_its_reason() {
        let reads = reads(&[("a", Ok(b"x")), ("b", Err("denied"))]);

        assert_eq!(reads.content("a"), Some(b"x".as_slice()));
        assert_eq!(reads.failure("a"), None);
        assert_eq!(reads.failure("b"), Some("denied"));
        assert_eq!(reads.content("b"), None);
        assert!(reads.get("c").is_none());
    }

    #[test]
    fn the_parts_keep_every_content_and_every_failure() {
        let (contents, failures) = reads(&[("a", Ok(b"x")), ("b", Err("denied"))]).into_parts();

        assert_eq!(contents.len(), 1);
        assert_eq!(contents["a"], b"x");
        assert_eq!(failures.len(), 1);
        assert_eq!(
            (failures[0].path.as_str(), failures[0].error.as_str()),
            ("b", "denied")
        );
    }

    #[test]
    fn only_paths_read_on_both_sides_are_compared_and_the_rest_keep_their_reasons() {
        let paths = vec!["both".to_string(), "broken".to_string()];
        let left = reads(&[("both", Ok(b"l")), ("broken", Ok(b"l"))]);
        let right = reads(&[("both", Ok(b"r")), ("broken", Err("denied"))]);

        let verified = verified_content_pairs(&paths, &left, &right);

        assert_eq!(
            verified.pairs["both"],
            (b"l".to_vec(), b"r".to_vec()),
            "both sides were read"
        );
        assert!(!verified.pairs.contains_key("broken"));
        assert_eq!(verified.failures.len(), 1);
        assert_eq!(verified.failures[0].path, "broken");
        assert!(verified.failures[0].error.contains("denied"));
    }

    #[test]
    fn the_paths_known_to_be_absent_are_not_read() {
        let paths = vec!["a".to_string(), "missing".to_string(), "dir/x".to_string()];
        let mut tree = tree_with(&["a", "other"]);
        tree.nodes.push(FileNode::new_dir("dir"));

        assert_eq!(not_absent_in(&paths, &tree), vec!["a", "dir/x"]);
    }

    #[test]
    fn a_side_without_the_file_does_not_need_to_be_read() {
        let present = tree_with(&["a"]);
        let absent = tree_with(&[]);
        let read = reads(&[("a", Ok(b"x"))]);
        let unread = ContentReads::default();

        assert!(three_way_contents_read(
            "a",
            [(&present, &read), (&present, &read), (&absent, &unread)]
        ));
    }

    #[test]
    fn a_side_with_the_file_that_was_not_read_leaves_the_three_way_comparison_incomplete() {
        let present = tree_with(&["a"]);
        let read = reads(&[("a", Ok(b"x"))]);
        let failed = reads(&[("a", Err("denied"))]);
        let unasked = ContentReads::default();

        assert!(!three_way_contents_read(
            "a",
            [(&present, &read), (&present, &read), (&present, &failed)]
        ));
        assert!(!three_way_contents_read(
            "a",
            [(&present, &unasked), (&present, &read), (&present, &read)]
        ));
    }
}
