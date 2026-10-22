//! diff の出力に足した欄（F8。参照先のリンク文字列・参照先のハッシュ・ファイル全体の競合）の JSON の
//! 形の単体テスト
use serde_json::json;

use super::*;

// @kotowari[REQ-cli-099]
#[test]
fn a_whole_file_conflict_has_only_the_whole_file_range() {
    let area = ConflictArea::whole_file();

    assert_eq!(
        serde_json::to_value(&area).unwrap(),
        json!({"ref_range": {"whole_file": true}})
    );
}

// @kotowari[REQ-cli-065]
#[test]
fn a_line_conflict_keeps_the_fields_of_the_region() {
    let region = crate::diff::conflict::ConflictRegion {
        ref_range: 1..2,
        left_lines: vec!["l".into()],
        right_lines: vec!["r".into()],
    };

    let value = serde_json::to_value(ConflictArea::Lines(region.clone())).unwrap();

    assert_eq!(value, serde_json::to_value(&region).unwrap());
    let back: ConflictArea = serde_json::from_value(value).unwrap();
    assert_eq!(back, ConflictArea::Lines(region));
}

// @kotowari[REQ-cli-100]
#[test]
fn the_ref_link_target_is_shown_only_with_a_ref_and_is_null_when_the_ref_is_no_link() {
    let targets = |ref_: Option<Option<String>>| LinkTargets {
        left: Some("l".into()),
        right: None,
        ref_,
    };

    assert_eq!(
        serde_json::to_value(targets(None)).unwrap(),
        json!({"left": "l", "right": null})
    );
    assert_eq!(
        serde_json::to_value(targets(Some(None))).unwrap(),
        json!({"left": "l", "right": null, "ref": null})
    );
    assert_eq!(
        serde_json::to_value(targets(Some(Some("b".into())))).unwrap(),
        json!({"left": "l", "right": null, "ref": "b"})
    );
}
