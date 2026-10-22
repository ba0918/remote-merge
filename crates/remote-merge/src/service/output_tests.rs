//! text と JSON の書き出しの単体テスト
use super::*;

/// 左（local）と右（dev）を比べ、差も参照先もない diff の項目
fn plain_diff(path: &str) -> DiffOutput {
    DiffOutput {
        encodings: Default::default(),
        byte_only_difference: false,
        path: path.into(),
        left: SourceInfo {
            label: "local".into(),
            root: ".".into(),
        },
        right: SourceInfo {
            label: "dev".into(),
            root: "/r".into(),
        },
        ref_: None,
        binary: false,
        symlink: false,
        link_targets: None,
        truncated: false,
        hunks: vec![],
        ref_hunks: None,
        left_hash: None,
        right_hash: None,
        ref_hash: None,
        note: None,
        conflict_count: 0,
        conflict_regions: vec![],
    }
}

/// 結果のない merge の出力
fn empty_merge() -> MergeOutput {
    MergeOutput {
        merged: vec![],
        skipped: vec![],
        deleted: vec![],
        failed: vec![],
        ref_: None,
    }
}

/// 書き込み先 `label`（root_dir は /app）の、結果のない success の sync の結果
fn empty_target(label: &str) -> SyncTargetResult {
    SyncTargetResult {
        target: SourceInfo {
            label: label.into(),
            root: "/app".into(),
        },
        merged: vec![],
        skipped: vec![],
        deleted: vec![],
        failed: vec![],
        status: SyncTargetStatus::Success,
    }
}

/// local（root_dir は /app）から `targets` への sync の出力
fn sync_from_local(targets: Vec<SyncTargetResult>, summary: SyncSummary) -> SyncOutput {
    SyncOutput {
        left: SourceInfo {
            label: "local".into(),
            root: "/app".into(),
        },
        targets,
        summary,
    }
}

fn sample_status() -> StatusOutput {
    StatusOutput {
        left: SourceInfo {
            label: "local".into(),
            root: "/home/user/app".into(),
        },
        right: SourceInfo {
            label: "develop".into(),
            root: "dev:/var/www".into(),
        },
        ref_: None,
        agent: None,
        files: Some(vec![
            FileStatus {
                path: "src/config.ts".into(),
                status: FileStatusKind::Modified,
                ref_badge: None,
            },
            FileStatus {
                path: "src/new.ts".into(),
                status: FileStatusKind::LeftOnly,
                ref_badge: None,
            },
            FileStatus {
                path: ".env".into(),
                status: FileStatusKind::Modified,
                ref_badge: None,
            },
        ]),
        summary: StatusSummary {
            modified: 2,
            left_only: 1,
            right_only: 0,
            equal: 0,
            ref_differs: None,
            ref_only: None,
            ref_missing: None,
            unreadable: 0,
        },
        errors: Vec::new(),
    }
}

#[test]
fn test_format_status_text() {
    let text = format_status_text(&sample_status(), false);
    assert!(text.contains("M src/config.ts"));
    assert!(text.contains("L src/new.ts"));
    assert!(text.lines().any(|line| line == "M .env"), "{text}");
    assert!(text.contains("Summary: 2 modified, 1 left only"));
}

#[test]
fn test_format_status_text_agent_connected() {
    let mut output = sample_status();
    output.agent = Some(AgentStatus::Connected);
    let text = format_status_text(&output, false);
    assert!(text.contains("Agent: connected"));
}

#[test]
fn test_format_status_text_no_agent() {
    let output = sample_status();
    let text = format_status_text(&output, false);
    assert!(!text.contains("Agent:"));
}

#[test]
fn test_format_diff_text() {
    let output = DiffOutput {
        right: SourceInfo {
            label: "develop".into(),
            root: "/var/www".into(),
        },
        hunks: vec![DiffHunk {
            index: 0,
            left_start: 10,
            right_start: 10,
            lines: vec![
                DiffLine {
                    line_type: DiffLineType::Context,
                    content: "  fn hello() {".into(),
                },
                DiffLine {
                    line_type: DiffLineType::Removed,
                    content: "  old".into(),
                },
                DiffLine {
                    line_type: DiffLineType::Added,
                    content: "  new".into(),
                },
            ],
        }],
        ..plain_diff("src/config.ts")
    };
    let text = format_diff_text(&output);
    assert!(text.contains("--- a/src/config.ts (local)"));
    assert!(text.contains("+++ b/src/config.ts (develop)"));
    assert!(text.contains("-  old"));
    assert!(text.contains("+  new"));
    assert!(!text.contains("truncated"));
}

#[test]
fn test_format_diff_text_truncated() {
    let output = DiffOutput {
        left: SourceInfo {
            label: "l".into(),
            root: ".".into(),
        },
        right: SourceInfo {
            label: "r".into(),
            root: "/r".into(),
        },
        truncated: true,
        ..plain_diff("big.rs")
    };
    let text = format_diff_text(&output);
    assert!(text.contains("truncated"));
}

#[test]
fn test_format_merge_text() {
    let output = MergeOutput {
        merged: vec![MergeFileResult {
            path: "a.rs".into(),
            status: "ok".into(),
            backup: Some("a.rs.bak".into()),
            ..Default::default()
        }],
        skipped: vec![MergeSkipped {
            path: "kind.txt".into(),
            reason: "source and destination have different file types".into(),
        }],
        ..empty_merge()
    };
    let text = format_merge_text(&output);
    assert!(text.contains("Merged: a.rs (backup: a.rs.bak)"));
    assert!(text.contains("Skipped: kind.txt (source and destination have different file types)"));
}

#[test]
fn test_format_json() {
    let output = sample_status();
    let json = format_json(&output).unwrap();
    assert!(json.contains("\"modified\""));
    assert!(json.contains("\"local\""));
}

#[test]
fn test_output_format_parse() {
    assert_eq!(OutputFormat::parse("text").unwrap(), OutputFormat::Text);
    assert_eq!(OutputFormat::parse("json").unwrap(), OutputFormat::Json);
    assert_eq!(OutputFormat::parse("diff").unwrap(), OutputFormat::Text);
    assert!(OutputFormat::parse("yaml").is_err());
}

#[test]
fn test_format_status_text_with_ref_badges() {
    let output = StatusOutput {
        left: SourceInfo {
            label: "local".into(),
            root: ".".into(),
        },
        right: SourceInfo {
            label: "dev".into(),
            root: "/r".into(),
        },
        ref_: Some(SourceInfo {
            label: "staging".into(),
            root: "/s".into(),
        }),
        agent: None,
        files: Some(vec![
            FileStatus {
                path: "a.rs".into(),
                status: FileStatusKind::Modified,
                ref_badge: Some("differs".into()),
            },
            FileStatus {
                path: "b.rs".into(),
                status: FileStatusKind::Equal,
                ref_badge: Some("missing_in_ref".into()),
            },
            FileStatus {
                path: "c.rs".into(),
                status: FileStatusKind::Modified,
                ref_badge: Some("all_equal".into()),
            },
        ]),
        summary: StatusSummary {
            modified: 2,
            left_only: 0,
            right_only: 0,
            equal: 1,
            ref_differs: Some(1),
            ref_only: Some(0),
            ref_missing: Some(1),
            unreadable: 0,
        },
        errors: Vec::new(),
    };
    let text = format_status_text(&output, false);
    assert!(text.contains("= b.rs [ref-]"));
    // all_equal badge should NOT be displayed
    assert!(!text.contains("c.rs [ref"));
    assert!(text.contains("Ref: 1 differs, 0 ref-only, 1 ref-missing"));
}

#[test]
fn test_format_status_text_no_ref_backward_compat() {
    let output = StatusOutput {
        left: SourceInfo {
            label: "local".into(),
            root: ".".into(),
        },
        right: SourceInfo {
            label: "dev".into(),
            root: "/r".into(),
        },
        ref_: None,
        agent: None,
        files: Some(vec![FileStatus {
            path: "a.rs".into(),
            status: FileStatusKind::Modified,
            ref_badge: None,
        }]),
        summary: StatusSummary {
            modified: 1,
            left_only: 0,
            right_only: 0,
            equal: 0,
            ref_differs: None,
            ref_only: None,
            ref_missing: None,
            unreadable: 0,
        },
        errors: Vec::new(),
    };
    let text = format_status_text(&output, false);
    assert!(!text.contains("Ref:"));
    assert!(!text.contains("[ref"));
}

#[test]
fn test_format_diff_text_with_ref_hunks() {
    let output = DiffOutput {
        ref_: Some(SourceInfo {
            label: "staging".into(),
            root: "/s".into(),
        }),
        ref_hunks: Some(vec![DiffHunk {
            index: 0,
            left_start: 1,
            right_start: 1,
            lines: vec![
                DiffLine {
                    line_type: DiffLineType::Removed,
                    content: "old ref".into(),
                },
                DiffLine {
                    line_type: DiffLineType::Added,
                    content: "new ref".into(),
                },
            ],
        }]),
        ..plain_diff("config.ts")
    };
    let text = format_diff_text(&output);
    assert!(text.contains("--- ref:staging:config.ts (reference diff vs left)"));
    assert!(text.contains("-old ref"));
    assert!(text.contains("+new ref"));
}

#[test]
fn test_format_diff_text_no_ref_backward_compat() {
    let output = DiffOutput {
        left: SourceInfo {
            label: "l".into(),
            root: ".".into(),
        },
        right: SourceInfo {
            label: "r".into(),
            root: "/r".into(),
        },
        ..plain_diff("a.rs")
    };
    let text = format_diff_text(&output);
    assert!(!text.contains("ref"));
}

#[test]
fn test_format_diff_text_binary() {
    // T-3: binary=true の場合 "Binary files differ" が出力される
    let output = DiffOutput {
        binary: true,
        ..plain_diff("image.png")
    };
    let text = format_diff_text(&output);
    assert!(text.contains("Binary files differ"));
    assert!(text.contains("--- a/image.png"));
    assert!(text.contains("+++ b/image.png"));
    // hunks の @@ ヘッダは出力されない
    assert!(!text.contains("@@"));
}

#[test]
fn test_format_diff_text_symlink() {
    let output = DiffOutput {
        symlink: true,
        link_targets: Some(crate::service::types::LinkTargets {
            left: Some("before.txt".into()),
            right: Some("after.txt".into()),
            ref_: None,
        }),
        ..plain_diff("link")
    };
    let text = format_diff_text(&output);
    assert!(text.contains("Symbolic link targets differ"));
    assert!(!text.contains("Binary files differ"));
}

#[test]
fn test_format_merge_text_dry_run_with_ref_badge() {
    let output = MergeOutput {
        merged: vec![MergeFileResult {
            path: "a.rs".into(),
            status: "would merge".into(),
            ref_badge: Some("differs".into()),
            ..Default::default()
        }],
        ref_: Some(SourceInfo {
            label: "staging".into(),
            root: "/s".into(),
        }),
        ..empty_merge()
    };
    let text = format_merge_text(&output);
    assert!(text.contains("Would merge: a.rs [ref≠]"));
}

#[test]
fn test_format_merge_text_with_ref_badge() {
    let output = MergeOutput {
        merged: vec![MergeFileResult {
            path: "a.rs".into(),
            status: "ok".into(),
            ref_badge: Some("differs".into()),
            ..Default::default()
        }],
        ref_: Some(SourceInfo {
            label: "staging".into(),
            root: "/s".into(),
        }),
        ..empty_merge()
    };
    let text = format_merge_text(&output);
    assert!(text.contains("Merged: a.rs [ref≠]"));
}

#[test]
fn test_format_merge_text_hunk_merge_info() {
    let output = MergeOutput {
        merged: vec![MergeFileResult {
            path: "src/foo.rs".into(),
            status: "merged".into(),
            hunk_info: Some(HunkMergeInfo {
                hunks_applied: vec![0, 2, 5],
                hunks_total: 8,
                direction: "left_to_right".into(),
            }),
            ..Default::default()
        }],
        ..empty_merge()
    };
    let text = format_merge_text(&output);
    assert!(
        text.contains("(hunks: 0,2,5/8)"),
        "hunk info missing in: {}",
        text
    );
    assert!(text.contains("Merged: src/foo.rs"));
}

// ── multi diff text tests ──

fn sample_diff(path: &str) -> DiffOutput {
    DiffOutput {
        hunks: vec![DiffHunk {
            index: 0,
            left_start: 1,
            right_start: 1,
            lines: vec![
                DiffLine {
                    line_type: DiffLineType::Removed,
                    content: "old".into(),
                },
                DiffLine {
                    line_type: DiffLineType::Added,
                    content: "new".into(),
                },
            ],
        }],
        ..plain_diff(path)
    }
}

#[test]
fn test_format_multi_diff_text_single_file() {
    let output = MultiDiffOutput {
        files: vec![sample_diff("a.rs")],
        errors: vec![],
        summary: MultiDiffSummary {
            scanned_files: 1,
            files_with_changes: 1,
        },
        truncated: false,
        changed_files_total: None,
    };
    let text = format_multi_diff_text(&output);
    assert!(text.contains("--- a/a.rs"));
    assert!(text.contains("+++ b/a.rs"));
    assert!(text.contains("-old"));
    assert!(text.contains("+new"));
    assert!(text.contains("1 file(s) with changes out of 1 total"));
}

#[test]
fn test_format_multi_diff_text_multiple_files() {
    let output = MultiDiffOutput {
        files: vec![sample_diff("a.rs"), sample_diff("b.rs")],
        errors: vec![],
        summary: MultiDiffSummary {
            scanned_files: 5,
            files_with_changes: 2,
        },
        truncated: false,
        changed_files_total: None,
    };
    let text = format_multi_diff_text(&output);
    assert!(text.contains("--- a/a.rs"));
    assert!(text.contains("--- a/b.rs"));
    assert!(text.contains("2 file(s) with changes out of 5 total"));
}

#[test]
fn test_format_multi_diff_text_truncated() {
    let output = MultiDiffOutput {
        files: vec![sample_diff("a.rs")],
        errors: vec![],
        summary: MultiDiffSummary {
            scanned_files: 10,
            files_with_changes: 1,
        },
        truncated: true,
        changed_files_total: Some(10),
    };
    let text = format_multi_diff_text(&output);
    assert!(text.contains("... and 9 more files (truncated, use --max-files 0 for all)"));
}

// ── status header tests ──

#[test]
fn test_status_header_without_ref() {
    let text = format_status_text(&sample_status(), false);
    assert!(text.starts_with("Comparing: local \u{2194} develop"));
    assert!(!text.contains("(ref:"));
}

#[test]
fn test_status_header_with_ref() {
    let output = StatusOutput {
        left: SourceInfo {
            label: "local".into(),
            root: ".".into(),
        },
        right: SourceInfo {
            label: "staging".into(),
            root: "/s".into(),
        },
        ref_: Some(SourceInfo {
            label: "develop".into(),
            root: "/d".into(),
        }),
        agent: None,
        files: Some(vec![]),
        summary: StatusSummary::default(),
        errors: Vec::new(),
    };
    let text = format_status_text(&output, false);
    assert!(text.starts_with("Comparing: local \u{2194} staging (ref: develop)"));
}

#[test]
fn test_status_header_appears_in_summary_only_mode() {
    let text = format_status_text(&sample_status(), true);
    assert!(text.contains("Comparing: local \u{2194} develop"));
    assert!(text.contains("Summary:"));
    // ファイル一覧は出力されない
    assert!(!text.contains("src/config.ts"));
}

// ── binary diff with SHA-256 hashes ──

#[test]
fn test_format_diff_text_binary_with_hashes() {
    let output = DiffOutput {
        binary: true,
        left_hash: Some("abc123def456".into()),
        right_hash: Some("789xyz000111".into()),
        ..plain_diff("image.png")
    };
    let text = format_diff_text(&output);
    assert!(
        text.contains(
            "Binary files differ (left: sha256=abc123def456, right: sha256=789xyz000111)"
        )
    );
    assert!(!text.contains("@@"));
}

#[test]
fn test_format_diff_text_binary_left_only_hash() {
    let output = DiffOutput {
        binary: true,
        left_hash: Some("abc123".into()),
        ..plain_diff("image.png")
    };
    let text = format_diff_text(&output);
    assert!(text.contains("Binary files differ (left: sha256=abc123, right: missing)"));
}

#[test]
fn test_format_diff_text_binary_no_hashes_fallback() {
    let output = DiffOutput {
        binary: true,
        ..plain_diff("image.png")
    };
    let text = format_diff_text(&output);
    assert!(text.contains("Binary files differ"));
    assert!(!text.contains("sha256="));
}

// ── MergeOutcome formatter tests ──

#[test]
fn test_format_merge_outcome_text_no_files() {
    let text = format_merge_outcome_text(&MergeOutcome::NoFilesToMerge);
    assert_eq!(text, "no files to merge in the specified path(s)");
}

#[test]
fn test_format_merge_outcome_text_r2r_blocked() {
    let text = format_merge_outcome_text(&MergeOutcome::R2rBlocked {
        left: "develop".into(),
        right: "staging".into(),
    });
    assert!(text.contains("Warning: merging between two remote servers"));
    assert!(text.contains("develop → staging"));
    assert!(text.contains("--force"));
}

#[test]
fn test_format_merge_outcome_json_no_files() {
    let json = format_merge_outcome_json(&MergeOutcome::NoFilesToMerge).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["merged"], serde_json::json!([]));
    assert_eq!(v["skipped"], serde_json::json!([]));
    assert_eq!(v["failed"], serde_json::json!([]));
}

#[test]
fn test_format_merge_outcome_json_r2r_blocked() {
    let json = format_merge_outcome_json(&MergeOutcome::R2rBlocked {
        left: "develop".into(),
        right: "staging".into(),
    })
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["merged"], serde_json::json!([]));
    assert_eq!(v["failed"][0]["path"], "");
    assert!(
        v["failed"][0]["error"]
            .as_str()
            .unwrap()
            .contains("--force")
    );
}

// ── conflict output tests ──

#[test]
fn test_format_diff_text_with_conflicts() {
    let output = DiffOutput {
        left: SourceInfo {
            label: "l".into(),
            root: ".".into(),
        },
        right: SourceInfo {
            label: "r".into(),
            root: "/r".into(),
        },
        conflict_count: 2,
        ..plain_diff("a.rs")
    };
    let text = format_diff_text(&output);
    assert!(text.contains("Conflicts: 2 region(s)"));
}

#[test]
fn test_format_multi_diff_text_with_conflicts() {
    let mut diff = sample_diff("a.rs");
    diff.conflict_count = 3;
    let output = MultiDiffOutput {
        files: vec![diff],
        errors: vec![],
        summary: MultiDiffSummary {
            scanned_files: 1,
            files_with_changes: 1,
        },
        truncated: false,
        changed_files_total: None,
    };
    let text = format_multi_diff_text(&output);
    assert!(text.contains("3 conflict(s) detected"));
}

#[test]
fn test_format_multi_diff_text_no_conflicts() {
    let output = MultiDiffOutput {
        files: vec![sample_diff("a.rs")],
        errors: vec![],
        summary: MultiDiffSummary {
            scanned_files: 1,
            files_with_changes: 1,
        },
        truncated: false,
        changed_files_total: None,
    };
    let text = format_multi_diff_text(&output);
    assert!(!text.contains("conflict(s) detected"));
}

// ── backup list / rollback text tests ──

#[test]
fn test_format_backup_list_text_with_sessions() {
    let output = BackupListOutput {
        target: SourceInfo {
            label: "develop".into(),
            root: "/var/www".into(),
        },
        sessions: vec![
            BackupSession::new(
                "20240115-140000".into(),
                vec![
                    BackupEntry {
                        path: "src/config.ts".into(),
                        size: Some(1234),
                        link_target: None,
                    },
                    BackupEntry {
                        path: "src/index.ts".into(),
                        size: Some(5678),
                        link_target: None,
                    },
                ],
                false,
            ),
            BackupSession::new(
                "20240114-100000".into(),
                vec![BackupEntry {
                    path: "src/old.ts".into(),
                    size: Some(456),
                    link_target: None,
                }],
                true,
            ),
        ],
    };
    let text = format_backup_list_text(&output);
    assert!(text.contains("Backup sessions for develop:"));
    assert!(text.contains("20240115-140000 (2 files)"));
    assert!(text.contains("src/config.ts (1234 bytes)"));
    assert!(text.contains("20240114-100000 (1 file) [expired]"));
}

#[test]
fn test_format_backup_list_text_empty() {
    let output = BackupListOutput {
        target: SourceInfo {
            label: "local".into(),
            root: ".".into(),
        },
        sessions: vec![],
    };
    let text = format_backup_list_text(&output);
    assert!(text.contains("no backup sessions found"));
}

#[test]
fn test_format_rollback_text_success() {
    let output = RollbackOutput {
        target: SourceInfo {
            label: "develop".into(),
            root: "/var/www".into(),
        },
        session_id: "20240115-140000".into(),
        restored: vec![
            RollbackFileResult {
                path: "src/config.ts".into(),
                pre_rollback_backup: Some("20240116-090000".into()),
            },
            RollbackFileResult {
                path: "src/index.ts".into(),
                pre_rollback_backup: Some("20240116-090000".into()),
            },
        ],
        skipped: vec![RollbackSkipped {
            path: "src/app.ts".into(),
            reason: "symlink restore not supported".into(),
        }],
        failed: vec![],
    };
    let text = format_rollback_text(&output);
    assert!(text.contains("Rollback session 20240115-140000 for develop:"));
    assert!(text.contains("\u{2713} src/config.ts (pre-rollback backup: 20240116-090000)"));
    assert!(text.contains("- src/app.ts (skipped: symlink restore not supported)"));
    assert!(text.contains("Restored 2 file(s), skipped 1."));
}

#[test]
fn test_format_rollback_text_with_failures() {
    let output = RollbackOutput {
        target: SourceInfo {
            label: "develop".into(),
            root: "/var/www".into(),
        },
        session_id: "20240115-140000".into(),
        restored: vec![RollbackFileResult {
            path: "a.rs".into(),
            pre_rollback_backup: None,
        }],
        skipped: vec![],
        failed: vec![RollbackFailure {
            path: "b.rs".into(),
            error: "permission denied".into(),
        }],
    };
    let text = format_rollback_text(&output);
    assert!(text.contains("\u{2717} b.rs (error: permission denied)"));
    assert!(text.contains("Restored 1 file(s), failed 1."));
}

// ── format_merge_text: dry-run のバックアップ表示 ──

#[test]
fn test_format_merge_text_dryrun_no_backup() {
    // dry-run + backup なし → バックアップ情報を表示しない
    let output = MergeOutput {
        merged: vec![MergeFileResult {
            path: "src/lib.rs".into(),
            status: "would merge".into(),
            ..Default::default()
        }],
        ..empty_merge()
    };
    let text = format_merge_text(&output);
    assert!(text.contains("Would merge: src/lib.rs"));
    assert!(!text.contains("backup"));
}

// ── format_merge_text: --delete 表示 ──

#[test]
fn test_format_merge_text_with_deleted() {
    let output = MergeOutput {
        merged: vec![MergeFileResult {
            path: "updated.rs".into(),
            status: "ok".into(),
            ..Default::default()
        }],
        deleted: vec![DeleteFileResult {
            path: "old-file.txt".into(),
            status: DeleteStatus::Ok,
            backup: Some("session123/old-file.txt".into()),
        }],
        ..empty_merge()
    };
    let text = format_merge_text(&output);
    assert!(
        text.contains("Deleted: old-file.txt (backup: session123/old-file.txt)"),
        "text: {}",
        text
    );
    assert!(text.contains("Merged: updated.rs"));
}

#[test]
fn test_format_merge_text_deleted_no_backup() {
    let output = MergeOutput {
        deleted: vec![DeleteFileResult {
            path: "remove-me.txt".into(),
            status: DeleteStatus::Ok,
            backup: None,
        }],
        ..empty_merge()
    };
    let text = format_merge_text(&output);
    assert!(text.contains("Deleted: remove-me.txt"), "text: {}", text);
    assert!(!text.contains("backup"));
}

#[test]
fn test_format_merge_text_plain_merge_has_no_hunk_or_delete_lines() {
    // 全体の merge で deleted が空のときは、hunk の情報も削除の行も出さない
    let output = MergeOutput {
        merged: vec![MergeFileResult {
            path: "a.rs".into(),
            status: "ok".into(),
            ..Default::default()
        }],
        ..empty_merge()
    };
    let text = format_merge_text(&output);
    assert!(
        !text.contains("hunks:"),
        "hunk info should not appear in normal merge: {}",
        text
    );
    assert!(!text.contains("Deleted"));
    assert!(!text.contains("Delete"));
}

// ── format_sync_text ──

#[test]
fn format_sync_text_basic() {
    let output = sync_from_local(
        vec![SyncTargetResult {
            merged: vec![MergeFileResult {
                path: "a.txt".into(),
                status: "ok".into(),
                ..Default::default()
            }],
            ..empty_target("server1")
        }],
        SyncSummary {
            total_servers: 1,
            successful_servers: 1,
            total_files_merged: 1,
            total_files_deleted: 0,
            total_files_failed: 0,
        },
    );
    let text = format_sync_text(&output);
    assert!(text.contains("Sync: local \u{2192} server1"));
    assert!(text.contains("[server1] success"));
    assert!(text.contains("ok"));
    assert!(text.contains("a.txt"));
    assert!(text.contains("1/1 servers successful"));
}

#[test]
fn format_sync_text_with_delete() {
    let output = sync_from_local(
        vec![SyncTargetResult {
            deleted: vec![DeleteFileResult {
                path: "old.txt".into(),
                status: DeleteStatus::Ok,
                backup: Some("session123/old.txt".into()),
            }],
            ..empty_target("server1")
        }],
        SyncSummary {
            total_servers: 1,
            successful_servers: 1,
            total_files_merged: 0,
            total_files_deleted: 1,
            total_files_failed: 0,
        },
    );
    let text = format_sync_text(&output);
    assert!(text.contains("deleted"));
    assert!(text.contains("backup: session123/old.txt"));
    assert!(text.contains("1 files deleted"));
}

#[test]
fn format_sync_text_multiple_servers() {
    let output = sync_from_local(
        vec![
            SyncTargetResult {
                merged: vec![MergeFileResult {
                    path: "a.txt".into(),
                    status: "ok".into(),
                    ..Default::default()
                }],
                ..empty_target("server1")
            },
            SyncTargetResult {
                failed: vec![MergeFailure {
                    path: "b.txt".into(),
                    error: "connection lost".into(),
                }],
                status: SyncTargetStatus::Failed,
                ..empty_target("server2")
            },
        ],
        SyncSummary {
            total_servers: 2,
            successful_servers: 1,
            total_files_merged: 1,
            total_files_deleted: 0,
            total_files_failed: 1,
        },
    );
    let text = format_sync_text(&output);
    assert!(text.contains("Sync: local \u{2192} server1, server2"));
    assert!(text.contains("[server1] success"));
    assert!(text.contains("[server2] failed"));
    assert!(text.contains("1/2 servers successful"));
    assert!(text.contains("1 files failed"));
}

#[test]
fn format_sync_text_with_skipped() {
    let output = sync_from_local(
        vec![SyncTargetResult {
            skipped: vec![MergeSkipped {
                path: "kind.txt".into(),
                reason: "source and destination have different file types".into(),
            }],
            ..empty_target("server1")
        }],
        SyncSummary {
            total_servers: 1,
            successful_servers: 1,
            total_files_merged: 0,
            total_files_deleted: 0,
            total_files_failed: 0,
        },
    );
    let text = format_sync_text(&output);
    assert!(text.contains("skip"));
    assert!(text.contains("kind.txt"));
    assert!(text.contains("different file types"));
}

#[test]
fn format_sync_text_dry_run() {
    let output = sync_from_local(
        vec![SyncTargetResult {
            merged: vec![
                MergeFileResult {
                    path: "src/config.ts".into(),
                    status: "would merge".into(),
                    ..Default::default()
                },
                MergeFileResult {
                    path: "src/index.ts".into(),
                    status: "would merge".into(),
                    ..Default::default()
                },
            ],
            ..empty_target("server1")
        }],
        SyncSummary {
            total_servers: 1,
            successful_servers: 1,
            total_files_merged: 2,
            total_files_deleted: 0,
            total_files_failed: 0,
        },
    );
    let text = format_sync_text(&output);
    // dry-run ではバッジが "plan" になる
    assert!(text.contains("plan"));
    assert!(text.contains("src/config.ts"));
    assert!(text.contains("src/index.ts"));
}

#[test]
fn format_sync_text_with_backup() {
    let output = sync_from_local(
        vec![SyncTargetResult {
            merged: vec![MergeFileResult {
                path: "a.txt".into(),
                status: "ok".into(),
                backup: Some("20260311-120000/a.txt".into()),
                ..Default::default()
            }],
            ..empty_target("server1")
        }],
        SyncSummary {
            total_servers: 1,
            successful_servers: 1,
            total_files_merged: 1,
            total_files_deleted: 0,
            total_files_failed: 0,
        },
    );
    let text = format_sync_text(&output);
    assert!(text.contains("backup: 20260311-120000/a.txt"));
}

// ── JsonError ──

#[test]
fn test_json_error_serialize() {
    let err = JsonError {
        error: "something went wrong".to_string(),
    };
    let json = serde_json::to_string(&err).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["error"], "something went wrong");
    // Verify no extra fields
    assert_eq!(parsed.as_object().unwrap().len(), 1);
}

// ── status の L/R の印 ──

#[test]
fn test_status_text_right_only_symbol() {
    let output = StatusOutput {
        left: SourceInfo {
            label: "local".into(),
            root: ".".into(),
        },
        right: SourceInfo {
            label: "dev".into(),
            root: "/r".into(),
        },
        ref_: None,
        agent: None,
        files: Some(vec![FileStatus {
            path: "only_right.rs".into(),
            status: FileStatusKind::RightOnly,
            ref_badge: None,
        }]),
        summary: StatusSummary {
            modified: 0,
            left_only: 0,
            right_only: 1,
            equal: 0,
            ref_differs: None,
            ref_only: None,
            ref_missing: None,
            unreadable: 0,
        },
        errors: Vec::new(),
    };
    let text = format_status_text(&output, false);
    assert!(
        text.contains("R only_right.rs"),
        "right_only should use 'R ' prefix"
    );
    assert!(
        !text.contains("- only_right.rs"),
        "should not use old '-' prefix"
    );
}
