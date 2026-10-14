//! status のファイルの判定（docs/ir/cli/status.md の REQ-cli-027、REQ-cli-028）の契約テスト。
//!
//! 右がサーバで --ref がないときはハッシュの経路、右が local のときは中身を読む経路で比べるため、
//! 中身で判定する場合は左右を入れ替えた二つの構成で確かめる。

use std::collections::HashMap;

use remote_merge::service::types::FileStatusKind;
use remote_merge_core::compare::{
    judge_hashes, judge_states, ContentJudgement, Judgement, Method, Role, SideState, Verdict,
};

use super::status_support::{args, fixture, symlink, write_at};

const T0: u64 = 1_700_000_000;

/// ハッシュの経路（右がサーバ）と中身を読む経路（右が local）の左右
const BOTH_PATHS: [(&str, &str); 2] = [("local", "develop"), ("develop", "local")];

// @kotowari[REQ-cli-027]
#[test]
fn one_sided_files_are_left_or_right_only_and_a_file_against_a_directory_is_modified() {
    let fixture = fixture();
    let (local, develop) = (fixture.local.path(), fixture.develop.path());
    write_at(local, "left.txt", b"left\n", T0);
    write_at(local, "src/left.rs", b"left\n", T0);
    write_at(develop, "right.txt", b"right\n", T0);
    write_at(develop, "src/right.rs", b"right\n", T0);
    write_at(local, "conflict", b"file\n", T0);
    write_at(develop, "conflict/inner.txt", b"file\n", T0);
    // 片側がファイルへの symlink でも、もう片側のディレクトリの中身は右だけにある
    symlink(local, "linked", "left.txt");
    write_at(develop, "linked/inner.txt", b"file\n", T0);

    let statuses = fixture.statuses("local", "develop");

    for (path, expected) in [
        ("left.txt", FileStatusKind::LeftOnly),
        ("src/left.rs", FileStatusKind::LeftOnly),
        ("right.txt", FileStatusKind::RightOnly),
        ("src/right.rs", FileStatusKind::RightOnly),
        ("conflict", FileStatusKind::Modified),
        ("conflict/inner.txt", FileStatusKind::RightOnly),
        ("linked/inner.txt", FileStatusKind::RightOnly),
    ] {
        assert_eq!(statuses[path], expected, "{path}: {statuses:?}");
    }
}

// @kotowari[REQ-cli-027]
#[test]
fn files_of_different_sizes_are_modified() {
    let fixture = fixture();
    write_at(fixture.local.path(), "size.txt", b"short\n", T0);
    write_at(fixture.develop.path(), "size.txt", b"much longer\n", T0);

    for (left, right) in BOTH_PATHS {
        let statuses = fixture.statuses(left, right);
        assert_eq!(
            statuses["size.txt"],
            FileStatusKind::Modified,
            "{left} {right}"
        );
    }
}

// @kotowari[REQ-cli-027]
#[test]
fn same_size_and_timestamp_are_equal_without_reading_the_content() {
    let fixture = fixture();
    write_at(fixture.local.path(), "meta.txt", b"alpha\n", T0);
    write_at(fixture.develop.path(), "meta.txt", b"bravo\n", T0);

    for (left, right) in BOTH_PATHS {
        let statuses = fixture.statuses(left, right);
        assert_eq!(
            statuses["meta.txt"],
            FileStatusKind::Equal,
            "{left} {right}"
        );
    }
}

// @kotowari[REQ-cli-027]
#[test]
fn same_size_with_different_timestamps_is_decided_by_the_content() {
    let fixture = fixture();
    let (local, develop) = (fixture.local.path(), fixture.develop.path());
    write_at(local, "same.txt", b"alpha\n", T0);
    write_at(develop, "same.txt", b"alpha\n", T0 + 100);
    write_at(local, "changed.txt", b"alpha\n", T0);
    write_at(develop, "changed.txt", b"bravo\n", T0 + 100);
    write_at(local, "same.bin", &[0x89, 0x50, 0x00, 0x01], T0);
    write_at(develop, "same.bin", &[0x89, 0x50, 0x00, 0x01], T0 + 100);
    write_at(local, "changed.bin", &[0x89, 0x50, 0x00, 0x01], T0);
    write_at(develop, "changed.bin", &[0x89, 0x50, 0x00, 0x02], T0 + 100);

    for (left, right) in BOTH_PATHS {
        let statuses = fixture.statuses(left, right);
        assert_eq!(
            statuses["same.txt"],
            FileStatusKind::Equal,
            "{left} {right}"
        );
        assert_eq!(
            statuses["changed.txt"],
            FileStatusKind::Modified,
            "{left} {right}"
        );
        assert_eq!(
            statuses["same.bin"],
            FileStatusKind::Equal,
            "{left} {right}"
        );
        assert_eq!(
            statuses["changed.bin"],
            FileStatusKind::Modified,
            "{left} {right}"
        );
    }
}

/// サイズと更新時刻が分からない状態はローカルのディレクトリからは作れないため、status と同じ
/// 比べる流れの判定の関数（状態の組の判定と、中身のハッシュでの決め直し）を組み合わせて確かめる。
// @kotowari[REQ-cli-027]
#[test]
fn unknown_size_or_timestamp_is_decided_by_the_content() {
    let unknown = SideState::File {
        size: None,
        mtime: None,
    };
    let sized = SideState::File {
        size: Some(6),
        mtime: None,
    };
    for state in [unknown, sized] {
        let judged = judge_states(
            (Role::Left, &state),
            (Role::Right, &state),
            Method::Metadata,
        );
        assert_eq!(judged, Judgement::ByContent, "{state:?}");
        for (hash, expected) in [("bravo", Verdict::Same), ("other", Verdict::Differs)] {
            let decided = judge_hashes(
                (Role::Left, &Ok("bravo".to_string())),
                (Role::Right, &Ok(hash.to_string())),
            );
            assert_eq!(decided, ContentJudgement::Decided(expected), "{state:?}");
        }
    }
}

// @kotowari[REQ-cli-028]
#[test]
fn two_symlinks_are_compared_by_their_target_text_without_reading_the_content() {
    let fixture = fixture();
    let (local, develop) = (fixture.local.path(), fixture.develop.path());
    // リンク先の文字列が同じで、リンク先の中身が左右で違う
    write_at(local, "target.txt", b"left\n", T0);
    write_at(develop, "target.txt", b"right\n", T0);
    symlink(local, "same_target", "target.txt");
    symlink(develop, "same_target", "target.txt");
    // リンク先の文字列が同じで、リンク先がない
    symlink(local, "dangling", "missing.txt");
    symlink(develop, "dangling", "missing.txt");
    // リンク先の文字列が違い、リンク先の中身は同じ
    write_at(local, "a.txt", b"same\n", T0);
    write_at(develop, "a.txt", b"same\n", T0);
    write_at(local, "b.txt", b"same\n", T0);
    write_at(develop, "b.txt", b"same\n", T0);
    symlink(local, "other_target", "a.txt");
    symlink(develop, "other_target", "b.txt");

    for (left, right) in BOTH_PATHS {
        let statuses = fixture.statuses(left, right);
        assert_eq!(
            statuses["same_target"],
            FileStatusKind::Equal,
            "{statuses:?}"
        );
        assert_eq!(statuses["dangling"], FileStatusKind::Equal, "{statuses:?}");
        assert_eq!(
            statuses["other_target"],
            FileStatusKind::Modified,
            "{statuses:?}"
        );
    }
}

// --checksum でも symlink の組はリンク先の文字列で比べ、辿った先を読まない。リンク先がない組も
// 読めなかったファイルにしない
// @kotowari[EX-cli-090]
#[test]
fn checksum_compares_two_symlinks_by_their_target_text_without_reading_the_content() {
    let fixture = fixture();
    let (local, develop) = (fixture.local.path(), fixture.develop.path());
    write_at(local, "target.txt", b"left\n", T0);
    write_at(develop, "target.txt", b"right\n", T0);
    symlink(local, "same_target", "target.txt");
    symlink(develop, "same_target", "target.txt");
    symlink(local, "dangling", "missing.txt");
    symlink(develop, "dangling", "missing.txt");

    for (left, right) in BOTH_PATHS {
        let mut args = args(Some(left), Some(right));
        args.all = true;
        args.checksum = true;
        let output = fixture.run(args).unwrap().output;
        let statuses: HashMap<String, FileStatusKind> = output
            .files
            .unwrap()
            .into_iter()
            .map(|file| (file.path, file.status))
            .collect();

        assert_eq!(
            statuses.get("same_target"),
            Some(&FileStatusKind::Equal),
            "{left} {right}: {statuses:?}"
        );
        assert_eq!(
            statuses.get("dangling"),
            Some(&FileStatusKind::Equal),
            "{left} {right}: {statuses:?}"
        );
        assert!(
            output.errors.is_empty(),
            "{left} {right}: {:?}",
            output.errors
        );
    }
}

// @kotowari[EX-cli-062]
#[test]
fn a_regular_file_against_a_symlink_to_the_same_content_is_modified() {
    let fixture = fixture();
    let (local, develop) = (fixture.local.path(), fixture.develop.path());
    // symlink 自体のサイズ（リンク先の文字列の長さ）もファイルと同じ 10 バイトにする
    write_at(local, "item.txt", b"hellohello", T0);
    write_at(local, "target.txt", b"hellohello", T0);
    write_at(develop, "target.txt", b"hellohello", T0);
    symlink(develop, "item.txt", "target.txt");

    for (left, right) in BOTH_PATHS {
        let statuses = fixture.statuses(left, right);
        assert_eq!(
            statuses["item.txt"],
            FileStatusKind::Modified,
            "{statuses:?}"
        );
    }
}
