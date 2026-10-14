//! 起点の途中の事実から決める規則の単体テスト
use std::collections::BTreeMap;

use super::*;

fn link(real: &str) -> ResolvedPath {
    ResolvedPath::Symlink {
        link_target: "t".into(),
        real_path: real.into(),
        target_kind: Some(crate::path_resolution::ExistingKind::Directory),
    }
}

fn exists(real: &str) -> ResolvedPath {
    ResolvedPath::Exists {
        real_path: real.into(),
        kind: crate::path_resolution::ExistingKind::File,
    }
}

fn blocked() -> ResolvedPath {
    ResolvedPath::AncestorNotDirectory {
        real_path: "/root/f".into(),
        regular_file: true,
    }
}

fn named(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|path| path.to_string()).collect()
}

#[test]
fn the_paths_to_check_are_every_element_of_every_start_once() {
    assert_eq!(
        paths_to_check(&named(&["a/b/c.txt", "a/b/", "d"])),
        ["a", "a/b", "a/b/c.txt", "d"]
    );
}

// @kotowari[REQ-compare-004]
#[test]
fn the_shallowest_link_outside_root_dir_on_the_way_to_each_start_is_found() {
    let facts = BTreeMap::from([
        ("in".to_string(), link("/root/real")),
        ("in/out".to_string(), link("/elsewhere")),
        ("in/out/x".to_string(), exists("/elsewhere/x")),
        ("ext".to_string(), link("/elsewhere")),
        ("plain".to_string(), exists("/root/plain")),
    ]);
    assert_eq!(
        outside_on_the_way(
            &named(&["in/out/x", "ext", "plain", "ext/y"]),
            &facts,
            "/root"
        ),
        ["in/out", "ext"]
    );
}

// @kotowari[REQ-compare-004]
#[test]
fn only_listed_links_resolving_outside_root_dir_are_outside() {
    let facts = BTreeMap::from([
        ("a".to_string(), link("/root/real")),
        ("b".to_string(), link("/elsewhere")),
    ]);
    assert_eq!(links_outside(&named(&["a", "b"]), &facts, "/root"), ["b"]);
}

// @kotowari[REQ-compare-003]
#[test]
fn a_start_whose_parent_is_not_a_directory_is_blocked() {
    let facts = BTreeMap::from([
        ("f/x".to_string(), blocked()),
        ("g".to_string(), exists("/root/g")),
    ]);
    assert_eq!(
        blocked_starts(&named(&["f/x/", "g", "unknown"]), &facts),
        ["f/x"]
    );
}
