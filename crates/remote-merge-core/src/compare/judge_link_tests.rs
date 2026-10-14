//! 二つの側の状態の組の判定（TBL-compare-001）のうち、リンク先を辿れない symlink の行の単体テスト
use super::*;
use crate::scan::outcome::LinkPoint;

fn link(target: &str) -> SideState {
    SideState::Symlink {
        target: target.into(),
        points_to: LinkPoint::NotDirectory,
    }
}

fn file() -> SideState {
    SideState::File {
        size: Some(1),
        mtime: Some(Mtime {
            secs: 0,
            nanos: 0,
            digits: 9,
        }),
    }
}

fn judge(first: &SideState, second: &SideState, method: Method) -> Judgement {
    judge_states((Role::Left, first), (Role::Right, second), method)
}

const BOTH: [Method; 2] = [Method::Metadata, Method::Content];

fn unfollowable(target: &str) -> SideState {
    SideState::Symlink {
        target: target.into(),
        points_to: LinkPoint::Unresolvable,
    }
}

// リンク先を辿れない symlink は、リンク文字列が同じでも判定できないとし、そうだった側を理由に並べる
// @kotowari[REQ-scan-018, TBL-compare-001]
#[test]
fn a_symlink_whose_target_cannot_be_followed_is_undecidable_with_its_sides_named() {
    for method in BOTH {
        for other in [link("loop"), file(), SideState::Absent] {
            let judged = judge(&unfollowable("loop"), &other, method);
            let Judgement::Decided(Verdict::Undecidable(why)) = judged else {
                panic!("{other:?}: {judged:?}");
            };
            assert_eq!(why, Undecidable::UnfollowableLink(vec![Role::Left]));
            assert_eq!(why.to_string(), "cannot follow the symlink target: left");
        }
        let judged = judge(&unfollowable("a"), &unfollowable("a"), method);
        assert_eq!(
            judged,
            Judgement::Decided(Verdict::Undecidable(Undecidable::UnfollowableLink(vec![
                Role::Left,
                Role::Right
            ])))
        );
    }
    let both = Undecidable::UnfollowableLink(vec![Role::Ref, Role::Right]);
    assert_eq!(
        both.to_string(),
        "cannot follow the symlink target: right; ref"
    );
}
