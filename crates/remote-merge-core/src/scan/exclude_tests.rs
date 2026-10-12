use super::*;

fn exclude(patterns: &[&str]) -> Exclude {
    Exclude::new(&patterns.iter().map(|p| p.to_string()).collect::<Vec<_>>())
}

// @kotowari[REQ-config-021]
#[test]
fn a_name_pattern_excludes_any_element_and_everything_below_it() {
    let rule = exclude(&["node_modules", "*.log"]);
    assert!(rule.excludes(b"node_modules"));
    assert!(rule.excludes(b"web/node_modules/pkg/index.js"));
    assert!(rule.excludes(b"logs/today.log"));
    assert!(!rule.excludes(b"src/main.rs"));
    assert!(!rule.excludes(b"node_modules_backup/a.txt"));
}

// @kotowari[REQ-config-022, EX-config-009]
#[test]
fn a_path_pattern_matches_the_whole_path_and_excludes_below_a_matched_directory() {
    let rule = exclude(&["a/b", "src/*.tmp"]);
    assert!(rule.excludes(b"a/b"));
    assert!(rule.excludes(b"a/b/c.txt"));
    assert!(rule.excludes(b"a/b/deep/d.txt"));
    assert!(!rule.excludes(b"a/keep.txt"));
    assert!(!rule.excludes(b"x/a/b"));
    assert!(rule.excludes(b"src/x.tmp"));
    assert!(!rule.excludes(b"src/sub/x.tmp"));
}

#[test]
fn the_root_dir_itself_is_never_excluded() {
    assert!(!exclude(&["*"]).excludes(b""));
}

// @kotowari[REQ-scan-012]
#[test]
fn a_name_that_is_not_utf8_is_matched_in_its_escaped_form() {
    let rule = exclude(&["*.log"]);
    assert!(rule.excludes(b"bad\xff.log"));
    assert!(!rule.excludes(b"bad\xff.txt"));
}

#[test]
fn a_pattern_with_a_parent_reference_is_not_applied() {
    assert!(!exclude(&["../a"]).excludes(b"a"));
}

#[test]
fn only_patterns_with_the_same_meaning_in_find_are_used_for_pruning() {
    let rule = exclude(&[
        "node_modules",
        "*.log",
        "a/b",
        "vendor/legacy/**",
        "src/*.tmp",
        "**/gen",
        "file?.txt",
        "[ab].txt",
        "{x,y}",
        "esc\\*",
        "",
        "/abs",
        "a//b",
        "./a",
    ]);
    assert_eq!(
        rule.prunable(),
        Prunable {
            names: vec!["node_modules".into(), "*.log".into()],
            paths: vec!["a/b".into(), "vendor/legacy".into()],
        }
    );
}
