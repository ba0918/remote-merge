//! diff サービス: ファイル差分の計算と変換。
//!
//! 既存の diff エンジン (`diff::engine::compute_diff`) を呼び出し、
//! 結果を Service 層の出力型 (`DiffOutput`) に変換する純粋関数群。

use crate::diff::conflict::detect_conflicts;
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

    let (hunks, truncated, binary, symlink) = match result {
        engine::DiffResult::Equal => (vec![], false, false, false),
        engine::DiffResult::Modified { hunks, lines, .. } => {
            let (h, t) = convert_hunks(&hunks, &lines, max_lines);
            (h, t, false, false)
        }
        engine::DiffResult::Binary { .. } => {
            // バイナリは hunks を空にし binary フラグで通知
            (vec![], false, true, false)
        }
        engine::DiffResult::SymlinkDiff { .. } => {
            // シンボリックリンクは hunks を空にし symlink フラグで通知
            (vec![], false, false, true)
        }
    };

    // Compute ref hunks (left vs ref)
    let (ref_hunks_out, ref_info_out) = match (ref_info, ref_content) {
        (Some(ri), Some(rc)) => {
            let ref_result = engine::compute_diff(left_content, rc);
            let ref_hunks = match ref_result {
                engine::DiffResult::Equal => Some(vec![]),
                engine::DiffResult::Modified {
                    hunks: rh,
                    lines: rl,
                    ..
                } => {
                    let (converted, _) = convert_hunks(&rh, &rl, max_lines);
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

    // テキストファイルの場合のみコンフリクト検出（binary/symlink は除外）
    let conflict_info = if !binary && !symlink {
        detect_conflicts(ref_content, left_content, right_content)
    } else {
        Default::default()
    };

    DiffOutput {
        path: path.to_string(),
        left: left_info,
        right: right_info,
        ref_: ref_info_out,
        sensitive,
        binary,
        symlink,
        truncated,
        hunks,
        ref_hunks: ref_hunks_out,
        left_hash: None,
        right_hash: None,
        note: None,
        conflict_count: conflict_info.conflict_count(),
        conflict_regions: conflict_info.regions,
    }
}

/// engine::DiffHunk を service::DiffHunk に変換する。
///
/// `max_lines` が指定された場合、Added/Removed 行の累計がその上限を超えたらトランケートする。
/// Context（Equal）行はカウントせず、常に出力に含める。
/// `max_lines = Some(0)` は無制限として扱う（None と同等）。
fn convert_hunks(
    hunks: &[engine::DiffHunk],
    all_lines: &[engine::DiffLine],
    max_lines: Option<usize>,
) -> (Vec<DiffHunk>, bool) {
    let mut result = Vec::new();
    let mut change_lines = 0usize;
    let mut truncated = false;

    for (i, hunk) in hunks.iter().enumerate() {
        let mut lines = Vec::new();

        for line in hunk.lines(all_lines) {
            if let Some(max) = max_lines {
                if max > 0 && change_lines >= max {
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
            // Added/Removed のみカウント（Context はカウントしない）
            if line.tag != DiffTag::Equal {
                change_lines += 1;
            }
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
///
/// sensitive マスク時（note 付き）も差分ありとして DIFF_FOUND を返す。
pub fn diff_exit_code(output: &DiffOutput) -> i32 {
    if output.binary
        || output.symlink
        || !output.hunks.is_empty()
        || (output.sensitive && output.note.is_some())
    {
        exit_code::DIFF_FOUND
    } else {
        exit_code::SUCCESS
    }
}

/// symlink 用の DiffOutput を構築（ターゲットパスの差分）
///
/// 前提: `left_target` と `right_target` の少なくとも一方は `Some`。
/// 呼び出し側で `find_symlink_target` の結果を OR チェックしてからこの関数を呼ぶ。
pub fn build_symlink_diff_output(
    path: &str,
    left_info: SourceInfo,
    right_info: SourceInfo,
    left_target: Option<&str>,
    right_target: Option<&str>,
    sensitive: bool,
) -> DiffOutput {
    let note = match (left_target, right_target) {
        (Some(_), Some(_)) => None,
        (Some(_), None) | (None, Some(_)) => Some("type mismatch: symlink vs file".to_string()),
        (None, None) => None,
    };

    let hunks = match (left_target, right_target) {
        (Some(lt), Some(rt)) if lt != rt => {
            vec![DiffHunk {
                index: 0,
                left_start: 1,
                right_start: 1,
                lines: vec![
                    DiffLine {
                        line_type: DiffLineType::Removed,
                        content: lt.to_string(),
                    },
                    DiffLine {
                        line_type: DiffLineType::Added,
                        content: rt.to_string(),
                    },
                ],
            }]
        }
        _ => vec![],
    };

    DiffOutput {
        path: path.to_string(),
        left: left_info,
        right: right_info,
        ref_: None,
        sensitive,
        binary: false,
        symlink: true,
        truncated: false,
        hunks,
        ref_hunks: None,
        left_hash: None,
        right_hash: None,
        note,
        conflict_count: 0,
        conflict_regions: vec![],
    }
}

/// sensitive ファイル用のマスクされた DiffOutput を構築
pub fn build_masked_diff_output(
    path: &str,
    left_info: SourceInfo,
    right_info: SourceInfo,
) -> DiffOutput {
    DiffOutput {
        path: path.to_string(),
        left: left_info,
        right: right_info,
        ref_: None,
        sensitive: true,
        binary: false,
        symlink: false,
        truncated: false,
        hunks: vec![],
        ref_hunks: None,
        left_hash: None,
        right_hash: None,
        note: Some("Content hidden (sensitive file). Use --force to show.".to_string()),
        conflict_count: 0,
        conflict_regions: vec![],
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
        // 100行すべてが変更行（old_N → new_N）→ max_lines=5 で truncate される
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
        // 変更行（Added/Removed）が 5 行以下であること
        let change_lines: usize = output
            .hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| l.line_type != DiffLineType::Context)
            .count();
        assert!(change_lines <= 5);
    }

    #[test]
    fn test_max_lines_zero_is_unlimited() {
        // max_lines=Some(0) は制限なし（None と同等）
        let old = (0..20)
            .map(|i| format!("old_{}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let new = (0..20)
            .map(|i| format!("new_{}", i))
            .collect::<Vec<_>>()
            .join("\n");

        let output = build_diff_output(
            "file.rs",
            info("l"),
            info("r"),
            &old,
            &new,
            false,
            Some(0),
            None,
            None,
        );
        assert!(
            !output.truncated,
            "max_lines=0 should be treated as unlimited"
        );
    }

    #[test]
    fn test_max_lines_counts_only_change_lines_not_context() {
        // left と right で 1 行だけ変更、前後に context が多数ある場合
        // max_lines=3 → context は 3 行未満の変更行ではトランケートしない
        let old = "ctx1\nctx2\nctx3\nORIG\nctx4\nctx5\nctx6\n";
        let new = "ctx1\nctx2\nctx3\nMODIF\nctx4\nctx5\nctx6\n";

        let output = build_diff_output(
            "file.rs",
            info("l"),
            info("r"),
            old,
            new,
            false,
            Some(3),
            None,
            None,
        );
        // 変更行は removed + added = 2 行だけなので truncated にならない
        assert!(
            !output.truncated,
            "2 change lines should not trigger max_lines=3"
        );
        // context 行は出力に含まれる
        let has_context = output
            .hunks
            .iter()
            .flat_map(|h| &h.lines)
            .any(|l| l.line_type == DiffLineType::Context);
        assert!(has_context, "context lines should appear in output");
    }

    #[test]
    fn test_max_lines_larger_than_actual_changes_no_truncation() {
        // 変更行が max_lines より少ない場合は truncated=false
        let output = build_diff_output(
            "a.rs",
            info("l"),
            info("r"),
            "aaa\nbbb\nccc\n",
            "aaa\nBBB\nccc\n",
            false,
            Some(100),
            None,
            None,
        );
        assert!(!output.truncated);
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
    fn test_exit_code_binary_diff_found() {
        // T-1: binary=true, hunks=[] → DIFF_FOUND
        let output = DiffOutput {
            path: "image.png".into(),
            left: info("l"),
            right: info("r"),
            ref_: None,
            sensitive: false,
            binary: true,
            symlink: false,
            truncated: false,
            hunks: vec![],
            ref_hunks: None,
            left_hash: None,
            right_hash: None,
            note: None,
            conflict_count: 0,
            conflict_regions: vec![],
        };
        assert_eq!(diff_exit_code(&output), exit_code::DIFF_FOUND);
    }

    #[test]
    fn test_exit_code_symlink_diff_found() {
        // symlink=true → DIFF_FOUND
        let output = DiffOutput {
            path: "link".into(),
            left: info("l"),
            right: info("r"),
            ref_: None,
            sensitive: false,
            binary: false,
            symlink: true,
            truncated: false,
            hunks: vec![],
            ref_hunks: None,
            left_hash: None,
            right_hash: None,
            note: None,
            conflict_count: 0,
            conflict_regions: vec![],
        };
        assert_eq!(diff_exit_code(&output), exit_code::DIFF_FOUND);
    }

    #[test]
    fn test_symlink_diff_sets_symlink_flag() {
        // compute_diff が SymlinkDiff を返すケースは engine 側でのみ構築されるため、
        // ここでは build_diff_output 経由ではなく直接フラグを検証する。
        // engine::compute_diff はテキスト入力のみ受け付けるため、
        // SymlinkDiff は engine 層のテストでカバーし、ここでは出力型の分離を検証。
        let output = DiffOutput {
            path: "link".into(),
            left: info("l"),
            right: info("r"),
            ref_: None,
            sensitive: false,
            binary: false,
            symlink: true,
            truncated: false,
            hunks: vec![],
            ref_hunks: None,
            left_hash: None,
            right_hash: None,
            note: None,
            conflict_count: 0,
            conflict_regions: vec![],
        };
        assert!(output.symlink);
        assert!(!output.binary);
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
        // ref_hunks も max_lines の制限を受け、変更行のみカウントされる
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
        // 変更行（Added/Removed）が 5 行以下であること
        let main_change_lines: usize = output
            .hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| l.line_type != DiffLineType::Context)
            .count();
        let ref_change_lines: usize = output
            .ref_hunks
            .as_ref()
            .unwrap()
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| l.line_type != DiffLineType::Context)
            .count();
        assert!(
            main_change_lines <= 5,
            "main hunks: {} change lines",
            main_change_lines
        );
        assert!(
            ref_change_lines <= 5,
            "ref hunks: {} change lines",
            ref_change_lines
        );
    }

    // ── build_symlink_diff_output tests ──

    #[test]
    fn test_build_symlink_diff_output_both_symlinks_different() {
        let output = build_symlink_diff_output(
            "link",
            info("local"),
            info("dev"),
            Some("/old/target"),
            Some("/new/target"),
            false,
        );
        assert!(output.symlink);
        assert!(!output.binary);
        assert!(output.note.is_none());
        assert_eq!(output.hunks.len(), 1);
        assert!(output.hunks[0]
            .lines
            .iter()
            .any(|l| l.content == "/old/target"));
        assert!(output.hunks[0]
            .lines
            .iter()
            .any(|l| l.content == "/new/target"));
    }

    #[test]
    fn test_build_symlink_diff_output_both_symlinks_same() {
        let output = build_symlink_diff_output(
            "link",
            info("local"),
            info("dev"),
            Some("/same/target"),
            Some("/same/target"),
            false,
        );
        assert!(output.symlink);
        assert!(output.hunks.is_empty());
    }

    #[test]
    fn test_build_symlink_diff_output_type_mismatch() {
        let output = build_symlink_diff_output(
            "link",
            info("local"),
            info("dev"),
            Some("/target"),
            None,
            false,
        );
        assert!(output.symlink);
        assert_eq!(
            output.note.as_deref(),
            Some("type mismatch: symlink vs file")
        );
    }

    #[test]
    fn test_build_symlink_diff_output_dangling() {
        let output = build_symlink_diff_output(
            "link",
            info("local"),
            info("dev"),
            Some(""),
            Some("/target"),
            false,
        );
        assert!(output.symlink);
        // 空文字列でも差分として表示される
        assert_eq!(output.hunks.len(), 1);
    }

    // ── build_masked_diff_output tests ──

    #[test]
    fn test_build_masked_diff_output() {
        let output = build_masked_diff_output(".env", info("local"), info("dev"));
        assert!(output.sensitive);
        assert!(output.hunks.is_empty());
        assert!(output.left_hash.is_none());
        assert!(output.right_hash.is_none());
        assert!(output.note.as_deref().unwrap().contains("Content hidden"));
        assert!(output.note.as_deref().unwrap().contains("--force"));
    }

    #[test]
    fn test_build_masked_diff_output_is_not_binary() {
        let output = build_masked_diff_output(".env", info("local"), info("dev"));
        assert!(!output.binary);
        assert!(!output.symlink);
    }

    // ── diff_exit_code + sensitive mask tests ──

    #[test]
    fn test_exit_code_sensitive_masked_diff_found() {
        // sensitive マスク（note 付き）→ 差分ありとして DIFF_FOUND
        let output = build_masked_diff_output(".env", info("l"), info("r"));
        assert_eq!(diff_exit_code(&output), exit_code::DIFF_FOUND);
    }

    #[test]
    fn test_exit_code_sensitive_forced_no_diff() {
        // --force で sensitive ファイルを表示（差分なし）→ SUCCESS
        let output = DiffOutput {
            path: ".env".into(),
            left: info("l"),
            right: info("r"),
            ref_: None,
            sensitive: true,
            binary: false,
            symlink: false,
            truncated: false,
            hunks: vec![],
            ref_hunks: None,
            left_hash: None,
            right_hash: None,
            note: None,
            conflict_count: 0,
            conflict_regions: vec![],
        };
        assert_eq!(diff_exit_code(&output), exit_code::SUCCESS);
    }

    // ── conflict detection in build_diff_output ──

    #[test]
    fn test_conflict_count_with_ref() {
        let output = build_diff_output(
            "a.rs",
            info("l"),
            info("r"),
            "B\n",
            "C\n",
            false,
            None,
            Some(info("ref")),
            Some("A\n"),
        );
        assert_eq!(output.conflict_count, 1);
        assert!(!output.conflict_regions.is_empty());
    }

    #[test]
    fn test_conflict_count_without_ref() {
        let output = build_diff_output(
            "a.rs",
            info("l"),
            info("r"),
            "B\n",
            "C\n",
            false,
            None,
            None,
            None,
        );
        assert_eq!(output.conflict_count, 0);
        assert!(output.conflict_regions.is_empty());
    }

    #[test]
    fn test_conflict_count_binary_is_zero() {
        let binary_content = "hello\x00world";
        let output = build_diff_output(
            "image.png",
            info("l"),
            info("r"),
            binary_content,
            "different\x00data",
            false,
            None,
            Some(info("ref")),
            Some("original\x00content"),
        );
        assert!(output.binary);
        assert_eq!(output.conflict_count, 0);
        assert!(output.conflict_regions.is_empty());
    }
}
