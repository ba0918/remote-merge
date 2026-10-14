//! rollback の symlink 同士の更新の記録の書き戻し（REQ-backup-010・033）の単体テスト
use super::*;

fn link_update() -> Recorded {
    Recorded::LinkUpdate {
        expected_target: "new.txt".to_string(),
        real_parent: "/r".to_string(),
    }
}

fn link_state(target: &str) -> SideState {
    SideState::Symlink {
        target: target.as_bytes().to_vec(),
        points_to: remote_merge_core::scan::outcome::LinkPoint::NotDirectory,
    }
}

fn link_entry(target: &str) -> ResolvedPath {
    ResolvedPath::Symlink {
        link_target: target.to_string(),
        real_path: format!("/r/{target}"),
        target_kind: Some(ExistingKind::File),
    }
}

// @kotowari[REQ-backup-010, EX-backup-020]
#[test]
fn a_link_still_holding_the_merged_string_is_restored() {
    let entry = one(
        link_update(),
        facts(link_state("new.txt"), link_entry("new.txt")),
    );
    assert_eq!(entry, RestoreEntry::Restore);
}

// @kotowari[REQ-backup-010, REQ-backup-033, EX-backup-022]
#[test]
fn a_link_whose_string_changed_is_skipped_even_if_it_reaches_the_same_file() {
    let rewritten = one(
        link_update(),
        facts(link_state("./new.txt"), link_entry("./new.txt")),
    );
    let replaced = one(
        link_update(),
        facts(file_state(), exists("/r/a.txt", ExistingKind::File)),
    );
    for entry in [rewritten, replaced] {
        assert_eq!(entry, RestoreEntry::Skip(LINK_CHANGED.to_string()));
    }
}
