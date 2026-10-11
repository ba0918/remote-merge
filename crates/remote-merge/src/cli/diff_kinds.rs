//! diff で、片側が通常のディレクトリで他方が通常ファイルのパスの項目を作る。
//!
//! ディレクトリ側の子は別の項目として比べるので、この項目はディレクトリ側を中身なしとして
//! ファイル側の本文を差として示し、種類の差を note に書く。

use crate::diff::binary::compute_sha256;
use crate::diff::engine::is_binary;
use crate::service::diff::build_diff_output;
use crate::service::types::{DiffOutput, SourceInfo};

/// 通常ファイルの側の本文。左がファイルなら `Left`、右がファイルなら `Right`
pub(super) enum FileSide<'a> {
    Left(&'a [u8]),
    Right(&'a [u8]),
}

/// 片側がディレクトリで他方が通常ファイルのパス `path` の項目
pub(super) fn directory_against_file(
    path: &str,
    (left_info, right_info): (SourceInfo, SourceInfo),
    file: FileSide<'_>,
    max_lines: Option<usize>,
) -> DiffOutput {
    let (body, note, left_body, right_body) = match file {
        FileSide::Left(body) => (body, "type mismatch: file vs directory", Some(body), None),
        FileSide::Right(body) => (body, "type mismatch: directory vs file", None, Some(body)),
    };
    let mut output = if is_binary(body) {
        let mut output = build_diff_output(path, left_info, right_info, "", "", None, None, None);
        output.binary = true;
        output.left_hash = left_body.map(compute_sha256);
        output.right_hash = right_body.map(compute_sha256);
        output
    } else {
        let text =
            |side: Option<&[u8]>| String::from_utf8_lossy(side.unwrap_or_default()).into_owned();
        build_diff_output(
            path,
            left_info,
            right_info,
            &text(left_body),
            &text(right_body),
            max_lines,
            None,
            None,
        )
    };
    output.note = Some(note.into());
    // ファイルはファイル側にだけあるので、中身が空でも変更のあるファイルと数える
    output.one_sided = true;
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(label: &str) -> SourceInfo {
        SourceInfo {
            label: label.into(),
            root: "/root".into(),
        }
    }

    fn lines(output: &DiffOutput) -> Vec<(String, String)> {
        output
            .hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .map(|line| (format!("{:?}", line.line_type), line.content.clone()))
            .collect()
    }

    #[test]
    fn a_file_on_the_right_is_shown_as_added_lines_with_the_kind_difference() {
        let output = directory_against_file(
            "x",
            (info("left"), info("right")),
            FileSide::Right(b"body\n"),
            None,
        );

        assert_eq!(
            output.note.as_deref(),
            Some("type mismatch: directory vs file")
        );
        assert_eq!(lines(&output).len(), 1, "{output:?}");
        assert!(lines(&output)[0].1 == "body", "{output:?}");
        assert!(output.has_changes());
    }

    #[test]
    fn an_empty_file_against_a_directory_counts_as_a_change() {
        for file in [FileSide::Left(b""), FileSide::Right(b"")] {
            let output = directory_against_file("x", (info("left"), info("right")), file, None);

            assert!(output.has_changes(), "{output:?}");
        }
    }

    #[test]
    fn a_binary_file_on_the_left_is_shown_by_its_hash() {
        let output = directory_against_file(
            "x",
            (info("left"), info("right")),
            FileSide::Left(b"\0bin"),
            None,
        );

        assert!(output.binary);
        assert_eq!(output.left_hash, Some(compute_sha256(b"\0bin")));
        assert_eq!(output.right_hash, None);
        assert_eq!(
            output.note.as_deref(),
            Some("type mismatch: file vs directory")
        );
    }
}
