//! status が中身を読めなかったファイルの扱い（docs/ir/cli/unreadable.md の REQ-cli-088）。
//!
//! 読めなかったファイルは理由とともに errors に出し、失敗を捨てない。読み比べて判定するはずだった
//! ファイルは "modified" とも "equal" ともしない。

use super::content_reads::ContentReads;
use super::types::{FileStatus, StatusOutput, UnreadableFile};

/// 側の名前と、その側の中身の読み込みの結果
pub type SideReads<'a> = (&'a str, &'a ContentReads);

/// `files` のうち、どれかの側で中身を読めなかったファイルを、読めなかった側ごとの理由とともに
/// 返す。中身を読み比べて判定するはずだったファイル（`compared`）は `files` から外す。参照先との
/// 関係のためだけに読んだファイルは、中身を読まずに決めた判定を残す
pub fn take_unreadable_files(
    files: &mut Vec<FileStatus>,
    compared: &[String],
    sides: &[SideReads<'_>],
) -> Vec<UnreadableFile> {
    let mut unreadable = Vec::new();
    files.retain(|file| {
        let reasons: Vec<String> = sides
            .iter()
            .filter_map(|(name, reads)| {
                reads
                    .failure(&file.path)
                    .map(|reason| format!("{name}: {reason}"))
            })
            .collect();
        if reasons.is_empty() {
            return true;
        }
        unreadable.push(UnreadableFile {
            path: file.path.clone(),
            reason: reasons.join("; "),
        });
        !compared.contains(&file.path)
    });
    unreadable
}

/// 出力に読めなかったファイルを errors として加え、集計にその数を出す
pub fn with_unreadable_files(
    mut output: StatusOutput,
    unreadable: Vec<UnreadableFile>,
) -> StatusOutput {
    output.summary.unreadable = unreadable.len();
    output.errors = unreadable;
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::types::FileStatusKind;

    fn file(path: &str, status: FileStatusKind) -> FileStatus {
        FileStatus {
            path: path.into(),
            status,
            hunks: None,
            ref_badge: None,
        }
    }

    fn failed(path: &str, reason: &str) -> ContentReads {
        let mut reads = ContentReads::default();
        reads.record_failure(path, reason);
        reads
    }

    #[test]
    fn a_compared_file_that_could_not_be_read_is_taken_out_of_the_judgement() {
        let mut files = vec![
            file("a.txt", FileStatusKind::Modified),
            file("b.txt", FileStatusKind::Modified),
        ];
        let left = ContentReads::default();
        let right = failed("a.txt", "denied");

        let unreadable = take_unreadable_files(
            &mut files,
            &["a.txt".to_string(), "b.txt".to_string()],
            &[("local", &left), ("develop", &right)],
        );

        assert_eq!(unreadable.len(), 1);
        assert_eq!(unreadable[0].path, "a.txt");
        assert!(unreadable[0].reason.contains("denied"));
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "b.txt");
    }

    #[test]
    fn a_file_read_only_for_the_reference_keeps_its_judgement_and_is_still_reported() {
        let mut files = vec![file("a.txt", FileStatusKind::LeftOnly)];
        let reference = failed("a.txt", "denied");

        let unreadable = take_unreadable_files(&mut files, &[], &[("staging", &reference)]);

        assert_eq!(unreadable.len(), 1);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, FileStatusKind::LeftOnly);
    }

    #[test]
    fn the_output_carries_the_unreadable_files_and_their_count() {
        let output = StatusOutput {
            left: crate::service::types::SourceInfo {
                label: "local".into(),
                root: "/l".into(),
            },
            right: crate::service::types::SourceInfo {
                label: "develop".into(),
                root: "/r".into(),
            },
            ref_: None,
            agent: None,
            files: Some(vec![]),
            summary: Default::default(),
            errors: Vec::new(),
        };
        let unreadable = vec![UnreadableFile {
            path: "a.txt".into(),
            reason: "local: denied".into(),
        }];

        let output = with_unreadable_files(output, unreadable.clone());

        assert_eq!(output.errors, unreadable);
        assert_eq!(output.summary.unreadable, 1);
    }
}
