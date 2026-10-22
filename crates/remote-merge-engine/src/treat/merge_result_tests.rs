//! merge と sync の一つの書き込み先の結果を出力の欄に詰める規則（REQ-cli-048・049、REQ-merge-024・
//! 026・034、REQ-cli-047）の単体テスト
use serde_json::json;

use super::*;
use crate::service::types::DeleteStatus;

fn outcomes() -> Vec<(String, Outcome)> {
    vec![
        (
            "a.txt".into(),
            Outcome::Merged {
                backup: Some("s/a.txt".into()),
                ref_badge: Some("differs".into()),
            },
        ),
        ("b.txt".into(), Outcome::Deleted { backup: None }),
        ("c.txt".into(), Outcome::Skipped("why".into())),
        ("d.txt".into(), Outcome::Failed("broken".into())),
        ("e.txt".into(), Outcome::WouldMerge { ref_badge: None }),
        ("f.txt".into(), Outcome::WouldDelete),
    ]
}

// @kotowari[REQ-cli-048, REQ-merge-026]
#[test]
fn each_outcome_goes_to_its_list_in_path_order() {
    let lists = target_lists(outcomes());

    let merged: Vec<(&str, &str)> = lists
        .merged
        .iter()
        .map(|file| (file.path.as_str(), file.status.as_str()))
        .collect();
    assert_eq!(merged, [("a.txt", "ok"), ("e.txt", "would merge")]);
    assert_eq!(lists.merged[0].backup.as_deref(), Some("s/a.txt"));
    assert_eq!(lists.merged[0].ref_badge.as_deref(), Some("differs"));
    let deleted: Vec<(&str, DeleteStatus)> = lists
        .deleted
        .iter()
        .map(|file| (file.path.as_str(), file.status))
        .collect();
    assert_eq!(
        deleted,
        [
            ("b.txt", DeleteStatus::Ok),
            ("f.txt", DeleteStatus::WouldDelete)
        ]
    );
    assert_eq!(lists.skipped[0].path, "c.txt");
    assert_eq!(lists.failed[0].error, "broken");
}

// --dry-run の削除予定は "would delete" で出す（F12）
// @kotowari[REQ-merge-026]
#[test]
fn a_planned_delete_is_serialized_as_would_delete() {
    assert_eq!(
        serde_json::to_value(DeleteStatus::WouldDelete).unwrap(),
        json!("would delete")
    );
}

// 書き込み先を変えた後で失敗したファイル（中身は書いたが権限を変えられなかったものなど）は、failed に
// 出し、書き込み先を変えたことも残す（sync の status を決めるのに使う。TBL-cli-007）
// @kotowari[TBL-cli-007, REQ-cli-041]
#[test]
fn a_file_failed_after_changing_the_destination_is_failed_and_counts_as_changed() {
    let lists = target_lists(vec![(
        "a.txt".to_string(),
        Outcome::FailedAfterChange("content written, but permissions not changed".into()),
    )]);

    assert_eq!(lists.failed.len(), 1);
    assert!(lists.merged.is_empty());
    assert!(lists.changed_but_failed);
    assert!(!target_lists(vec![("b".into(), Outcome::Failed("x".into()))]).changed_but_failed);
}

// どの種類の結果も一件あれば空でなく、結果がないときだけ "no files to merge" にする
#[test]
fn the_lists_are_empty_only_without_any_outcome() {
    assert!(target_lists(Vec::new()).is_empty());
    for outcome in outcomes() {
        let shown = format!("{outcome:?}");
        assert!(!target_lists(vec![outcome]).is_empty(), "{shown}");
    }
}
