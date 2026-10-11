//! diff の、root_dir 全体の走査で比べる結果と、名前で指定したパスを一つずつ比べる結果を合わせる。
//!
//! 名前で指定したパスは、root_dir 全体を指すパスと一緒に指定されても一つずつ比べる
//! （docs/ir/cli/diff-strategy.md の REQ-cli-081）。合わせるときは、そのパスには一つずつ比べた
//! 結果だけを使う。

use std::collections::HashMap;

use crate::service::types::{DiffError, FileStatus};
use crate::tree::FileTree;

/// 比べる対象を決めた結果（left_tree, right_tree, statuses, existing_files, diff_files,
/// plain_file_sizes, read_errors）。plain_file_sizes は diff_files のうち、中身を読む前に通常の
/// ファイルと分かっているパスの左右の大きさ（ない側は None）。--max-files の枠が埋まった後に
/// 読まずに数えてよいかの判定に使う。read_errors は名前で指定したファイルの状態を決めるために
/// 中身を読めなかったパスで、diff_files には入れない。
pub(super) type DiffScanResult = (
    FileTree,
    FileTree,
    Vec<FileStatus>,
    Vec<String>,
    Vec<String>,
    PlainFileSizes,
    Vec<DiffError>,
);

/// パスごとの、通常のファイルとしてある側の大きさ（左, 右）
pub(super) type PlainFileSizes = HashMap<String, (Option<u64>, Option<u64>)>;

/// 指定したパスが、左右のどちらにも見つからなかった
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PathsNotFound;

impl std::fmt::Display for PathsNotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("specified path(s) not found on either side")
    }
}

impl std::error::Error for PathsNotFound {}

/// 全体の走査の結果 `whole` と、名前で指定したパス `names` を一つずつ比べた結果 `named` を合わせる。
/// 一方で指定したパスが見つからなくても、もう一方の結果があればそれを使う
pub(super) fn combine(
    whole: Option<anyhow::Result<DiffScanResult>>,
    named: Option<anyhow::Result<DiffScanResult>>,
    names: &[String],
) -> anyhow::Result<DiffScanResult> {
    let whole = whole.map(found).transpose()?.flatten();
    let named = named.map(found).transpose()?.flatten();
    match (whole, named) {
        (Some(whole), Some(named)) => Ok(union(whole, named, names)),
        (Some(result), None) | (None, Some(result)) => Ok(result),
        (None, None) => Err(PathsNotFound.into()),
    }
}

/// 指定したパスが見つからなかったなら None、ほかの失敗はそのまま
fn found(result: anyhow::Result<DiffScanResult>) -> anyhow::Result<Option<DiffScanResult>> {
    match result {
        Ok(result) => Ok(Some(result)),
        Err(error) if error.is::<PathsNotFound>() => Ok(None),
        Err(error) => Err(error),
    }
}

/// 名前で指定したパス `names` には `named` の結果だけを使い、ほかは `whole` の結果を使う。比べる
/// パスは全体の走査と同じくパスの順に並べる
fn union(whole: DiffScanResult, named: DiffScanResult, names: &[String]) -> DiffScanResult {
    let (left_tree, right_tree, statuses, existing, diff_files, sizes, errors) = whole;
    let (_, _, named_statuses, named_existing, named_diff_files, named_sizes, named_errors) = named;
    let unnamed = |path: &String| !names.contains(path);
    // 全体の走査と同じく、パスの順に一度ずつ並べる
    let joined = |paths: Vec<String>, more: Vec<String>| {
        let mut joined: Vec<String> = paths.into_iter().filter(unnamed).chain(more).collect();
        joined.sort();
        joined.dedup();
        joined
    };
    (
        left_tree,
        right_tree,
        statuses
            .into_iter()
            .filter(|status| unnamed(&status.path))
            .chain(named_statuses)
            .collect(),
        joined(existing, named_existing),
        joined(diff_files, named_diff_files),
        sizes
            .into_iter()
            .filter(|(path, _)| unnamed(path))
            .chain(named_sizes)
            .collect(),
        errors
            .into_iter()
            .filter(|error| unnamed(&error.path))
            .chain(named_errors)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::types::FileStatusKind;

    fn strings(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|path| path.to_string()).collect()
    }

    fn result(diff_files: &[&str], errors: &[&str]) -> DiffScanResult {
        let statuses = diff_files
            .iter()
            .map(|path| FileStatus {
                path: path.to_string(),
                status: FileStatusKind::Modified,
                hunks: None,
                ref_badge: None,
            })
            .collect();
        (
            FileTree::new("/root"),
            FileTree::new("/root"),
            statuses,
            strings(diff_files),
            strings(diff_files),
            diff_files
                .iter()
                .map(|path| (path.to_string(), (Some(1), Some(2))))
                .collect(),
            errors
                .iter()
                .map(|path| DiffError {
                    path: path.to_string(),
                    reason: "unreadable".into(),
                })
                .collect(),
        )
    }

    #[test]
    fn named_paths_take_the_result_of_the_one_by_one_comparison() {
        let whole = result(&["a.txt", "b.txt"], &["b.txt", "c.txt"]);
        let named = result(&["b.txt", "app.log"], &[]);

        let (_, _, statuses, existing, diff_files, sizes, errors) = combine(
            Some(Ok(whole)),
            Some(Ok(named)),
            &strings(&["b.txt", "app.log"]),
        )
        .unwrap();

        assert_eq!(diff_files, strings(&["a.txt", "app.log", "b.txt"]));
        assert_eq!(existing, diff_files);
        assert_eq!(statuses.len(), 3);
        assert_eq!(sizes.len(), 3);
        let errors: Vec<&str> = errors.iter().map(|error| error.path.as_str()).collect();
        assert_eq!(errors, ["c.txt"]);
    }

    #[test]
    fn paths_not_found_on_one_part_use_the_other_part() {
        let named = || Some(Ok(result(&["app.log"], &[])));
        let not_found = || Some(Err(PathsNotFound.into()));

        let (.., diff_files, _, _) = combine(not_found(), named(), &strings(&["app.log"])).unwrap();
        assert_eq!(diff_files, ["app.log"]);

        let error = combine(not_found(), not_found(), &[]).unwrap_err();
        assert!(error.is::<PathsNotFound>(), "{error}");
        let error = combine(Some(Err(anyhow::anyhow!("lost"))), named(), &[]).unwrap_err();
        assert_eq!(error.to_string(), "lost");
    }
}
