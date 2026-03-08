//! merge サービス: ファイルマージの結果型組み立て。
//!
//! 実際のI/O操作（SSH書き込み・バックアップ）は CLI 層が CoreRuntime 経由で行う。
//! このモジュールは結果の組み立てと dry-run 判定のみ。

use super::status::is_sensitive;
use super::types::*;

/// マージ対象ファイルの前処理結果
#[derive(Debug, Clone)]
pub struct MergePlan {
    /// マージ対象ファイル
    pub files: Vec<String>,
    /// スキップ対象（センシティブ等）
    pub skipped: Vec<MergeSkipped>,
}

/// マージ対象をフィルタリングし、MergePlan を構築する（純粋関数）。
///
/// センシティブファイルは `--force` なしではスキップする。
pub fn plan_merge(paths: &[String], sensitive_patterns: &[String], force: bool) -> MergePlan {
    let mut files = Vec::new();
    let mut skipped = Vec::new();

    for path in paths {
        if !force && is_sensitive(path, sensitive_patterns) {
            skipped.push(MergeSkipped {
                path: path.clone(),
                reason: "sensitive file".into(),
            });
        } else {
            files.push(path.clone());
        }
    }

    MergePlan { files, skipped }
}

/// マージ結果を組み立てる（純粋関数）。
pub fn build_merge_output(
    merged: Vec<MergeFileResult>,
    skipped: Vec<MergeSkipped>,
    failed: Vec<MergeFailure>,
    ref_info: Option<SourceInfo>,
) -> MergeOutput {
    MergeOutput {
        merged,
        skipped,
        failed,
        ref_: ref_info,
    }
}

/// merge の exit code を判定する。
pub fn merge_exit_code(output: &MergeOutput) -> i32 {
    if output.failed.is_empty() {
        exit_code::SUCCESS
    } else {
        exit_code::ERROR
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_merge_skips_sensitive() {
        let paths = vec![".env".into(), "src/main.rs".into(), "secret.pem".into()];
        let patterns = vec![".env".into(), "*.pem".into()];
        let plan = plan_merge(&paths, &patterns, false);

        assert_eq!(plan.files, vec!["src/main.rs"]);
        assert_eq!(plan.skipped.len(), 2);
        assert_eq!(plan.skipped[0].path, ".env");
        assert_eq!(plan.skipped[1].path, "secret.pem");
    }

    #[test]
    fn test_plan_merge_force_includes_sensitive() {
        let paths = vec![".env".into(), "src/main.rs".into()];
        let patterns = vec![".env".into()];
        let plan = plan_merge(&paths, &patterns, true);

        assert_eq!(plan.files.len(), 2);
        assert!(plan.skipped.is_empty());
    }

    #[test]
    fn test_plan_merge_empty() {
        let plan = plan_merge(&[], &[], false);
        assert!(plan.files.is_empty());
        assert!(plan.skipped.is_empty());
    }

    #[test]
    fn test_build_merge_output() {
        let output = build_merge_output(
            vec![MergeFileResult {
                path: "a.rs".into(),
                status: "ok".into(),
                backup: None,
                ref_badge: None,
            }],
            vec![],
            vec![],
            None,
        );
        assert_eq!(output.merged.len(), 1);
        assert!(output.failed.is_empty());
    }

    #[test]
    fn test_merge_exit_code_success() {
        let output = build_merge_output(
            vec![MergeFileResult {
                path: "a.rs".into(),
                status: "ok".into(),
                backup: None,
                ref_badge: None,
            }],
            vec![],
            vec![],
            None,
        );
        assert_eq!(merge_exit_code(&output), exit_code::SUCCESS);
    }

    #[test]
    fn test_merge_exit_code_failure() {
        let output = build_merge_output(
            vec![],
            vec![],
            vec![MergeFailure {
                path: "a.rs".into(),
                error: "write error".into(),
            }],
            None,
        );
        assert_eq!(merge_exit_code(&output), exit_code::ERROR);
    }

    #[test]
    fn test_build_merge_output_with_ref() {
        let output = build_merge_output(
            vec![MergeFileResult {
                path: "a.rs".into(),
                status: "ok".into(),
                backup: None,
                ref_badge: Some("differs".into()),
            }],
            vec![],
            vec![],
            Some(SourceInfo {
                label: "staging".into(),
                root: "/s".into(),
            }),
        );
        assert!(output.ref_.is_some());
        assert_eq!(output.ref_.as_ref().unwrap().label, "staging");
        assert_eq!(output.merged[0].ref_badge.as_ref().unwrap(), "differs");
    }

    #[test]
    fn test_build_merge_output_no_ref_backward_compat() {
        let output = build_merge_output(
            vec![MergeFileResult {
                path: "a.rs".into(),
                status: "ok".into(),
                backup: None,
                ref_badge: None,
            }],
            vec![],
            vec![],
            None,
        );
        assert!(output.ref_.is_none());
        let json = serde_json::to_string(&output).unwrap();
        assert!(!json.contains("\"ref\""));
        assert!(!json.contains("\"ref_badge\""));
    }
}
