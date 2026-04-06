//! sync サービス: 1:N マルチサーバ同期の計画・結果組み立て。
//! 純粋関数のみ。I/O は cli/sync.rs で行う。

use super::status::is_sensitive;
use super::types::*;

/// SyncTargetResult の status を判定する（純粋関数）。
///
/// - merged > 0 && failed == 0 → Success
/// - merged > 0 && failed > 0  → Partial
/// - merged == 0 && failed > 0  → Failed
/// - merged == 0 && failed == 0  → Success（全スキップ or 差分なし）
///
/// **前提条件:** 呼び出し元は `execute_deletions` が返す delete failures を
/// `result.failed` に事前にマージしておくこと。そうしないと削除失敗が
/// ステータス判定に反映されない。
pub fn compute_target_status(result: &SyncTargetResult) -> SyncTargetStatus {
    let has_merged = !result.merged.is_empty();
    let has_failed = !result.failed.is_empty();

    match (has_merged, has_failed) {
        (true, false) => SyncTargetStatus::Success,
        (true, true) => SyncTargetStatus::Partial,
        (false, true) => SyncTargetStatus::Failed,
        (false, false) => SyncTargetStatus::Success,
    }
}

/// SyncSummary を集計する（純粋関数）。
pub fn compute_sync_summary(results: &[SyncTargetResult]) -> SyncSummary {
    let total_servers = results.len();
    let successful_servers = results
        .iter()
        .filter(|r| r.status == SyncTargetStatus::Success)
        .count();
    let total_files_merged: usize = results.iter().map(|r| r.merged.len()).sum();
    let total_files_deleted: usize = results.iter().map(|r| r.deleted.len()).sum();
    let total_files_failed: usize = results.iter().map(|r| r.failed.len()).sum();

    SyncSummary {
        total_servers,
        successful_servers,
        total_files_merged,
        total_files_deleted,
        total_files_failed,
    }
}

/// sync の exit code を判定する（純粋関数）。
///
/// 全 Success → 0、1つでも Failed/Partial → 2（ERROR）
pub fn sync_exit_code(output: &SyncOutput) -> i32 {
    let all_success = output
        .targets
        .iter()
        .all(|t| t.status == SyncTargetStatus::Success);

    if all_success {
        exit_code::SUCCESS
    } else {
        exit_code::ERROR
    }
}

/// --delete 時に削除対象となるファイルを抽出する（純粋関数）。
///
/// RightOnly ファイルのうち、`resolved_paths` に含まれるものを抽出。
/// sensitive ファイルは `force=false` で MergeSkipped に振り分ける。
pub fn plan_deletions(
    statuses: &[FileStatus],
    resolved_paths: &[String],
    sensitive_patterns: &[String],
    force: bool,
) -> (Vec<String>, Vec<MergeSkipped>) {
    let mut to_delete = Vec::new();
    let mut skipped = Vec::new();

    for file in statuses {
        if file.status != FileStatusKind::RightOnly {
            continue;
        }
        if !resolved_paths.contains(&file.path) {
            continue;
        }
        if !force && is_sensitive(&file.path, sensitive_patterns) {
            skipped.push(MergeSkipped {
                path: file.path.clone(),
                reason: "sensitive file (use --force to include)".into(),
            });
            continue;
        }
        to_delete.push(file.path.clone());
    }

    (to_delete, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ヘルパー関数 ──

    fn make_file_status(path: &str, kind: FileStatusKind) -> FileStatus {
        FileStatus {
            path: path.into(),
            status: kind,
            sensitive: false,
            hunks: None,
            ref_badge: None,
        }
    }

    fn make_target_result(
        label: &str,
        merged_count: usize,
        failed_count: usize,
    ) -> SyncTargetResult {
        SyncTargetResult {
            target: SourceInfo {
                label: label.into(),
                root: "/app".into(),
            },
            merged: (0..merged_count)
                .map(|i| MergeFileResult {
                    path: format!("file{i}.rs"),
                    status: "ok".into(),
                    backup: None,
                    ref_badge: None,
                    hunks_applied: None,
                    hunks_total: None,
                    direction: None,
                })
                .collect(),
            skipped: vec![],
            deleted: vec![],
            failed: (0..failed_count)
                .map(|i| MergeFailure {
                    path: format!("fail{i}.rs"),
                    error: "permission denied".into(),
                })
                .collect(),
            status: SyncTargetStatus::Success, // 仮値（テストで compute_target_status を呼んで検証する）
        }
    }

    fn make_sync_output(targets: Vec<SyncTargetResult>) -> SyncOutput {
        let summary = compute_sync_summary(&targets);
        SyncOutput {
            left: SourceInfo {
                label: "local".into(),
                root: "/app".into(),
            },
            targets,
            summary,
        }
    }

    // ── compute_target_status ──

    #[test]
    fn compute_target_status_all_success() {
        let result = make_target_result("server1", 3, 0);
        assert_eq!(compute_target_status(&result), SyncTargetStatus::Success);
    }

    #[test]
    fn compute_target_status_partial() {
        let result = make_target_result("server1", 2, 1);
        assert_eq!(compute_target_status(&result), SyncTargetStatus::Partial);
    }

    #[test]
    fn compute_target_status_all_failed() {
        let result = make_target_result("server1", 0, 3);
        assert_eq!(compute_target_status(&result), SyncTargetStatus::Failed);
    }

    #[test]
    fn compute_target_status_no_files() {
        let result = make_target_result("server1", 0, 0);
        assert_eq!(compute_target_status(&result), SyncTargetStatus::Success);
    }

    // ── compute_sync_summary ──

    #[test]
    fn compute_sync_summary_multiple_servers() {
        let mut r1 = make_target_result("server1", 3, 0);
        r1.status = SyncTargetStatus::Success;
        r1.deleted = vec![DeleteFileResult {
            path: "old.txt".into(),
            status: DeleteStatus::Ok,
            backup: None,
        }];

        let mut r2 = make_target_result("server2", 1, 2);
        r2.status = SyncTargetStatus::Partial;

        let summary = compute_sync_summary(&[r1, r2]);
        assert_eq!(summary.total_servers, 2);
        assert_eq!(summary.successful_servers, 1);
        assert_eq!(summary.total_files_merged, 4); // 3 + 1
        assert_eq!(summary.total_files_deleted, 1);
        assert_eq!(summary.total_files_failed, 2); // 0 + 2
    }

    // ── sync_exit_code ──

    #[test]
    fn sync_exit_code_all_success() {
        let mut r1 = make_target_result("server1", 3, 0);
        r1.status = SyncTargetStatus::Success;
        let mut r2 = make_target_result("server2", 2, 0);
        r2.status = SyncTargetStatus::Success;

        let output = make_sync_output(vec![r1, r2]);
        assert_eq!(sync_exit_code(&output), 0);
    }

    #[test]
    fn sync_exit_code_some_failed() {
        let mut r1 = make_target_result("server1", 3, 0);
        r1.status = SyncTargetStatus::Success;
        let mut r2 = make_target_result("server2", 0, 2);
        r2.status = SyncTargetStatus::Failed;

        let output = make_sync_output(vec![r1, r2]);
        assert_eq!(sync_exit_code(&output), 2);
    }

    // ── plan_deletions ──

    #[test]
    fn plan_deletions_extracts_right_only() {
        let statuses = vec![
            make_file_status("a.rs", FileStatusKind::Modified),
            make_file_status("b.rs", FileStatusKind::RightOnly),
            make_file_status("c.rs", FileStatusKind::LeftOnly),
            make_file_status("d.rs", FileStatusKind::RightOnly),
        ];
        let resolved = vec!["b.rs".into(), "d.rs".into()];
        let (to_delete, skipped) = plan_deletions(&statuses, &resolved, &[], false);

        assert_eq!(to_delete, vec!["b.rs", "d.rs"]);
        assert!(skipped.is_empty());
    }

    #[test]
    fn plan_deletions_sensitive_skipped_without_force() {
        let statuses = vec![
            make_file_status("app.rs", FileStatusKind::RightOnly),
            make_file_status(".env", FileStatusKind::RightOnly),
            make_file_status("secret.pem", FileStatusKind::RightOnly),
        ];
        let resolved: Vec<String> = vec!["app.rs".into(), ".env".into(), "secret.pem".into()];
        let patterns = vec![".env".into(), "*.pem".into()];

        let (to_delete, skipped) = plan_deletions(&statuses, &resolved, &patterns, false);

        assert_eq!(to_delete, vec!["app.rs"]);
        assert_eq!(skipped.len(), 2);
        assert_eq!(skipped[0].path, ".env");
        assert_eq!(skipped[0].reason, "sensitive file (use --force to include)");
        assert_eq!(skipped[1].path, "secret.pem");
    }

    #[test]
    fn plan_deletions_no_right_only() {
        let statuses = vec![
            make_file_status("a.rs", FileStatusKind::Modified),
            make_file_status("b.rs", FileStatusKind::LeftOnly),
            make_file_status("c.rs", FileStatusKind::Equal),
        ];
        let resolved: Vec<String> = vec!["a.rs".into(), "b.rs".into(), "c.rs".into()];

        let (to_delete, skipped) = plan_deletions(&statuses, &resolved, &[], false);

        assert!(to_delete.is_empty());
        assert!(skipped.is_empty());
    }

    #[test]
    fn plan_deletions_sensitive_with_force() {
        let statuses = vec![make_file_status(".env", FileStatusKind::RightOnly)];
        let resolved: Vec<String> = vec![".env".into()];
        let patterns = vec![".env".into()];

        let (to_delete, skipped) = plan_deletions(&statuses, &resolved, &patterns, true);

        assert_eq!(to_delete, vec![".env"]);
        assert!(skipped.is_empty());
    }
}
