//! status のファイルの判定（docs/ir/cli/status.md の REQ-cli-027、REQ-cli-028）の契約テスト。
//!
//! 右がサーバで --ref がないときはハッシュの経路、右が local のときは中身を読む経路で比べるため、
//! 中身で判定する場合は左右を入れ替えた二つの構成で確かめる。

use std::collections::HashMap;
use std::path::PathBuf;

use remote_merge::service::status::{
    compute_status_from_trees, needs_content_compare, refine_status_with_content,
};
use remote_merge::service::types::FileStatusKind;
use remote_merge::tree::{FileNode, FileTree};

use super::status_support::{fixture, symlink, write_at};

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

    let statuses = fixture.statuses("local", "develop");

    for (path, expected) in [
        ("left.txt", FileStatusKind::LeftOnly),
        ("src/left.rs", FileStatusKind::LeftOnly),
        ("right.txt", FileStatusKind::RightOnly),
        ("src/right.rs", FileStatusKind::RightOnly),
        ("conflict", FileStatusKind::Modified),
        ("conflict/inner.txt", FileStatusKind::RightOnly),
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

/// サイズと更新時刻が分からないノードはローカルのディレクトリからは作れないため、
/// status と同じ順に公開の判定関数を組み合わせて確かめる。
// @kotowari[REQ-cli-027]
#[test]
fn unknown_size_or_timestamp_is_decided_by_the_content() {
    let tree = |nodes: Vec<FileNode>| FileTree {
        root: PathBuf::from("/unused"),
        nodes,
    };
    let mut sized = FileNode::new_file("no_mtime.txt");
    sized.size = Some(6);
    let left = tree(vec![FileNode::new_file("no_meta.txt"), sized.clone()]);
    let right = tree(vec![FileNode::new_file("no_meta.txt"), sized]);

    for (content, expected) in [
        (b"bravo\n", FileStatusKind::Equal),
        (b"other\n", FileStatusKind::Modified),
    ] {
        let mut files = compute_status_from_trees(&left, &right, &[]);
        let paths = needs_content_compare(&files, &left, &right);
        assert_eq!(paths.len(), 2, "{paths:?}");
        let contents: HashMap<String, (Vec<u8>, Vec<u8>)> = paths
            .into_iter()
            .map(|path| (path, (b"bravo\n".to_vec(), content.to_vec())))
            .collect();
        refine_status_with_content(&mut files, &contents);
        for file in &files {
            assert_eq!(file.status, expected, "{files:?}");
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

// @kotowari[EX-cli-062]
#[test]
fn a_regular_file_against_a_symlink_to_the_same_content_is_modified() {
    let fixture = fixture();
    let (local, develop) = (fixture.local.path(), fixture.develop.path());
    write_at(local, "item.txt", b"hello\n", T0);
    write_at(local, "real.txt", b"hello\n", T0);
    write_at(develop, "real.txt", b"hello\n", T0);
    symlink(develop, "item.txt", "real.txt");

    for (left, right) in BOTH_PATHS {
        let statuses = fixture.statuses(left, right);
        assert_eq!(
            statuses["item.txt"],
            FileStatusKind::Modified,
            "{statuses:?}"
        );
    }
}
