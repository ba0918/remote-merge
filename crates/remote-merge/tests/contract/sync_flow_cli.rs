#![cfg(unix)]
//! sync が書き込み先ごとに比べる流れで比べ、扱いを書き込み先の status・並び・削除予定に訳すこと
//! （docs/ir/cli/sync.md の REQ-cli-039・041・045・072、TBL-cli-007、docs/ir/compare/judgement.md の
//! REQ-compare-009、docs/ir/backup/failure.md の REQ-backup-018）の契約テスト。組み方は sync_support
//! にある。

use std::fs;
use std::os::unix::fs::PermissionsExt;

use remote_merge::service::output::format_sync_text;
use remote_merge::service::types::{DeleteStatus, SyncTargetStatus};

use super::sync_support::{args, fixture, labels};

// @kotowari[REQ-cli-041, TBL-cli-007, EX-cli-103]
#[test]
fn ex_cli_103_a_kind_mismatch_makes_the_target_partial() {
    let fixture = fixture();
    fixture.write("local", "x", "a file\n");
    fs::create_dir_all(fixture.root("develop").join("x")).unwrap();
    fixture.write("develop", "x/in.txt", "kept\n");
    // 大きさで違うと決まる組（同じ大きさで更新時刻も同じなら、大きさと更新時刻で比べて書かない）
    fixture.write("local", "a.txt", "new content\n");
    fixture.write("develop", "a.txt", "old\n");

    let (output, code) = fixture.sync(args(&["."], &["develop"]));

    assert_eq!(
        output.targets[0].status,
        SyncTargetStatus::Partial,
        "{output:?}"
    );
    assert_eq!(code, 2);
    assert_eq!(fixture.read("develop", "a.txt"), "new content\n");
}

// 書き込むファイルがなく消すファイルだけの書き込み先も、消せたファイルを数える（F14）
// @kotowari[REQ-cli-041, TBL-cli-007, EX-cli-112]
#[test]
fn ex_cli_112_a_target_with_only_deletions_and_one_failure_is_partial() {
    let fixture = fixture();
    fixture.write("develop", "gone.txt", "x\n");
    let locked = fixture.root("develop").join("locked");
    fs::create_dir_all(&locked).unwrap();
    fixture.write("develop", "locked/stuck.txt", "x\n");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
    let applies = fs::File::create(locked.join("probe")).is_err();
    let mut delete = args(&["."], &["develop"]);
    delete.delete = true;

    let result = fixture.run(delete);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    if !applies {
        eprintln!("note: running as root; undeletable files cannot be made");
        return;
    }

    let remote_merge::cli::sync::SyncCommandOutput::Result(output) = result.unwrap().output else {
        panic!("expected a sync result")
    };
    let target = &output.targets[0];
    assert_eq!(target.status, SyncTargetStatus::Partial, "{output:?}");
    assert_eq!(target.deleted.len(), 1, "{output:?}");
    assert_eq!(target.failed.len(), 1, "{output:?}");
}

// @kotowari[REQ-cli-039, EX-cli-113]
#[test]
fn ex_cli_113_a_target_that_cannot_be_connected_keeps_its_place_in_the_order() {
    let fixture = fixture();
    fixture.write("local", "a.txt", "new\n");

    let (output, code) = fixture.sync(args(&["."], &["offline", "develop", "staging"]));

    assert_eq!(labels(&output), ["offline", "develop", "staging"]);
    assert_eq!(
        output.targets[0].status,
        SyncTargetStatus::Failed,
        "{output:?}"
    );
    assert_eq!(code, 2);
    assert_eq!(fixture.read("staging", "a.txt"), "new\n");
}

// @kotowari[REQ-compare-009, EX-compare-011]
#[test]
fn ex_compare_011_a_destination_only_socket_is_not_deleted() {
    let fixture = fixture();
    let socket = fixture.root("develop").join("app.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let mut delete = args(&["."], &["develop"]);
    delete.delete = true;

    let (output, code) = fixture.sync(delete);

    let failed = &output.targets[0].failed;
    assert_eq!(failed.len(), 1, "{output:?}");
    assert_eq!(failed[0].path, "app.sock");
    assert!(failed[0].error.contains("not a regular file"), "{output:?}");
    assert!(socket.exists());
    assert_eq!(code, 2);
}

// @kotowari[REQ-cli-045, REQ-cli-072]
#[test]
fn a_dry_run_lists_planned_deletions_as_would_delete() {
    let fixture = fixture();
    fixture.write("develop", "old.txt", "x\n");
    let mut planned = args(&["."], &["develop"]);
    planned.delete = true;
    planned.dry_run = true;

    let (output, _) = fixture.sync(planned);

    let deleted = &output.targets[0].deleted;
    assert_eq!(deleted[0].path, "old.txt", "{output:?}");
    assert_eq!(deleted[0].status, DeleteStatus::WouldDelete);
    let text = format_sync_text(&output);
    assert!(
        text.lines()
            .any(|line| line.trim() == "plan   old.txt (delete)"),
        "{text}"
    );
    assert!(fixture.exists("develop", "old.txt"));
}
