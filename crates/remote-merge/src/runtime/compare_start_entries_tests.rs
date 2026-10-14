//! 二つの走査の一覧を一つにする規則（REQ-cli-021・REQ-cli-084）の単体テスト
use super::*;

// 起点そのものを集める走査が打ち切ったことも、数えた件数も、一つにした一覧に残す
// @kotowari[REQ-cli-084, REQ-cli-021]
#[test]
fn joining_keeps_the_truncation_and_the_count_of_both_scans() {
    let below = Listing {
        counted: 2,
        ..Listing::default()
    };
    let themselves = Listing {
        truncated: true,
        counted: 1,
        unresolvable_names: vec!["x\\xff".into()],
        ..Listing::default()
    };

    let joined = joined(below, themselves);

    assert!(joined.truncated);
    assert_eq!(joined.counted, 3);
    assert_eq!(joined.unresolvable_names, ["x\\xff"]);
}
