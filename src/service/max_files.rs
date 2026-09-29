//! diff の --max-files による打ち切り。
//!
//! 数えるのはディレクトリを展開した後の変更のあるファイル（`DiffOutput::has_changes`）だけで、
//! 変更のない項目は数えず打ち切りもしない。

use crate::service::types::DiffOutput;

/// 打ち切った後の diff の項目と、打ち切ったときの変更のあるファイルの総数
#[derive(Debug)]
pub struct LimitedFiles {
    pub files: Vec<DiffOutput>,
    /// 打ち切ったときだけ Some（変更のあるファイルの総数）
    pub changed_files_total: Option<usize>,
}

/// 並びを保ったまま、変更のあるファイルを先頭から `max_files` 件までに絞る。0 は無制限。
pub fn limit_changed_files(files: Vec<DiffOutput>, max_files: usize) -> LimitedFiles {
    let total = files.iter().filter(|file| file.has_changes()).count();
    if max_files == 0 || total <= max_files {
        return LimitedFiles {
            files,
            changed_files_total: None,
        };
    }
    let mut kept_changed = 0;
    let files = files
        .into_iter()
        .filter(|file| {
            if !file.has_changes() {
                return true;
            }
            kept_changed += 1;
            kept_changed <= max_files
        })
        .collect();
    LimitedFiles {
        files,
        changed_files_total: Some(total),
    }
}

/// 打ち切った diff のテキストに出す、出さなかった変更のあるファイルの数
pub fn omitted_changed_files(files: &[DiffOutput], changed_files_total: usize) -> usize {
    let shown = files.iter().filter(|file| file.has_changes()).count();
    changed_files_total.saturating_sub(shown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::types::{DiffHunk, SourceInfo};

    fn entry(path: &str, changed: bool) -> DiffOutput {
        let source = SourceInfo {
            label: "local".into(),
            root: ".".into(),
        };
        DiffOutput {
            path: path.into(),
            left: source.clone(),
            right: source,
            ref_: None,
            sensitive: false,
            binary: false,
            symlink: false,
            link_targets: None,
            truncated: false,
            hunks: if changed {
                vec![DiffHunk {
                    index: 0,
                    left_start: 1,
                    right_start: 1,
                    lines: vec![],
                }]
            } else {
                vec![]
            },
            ref_hunks: None,
            left_hash: None,
            right_hash: None,
            note: None,
            conflict_count: 0,
            conflict_regions: vec![],
        }
    }

    fn paths(files: &[DiffOutput]) -> Vec<&str> {
        files.iter().map(|file| file.path.as_str()).collect()
    }

    #[test]
    fn unchanged_entries_are_neither_counted_nor_dropped() {
        let files = vec![
            entry("same", false),
            entry("a", true),
            entry("b", true),
            entry("same2", false),
        ];

        let limited = limit_changed_files(files, 1);

        assert_eq!(paths(&limited.files), vec!["same", "a", "same2"]);
        assert_eq!(limited.changed_files_total, Some(2));
    }

    #[test]
    fn not_truncated_when_changed_files_fit() {
        let files = vec![entry("a", true), entry("same", false), entry("b", true)];

        let limited = limit_changed_files(files, 2);

        assert_eq!(paths(&limited.files), vec!["a", "same", "b"]);
        assert_eq!(limited.changed_files_total, None);
    }

    #[test]
    fn zero_means_unlimited() {
        let files = vec![entry("a", true), entry("b", true)];

        let limited = limit_changed_files(files, 0);

        assert_eq!(limited.files.len(), 2);
        assert_eq!(limited.changed_files_total, None);
    }

    #[test]
    fn omitted_count_ignores_unchanged_entries() {
        let files = vec![
            entry("same", false),
            entry("a", true),
            entry("same2", false),
        ];

        assert_eq!(omitted_changed_files(&files, 4), 3);
    }
}
