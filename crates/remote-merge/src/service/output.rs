//! 各コマンドの出力型を、人間向けのテキストか JSON の文字列にする。

use serde::Serialize;

use super::types::*;

/// JSON エラーレスポンス（`--format json` 指定時のエラー出力用）
#[derive(Debug, Serialize)]
pub struct JsonError {
    pub error: String,
}

/// 出力フォーマット
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

impl OutputFormat {
    /// 文字列からパースする（"diff" を text と同じに扱う。status・merge・diff の値）
    pub fn parse(s: &str) -> anyhow::Result<Self> {
        match s {
            "text" | "diff" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            other => anyhow::bail!("Unknown format: '{}' (expected text, json, or diff)", other),
        }
    }

    /// 文字列からパースする（"diff" を受け付けない。rollback・logs・sync の値。help も text と
    /// json だけを示す）
    pub fn parse_text_or_json(s: &str) -> anyhow::Result<Self> {
        match s {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            other => anyhow::bail!("Unknown format: '{}' (expected text or json)", other),
        }
    }
}

/// status テキスト出力のヘッダ行を生成する
fn format_status_header(left: &str, right: &str, ref_: Option<&SourceInfo>) -> String {
    match ref_ {
        Some(ref_info) => format!(
            "Comparing: {} \u{2194} {} (ref: {})",
            left, right, ref_info.label
        ),
        None => format!("Comparing: {} \u{2194} {}", left, right),
    }
}

/// ref バッジ文字列を表示用テキストに変換する
fn ref_badge_display(badge: &str) -> &'static str {
    match badge {
        "differs" => " [ref≠]",
        "exists_only_in_ref" => " [ref+]",
        "missing_in_ref" => " [ref-]",
        _ => "", // all_equal — don't display
    }
}

/// StatusOutput をテキストフォーマットする（git status 風）
pub fn format_status_text(output: &StatusOutput, summary_only: bool) -> String {
    let mut lines = Vec::new();

    // ヘッダ行: 比較対象を表示
    let header = format_status_header(
        &output.left.label,
        &output.right.label,
        output.ref_.as_ref(),
    );
    lines.push(header);
    lines.push(String::new());

    if !summary_only && let Some(files) = &output.files {
        for file in files {
            let prefix = match file.status {
                FileStatusKind::Modified => "M ",
                FileStatusKind::LeftOnly => "L ",
                FileStatusKind::RightOnly => "R ",
                FileStatusKind::Equal => "= ",
            };
            let ref_badge_str = file
                .ref_badge
                .as_ref()
                .map(|b| ref_badge_display(b))
                .unwrap_or("");
            lines.push(format!("{}{}{}", prefix, file.path, ref_badge_str));
        }
        lines.push(String::new());
    }

    lines.push(format!(
        "Summary: {} modified, {} left only, {} right only, {} equal",
        output.summary.modified,
        output.summary.left_only,
        output.summary.right_only,
        output.summary.equal,
    ));

    if let (Some(rd), Some(ro), Some(rm)) = (
        output.summary.ref_differs,
        output.summary.ref_only,
        output.summary.ref_missing,
    ) {
        lines.push(format!(
            "Ref: {} differs, {} ref-only, {} ref-missing",
            rd, ro, rm
        ));
    }

    // Agent 接続状態
    if let Some(agent) = output.agent {
        let label = match agent {
            AgentStatus::Connected => "Agent: connected",
        };
        lines.push(label.to_string());
    }

    lines.join("\n")
}

/// --max-lines で差分を打ち切ったファイルの末尾に出す印
const TRUNCATED_MARKER: &str = "... (output truncated)";

/// DiffOutput をテキストフォーマットする（unified diff 風）
pub fn format_diff_text(output: &DiffOutput) -> String {
    let mut lines = Vec::new();

    lines.push(format!("--- a/{} ({})", output.path, output.left.label));
    lines.push(format!("+++ b/{} ({})", output.path, output.right.label));

    if output.symlink {
        if let Some(targets) = &output.link_targets {
            if targets.left != targets.right {
                lines.push("Symbolic link targets differ".into());
                if let Some(left) = &targets.left {
                    lines.push(format!("-link target: {left}"));
                }
                if let Some(right) = &targets.right {
                    lines.push(format!("+link target: {right}"));
                }
            } else if let Some(target) = &targets.left {
                lines.push(format!("Link target: {target}"));
            }
        }
        if let Some(note) = &output.note {
            lines.push(note.clone());
        }
        if output.binary {
            lines.push(format_binary_hashes(output));
            return lines.join("\n");
        }
        if !output.hunks.is_empty() {
            lines.push("Resolved content differs".into());
        }
        for hunk in &output.hunks {
            push_hunk_lines(&mut lines, hunk);
        }
        if output.truncated {
            lines.push(TRUNCATED_MARKER.into());
        }
        return lines.join("\n");
    }

    if output.binary {
        lines.push(format_binary_hashes(output));
        // バイナリのファイル全体の競合も、そのファイルに示す（REQ-cli-065）
        push_conflicts(&mut lines, output);
        return lines.join("\n");
    }

    if let Some(note) = &output.note {
        lines.push(note.to_string());
    }

    for hunk in &output.hunks {
        push_hunk(&mut lines, hunk);
    }

    if output.truncated {
        lines.push(TRUNCATED_MARKER.into());
    }

    if let (Some(ref_info), Some(ref_hunks)) = (&output.ref_, &output.ref_hunks)
        && !ref_hunks.is_empty()
    {
        lines.push(String::new());
        lines.push(format!(
            "--- ref:{}:{} (reference diff vs left)",
            ref_info.label, output.path
        ));
        for hunk in ref_hunks {
            push_hunk(&mut lines, hunk);
        }
    }

    push_conflicts(&mut lines, output);
    lines.join("\n")
}

/// hunk を、左右の行の数を添えた見出しとその行で足す
fn push_hunk(lines: &mut Vec<String>, hunk: &DiffHunk) {
    lines.push(format!(
        "@@ -{},{} +{},{} @@",
        hunk.left_start,
        hunk.lines
            .iter()
            .filter(|l| l.line_type != DiffLineType::Added)
            .count(),
        hunk.right_start,
        hunk.lines
            .iter()
            .filter(|l| l.line_type != DiffLineType::Removed)
            .count(),
    ));
    push_hunk_lines(lines, hunk);
}

/// hunk の行を、文脈・追加・削除の印を頭に付けて足す
fn push_hunk_lines(lines: &mut Vec<String>, hunk: &DiffHunk) {
    for line in &hunk.lines {
        let prefix = match line.line_type {
            DiffLineType::Context => " ",
            DiffLineType::Added => "+",
            DiffLineType::Removed => "-",
        };
        lines.push(format!("{}{}", prefix, line.content));
    }
}

/// 競合があれば、その数の行を足す
fn push_conflicts(lines: &mut Vec<String>, output: &DiffOutput) {
    if output.conflict_count > 0 {
        lines.push(String::new());
        lines.push(format!(
            "Conflicts: {} region(s) where both sides changed the same lines differently",
            output.conflict_count
        ));
    }
}

fn format_binary_hashes(output: &DiffOutput) -> String {
    match (&output.left_hash, &output.right_hash) {
        (Some(lh), Some(rh)) => {
            format!("Binary files differ (left: sha256={lh}, right: sha256={rh})")
        }
        (Some(lh), None) => format!("Binary files differ (left: sha256={lh}, right: missing)"),
        (None, Some(rh)) => format!("Binary files differ (left: missing, right: sha256={rh})"),
        (None, None) => "Binary files differ".into(),
    }
}

/// 複数ファイル diff のテキスト出力をフォーマットする
pub fn format_multi_diff_text(output: &MultiDiffOutput) -> String {
    let mut result = String::new();
    for (i, diff) in output.files.iter().enumerate() {
        if i > 0 {
            result.push('\n');
        }
        result.push_str(&format_diff_text(diff));
    }
    if output.truncated
        && let Some(total) = output.changed_files_total
    {
        result.push_str(&format!(
            "\n... and {} more files (truncated, use --max-files 0 for all)\n",
            total - output.summary.files_with_changes
        ));
    }
    // Summary line
    result.push_str(&format!(
        "\n{} file(s) with changes out of {} total\n",
        output.summary.files_with_changes, output.summary.scanned_files
    ));

    let total_conflicts: usize = output.files.iter().map(|f| f.conflict_count).sum();
    if total_conflicts > 0 {
        result.push_str(&format!(
            "{} conflict(s) detected across files\n",
            total_conflicts
        ));
    }

    result
}

/// --hunks で差分のないファイルの merged の status（JSON の形は変えない）
pub const NO_CHANGES: &str = "skipped (no changes)";

/// MergeOutput をテキストフォーマットする
pub fn format_merge_text(output: &MergeOutput) -> String {
    let mut lines = Vec::new();

    for result in &output.merged {
        // --hunks で差分のないファイルは、書き込んだ行でなくスキップの行で出す（REQ-merge-030）
        if result.status == NO_CHANGES {
            lines.push(format!("Skipped: {} (no changes)", result.path));
            continue;
        }
        let backup_info = result
            .backup
            .as_ref()
            .map(|b| format!(" (backup: {})", b))
            .unwrap_or_default();
        let ref_badge_str = result
            .ref_badge
            .as_ref()
            .map(|b| ref_badge_display(b))
            .unwrap_or("");
        let prefix = if result.status == "would merge" {
            "Would merge"
        } else {
            "Merged"
        };
        let hunk_info = if let Some(info) = &result.hunk_info {
            format!(
                " (hunks: {}/{})",
                info.hunks_applied
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
                info.hunks_total
            )
        } else {
            String::new()
        };
        lines.push(format!(
            "{}: {}{}{}{}",
            prefix, result.path, hunk_info, backup_info, ref_badge_str
        ));
        if let Some(note) = result.copy_note() {
            lines.push(format!("  {note}"));
        }
    }

    // 削除したファイルと、--dry-run で消す予定のファイル（REQ-merge-027）
    for del in &output.deleted {
        let line = match del.status {
            DeleteStatus::Ok => {
                let mut line = format!("Deleted: {}", del.path);
                if let Some(ref backup) = del.backup {
                    line.push_str(&format!(" (backup: {})", backup));
                }
                line
            }
            DeleteStatus::WouldDelete => format!("Would delete: {}", del.path),
        };
        lines.push(line);
    }

    for skip in &output.skipped {
        lines.push(format!("Skipped: {} ({})", skip.path, skip.reason));
    }

    for fail in &output.failed {
        lines.push(format!("Failed: {} ({})", fail.path, fail.error));
    }

    lines.join("\n")
}

/// MergeOutcome をテキストフォーマットする
pub fn format_merge_outcome_text(outcome: &MergeOutcome) -> String {
    match outcome {
        MergeOutcome::NoFilesToMerge => "no files to merge in the specified path(s)".to_string(),
        MergeOutcome::R2rBlocked { left, right } => {
            format!(
                "Warning: merging between two remote servers ({} → {})\nUse --force to proceed, or --dry-run to preview changes.",
                left, right
            )
        }
    }
}

/// MergeOutcome を JSON フォーマットする
pub fn format_merge_outcome_json(outcome: &MergeOutcome) -> anyhow::Result<String> {
    match outcome {
        MergeOutcome::NoFilesToMerge => {
            let output = MergeOutput {
                merged: vec![],
                skipped: vec![],
                deleted: vec![],
                failed: vec![],
                ref_: None,
            };
            format_json(&output)
        }
        MergeOutcome::R2rBlocked { left, right } => {
            let output = MergeOutput {
                merged: vec![],
                skipped: vec![],
                deleted: vec![],
                failed: vec![MergeFailure {
                    path: String::new(),
                    error: format!(
                        "Remote-to-remote merge blocked ({} → {}). Use --force to proceed.",
                        left, right
                    ),
                }],
                ref_: None,
            };
            format_json(&output)
        }
    }
}

/// BackupListOutput をテキストフォーマットする
pub fn format_backup_list_text(output: &BackupListOutput) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Backup sessions for {}:", output.target.label));
    lines.push(String::new());

    if output.sessions.is_empty() {
        lines.push("  (no backup sessions found)".into());
        return lines.join("\n");
    }

    for session in &output.sessions {
        let file_count = session.file_count;
        let expired_tag = if session.expired { " [expired]" } else { "" };
        if file_count == 0 {
            lines.push(format!("  {} (empty){expired_tag}", session.session_id,));
        } else {
            let file_word = if file_count == 1 { "file" } else { "files" };
            lines.push(format!(
                "  {} ({} {}){expired_tag}",
                session.session_id, file_count, file_word,
            ));
        }
        for entry in &session.files {
            if let Some(link_target) = &entry.link_target {
                lines.push(format!("    {} -> {} (symlink)", entry.path, link_target));
            } else if let Some(size) = entry.size {
                lines.push(format!("    {} ({} bytes)", entry.path, size));
            }
        }
        lines.push(String::new());
    }

    lines.join("\n")
}

/// RollbackOutput をテキストフォーマットする
pub fn format_rollback_text(output: &RollbackOutput) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "Rollback session {} for {}:",
        output.session_id, output.target.label
    ));
    lines.push(String::new());

    for result in &output.restored {
        let backup_info = result
            .pre_rollback_backup
            .as_ref()
            .map(|b| format!(" (pre-rollback backup: {})", b))
            .unwrap_or_default();
        lines.push(format!("  \u{2713} {}{}", result.path, backup_info));
    }

    for s in &output.skipped {
        lines.push(format!("  - {} (skipped: {})", s.path, s.reason));
    }

    lines.extend(output.failed.iter().map(format_rollback_failed_line));

    lines.push(String::new());
    let restored = output.restored.len();
    let skipped = output.skipped.len();
    let failed = output.failed.len();

    let mut summary_parts = Vec::new();
    summary_parts.push(format!("Restored {} file(s)", restored));
    if skipped > 0 {
        summary_parts.push(format!("skipped {}", skipped));
    }
    if failed > 0 {
        summary_parts.push(format!("failed {}", failed));
    }
    lines.push(format!("{}.", summary_parts.join(", ")));

    lines.join("\n")
}

/// rollback --dry-run の結果をテキストフォーマットする（警告は呼び出し側が標準エラーに出す）
pub fn format_rollback_dry_run_text(output: &RollbackOutput) -> String {
    let mut lines = vec![format!(
        "Dry run - would restore from session {}:",
        output.session_id
    )];
    for file in &output.restored {
        lines.push(format!("  \u{2713} {}", file.path));
    }
    for skipped in &output.skipped {
        lines.push(format!(
            "  - {} (skipped: {})",
            skipped.path, skipped.reason
        ));
    }
    lines.extend(output.failed.iter().map(format_rollback_failed_line));
    lines.join("\n")
}

/// 戻せなかったファイル一件の行（実行時と dry-run で同じ形）
fn format_rollback_failed_line(failure: &RollbackFailure) -> String {
    format!("  \u{2717} {} (error: {})", failure.path, failure.error)
}

/// SyncOutput をテキストフォーマットする
pub fn format_sync_text(output: &SyncOutput) -> String {
    let mut lines = Vec::new();

    // ヘッダ: "Sync: {left} → {target1}, {target2}, ..."
    let target_names: Vec<&str> = output
        .targets
        .iter()
        .map(|t| t.target.label.as_str())
        .collect();
    lines.push(format!(
        "Sync: {} \u{2192} {}",
        output.left.label,
        target_names.join(", ")
    ));
    lines.push(String::new());

    // 各ターゲットの結果
    for target_result in &output.targets {
        let status_str = match target_result.status {
            SyncTargetStatus::Success => "success",
            SyncTargetStatus::Partial => "partial",
            SyncTargetStatus::Failed => "failed",
        };
        lines.push(format!("[{}] {}", target_result.target.label, status_str));

        // マージ済みファイル
        for file in &target_result.merged {
            let badge = if file.status == "would merge" {
                "plan"
            } else {
                "ok"
            };
            let mut line = format!("  {:<6} {}", badge, file.path);
            if let Some(note) = file.copy_note() {
                line.push_str(&format!(" ({note})"));
            }
            if let Some(ref backup) = file.backup {
                line.push_str(&format!(" (backup: {})", backup));
            }
            lines.push(line);
        }

        // 削除したファイルと、--dry-run で消す予定のファイル（書き込む予定と同じ "plan" で始める。F12）
        for file in &target_result.deleted {
            let line = match file.status {
                DeleteStatus::WouldDelete => format!("  {:<6} {} (delete)", "plan", file.path),
                DeleteStatus::Ok => {
                    let mut line = format!("  {:<6} {} (deleted", "D", file.path);
                    if let Some(ref backup) = file.backup {
                        line.push_str(&format!(", backup: {}", backup));
                    }
                    line.push(')');
                    line
                }
            };
            lines.push(line);
        }

        // スキップファイル
        for skip in &target_result.skipped {
            lines.push(format!("  skip   {} ({})", skip.path, skip.reason));
        }

        // 失敗ファイル
        for fail in &target_result.failed {
            lines.push(format!("  FAILED {} ({})", fail.path, fail.error));
        }

        lines.push(String::new());
    }

    // サマリー行
    let s = &output.summary;
    let mut summary_parts = vec![
        format!(
            "{}/{} servers successful",
            s.successful_servers, s.total_servers
        ),
        format!("{} files merged", s.total_files_merged),
    ];
    if s.total_files_deleted > 0 {
        summary_parts.push(format!("{} files deleted", s.total_files_deleted));
    }
    if s.total_files_failed > 0 {
        summary_parts.push(format!("{} files failed", s.total_files_failed));
    }
    lines.push(format!("Summary: {}", summary_parts.join(", ")));

    lines.join("\n")
}

/// 任意の Serialize 可能な型を JSON 文字列にする
pub fn format_json<T: serde::Serialize>(value: &T) -> anyhow::Result<String> {
    Ok(serde_json::to_string(value)?)
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
