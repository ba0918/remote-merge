//! rollback のセッション全体を書き戻さない扱い（REQ-backup-003）の単体テスト
use super::*;

// 一件でもほかの場所を指すか第三者が変えたリンクがあれば、セッションのどのファイルも書き戻さない
// @kotowari[REQ-backup-003, EX-backup-005, EX-backup-021]
#[test]
fn one_changed_location_blocks_every_file_of_the_session() {
    let plan = rollback_plan(vec![
        (
            "first.txt".to_string(),
            recorded_file("/r/first.txt"),
            facts(file_state(), exists("/r/first.txt", ExistingKind::File)),
        ),
        (
            "shared/second.txt".to_string(),
            recorded_file("/old/second.txt"),
            facts(file_state(), exists("/new/second.txt", ExistingKind::File)),
        ),
        (
            "gone/third.txt".to_string(),
            recorded_file("/r/gone/third.txt"),
            facts(SideState::Absent, missing("/r/gone", false)),
        ),
    ]);
    assert_eq!(
        plan,
        vec![
            (
                "first.txt".to_string(),
                RestoreEntry::Skip(CHANGED_LOCATION.to_string())
            ),
            (
                "shared/second.txt".to_string(),
                RestoreEntry::Skip(CHANGED_LOCATION.to_string())
            ),
            (
                "gone/third.txt".to_string(),
                RestoreEntry::Skip(PARENT_GONE.to_string())
            ),
        ]
    );
}

// 一件でもリンク先を辿れなければ、セッションのすべてを "cannot resolve path: " と原因で failed に出す
// @kotowari[REQ-backup-003]
#[test]
fn one_unresolvable_path_fails_every_file_of_the_session_with_the_cause() {
    let plan = rollback_plan(vec![
        (
            "first.txt".to_string(),
            recorded_file("/r/first.txt"),
            facts(file_state(), exists("/r/first.txt", ExistingKind::File)),
        ),
        (
            "second.txt".to_string(),
            recorded_file("/r/second.txt"),
            TargetFacts::Unresolvable("too many levels of symbolic links".to_string()),
        ),
    ]);
    assert_eq!(
        plan,
        vec![
            (
                "first.txt".to_string(),
                RestoreEntry::Fail(
                    "cannot resolve path: too many levels of symbolic links".to_string()
                )
            ),
            (
                "second.txt".to_string(),
                RestoreEntry::Fail(
                    "cannot resolve path: too many levels of symbolic links".to_string()
                )
            ),
        ]
    );
}
