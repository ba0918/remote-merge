//! 比べる側の決め方（TBL-cli-004・TBL-cli-006・TBL-cli-008、REQ-cli-037・050・062・066・073）の単体テスト
use super::*;
use crate::side::Side;

fn servers() -> Vec<String> {
    vec!["staging".into(), "develop".into()]
}

fn remote(name: &str) -> Side {
    Side::Remote(name.into())
}

// 既定サーバは名前順で最初のサーバ
// @kotowari[REQ-cli-034, TBL-cli-004]
#[test]
fn each_row_of_the_table_picks_the_sides() {
    let cases = [
        (None, None, (Side::Local, remote("develop"))),
        (None, Some("staging"), (Side::Local, remote("staging"))),
        (
            Some("staging"),
            None,
            (remote("staging"), remote("develop")),
        ),
        (
            Some("develop"),
            Some("staging"),
            (remote("develop"), remote("staging")),
        ),
        (
            Some("local"),
            Some("develop"),
            (Side::Local, remote("develop")),
        ),
    ];
    for (left, right, expected) in cases {
        assert_eq!(
            compared_sides(left, right, &servers()),
            Ok(expected),
            "{left:?} {right:?}"
        );
    }
}

// @kotowari[REQ-cli-034]
#[test]
fn the_same_sides_and_unknown_servers_are_errors() {
    for (left, right) in [
        (Some("develop"), Some("develop")),
        (Some("local"), Some("local")),
    ] {
        let error = compared_sides(left, right, &servers()).unwrap_err();
        assert!(error.contains("must be different"), "{error}");
    }
    let unknown = compared_sides(None, Some("nowhere"), &servers()).unwrap_err();
    assert_eq!(unknown, "Server 'nowhere' not found in config");
}

// @kotowari[REQ-cli-034]
#[test]
fn sides_made_the_same_by_the_default_server_say_so() {
    let error = compared_sides(Some("develop"), None, &servers()).unwrap_err();
    assert!(error.contains("default server"), "{error}");
    assert!(error.contains("develop"), "{error}");
}

// @kotowari[REQ-cli-034]
#[test]
fn a_needed_default_server_without_any_server_is_an_error() {
    for (left, right) in [(None, None), (Some("local"), None)] {
        assert!(compared_sides(left, right, &[]).is_err(), "{left:?}");
    }
    assert_eq!(
        compared_sides(None, Some("local"), &[]).map(|sides| sides.1),
        Err("--left and --right must be different (both resolved to 'local')".into())
    );
}

// @kotowari[REQ-cli-046, TBL-cli-008]
#[test]
fn merge_needs_both_sides_and_different_known_ones() {
    assert_eq!(
        merge_sides(None, Some("develop"), &servers()),
        Err(
            "--left and --right are required for merge command (e.g. --left local --right staging)"
                .into()
        )
    );
    assert_eq!(
        merge_sides(Some("develop"), Some("develop"), &servers()),
        Err("--left and --right must be different (both resolved to 'develop')".into())
    );
    assert_eq!(
        merge_sides(Some("nowhere"), Some("develop"), &servers()),
        Err("Server 'nowhere' not found in config".into())
    );
    assert_eq!(
        merge_sides(Some("local"), Some("develop"), &servers()),
        Ok((Side::Local, remote("develop")))
    );
}

// @kotowari[REQ-cli-038, TBL-cli-006]
#[test]
fn sync_needs_a_source_and_distinct_known_targets() {
    let right = |names: &[&str]| {
        names
            .iter()
            .map(|name| name.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        sync_sides(None, &right(&["develop"]), &servers()),
        Err("--left is required for sync command".into())
    );
    assert_eq!(
        sync_sides(Some("local"), &[], &servers()),
        Err("--right requires at least one target server for sync command".into())
    );
    assert_eq!(
        sync_sides(Some("local"), &right(&["develop", "develop"]), &servers()),
        Err("Duplicate target server: develop".into())
    );
    assert_eq!(
        sync_sides(Some("develop"), &right(&["develop"]), &servers()),
        Err("--left and --right must be different (both resolved to 'develop')".into())
    );
    assert_eq!(
        sync_sides(Some("local"), &right(&["nowhere"]), &servers()),
        Err("Server 'nowhere' not found in config".into())
    );
    assert_eq!(
        sync_sides(Some("local"), &right(&["staging", "develop"]), &servers()),
        Ok((Side::Local, vec![remote("staging"), remote("develop")]))
    );
}

// @kotowari[REQ-cli-062, REQ-cli-037, REQ-cli-050, REQ-cli-066]
#[test]
fn a_reference_is_used_unless_it_is_one_of_the_sides() {
    let sides = (Side::Local, remote("develop"));
    assert_eq!(
        reference_side(None, &sides, &servers()),
        Ok(Reference::None)
    );
    assert_eq!(
        reference_side(Some("staging"), &sides, &servers()),
        Ok(Reference::Use(remote("staging")))
    );
    let same_as_left = reference_side(Some("local"), &sides, &servers()).unwrap();
    assert_eq!(
        same_as_left,
        Reference::Skipped(
            "Warning: --ref server is the same as left side; ref comparison skipped.".into()
        )
    );
    assert_eq!(
        reference_side(Some("develop"), &sides, &servers()),
        Ok(Reference::Skipped(
            "Warning: --ref server is the same as right side; ref comparison skipped.".into()
        ))
    );
    assert_eq!(
        reference_side(Some("nowhere"), &sides, &servers()),
        Err("Server 'nowhere' not found in config".into())
    );
}

// リモート間の merge は --force か --dry-run がなければ止め、ローカルを含む組は止めない
// @kotowari[REQ-cli-073]
#[test]
fn only_a_remote_to_remote_merge_without_force_or_dry_run_is_blocked() {
    let (local, a, b) = (Side::Local, remote("a"), remote("b"));
    assert!(remote_to_remote_blocked(&a, &b, false, false));
    assert!(!remote_to_remote_blocked(&a, &b, true, false));
    assert!(!remote_to_remote_blocked(&a, &b, false, true));
    assert!(!remote_to_remote_blocked(&local, &b, false, false));
    assert!(!remote_to_remote_blocked(&a, &local, false, false));
}
