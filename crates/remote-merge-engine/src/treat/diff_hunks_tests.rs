//! 行の差分を出力の hunk にして --max-lines で打ち切る規則（REQ-cli-054・058）の単体テスト
use remote_merge_core::content::{Content, Interpretation, interpret};
use remote_merge_core::diff::engine::PairScript;

use super::*;
use crate::service::types::DiffLineType;

/// `old` から `new` への差分を、diff の出力と同じく表示用の行の差分から hunk にする
fn shown_hunks(old: &str, new: &str, max_lines: Option<usize>) -> ShownHunks {
    let (Ok(Content::Text(old)), Ok(Content::Text(new))) = (
        interpret(old.as_bytes(), Interpretation::Unspecified),
        interpret(new.as_bytes(), Interpretation::Unspecified),
    ) else {
        panic!("the texts of a test are text");
    };
    shown_script(PairScript::new(&old, &new).display(), max_lines)
}

fn kinds(shown: &ShownHunks) -> Vec<(DiffLineType, String)> {
    shown
        .hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .map(|line| (line.line_type, line.content.clone()))
        .collect()
}

// @kotowari[REQ-cli-058]
#[test]
fn the_same_texts_have_no_hunks() {
    let shown = shown_hunks("a\n", "a\n", None);
    assert!(shown.hunks.is_empty());
    assert!(!shown.truncated);
}

// @kotowari[REQ-cli-058, REQ-cli-060]
#[test]
fn a_change_is_shown_with_removed_and_added_lines_and_zero_based_starts() {
    let shown = shown_hunks("a\nb\n", "a\nc\n", None);

    assert_eq!(
        kinds(&shown),
        [
            (DiffLineType::Context, "a".to_string()),
            (DiffLineType::Removed, "b".to_string()),
            (DiffLineType::Added, "c".to_string()),
        ]
    );
    assert_eq!(shown.hunks[0].index, 0);
    assert_eq!(shown.hunks[0].left_start, 0);
    assert_eq!(shown.hunks[0].right_start, 0);
    let one_sided = shown_hunks("", "x\n", None);
    assert_eq!(kinds(&one_sided), [(DiffLineType::Added, "x".to_string())]);
}

// 追加と削除の行がちょうど上限で、その後に出す行がなければ truncated にしない（何も省いていない）
// @kotowari[REQ-cli-054]
#[test]
fn exactly_the_limit_with_nothing_left_is_not_truncated() {
    let shown = shown_hunks("a\n", "a\nb\n", Some(1));

    assert_eq!(
        kinds(&shown),
        [
            (DiffLineType::Context, "a".to_string()),
            (DiffLineType::Added, "b".to_string())
        ]
    );
    assert!(!shown.truncated);
}

// 上限に達した後に出す行が残っていれば、打ち切って truncated にする
// @kotowari[REQ-cli-054]
#[test]
fn a_line_left_after_the_limit_is_truncated() {
    let shown = shown_hunks("a\nx\nz\n", "a\nb\nz\n", Some(1));

    assert!(shown.truncated);
}

// @kotowari[REQ-cli-054]
#[test]
fn change_lines_stop_at_the_limit_and_context_is_not_counted() {
    let shown = shown_hunks("a\nb\nc\nx\ny\n", "a\nb\nc\n1\n2\n3\n", Some(3));

    let changes = kinds(&shown)
        .iter()
        .filter(|(kind, _)| *kind != DiffLineType::Context)
        .count();
    assert_eq!(changes, 3);
    assert_eq!(
        kinds(&shown).last().unwrap(),
        &(DiffLineType::Added, "1".to_string())
    );
    assert!(shown.truncated);
}

// @kotowari[REQ-cli-054]
#[test]
fn zero_or_no_limit_shows_every_change() {
    for limit in [Some(0), None] {
        let shown = shown_hunks("x\ny\n", "1\n2\n3\n", limit);
        assert_eq!(kinds(&shown).len(), 5, "{limit:?}");
        assert!(!shown.truncated, "{limit:?}");
    }
}
