//! 扱いの並びのうち、書かずに failed に出すもの（読めない書き込み先・確かめられないリンク先・
//! 左右のどちらにもない名前。REQ-merge-017・019・033・035）の単体テスト
use remote_merge_core::compare::{Present, Role, SideState, Verdict};
use remote_merge_core::scan::outcome::LinkPoint;

use super::super::merge::MergeEntry;
use super::super::test_support::file;
use super::tests::{paths, plan_for, two, PLAIN};
use super::*;

// 書き換えるか消す書き込み先のハッシュを読めなければ、読み込みの失敗として書かず消さない
// （REQ-merge-017・019、REQ-compare-015）
// @kotowari[REQ-merge-017, REQ-merge-019, REQ-compare-015]
#[test]
fn a_destination_whose_hash_could_not_be_read_fails_as_a_read_failure() {
    use remote_merge_core::compare::{SideFailure, Undecidable};
    use remote_merge_core::op_failure::OpFailure;

    let mut unreadable = two(file(), file(), Verdict::Differs);
    unreadable
        .hashes
        .insert(Role::Right, Err(OpFailure::PermissionDenied));
    let left_failed = Verdict::Undecidable(Undecidable::ReadFailed(vec![SideFailure {
        role: Role::Left,
        failure: OpFailure::PermissionDenied,
    }]));
    let mut both = two(file(), file(), left_failed);
    both.hashes
        .insert(Role::Right, Err(OpFailure::PermissionDenied));
    let listed = paths(vec![("a.txt", unreadable), ("b.txt", both)]);

    let planned = plan_for(&listed, &["a.txt", "b.txt"], PLAIN);

    assert_eq!(
        planned,
        [
            (
                "a.txt".to_string(),
                MergeEntry::Fail("read failed: right: permission denied".to_string())
            ),
            (
                "b.txt".to_string(),
                MergeEntry::Fail(
                    "read failed: left: permission denied; right: permission denied".to_string()
                )
            ),
        ]
    );
}

// 左右のどちらにもない名前は、祖先が種類の違いでスキップする対象でも failed に出し、そのために祖先を
// skipped に出さない（REQ-merge-035）
// @kotowari[REQ-merge-035, REQ-merge-001]
#[test]
fn a_missing_name_under_a_kind_mismatch_fails_without_skipping_the_ancestor() {
    let listed = paths(vec![
        ("x", two(file(), SideState::Directory, Verdict::KindDiffers)),
        (
            "x/only.txt",
            two(
                SideState::Absent,
                file(),
                Verdict::OneSided(Present::Second),
            ),
        ),
    ]);
    let delete = PlanOptions {
        delete: true,
        ..PLAIN
    };

    let planned = plan_for(&listed, &["x/nope.txt"], delete);

    assert_eq!(
        planned,
        [(
            "x/nope.txt".to_string(),
            MergeEntry::Fail(NOT_FOUND.to_string())
        )]
    );
}

// 書き込み先の symlink のリンク先が UTF-8 として読めないか置換文字を含むなら、変わっていないことを
// 確かめられないので書かない（REQ-merge-033）
// @kotowari[REQ-merge-033]
#[test]
fn a_destination_link_whose_target_is_not_plain_utf8_is_not_replaced() {
    let link_to = |target: &[u8]| SideState::Symlink {
        target: target.to_vec(),
        points_to: LinkPoint::NotDirectory,
    };
    let listed = paths(vec![
        (
            "bytes",
            two(link_to(b"a.txt"), link_to(b"\xff.txt"), Verdict::Differs),
        ),
        (
            "replacement",
            two(
                link_to(b"a.txt"),
                link_to("\u{fffd}.txt".as_bytes()),
                Verdict::Differs,
            ),
        ),
        (
            "plain",
            two(link_to(b"a.txt"), link_to(b"b.txt"), Verdict::Differs),
        ),
    ]);

    let planned = plan_for(&listed, &["bytes", "replacement", "plain"], PLAIN);

    assert_eq!(planned[0].0, "bytes");
    assert_eq!(
        planned[0].1,
        MergeEntry::Fail(LINK_UNVERIFIABLE.to_string())
    );
    assert_eq!(planned[1].0, "plain");
    assert_eq!(planned[1].1, MergeEntry::WriteLink);
    assert_eq!(
        planned[2].1,
        MergeEntry::Fail(LINK_UNVERIFIABLE.to_string())
    );
}

// 読み込み元の symlink のリンク文字列が UTF-8 として読めなければ作れないので、--dry-run でも実行と
// 同じく計画の段で failed にする（REQ-compare-005 の、実行時に書かないものを書く予定に出さない）
// @kotowari[REQ-compare-005, REQ-cli-091]
#[test]
fn a_source_link_whose_target_is_not_utf8_fails_in_the_plan() {
    let link_to = |target: &[u8]| SideState::Symlink {
        target: target.to_vec(),
        points_to: LinkPoint::NotDirectory,
    };
    let listed = paths(vec![
        (
            "new",
            two(
                link_to(b"\xff.txt"),
                SideState::Absent,
                Verdict::OneSided(Present::First),
            ),
        ),
        (
            "plain",
            two(
                link_to(b"a.txt"),
                SideState::Absent,
                Verdict::OneSided(Present::First),
            ),
        ),
    ]);

    let planned = plan_for(&listed, &["new", "plain"], PLAIN);

    assert_eq!(
        planned,
        [
            (
                "new".to_string(),
                MergeEntry::Fail(SOURCE_LINK_NOT_UTF8.to_string())
            ),
            ("plain".to_string(), MergeEntry::WriteLink),
        ]
    );
}
