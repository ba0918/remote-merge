use super::*;

fn strings(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|p| p.to_string()).collect()
}

#[test]
fn without_directories_or_include_the_whole_root_dir_is_scanned() {
    assert_eq!(scope_for(None, None), ScanScope::Whole);
}

// @kotowari[REQ-config-004, REQ-config-032]
#[test]
fn include_values_are_scanned_with_themselves_and_none_left_scans_nothing() {
    assert_eq!(
        scope_for(None, Some(&strings(&["src", "docs"]))),
        ScanScope::Starts(vec![Start::value("src"), Start::value("docs")])
    );
    assert_eq!(scope_for(None, Some(&[])), ScanScope::Starts(vec![]));
}

// @kotowari[REQ-scan-009, TBL-scan-001]
#[test]
fn each_named_directory_is_scanned_below_with_its_own_limit() {
    assert_eq!(
        scope_for(Some(&strings(&["a/", "b"])), None),
        ScanScope::Starts(vec![Start::below("a/", 0), Start::below("b", 1)])
    );
}

// @kotowari[REQ-scan-019]
#[test]
fn a_named_directory_is_narrowed_to_the_include_values_from_the_root_dir() {
    let include = strings(&["src", "lib/core"]);
    let dirs = strings(&["src/sub/", "lib/", "docs/", "lib/core/x"]);
    assert_eq!(
        scope_for(Some(&dirs), Some(&include)),
        ScanScope::Starts(vec![
            Start::below("src/sub/", 0),
            Start {
                path: "lib/core".into(),
                with_self: true,
                group: 1,
            },
            Start::below("lib/core/x", 3),
        ])
    );
}
