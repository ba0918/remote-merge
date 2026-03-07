//! Markdown レポート生成（純粋関数）。
//!
//! キャッシュ済みのファイル差分をまとめて Markdown レポートに変換する。
//! AppState に直接依存せず、`ReportInput` 構造体を介して入力を受け取る。

use crate::diff::engine::{DiffLine, DiffResult, DiffTag};
use crate::filter;
use crate::service::status::is_sensitive;

/// レポート生成に必要な入力
pub struct ReportInput<'a> {
    pub left_label: &'a str,
    pub right_label: &'a str,
    pub left_root: &'a str,
    pub right_root: &'a str,
    pub sensitive_patterns: &'a [String],
    pub exclude_patterns: &'a [String],
    pub files: Vec<ReportFileEntry<'a>>,
}

/// レポート内の1ファイル分の情報
pub struct ReportFileEntry<'a> {
    pub path: &'a str,
    pub left_content: Option<&'a str>,
    pub right_content: Option<&'a str>,
    pub diff: Option<&'a DiffResult>,
}

/// 大ファイル閾値（5MB）
const LARGE_FILE_THRESHOLD: usize = 5 * 1024 * 1024;

/// キャッシュ済みファイルから Markdown レポートを生成する（純粋関数）。
///
/// - `exclude_patterns` にマッチするパスはレポートから除外
/// - 差分がないファイル（`DiffResult::Equal`）は詳細を省略し、サマリーにカウントのみ
pub fn generate_report(input: &ReportInput) -> String {
    let mut out = String::new();

    // exclude フィルタ適用
    let included: Vec<&ReportFileEntry> = input
        .files
        .iter()
        .filter(|e| !filter::is_path_excluded(e.path, input.exclude_patterns))
        .collect();

    // Equal / non-Equal を分離してカウント
    let (equal_files, diff_files): (Vec<&&ReportFileEntry>, Vec<&&ReportFileEntry>) = included
        .iter()
        .partition(|e| matches!(e.diff, Some(DiffResult::Equal)) || e.diff.is_none());

    out.push_str("# remote-merge Report\n\n");
    out.push_str(&format!(
        "- **Left:** {} ({})\n",
        input.left_label, input.left_root
    ));
    out.push_str(&format!(
        "- **Right:** {} ({})\n",
        input.right_label, input.right_root
    ));
    out.push_str(&format!(
        "- **Files:** {} total ({} changed, {} unchanged)\n\n",
        included.len(),
        diff_files.len(),
        equal_files.len(),
    ));
    out.push_str("---\n\n");

    if included.is_empty() {
        out.push_str("No cached files to report.\n");
        return out;
    }

    // 差分があるファイルのみ詳細セクションを出力
    for entry in &included {
        let is_equal = matches!(entry.diff, Some(DiffResult::Equal));
        let has_no_diff = entry.diff.is_none();

        // Equal / no-diff はスキップ（サマリーに含まれている）
        if is_equal || has_no_diff {
            continue;
        }

        let sensitive = is_sensitive(entry.path, input.sensitive_patterns);

        out.push_str(&format!("## {}\n\n", entry.path));

        if sensitive {
            out.push_str("⚠ **Sensitive file** — content excluded from report.\n\n");
            continue;
        }

        // サイズチェック
        let too_large = entry
            .left_content
            .is_some_and(|c| c.len() > LARGE_FILE_THRESHOLD)
            || entry
                .right_content
                .is_some_and(|c| c.len() > LARGE_FILE_THRESHOLD);

        if too_large {
            out.push_str("(too large, skipped)\n\n");
            continue;
        }

        match entry.diff {
            Some(DiffResult::Modified { lines, .. }) => {
                out.push_str("```diff\n");
                for line in lines {
                    out.push_str(&format_diff_line(line));
                }
                out.push_str("```\n\n");
            }
            Some(DiffResult::Binary { left, right }) => {
                out.push_str("Binary file.\n");
                if let Some(l) = left {
                    out.push_str(&format!(
                        "- Left: {} bytes, SHA-256: {}\n",
                        l.size, l.sha256
                    ));
                }
                if let Some(r) = right {
                    out.push_str(&format!(
                        "- Right: {} bytes, SHA-256: {}\n",
                        r.size, r.sha256
                    ));
                }
                out.push('\n');
            }
            Some(DiffResult::SymlinkDiff {
                left_target,
                right_target,
            }) => {
                out.push_str("Symlink:\n");
                out.push_str(&format!(
                    "- Left target: {}\n",
                    left_target.as_deref().unwrap_or("(missing)")
                ));
                out.push_str(&format!(
                    "- Right target: {}\n\n",
                    right_target.as_deref().unwrap_or("(missing)")
                ));
            }
            // Equal / None はすでにスキップ済み
            _ => {}
        }
    }

    // 変更なしファイルの一覧（コンパクト表示）
    if !equal_files.is_empty() {
        out.push_str("## Unchanged files\n\n");
        for entry in &equal_files {
            out.push_str(&format!("- {}\n", entry.path));
        }
        out.push('\n');
    }

    out
}

/// DiffLine を unified diff 形式の1行文字列に変換する。
fn format_diff_line(line: &DiffLine) -> String {
    let prefix = match line.tag {
        DiffTag::Equal => ' ',
        DiffTag::Delete => '-',
        DiffTag::Insert => '+',
    };
    format!("{}{}\n", prefix, line.value)
}

/// タイムスタンプ付きレポートファイル名を生成する。
pub fn report_filename() -> String {
    let now = chrono::Local::now();
    format!("remote-merge-report-{}.md", now.format("%Y%m%d-%H%M%S"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::engine::{DiffLine, DiffResult, DiffTag};

    fn make_modified_diff(lines: Vec<DiffLine>) -> DiffResult {
        DiffResult::Modified {
            hunks: vec![],
            merge_hunks: vec![],
            lines,
            stats: crate::diff::engine::DiffStats {
                insertions: 0,
                deletions: 0,
                equal: 0,
            },
            merge_hunk_line_indices: vec![],
        }
    }

    #[test]
    fn test_generate_report_empty() {
        let input = ReportInput {
            left_label: "local",
            right_label: "develop",
            left_root: "/home/user/project",
            right_root: "/var/www/project",
            sensitive_patterns: &[],
            exclude_patterns: &[],
            files: vec![],
        };
        let report = generate_report(&input);
        assert!(report.contains("No cached files to report"));
    }

    #[test]
    fn test_equal_file_not_in_detail_but_in_unchanged() {
        let diff = DiffResult::Equal;
        let input = ReportInput {
            left_label: "local",
            right_label: "develop",
            left_root: "/home",
            right_root: "/var",
            sensitive_patterns: &[],
            exclude_patterns: &[],
            files: vec![ReportFileEntry {
                path: "src/main.rs",
                left_content: Some("fn main() {}"),
                right_content: Some("fn main() {}"),
                diff: Some(&diff),
            }],
        };
        let report = generate_report(&input);
        // 詳細セクション（## src/main.rs）は出ない
        assert!(!report.contains("## src/main.rs"));
        // Unchanged files セクションにリスト表示される
        assert!(report.contains("Unchanged files"));
        assert!(report.contains("- src/main.rs"));
        // サマリーに 0 changed, 1 unchanged
        assert!(report.contains("0 changed"));
        assert!(report.contains("1 unchanged"));
    }

    #[test]
    fn test_generate_report_with_modified_file() {
        let diff = make_modified_diff(vec![
            DiffLine {
                tag: DiffTag::Delete,
                value: "old line".to_string(),
                old_index: Some(0),
                new_index: None,
            },
            DiffLine {
                tag: DiffTag::Insert,
                value: "new line".to_string(),
                old_index: None,
                new_index: Some(0),
            },
        ]);
        let input = ReportInput {
            left_label: "local",
            right_label: "develop",
            left_root: "/home",
            right_root: "/var",
            sensitive_patterns: &[],
            exclude_patterns: &[],
            files: vec![ReportFileEntry {
                path: "config.toml",
                left_content: Some("old line"),
                right_content: Some("new line"),
                diff: Some(&diff),
            }],
        };
        let report = generate_report(&input);
        assert!(report.contains("```diff"));
        assert!(report.contains("-old line"));
        assert!(report.contains("+new line"));
    }

    #[test]
    fn test_generate_report_sensitive_file_excluded() {
        // sensitive + modified なファイル → ⚠ 表示
        let diff = make_modified_diff(vec![DiffLine {
            tag: DiffTag::Delete,
            value: "SECRET=xxx".to_string(),
            old_index: Some(0),
            new_index: None,
        }]);
        let input = ReportInput {
            left_label: "local",
            right_label: "develop",
            left_root: "/home",
            right_root: "/var",
            sensitive_patterns: &[".env*".to_string()],
            exclude_patterns: &[],
            files: vec![ReportFileEntry {
                path: ".env.production",
                left_content: Some("SECRET=xxx"),
                right_content: Some("SECRET=yyy"),
                diff: Some(&diff),
            }],
        };
        let report = generate_report(&input);
        assert!(report.contains("Sensitive file"));
        assert!(!report.contains("SECRET"));
    }

    #[test]
    fn test_generate_report_large_file_skipped() {
        let content = "x".repeat(LARGE_FILE_THRESHOLD + 1);
        let diff = make_modified_diff(vec![DiffLine {
            tag: DiffTag::Insert,
            value: "big".to_string(),
            old_index: None,
            new_index: Some(0),
        }]);
        let input = ReportInput {
            left_label: "local",
            right_label: "develop",
            left_root: "/home",
            right_root: "/var",
            sensitive_patterns: &[],
            exclude_patterns: &[],
            files: vec![ReportFileEntry {
                path: "big.bin",
                left_content: Some(&content),
                right_content: None,
                diff: Some(&diff),
            }],
        };
        let report = generate_report(&input);
        assert!(report.contains("too large, skipped"));
    }

    #[test]
    fn test_exclude_patterns_filter_files() {
        let diff = make_modified_diff(vec![DiffLine {
            tag: DiffTag::Insert,
            value: "backup data".to_string(),
            old_index: None,
            new_index: Some(0),
        }]);
        let input = ReportInput {
            left_label: "local",
            right_label: "develop",
            left_root: "/home",
            right_root: "/var",
            sensitive_patterns: &[],
            exclude_patterns: &[
                ".remote-merge-backup".to_string(),
                "node_modules".to_string(),
            ],
            files: vec![
                ReportFileEntry {
                    path: ".remote-merge-backup/src/old.rs",
                    left_content: Some("old"),
                    right_content: None,
                    diff: Some(&diff),
                },
                ReportFileEntry {
                    path: "node_modules/pkg/index.js",
                    left_content: Some("module"),
                    right_content: Some("module"),
                    diff: Some(&DiffResult::Equal),
                },
            ],
        };
        let report = generate_report(&input);
        // 除外されたファイルは一切出ない
        assert!(!report.contains(".remote-merge-backup"));
        assert!(!report.contains("node_modules"));
        assert!(report.contains("0 changed"));
    }

    #[test]
    fn test_exclude_patterns_mid_path_segment() {
        let diff = DiffResult::Equal;
        let input = ReportInput {
            left_label: "local",
            right_label: "develop",
            left_root: "/home",
            right_root: "/var",
            sensitive_patterns: &[],
            exclude_patterns: &["*.log".to_string()],
            files: vec![ReportFileEntry {
                path: "logs/app.log",
                left_content: Some("log"),
                right_content: Some("log"),
                diff: Some(&diff),
            }],
        };
        let report = generate_report(&input);
        assert!(!report.contains("app.log"));
    }

    #[test]
    fn test_report_filename_format() {
        let name = report_filename();
        assert!(name.starts_with("remote-merge-report-"));
        assert!(name.ends_with(".md"));
    }

    #[test]
    fn test_format_diff_line_prefixes() {
        assert_eq!(
            format_diff_line(&DiffLine {
                tag: DiffTag::Equal,
                value: "same".to_string(),
                old_index: Some(0),
                new_index: Some(0),
            }),
            " same\n"
        );
        assert_eq!(
            format_diff_line(&DiffLine {
                tag: DiffTag::Delete,
                value: "removed".to_string(),
                old_index: Some(0),
                new_index: None,
            }),
            "-removed\n"
        );
        assert_eq!(
            format_diff_line(&DiffLine {
                tag: DiffTag::Insert,
                value: "added".to_string(),
                old_index: None,
                new_index: Some(0),
            }),
            "+added\n"
        );
    }
}
