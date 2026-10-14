//! merge と sync の走査の範囲（docs/ir/scan/limits.md の TBL-scan-001）と、名前で指定したパスの選び方
//! （docs/ir/merge/basic.md の REQ-merge-021・022）の単体テスト
use remote_merge_core::relative_path::RelPath;

use super::*;

fn targets(paths: &[&str], delete: bool) -> MergeTargets {
    let parsed: Vec<RelPath> = paths
        .iter()
        .map(|path| RelPath::parse(path).unwrap())
        .collect();
    merge_targets(&parsed, delete)
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| name.to_string()).collect()
}

// @kotowari[TBL-scan-001]
#[test]
fn each_row_of_the_table_chooses_the_scan_range() {
    let whole = |paths: &[&str], delete: bool| targets(paths, delete).scope;
    for paths in [&["."][..], &["./"], &["/"], &["a/b", "."]] {
        assert_eq!(whole(paths, false), MergeScope::Whole, "{paths:?}");
    }
    assert_eq!(whole(&["d/*.txt"], false), MergeScope::Whole);
    let many: Vec<String> = (0..21).map(|index| format!("d/{index}.txt")).collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    assert_eq!(whole(&many, false), MergeScope::Whole);
    assert_eq!(whole(&["d/", "e/a.txt"], false), MergeScope::Whole);
    assert_eq!(whole(&["d/a.txt"], true), MergeScope::Whole);
    assert_eq!(
        whole(&["d/", "e/f/"], false),
        MergeScope::Below(names(&["d", "e/f"]))
    );
    assert_eq!(
        whole(&["d/a.txt", "d/b.txt", "e/f/c.txt"], false),
        MergeScope::Below(names(&["d", "e/f"]))
    );
    assert_eq!(whole(&["d/a.txt", "top.txt"], false), MergeScope::Whole);
}

// @kotowari[REQ-merge-021, REQ-merge-022]
#[test]
fn the_named_paths_and_what_is_below_them_are_selected() {
    let chosen = targets(&["src/", "./a.txt", "a.txt"], false);

    assert_eq!(chosen.named, names(&["src", "a.txt"]));
    assert!(chosen.selects("a.txt"));
    assert!(chosen.selects("src"));
    assert!(chosen.selects("src/x/y.txt"));
    assert!(!chosen.selects("srcx/y.txt"));
    assert!(!chosen.selects("b.txt"));
    let all = targets(&["."], false);
    assert!(all.named.is_empty());
    assert!(all.selects("anything/at/all"));
}
