//! 走査で見つけた、UTF-8 として読めない名前のファイルの報告（docs/ir/scan/listing.md の
//! REQ-scan-012）。
//!
//! 走査はそのファイルを一覧に載せず、ツリーの `unreadable_names` に残す。status と diff は errors に、
//! merge と sync は failed に、パスと理由を出す。

use std::collections::BTreeMap;

use crate::tree::FileTree;

/// 側の名前と、その側を走査したツリー
pub type ScannedSide<'a> = (&'a str, &'a FileTree);

/// 読めない名前のファイルのパスと理由。同じパスが複数の側にあれば、理由を "; " でつなぐ
pub fn unreadable_names(sides: &[ScannedSide<'_>]) -> Vec<(String, String)> {
    let mut reasons: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for (side, tree) in sides {
        for name in &tree.unreadable_names {
            reasons
                .entry(name.as_str())
                .or_default()
                .push(format!("{side}: file name is not valid UTF-8"));
        }
    }
    reasons
        .into_iter()
        .map(|(path, reasons)| (path.to_string(), reasons.join("; ")))
        .collect()
}

/// [`unreadable_names`] のうち、指定したパス `requested` が比べる範囲にあるもの。パスの指定が
/// ないか、root_dir 全体を指すものや glob 文字を含むものがあれば全て
pub fn requested_unreadable_names(
    requested: &[String],
    sides: &[ScannedSide<'_>],
) -> Vec<(String, String)> {
    unreadable_names(sides)
        .into_iter()
        .filter(|(name, _)| requested.is_empty() || requested.iter().any(|path| covers(path, name)))
        .collect()
}

/// 指定したパス `requested` が `name` を比べる範囲に含むか
pub(crate) fn covers(requested: &str, name: &str) -> bool {
    let requested = requested.trim_end_matches('/');
    requested.is_empty()
        || requested == "."
        || requested.contains(['*', '?', '['])
        || name == requested
        || name
            .strip_prefix(requested)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(names: &[&str]) -> FileTree {
        let mut tree = FileTree::new("/root");
        tree.unreadable_names = names.iter().map(|name| name.to_string()).collect();
        tree
    }

    #[test]
    fn only_names_within_the_requested_paths_are_reported() {
        let left = tree(&["d/a\\xff", "e/b\\xff"]);
        let requested = |paths: &[&str]| {
            let paths: Vec<String> = paths.iter().map(|path| path.to_string()).collect();
            requested_unreadable_names(&paths, &[("local", &left)])
                .into_iter()
                .map(|(path, _)| path)
                .collect::<Vec<_>>()
        };

        assert_eq!(requested(&["d"]), vec!["d/a\\xff"]);
        assert_eq!(requested(&["d/"]), vec!["d/a\\xff"]);
        assert_eq!(requested(&["dd"]), Vec::<String>::new());
        assert_eq!(requested(&[]), vec!["d/a\\xff", "e/b\\xff"]);
        assert_eq!(requested(&["."]), vec!["d/a\\xff", "e/b\\xff"]);
        assert_eq!(requested(&["*.txt"]), vec!["d/a\\xff", "e/b\\xff"]);
    }

    #[test]
    fn each_unreadable_name_is_reported_once_with_the_sides_that_have_it() {
        let left = tree(&["a\\xff", "b\\xfe"]);
        let right = tree(&["a\\xff"]);

        let reported = unreadable_names(&[("local", &left), ("develop", &right)]);

        assert_eq!(
            reported,
            vec![
                (
                    "a\\xff".to_string(),
                    "local: file name is not valid UTF-8; develop: file name is not valid UTF-8"
                        .to_string()
                ),
                (
                    "b\\xfe".to_string(),
                    "local: file name is not valid UTF-8".to_string()
                ),
            ]
        );
    }
}
