//! 参照先を使う merge の確認で、中身のハッシュとリンク文字列と、その他の種類の参照先で決める
//! 単体テスト

use super::*;

fn hashed(mut comparison: PathComparison, hashes: [Result<&str, OpFailure>; 3]) -> PathComparison {
    let roles = [Role::Left, Role::Right, Role::Ref];
    for (role, hash) in roles.into_iter().zip(hashes) {
        comparison
            .hashes
            .insert(role, hash.map(|hash| hash.to_string()));
    }
    comparison
}

// 三つとも通常ファイルなら、比べ方によらず中身のハッシュで、三つとも symlink ならリンク文字列で比べる
// （大きさと更新時刻で同じとした組でも中身で決める。REQ-cli-051・REQ-compare-008）
// @kotowari[REQ-cli-051, REQ-compare-008, EX-compare-009]
#[test]
fn the_contents_or_the_link_strings_decide_even_when_the_metadata_says_same() {
    let by_metadata = || three([file(), file(), file()], [Differs, Same, Same]);
    let destination_changed = hashed(by_metadata(), [Ok("a"), Ok("b"), Ok("a")]);
    assert_eq!(
        reference_check(&destination_changed),
        ReferenceCheck::Fail(ReferenceFailure::DestinationChanged)
    );
    let both_changed = hashed(by_metadata(), [Ok("a"), Ok("b"), Ok("c")]);
    assert_eq!(
        reference_check(&both_changed),
        ReferenceCheck::Fail(ReferenceFailure::Conflict)
    );
    let unreadable = hashed(
        by_metadata(),
        [Ok("a"), Ok("b"), Err(OpFailure::PermissionDenied)],
    );
    assert_eq!(
        reference_check(&unreadable),
        ReferenceCheck::Fail(ReferenceFailure::Incomplete)
    );
    let links = three(
        [link("same"), link("other"), link("same")],
        [Differs, Same, Same],
    );
    assert_eq!(
        reference_check(&links),
        ReferenceCheck::Fail(ReferenceFailure::DestinationChanged)
    );
}

// 参照先がその他の種類なら、読み込みの失敗ではないので、その理由のまま failed に出す（REQ-compare-009）
// @kotowari[REQ-compare-009, REQ-cli-092]
#[test]
fn a_reference_of_another_kind_is_reported_with_its_reason() {
    let other = Verdict::Undecidable(Undecidable::NotRegularFile(vec![Role::Ref]));
    let comparison = three(
        [file(), file(), SideState::Other],
        [Differs, other.clone(), other],
    );

    let checked = reference_check(&comparison);

    assert_eq!(
        checked,
        ReferenceCheck::Fail(ReferenceFailure::Undecidable(
            "not a regular file: ref".to_string()
        ))
    );
    if let ReferenceCheck::Fail(failure) = checked {
        assert_eq!(failure.text(), "not a regular file: ref");
    }
}
