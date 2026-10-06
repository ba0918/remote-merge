//! 参照先を使う merge で、三者の存在と中身から書くかどうかを決める
//! （docs/ir/cli/merge.md の TBL-cli-009、REQ-cli-092、REQ-cli-051）。

use std::path::Path;

use crate::tree::FileTree;

use super::content_reads::{is_absent, ContentReads};
use super::merge::{has_three_way_conflict, reference_check_failure};

/// 三つのどれかの中身を読めなかったときの理由（TBL-cli-009 の文言）
pub const INCOMPLETE: &str = "three-way comparison incomplete";
/// 書き込み先だけが参照先から変わったときの理由（TBL-cli-009・REQ-cli-051 の文言）
pub const DESTINATION_CHANGED: &str = "destination changed since reference";
/// 書き込み先にあり参照先にないときの理由。TBL-cli-009 は文言を決めていない
pub const DESTINATION_NOT_IN_REFERENCE: &str =
    "destination exists but reference does not; cannot verify destination";

/// 一つの側のファイルの状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideContent<'a> {
    /// ないと分かっている
    Absent,
    /// あるかもしれないが中身を読めなかった
    Unread,
    Read(&'a [u8]),
}

/// TBL-cli-009 の扱い
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreeWayTreatment<'a> {
    Write,
    Refuse(&'static str),
    /// 三つとも読めたので REQ-cli-051 の確認に従う
    CheckAgainstReference {
        source: &'a [u8],
        destination: &'a [u8],
        reference: &'a [u8],
    },
}

/// 読み込み元・書き込み先・参照先の状態から TBL-cli-009 の扱いを決める（純粋関数）。
///
/// 読み込みに失敗した側が一つでもあれば、存在の組み合わせより先にその行を当てる。
/// 読み込み元がない組み合わせは表にないので、確かめられないものとして書かない
pub fn three_way_treatment<'a>(
    source: SideContent<'a>,
    destination: SideContent<'a>,
    reference: SideContent<'a>,
) -> ThreeWayTreatment<'a> {
    use SideContent::{Absent, Read, Unread};
    use ThreeWayTreatment::{CheckAgainstReference, Refuse, Write};
    if [source, destination, reference].contains(&Unread) {
        return Refuse(INCOMPLETE);
    }
    match (source, destination, reference) {
        (Read(_), Absent, Absent) => Write,
        (Read(_), Absent, Read(_)) => Refuse(DESTINATION_CHANGED),
        (Read(_), Read(_), Absent) => Refuse(DESTINATION_NOT_IN_REFERENCE),
        (Read(source), Read(destination), Read(reference)) => CheckAgainstReference {
            source,
            destination,
            reference,
        },
        _ => Refuse(INCOMPLETE),
    }
}

/// --force のない --ref の merge で、ファイルを書かない理由を返す（純粋関数）。
///
/// --dry-run では読み込みに失敗した行だけを当てる（REQ-cli-092）。左右のどちらかが symlink の
/// ファイルは参照先に対する新しい確認の対象外（FLAG-cli-027）なので、三者の衝突だけを見る
pub fn reference_refusal(
    treatment: ThreeWayTreatment<'_>,
    has_symlink: bool,
    dry_run: bool,
) -> Option<&'static str> {
    match treatment {
        ThreeWayTreatment::Refuse(INCOMPLETE) => Some(INCOMPLETE),
        _ if dry_run => None,
        ThreeWayTreatment::Write => None,
        ThreeWayTreatment::Refuse(reason) => Some(reason),
        ThreeWayTreatment::CheckAgainstReference {
            source,
            destination,
            reference,
        } => {
            if has_symlink {
                has_three_way_conflict(reference, source, destination)
                    .then_some("three-way conflict")
            } else {
                reference_check_failure(reference, source, destination)
            }
        }
    }
}

/// `tree` と読み込みの結果から、`path` の一つの側の状態を求める
pub fn side_content<'a>(path: &str, tree: &FileTree, reads: &'a ContentReads) -> SideContent<'a> {
    if is_absent(tree, path) {
        SideContent::Absent
    } else {
        reads
            .content(path)
            .map_or(SideContent::Unread, SideContent::Read)
    }
}

/// 読み込み元・書き込み先・参照先のツリーと読み込みの結果
pub struct ThreeWaySides<'a> {
    pub source: (&'a FileTree, &'a ContentReads),
    pub destination: (&'a FileTree, &'a ContentReads),
    pub reference: (&'a FileTree, &'a ContentReads),
}

/// `path` を書かない理由。ツリーと読み込みの結果から状態を求めて `reference_refusal` に渡す
pub fn reference_refusal_for(
    path: &str,
    sides: &ThreeWaySides<'_>,
    dry_run: bool,
) -> Option<&'static str> {
    let treatment = three_way_treatment(
        side_content(path, sides.source.0, sides.source.1),
        side_content(path, sides.destination.0, sides.destination.1),
        side_content(path, sides.reference.0, sides.reference.1),
    );
    let is_link = |tree: &FileTree| {
        tree.find_node(Path::new(path))
            .is_some_and(|node| node.is_symlink())
    };
    let has_symlink = is_link(sides.source.0) || is_link(sides.destination.0);
    reference_refusal(treatment, has_symlink, dry_run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use SideContent::{Absent, Read, Unread};

    const BASE: &[u8] = b"base\n";
    const CHANGED: &[u8] = b"changed\n";

    /// TBL-cli-009 の行ごとに、書くとき（plain）と --dry-run の理由
    #[test]
    fn each_row_of_the_three_way_table_decides_whether_to_write() {
        type Row = (
            &'static str,
            [SideContent<'static>; 3],
            Option<&'static str>,
            Option<&'static str>,
        );
        let rows: [Row; 10] = [
            ("source only", [Read(CHANGED), Absent, Absent], None, None),
            (
                "source and reference",
                [Read(CHANGED), Absent, Read(BASE)],
                Some(DESTINATION_CHANGED),
                None,
            ),
            (
                "all three, only the source changed",
                [Read(CHANGED), Read(BASE), Read(BASE)],
                None,
                None,
            ),
            (
                "all three, only the destination changed",
                [Read(BASE), Read(CHANGED), Read(BASE)],
                Some(DESTINATION_CHANGED),
                None,
            ),
            (
                "all three, both changed",
                [Read(CHANGED), Read(b"other\n"), Read(BASE)],
                Some("three-way conflict"),
                None,
            ),
            (
                "source and destination",
                [Read(CHANGED), Read(BASE), Absent],
                Some(DESTINATION_NOT_IN_REFERENCE),
                None,
            ),
            (
                "source unread",
                [Unread, Absent, Absent],
                Some(INCOMPLETE),
                Some(INCOMPLETE),
            ),
            (
                "destination unread",
                [Read(CHANGED), Unread, Read(BASE)],
                Some(INCOMPLETE),
                Some(INCOMPLETE),
            ),
            (
                "reference unread",
                [Read(CHANGED), Read(BASE), Unread],
                Some(INCOMPLETE),
                Some(INCOMPLETE),
            ),
            (
                "unread on a side whose absence would otherwise refuse",
                [Read(CHANGED), Absent, Unread],
                Some(INCOMPLETE),
                Some(INCOMPLETE),
            ),
        ];
        for (name, [source, destination, reference], plain, dry_run) in rows {
            let treatment = three_way_treatment(source, destination, reference);
            assert_eq!(reference_refusal(treatment, false, false), plain, "{name}");
            assert_eq!(reference_refusal(treatment, false, true), dry_run, "{name}");
        }
    }

    #[test]
    fn a_missing_source_is_not_written() {
        let treatment = three_way_treatment(Absent, Read(BASE), Read(BASE));

        assert_eq!(reference_refusal(treatment, false, false), Some(INCOMPLETE));
    }

    #[test]
    fn a_symlink_with_only_the_destination_changed_is_written_unless_both_changed() {
        let only_destination = three_way_treatment(Read(BASE), Read(CHANGED), Read(BASE));
        let both = three_way_treatment(Read(CHANGED), Read(b"other\n"), Read(BASE));

        assert_eq!(reference_refusal(only_destination, true, false), None);
        assert_eq!(
            reference_refusal(both, true, false),
            Some("three-way conflict")
        );
    }

    #[test]
    fn the_state_of_a_side_comes_from_the_tree_and_the_reads() {
        let mut tree = FileTree::new("/root");
        tree.nodes = vec![
            crate::tree::FileNode::new_file("read"),
            crate::tree::FileNode::new_file("failed"),
        ];
        let mut reads = ContentReads::default();
        reads.record_read("read", b"x".to_vec());
        reads.record_failure("failed", "denied");

        assert_eq!(side_content("read", &tree, &reads), Read(b"x"));
        assert_eq!(side_content("failed", &tree, &reads), Unread);
        assert_eq!(side_content("missing", &tree, &reads), Absent);
    }
}
