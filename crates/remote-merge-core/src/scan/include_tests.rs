use super::*;
use crate::path_resolution::{ExistingKind, ResolvedPath};

const ROOT: &str = "/srv/app";

fn exists(real_path: &str) -> ResolvedPath {
    ResolvedPath::Exists {
        real_path: real_path.into(),
        kind: ExistingKind::Directory,
    }
}

fn symlink(real_path: &str) -> ResolvedPath {
    ResolvedPath::Symlink {
        link_target: "x".into(),
        real_path: real_path.into(),
        target_kind: Some(ExistingKind::Directory),
    }
}

// @kotowari[REQ-config-032, EX-config-010]
#[test]
fn a_value_missing_on_the_scanned_side_is_skipped_with_a_warning() {
    let missing = ResolvedPath::Missing {
        real_parent: ROOT.into(),
        parent_exists: true,
    };
    let decision = decide_value(&missing, ROOT).unwrap();
    assert_eq!(decision, ValueDecision::Skip);
    assert_eq!(
        decision.warning("docs").unwrap(),
        "Include path does not exist, skipping: docs"
    );
}

// @kotowari[REQ-config-032]
#[test]
fn a_value_below_something_that_is_not_a_directory_stops_the_scan() {
    let blocked = ResolvedPath::AncestorNotDirectory {
        real_path: "/srv/app/src".into(),
        regular_file: true,
    };
    let error = decide_value(&blocked, ROOT).unwrap_err();
    assert_eq!(error.to_string(), "cannot resolve path: not a directory");
}

// @kotowari[REQ-config-033, EX-config-013]
#[test]
fn a_value_resolving_outside_the_root_dir_is_not_used_and_warns_with_the_value() {
    for resolved in [
        symlink("/srv/shared"),
        exists("/srv/shared/inner"),
        symlink("/srv/app2"),
    ] {
        let decision = decide_value(&resolved, ROOT).unwrap();
        assert_eq!(decision, ValueDecision::Outside);
        assert!(decision.warning("ext").unwrap().ends_with(": ext"));
    }
}

// @kotowari[REQ-config-034, REQ-config-036]
#[test]
fn values_inside_the_root_dir_are_scanned_including_links_and_dangling_links_inside_it() {
    for resolved in [
        exists("/srv/app/config/app.php"),
        symlink("/srv/app/releases/v2"),
        symlink("/srv/app/gone"),
    ] {
        let decision = decide_value(&resolved, ROOT).unwrap();
        assert_eq!(decision, ValueDecision::Scan);
        assert_eq!(decision.warning("v"), None);
    }
}

// @kotowari[REQ-config-034]
#[test]
fn a_dangling_link_whose_real_path_is_outside_the_root_dir_is_outside() {
    assert_eq!(
        decide_value(&symlink("/elsewhere/missing"), ROOT).unwrap(),
        ValueDecision::Outside
    );
}

#[test]
fn the_filesystem_root_contains_every_path() {
    assert!(is_inside("/etc/passwd", "/"));
    assert!(is_inside("/srv/app", "/srv/app"));
    assert!(!is_inside("/srv/app2", "/srv/app"));
}
