//! diff サービス: ファイル差分の計算と変換。
//!
//! 既存の diff エンジン (`diff::engine::compute_diff`) を呼び出し、
//! 結果を Service 層の出力型 (`DiffOutput`) に変換する純粋関数群。

use crate::diff::engine::{self, DiffTag};

use super::types::*;

/// diff エンジンの結果を DiffOutput に変換する（純粋関数）。
#[allow(clippy::too_many_arguments)]
pub fn build_diff_output(
    path: &str,
    left_info: SourceInfo,
    right_info: SourceInfo,
    left_content: &str,
    right_content: &str,
    sensitive: bool,
    max_lines: Option<usize>,
    ref_info: Option<SourceInfo>,
    ref_content: Option<&str>,
) -> DiffOutput {
    let result = engine::compute_diff(left_content, right_content);

    let (hunks, truncated, binary) = match result {
        engine::DiffResult::Equal => (vec![], false, false),
        engine::DiffResult::Modified { hunks, .. } => {
            let (h, t) = convert_hunks(&hunks, max_lines);
            (h, t, false)
        }
        engine::DiffResult::Binary { .. } | engine::DiffResult::SymlinkDiff { .. } => {
            // バイナリ/シンボリックリンクは hunks を空にし binary フラグで通知
            (vec![], false, true)
        }
    };

    // Compute ref hunks (left vs ref)
    let (ref_hunks_out, ref_info_out) = match (ref_info, ref_content) {
        (Some(ri), Some(rc)) => {
            let ref_result = engine::compute_diff(left_content, rc);
            let ref_hunks = match ref_result {
                engine::DiffResult::Equal => Some(vec![]),
                engine::DiffResult::Modified { hunks: rh, .. } => {
                    let (converted, _) = convert_hunks(&rh, max_lines);
                    Some(converted)
                }
                _ => Some(vec![]),
            };
            (ref_hunks, Some(ri))
        }
        (Some(ri), None) => {
            // ref server specified but file doesn't exist on ref
            (None, Some(ri))
        }
        _ => (None, None),
    };

    DiffOutput {
        path: path.to_string(),
        left: left_info,
        right: right_info,
        ref_: ref_info_out,
        sensitive,
        binary,
        truncated,
        hunks,
        ref_hunks: ref_hunks_out,
    }
}

/// engine::DiffHunk を service::DiffHunk に変換する。
///
/// `max_lines` が指定された場合、累計行数が上限を超えたらトランケートする。
fn convert_hunks(hunks: &[engine::DiffHunk], max_lines: Option<usize>) -> (Vec<DiffHunk>, bool) {
    let mut result = Vec::new();
    let mut total_lines = 0usize;
    let mut truncated = false;

    for (i, hunk) in hunks.iter().enumerate() {
        let mut lines = Vec::new();

        for line in &hunk.lines {
            if let Some(max) = max_lines {
                if total_lines >= max {
                    truncated = true;
                    break;
                }
            }

            lines.push(DiffLine {
                line_type: match line.tag {
                    DiffTag::Equal => DiffLineType::Context,
                    DiffTag::Delete => DiffLineType::Removed,
                    DiffTag::Insert => DiffLineType::Added,
                },
                content: line.value.clone(),
            });
            total_lines += 1;
        }

        if !lines.is_empty() {
            result.push(DiffHunk {
                index: i,
                left_start: hunk.old_start,
                right_start: hunk.new_start,
                lines,
            });
        }

        if truncated {
            break;
        }
    }

    (result, truncated)
}

/// diff の exit code を判定する。差分があれば 1、なければ 0。
pub fn diff_exit_code(output: &DiffOutput) -> i32 {
    if output.binary || !output.hunks.is_empty() {
        exit_code::DIFF_FOUND
    } else {
        exit_code::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(label: &str) -> SourceInfo {
        SourceInfo {
            label: label.into(),
            root: ".".into(),
        }
    }

    #[test]
    fn test_equal_files() {
        let output = build_diff_output(
            "a.rs",
            info("local"),
            info("dev"),
            "hello\nworld\n",
            "hello\nworld\n",
            false,
            None,
            None,
            None,
        );
        assert!(output.hunks.is_empty());
        assert!(!output.truncated);
    }

    #[test]
    fn test_modified_files() {
        let output = build_diff_output(
            "a.rs",
            info("local"),
            info("dev"),
            "line1\nline2\nline3\n",
            "line1\nchanged\nline3\n",
            false,
            None,
            None,
            None,
        );
        assert!(!output.hunks.is_empty());
        assert!(!output.truncated);

        // ハンクの中に removed/added がある
        let hunk = &output.hunks[0];
        assert!(hunk
            .lines
            .iter()
            .any(|l| l.line_type == DiffLineType::Removed));
        assert!(hunk
            .lines
            .iter()
            .any(|l| l.line_type == DiffLineType::Added));
    }

    #[test]
    fn test_max_lines_truncation() {
        let old = (0..100)
            .map(|i| format!("old_{}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let new = (0..100)
            .map(|i| format!("new_{}", i))
            .collect::<Vec<_>>()
            .join("\n");

        let output = build_diff_output(
            "big.rs",
            info("l"),
            info("r"),
            &old,
            &new,
            false,
            Some(5),
            None,
            None,
        );
        assert!(output.truncated);
        let total_lines: usize = output.hunks.iter().map(|h| h.lines.len()).sum();
        assert!(total_lines <= 5);
    }

    #[test]
    fn test_sensitive_flag() {
        let output = build_diff_output(
            ".env",
            info("l"),
            info("r"),
            "SECRET=old",
            "SECRET=new",
            true,
            None,
            None,
            None,
        );
        assert!(output.sensitive);
    }

    #[test]
    fn test_exit_code_no_diff() {
        let output = build_diff_output(
            "a.rs",
            info("l"),
            info("r"),
            "same",
            "same",
            false,
            None,
            None,
            None,
        );
        assert_eq!(diff_exit_code(&output), exit_code::SUCCESS);
    }

    #[test]
    fn test_exit_code_has_diff() {
        let output = build_diff_output(
            "a.rs",
            info("l"),
            info("r"),
            "old",
            "new",
            false,
            None,
            None,
            None,
        );
        assert_eq!(diff_exit_code(&output), exit_code::DIFF_FOUND);
    }

    #[test]
    fn test_hunk_line_types() {
        let output = build_diff_output(
            "a.rs",
            info("l"),
            info("r"),
            "aaa\nbbb\nccc\n",
            "aaa\nBBB\nccc\n",
            false,
            None,
            None,
            None,
        );
        let hunk = &output.hunks[0];
        // context, removed, added, context が含まれるはず
        let types: Vec<_> = hunk.lines.iter().map(|l| l.line_type).collect();
        assert!(types.contains(&DiffLineType::Context));
        assert!(types.contains(&DiffLineType::Removed));
        assert!(types.contains(&DiffLineType::Added));
    }

    #[test]
    fn test_ref_content_produces_ref_hunks() {
        let output = build_diff_output(
            "a.rs",
            info("local"),
            info("dev"),
            "line1\nline2\nline3\n",
            "line1\nchanged\nline3\n",
            false,
            None,
            Some(info("staging")),
            Some("line1\nref_changed\nline3\n"),
        );
        assert!(output.ref_.is_some());
        assert_eq!(output.ref_.as_ref().unwrap().label, "staging");
        assert!(output.ref_hunks.is_some());
        assert!(!output.ref_hunks.as_ref().unwrap().is_empty());
    }

    #[test]
    fn test_ref_content_same_as_left_produces_empty_ref_hunks() {
        let output = build_diff_output(
            "a.rs",
            info("local"),
            info("dev"),
            "same content\n",
            "different\n",
            false,
            None,
            Some(info("staging")),
            Some("same content\n"),
        );
        assert!(output.ref_hunks.is_some());
        assert!(output.ref_hunks.as_ref().unwrap().is_empty());
    }

    #[test]
    fn test_ref_content_none_produces_none_ref_hunks() {
        let output = build_diff_output(
            "a.rs",
            info("local"),
            info("dev"),
            "content\n",
            "other\n",
            false,
            None,
            Some(info("staging")),
            None,
        );
        assert!(output.ref_.is_some());
        assert!(output.ref_hunks.is_none());
    }

    #[test]
    fn test_no_ref_backward_compat() {
        let output = build_diff_output(
            "a.rs",
            info("local"),
            info("dev"),
            "old\n",
            "new\n",
            false,
            None,
            None,
            None,
        );
        assert!(output.ref_.is_none());
        assert!(output.ref_hunks.is_none());
    }

    #[test]
    fn test_binary_files_set_binary_flag() {
        // NULバイトを含むコンテンツ → compute_diff が Binary を返す
        let binary_content = "hello\x00world";
        let output = build_diff_output(
            "image.png",
            info("l"),
            info("r"),
            binary_content,
            "different\x00data",
            false,
            None,
            None,
            None,
        );
        assert!(output.binary, "binary flag should be true for binary files");
        assert!(output.hunks.is_empty(), "binary files should have no hunks");
    }

    #[test]
    fn test_text_files_binary_flag_false() {
        let output = build_diff_output(
            "a.rs",
            info("l"),
            info("r"),
            "old\n",
            "new\n",
            false,
            None,
            None,
            None,
        );
        assert!(!output.binary, "text files should have binary=false");
    }

    #[test]
    fn test_max_lines_applied_independently_to_ref_hunks() {
        let old = (0..100)
            .map(|i| format!("old_{}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let new = (0..100)
            .map(|i| format!("new_{}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let ref_content = (0..100)
            .map(|i| format!("ref_{}", i))
            .collect::<Vec<_>>()
            .join("\n");

        let output = build_diff_output(
            "big.rs",
            info("l"),
            info("r"),
            &old,
            &new,
            false,
            Some(5),
            Some(info("ref")),
            Some(&ref_content),
        );
        let main_lines: usize = output.hunks.iter().map(|h| h.lines.len()).sum();
        let ref_lines: usize = output
            .ref_hunks
            .as_ref()
            .unwrap()
            .iter()
            .map(|h| h.lines.len())
            .sum();
        assert!(main_lines <= 5);
        assert!(ref_lines <= 5);
    }
}
