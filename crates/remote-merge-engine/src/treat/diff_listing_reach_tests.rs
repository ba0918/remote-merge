//! 比べなかったパスの errors（参照先のない symlink と、root_dir の外を通って状態を集めなかったパス。
//! REQ-cli-022・026）の単体テスト
use std::collections::BTreeSet;

use remote_merge_core::compare::{Role, SideState, Verdict};

use super::super::test_support::{compared_with, file, pair};
use super::tests::{limits, paths, sources, whole};
use super::*;

// 参照先がないか読めない symlink は、項目を残してエラーにする（REQ-cli-022）
// @kotowari[REQ-cli-022]
#[test]
fn a_link_to_nothing_is_shown_and_reported() {
    let broken = || SideState::Symlink {
        target: b"missing".to_vec(),
        points_to: remote_merge_core::scan::outcome::LinkPoint::Missing,
    };
    let comparison = compared_with(
        &[(Role::Left, broken()), (Role::Right, broken())],
        vec![pair(Role::Left, Role::Right, Verdict::Same)],
        &[],
    );
    let listed = paths(vec![("l", comparison)]);

    let output = diff_listing(&listed, &sources(false), limits(100), &whole());

    assert_eq!(output.files.len(), 1);
    assert_eq!(output.errors[0].path, "l");
    let reason = &output.errors[0].reason;
    assert!(reason.contains("unreadable"), "{reason}");
}

// root_dir の外を通って状態を集めなかったパスは、名前で指定したか、他方の側に比べるファイルがあれば
// 比べなかったとして errors に出す。件数の上限か循環で打ち切った側は、打ち切りの誤りに任せる
// @kotowari[REQ-cli-026]
#[test]
fn a_path_not_collected_through_an_outside_link_is_reported_when_named_or_present_elsewhere() {
    let uncollected = |present: &[(Role, SideState)]| {
        let mut comparison = compared_with(present, vec![], &[]);
        comparison.states.not_collected.insert(Role::Left);
        comparison
    };
    let listed = paths(vec![
        ("named", uncollected(&[])),
        ("other", uncollected(&[(Role::Right, file())])),
        ("quiet", uncollected(&[])),
        ("dir", uncollected(&[(Role::Right, SideState::Directory)])),
    ]);
    let named = ["named".to_string()];
    let coverage = Coverage {
        named: &named,
        incomplete: BTreeSet::new(),
    };

    let output = diff_listing(&listed, &sources(false), limits(100), &coverage);

    let reported: Vec<&str> = output
        .errors
        .iter()
        .map(|error| error.path.as_str())
        .collect();
    assert_eq!(reported, ["named", "other"]);
    assert!(output.files.is_empty());
    let truncated = Coverage {
        named: &named,
        incomplete: BTreeSet::from([Role::Left]),
    };
    let quiet = diff_listing(&listed, &sources(false), limits(100), &truncated);
    assert!(quiet.errors.is_empty(), "{:?}", quiet.errors);
}

/// 左右の組が `verdict` で、参照先を読めなかったファイル
fn with_unreadable_ref(verdict: Verdict) -> remote_merge_core::compare::PathComparison {
    use remote_merge_core::compare::{SideFailure, Undecidable};
    use remote_merge_core::op_failure::OpFailure;
    let denied = || {
        Verdict::Undecidable(Undecidable::ReadFailed(vec![SideFailure {
            role: Role::Ref,
            failure: OpFailure::PermissionDenied,
        }]))
    };
    compared_with(
        &[
            (Role::Left, file()),
            (Role::Right, file()),
            (Role::Ref, file()),
        ],
        vec![
            pair(Role::Left, Role::Right, verdict),
            pair(Role::Left, Role::Ref, denied()),
            pair(Role::Right, Role::Ref, denied()),
        ],
        &[(Role::Left, b"l\n"), (Role::Right, b"r\n")],
    )
}

// 参照先を読めなかったことは、左右に差がなくても、--max-files の枠の外でも errors に出す
// @kotowari[REQ-cli-090, REQ-cli-055, REQ-cli-067]
#[test]
fn an_unreadable_ref_is_an_error_for_unchanged_files_and_files_beyond_the_limit() {
    let listed = paths(vec![
        ("a", with_unreadable_ref(Verdict::Differs)),
        ("b", with_unreadable_ref(Verdict::Differs)),
        ("same", with_unreadable_ref(Verdict::Same)),
    ]);

    let output = diff_listing(&listed, &sources(true), limits(1), &whole());

    let shown: Vec<&str> = output.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(shown, ["a"]);
    let errors: Vec<&str> = output
        .errors
        .iter()
        .map(|error| error.path.as_str())
        .collect();
    assert_eq!(errors, ["a", "b", "same"]);
    assert!(
        output
            .errors
            .iter()
            .all(|error| error.reason == "read failed: ref: permission denied")
    );
}

// 参照先の側だけが root_dir の外を通って状態を集めなかったか、外へ解決されるパスも、左右の側と同じく
// 比べなかった理由を errors に出す。打ち切った参照先の側は打ち切りの誤りに任せる
// @kotowari[REQ-cli-026, REQ-cli-063]
#[test]
fn a_ref_not_collected_through_an_outside_link_is_reported() {
    let differs = || {
        compared_with(
            &[(Role::Left, file()), (Role::Right, file())],
            vec![pair(Role::Left, Role::Right, Verdict::Differs)],
            &[(Role::Left, b"l\n"), (Role::Right, b"r\n")],
        )
    };
    let mut below = differs();
    below.states.not_collected.insert(Role::Ref);
    let mut itself = differs();
    itself.states.outside.insert(Role::Ref);
    let reached = paths(vec![("below", below.clone()), ("itself", itself.clone())]);
    let cut = paths(vec![("cut", below), ("itself", itself)]);
    let coverage = Coverage {
        named: &[],
        incomplete: BTreeSet::from([Role::Ref]),
    };

    let output = diff_listing(&reached, &sources(true), limits(100), &whole());
    let truncated = diff_listing(&cut, &sources(true), limits(100), &coverage);

    let reported = |output: &MultiDiffOutput| -> Vec<String> {
        output
            .errors
            .iter()
            .filter(|error| error.reason == OUTSIDE_NOT_COMPARED)
            .map(|error| error.path.clone())
            .collect()
    };
    assert_eq!(reported(&output), ["below", "itself"]);
    assert_eq!(reported(&truncated), ["itself"]);
    assert_eq!(output.files.len(), 2);
}
