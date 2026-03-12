//! diff 情報をクリップボード用 Markdown に変換する（純粋関数）。

use crate::diff::engine::{DiffResult, DiffTag};

/// クリップボード用メタデータ
pub struct ClipboardContext {
    pub file_path: String,
    pub left_label: String,
    pub right_label: String,
    pub left_root: String,
    pub right_root: String,
}

/// diff 情報を Markdown 形式に変換する（純粋関数）。
pub fn format_diff_for_clipboard(context: &ClipboardContext, diff: &DiffResult) -> String {
    let mut out = String::new();

    out.push_str(&format!("## File: {}\n\n", context.file_path));
    out.push_str(&format!(
        "- Left: {} ({})\n",
        context.left_label, context.left_root
    ));
    out.push_str(&format!(
        "- Right: {} ({})\n\n",
        context.right_label, context.right_root
    ));

    match diff {
        DiffResult::Equal => {
            out.push_str("No differences.\n");
        }
        DiffResult::Modified { lines, .. } => {
            out.push_str("```diff\n");
            for line in lines {
                let prefix = match line.tag {
                    DiffTag::Equal => ' ',
                    DiffTag::Delete => '-',
                    DiffTag::Insert => '+',
                };
                out.push_str(&format!("{}{}\n", prefix, line.value));
            }
            out.push_str("```\n");
        }
        DiffResult::Binary { left, right } => {
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
        }
        DiffResult::SymlinkDiff {
            left_target,
            right_target,
        } => {
            out.push_str("Symlink:\n");
            out.push_str(&format!(
                "- Left target: {}\n",
                left_target.as_deref().unwrap_or("(missing)")
            ));
            out.push_str(&format!(
                "- Right target: {}\n",
                right_target.as_deref().unwrap_or("(missing)")
            ));
        }
    }

    out.push_str("\n## Question\n\n");

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::engine::{DiffLine, DiffResult, DiffStats, DiffTag};

    fn ctx() -> ClipboardContext {
        ClipboardContext {
            file_path: "src/main.rs".to_string(),
            left_label: "local".to_string(),
            right_label: "develop".to_string(),
            left_root: "/home/user/project".to_string(),
            right_root: "/var/www/project".to_string(),
        }
    }

    #[test]
    fn test_format_equal() {
        let result = format_diff_for_clipboard(&ctx(), &DiffResult::Equal);
        assert!(result.contains("## File: src/main.rs"));
        assert!(result.contains("No differences"));
    }

    #[test]
    fn test_format_modified() {
        let diff = DiffResult::Modified {
            hunks: vec![],
            merge_hunks: vec![],
            lines: vec![
                DiffLine {
                    tag: DiffTag::Delete,
                    value: "old".to_string(),
                    old_index: Some(0),
                    new_index: None,
                },
                DiffLine {
                    tag: DiffTag::Insert,
                    value: "new".to_string(),
                    old_index: None,
                    new_index: Some(0),
                },
            ],
            stats: DiffStats {
                insertions: 1,
                deletions: 1,
                equal: 0,
            },
        };
        let result = format_diff_for_clipboard(&ctx(), &diff);
        assert!(result.contains("```diff"));
        assert!(result.contains("-old"));
        assert!(result.contains("+new"));
        assert!(result.contains("## Question"));
    }

    #[test]
    fn test_format_binary() {
        let diff = DiffResult::Binary {
            left: Some(crate::diff::binary::BinaryInfo {
                size: 1024,
                sha256: "abc123".to_string(),
            }),
            right: None,
        };
        let result = format_diff_for_clipboard(&ctx(), &diff);
        assert!(result.contains("Binary file"));
        assert!(result.contains("1024 bytes"));
        assert!(result.contains("abc123"));
    }

    #[test]
    fn test_format_symlink() {
        let diff = DiffResult::SymlinkDiff {
            left_target: Some("/usr/bin/foo".to_string()),
            right_target: None,
        };
        let result = format_diff_for_clipboard(&ctx(), &diff);
        assert!(result.contains("Symlink"));
        assert!(result.contains("/usr/bin/foo"));
        assert!(result.contains("(missing)"));
    }

    #[test]
    fn test_contains_metadata() {
        let result = format_diff_for_clipboard(&ctx(), &DiffResult::Equal);
        assert!(result.contains("Left: local"));
        assert!(result.contains("Right: develop"));
        assert!(result.contains("/home/user/project"));
    }
}
