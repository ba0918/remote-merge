//! rollback の書き戻しの扱い（docs/ir/backup/rollback-path.md、docs/ir/compare/reading.md の
//! TBL-compare-005 の rollback の行）の単体テスト
use remote_merge_core::compare::SideState;
use remote_merge_core::path_resolution::{ExistingKind, ResolvedPath};

use super::*;

fn file_state() -> SideState {
    SideState::File {
        size: Some(1),
        mtime: None,
    }
}

fn exists(real_path: &str, kind: ExistingKind) -> ResolvedPath {
    ResolvedPath::Exists {
        real_path: real_path.to_string(),
        kind,
    }
}

fn missing(real_parent: &str, parent_exists: bool) -> ResolvedPath {
    ResolvedPath::Missing {
        real_parent: real_parent.to_string(),
        parent_exists,
    }
}

fn root() -> ResolvedPath {
    exists("/r", ExistingKind::Directory)
}

fn recorded_file(real_path: &str) -> Recorded {
    Recorded::File {
        real_path: real_path.to_string(),
    }
}

fn facts(state: SideState, entry: ResolvedPath) -> TargetFacts {
    TargetFacts::Resolved {
        state,
        entry,
        parent: root(),
    }
}

fn one(recorded: Recorded, facts: TargetFacts) -> RestoreEntry {
    let mut plan = rollback_plan(vec![("a.txt".to_string(), recorded, facts)]);
    assert_eq!(plan.len(), 1);
    plan.remove(0).1
}

// @kotowari[REQ-backup-034]
#[test]
fn an_unchanged_file_at_the_recorded_place_is_restored() {
    let entry = one(
        recorded_file("/r/a.txt"),
        facts(file_state(), exists("/r/a.txt", ExistingKind::File)),
    );
    assert_eq!(entry, RestoreEntry::Restore);
}

// @kotowari[REQ-backup-027, REQ-backup-030]
#[test]
fn an_absent_file_whose_parent_is_at_the_recorded_place_is_recreated() {
    let entry = one(
        recorded_file("/r/a.txt"),
        facts(SideState::Absent, missing("/r", true)),
    );
    assert_eq!(entry, RestoreEntry::Restore);
}

// @kotowari[REQ-compare-009, TBL-compare-005, REQ-compare-014]
#[test]
fn a_target_that_is_not_a_regular_file_fails_with_the_target_side_name() {
    for (state, kind) in [
        (SideState::Other, ExistingKind::Other),
        (SideState::Directory, ExistingKind::Directory),
    ] {
        let entry = one(
            recorded_file("/r/a.txt"),
            facts(state, exists("/r/a.txt", kind)),
        );
        assert_eq!(
            entry,
            RestoreEntry::Fail("not a regular file: target".to_string())
        );
    }
}

// @kotowari[REQ-backup-031, EX-backup-027, EX-backup-029, EX-backup-030]
#[test]
fn an_absent_file_whose_parent_is_gone_or_not_a_directory_is_skipped() {
    let gone = one(
        recorded_file("/r/dir/a.txt"),
        facts(SideState::Absent, missing("/r/dir", false)),
    );
    let replaced = one(
        recorded_file("/r/dir/a.txt"),
        facts(
            SideState::Absent,
            ResolvedPath::AncestorNotDirectory {
                real_path: "/r/dir".to_string(),
                regular_file: true,
            },
        ),
    );
    for entry in [gone, replaced] {
        assert_eq!(entry, RestoreEntry::Skip(PARENT_GONE.to_string()));
    }
}

// @kotowari[REQ-backup-033]
#[test]
fn a_file_that_now_resolves_elsewhere_is_skipped_as_a_changed_location() {
    let moved = one(
        recorded_file("/r/a.txt"),
        facts(file_state(), exists("/elsewhere/a.txt", ExistingKind::File)),
    );
    let reparented = one(
        recorded_file("/r/dir/a.txt"),
        facts(SideState::Absent, missing("/elsewhere", true)),
    );
    let replaced_by_link = one(
        recorded_file("/r/a.txt"),
        facts(
            SideState::Symlink {
                target: b"other.txt".to_vec(),
                points_to: remote_merge_core::scan::outcome::LinkPoint::NotDirectory,
            },
            ResolvedPath::Symlink {
                link_target: "other.txt".to_string(),
                real_path: "/r/other.txt".to_string(),
                target_kind: Some(ExistingKind::File),
            },
        ),
    );
    for entry in [moved, reparented, replaced_by_link] {
        assert_eq!(entry, RestoreEntry::Skip(CHANGED_LOCATION.to_string()));
    }
}

// @kotowari[REQ-backup-032]
#[test]
fn a_replaced_symlink_is_never_restored() {
    let entry = one(
        Recorded::ReplacedLink,
        facts(file_state(), exists("/r/a.txt", ExistingKind::File)),
    );
    assert_eq!(entry, RestoreEntry::Skip(LINK_NOT_SUPPORTED.to_string()));
}

#[path = "rollback_block_tests.rs"]
mod blocks;
#[path = "rollback_link_tests.rs"]
mod links;
