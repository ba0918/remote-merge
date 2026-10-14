//! 走査が上限か循環で打ち切った一覧の誤り（REQ-scan-004・008・002）の単体テスト
use remote_merge_core::compare::{Incomplete, Role};

use super::*;

// @kotowari[REQ-scan-008]
#[test]
fn a_truncated_listing_is_reported_with_the_guidance() {
    let error = incomplete_error(&Incomplete::Truncated {
        role: Role::Right,
        max_entries: 5,
    });
    let text = error.to_string();
    assert!(
        text.starts_with("Tree scan truncated at 5 entries."),
        "{text}"
    );
    assert!(text.contains("--max-entries"), "{text}");
    assert!(text.contains("max_scan_entries"), "{text}");
}

// @kotowari[REQ-scan-002]
#[test]
fn a_cycle_is_reported_with_its_path() {
    let error = incomplete_error(&Incomplete::Cycle {
        role: Role::Left,
        path: "loopdir/back".into(),
    });
    assert!(
        error.to_string().contains("cycle at loopdir/back"),
        "{error}"
    );
}

// diff は打ち切った一覧の誤りを errors に一度ずつ載せる（REQ-cli-084・REQ-cli-021）
// @kotowari[REQ-cli-084, REQ-cli-021]
#[test]
fn diff_lists_each_incomplete_listing_once_in_errors() {
    let both_truncated = [
        Incomplete::Truncated {
            role: Role::Left,
            max_entries: 5,
        },
        Incomplete::Truncated {
            role: Role::Right,
            max_entries: 5,
        },
        Incomplete::Cycle {
            role: Role::Left,
            path: "d/loop".into(),
        },
    ];

    let errors = incomplete_errors(&both_truncated);

    assert_eq!(errors.len(), 2, "{errors:?}");
    assert_eq!(errors[0].path, ".");
    assert!(errors[0]
        .reason
        .starts_with("Tree scan truncated at 5 entries."));
    assert_eq!(errors[1].path, "d/loop");
    assert!(errors[1].reason.contains("cycle at d/loop"), "{errors:?}");
}
