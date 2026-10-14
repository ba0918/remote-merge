//! --hunks の選んだ hunk だけを書き込み先に当てる規則（REQ-merge-009・031・032、TBL-merge-001）の
//! 単体テスト
use super::*;

/// 文脈の 3 行より離れた二つの変更がある左右
fn two_changes() -> (String, String) {
    let middle: String = (0..8).map(|i| format!("m{i}\n")).collect();
    (
        format!("left 1\n{middle}left 2\n"),
        format!("right 1\n{middle}right 2\n"),
    )
}

// @kotowari[REQ-merge-030]
#[test]
fn the_same_texts_have_no_changes() {
    assert_eq!(
        merge_selected_hunks("a\n", "a\n", None, &[0]),
        Ok(HunkMerge::NoChanges)
    );
}

// @kotowari[REQ-merge-028, TBL-merge-001, REQ-merge-032]
#[test]
fn an_index_beyond_the_hunks_is_out_of_range() {
    let (left, right) = two_changes();
    assert_eq!(
        merge_selected_hunks(&left, &right, None, &[0, 2]),
        Err(HunkError::OutOfRange { index: 2, total: 2 })
    );
}

// @kotowari[REQ-merge-009, REQ-merge-032]
#[test]
fn only_the_selected_hunk_is_applied_to_the_destination() {
    let (left, right) = two_changes();

    let merged = merge_selected_hunks(&left, &right, None, &[1]).unwrap();

    let HunkMerge::Merged { content, total } = merged else {
        panic!("{merged:?}")
    };
    assert_eq!(total, 2);
    assert!(content.starts_with("right 1\n"), "{content}");
    assert!(content.ends_with("left 2\n"), "{content}");
}

// @kotowari[REQ-merge-031]
#[test]
fn a_conflict_anywhere_against_the_reference_stops_the_merge() {
    let conflict = merge_selected_hunks("1\nleft\n3\n", "1\nright\n3\n", Some(b"1\n2\n3\n"), &[0]);
    assert_eq!(conflict, Err(HunkError::Conflict));
    let binary_reference = merge_selected_hunks("a\n", "b\n", Some(b"\xff\n"), &[0]);
    assert_eq!(binary_reference, Err(HunkError::Conflict));
    let clean = merge_selected_hunks("L\n2\n", "1\n2\n", Some(b"1\n2\n"), &[0]);
    assert!(clean.is_ok(), "{clean:?}");
}

/// 全部の hunk を選んだときと、一つも選ばなかったときの当てた中身
fn all_and_none(left: &str, right: &str) -> (String, String) {
    let content = |selected: &[usize]| match merge_selected_hunks(left, right, None, selected) {
        Ok(HunkMerge::Merged { content, .. }) => content,
        other => panic!("{left:?} {right:?} {selected:?}: {other:?}"),
    };
    let HunkMerge::Merged { total, .. } = merge_selected_hunks(left, right, None, &[]).unwrap()
    else {
        panic!("{left:?} {right:?}: no changes");
    };
    let all: Vec<usize> = (0..total).collect();
    (content(&all), content(&[]))
}

// 末尾の改行の有無も、それを含む変更を選んだときだけ読み込み元から当てる
// @kotowari[REQ-merge-009]
#[test]
fn every_hunk_makes_the_source_and_no_hunk_keeps_the_destination_with_its_last_newline() {
    let middle: String = (0..8).map(|i| format!("m{i}\n")).collect();
    let cases = [
        ("x\ny\n".to_string(), String::new()),
        ("a".to_string(), "a\n".to_string()),
        ("a\n".to_string(), "a".to_string()),
        ("a\n".to_string(), "a\nb".to_string()),
        (format!("l1\n{middle}end"), format!("r1\n{middle}end\n")),
    ];
    for (left, right) in cases {
        let (all, none) = all_and_none(&left, &right);
        assert_eq!(all, left, "all hunks of {left:?} onto {right:?}");
        assert_eq!(none, right, "no hunk of {left:?} onto {right:?}");
    }
}
