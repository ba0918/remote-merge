//! 参照先を使う merge で、三者の存在と中身から書くかどうかを決める
//! （docs/ir/cli/merge.md の TBL-cli-009、REQ-cli-092、REQ-cli-051）。

use std::path::Path;

use crate::tree::{kinds_agree, EntryKind, FileNode, FileTree};

use super::content_reads::{is_absent, not_absent_in, ContentReads};
use super::merge::{has_three_way_conflict, reference_check_failure};

/// 三つのどれかの中身を読めなかったときの理由（TBL-cli-009 の文言）
pub const INCOMPLETE: &str = "three-way comparison incomplete";
/// 書き込み先だけが参照先から変わったときの理由（TBL-cli-009・REQ-cli-051 の文言）
pub const DESTINATION_CHANGED: &str = "destination changed since reference";
/// 書き込み先にあり参照先にないときの理由。TBL-cli-009 は文言を決めていない
pub const DESTINATION_NOT_IN_REFERENCE: &str =
    "destination exists but reference does not; cannot verify destination";
/// 左右の両方が参照先から変わったときの理由（TBL-cli-009・REQ-cli-051 の文言）
pub const THREE_WAY_CONFLICT: &str = "three-way conflict";

/// 一つの側のファイルの中身
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideContent<'a> {
    /// ないと分かっている
    Absent,
    /// あるかもしれないが中身を読めなかった（種類のそろわないパスでは読まない）
    Unread,
    Read(&'a [u8]),
}

/// 一つの側の種類と中身。種類はツリーにあるときだけ分かる
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SideState<'a> {
    pub kind: Option<EntryKind>,
    pub content: SideContent<'a>,
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
    /// 読み込み元と書き込み先の種類が違うので、--ref によらず REQ-merge-001 のスキップに従う
    DifferentKinds,
}

/// 読み込み元・書き込み先・参照先の状態から TBL-cli-009 の扱いを決める（純粋関数）。
///
/// 種類で決まる扱いを中身より先に当てる。読み込み元と書き込み先の種類が違えば REQ-merge-001 に
/// 従い、読み込み元と参照先の種類が違えば中身を見ずに参照先から変わったとみなす。次に、読み込みに
/// 失敗した側が一つでもあれば、存在の組み合わせより先にその行を当てる。読み込み元がない組み合わせは
/// 表にないので、確かめられないものとして書かない
pub fn three_way_treatment<'a>(
    source: SideState<'a>,
    destination: SideState<'a>,
    reference: SideState<'a>,
) -> ThreeWayTreatment<'a> {
    use SideContent::{Absent, Read, Unread};
    use ThreeWayTreatment::{CheckAgainstReference, DifferentKinds, Refuse, Write};
    if let (Some(source), Some(destination)) = (source.kind, destination.kind) {
        if source != destination {
            return DifferentKinds;
        }
    }
    if let (Some(source), Some(reference)) = (source.kind, reference.kind) {
        if source != reference {
            return match destination.content {
                Absent => Refuse(DESTINATION_CHANGED),
                _ => Refuse(THREE_WAY_CONFLICT),
            };
        }
    }
    let contents = (source.content, destination.content, reference.content);
    if [contents.0, contents.1, contents.2].contains(&Unread) {
        return Refuse(INCOMPLETE);
    }
    match contents {
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
/// ファイルは参照先に対する新しい確認の対象外（FLAG-cli-027）なので、三者の衝突だけを見る。
/// 読み込み元と書き込み先の種類が違うファイルは理由を返さず、REQ-merge-001 のスキップに任せる
pub fn reference_refusal(
    treatment: ThreeWayTreatment<'_>,
    has_symlink: bool,
    dry_run: bool,
) -> Option<&'static str> {
    match treatment {
        ThreeWayTreatment::Refuse(INCOMPLETE) => Some(INCOMPLETE),
        _ if dry_run => None,
        ThreeWayTreatment::Write | ThreeWayTreatment::DifferentKinds => None,
        ThreeWayTreatment::Refuse(reason) => Some(reason),
        ThreeWayTreatment::CheckAgainstReference {
            source,
            destination,
            reference,
        } => {
            if has_symlink {
                has_three_way_conflict(reference, source, destination).then_some(THREE_WAY_CONFLICT)
            } else {
                reference_check_failure(reference, source, destination)
            }
        }
    }
}

/// `tree` と読み込みの結果から、`path` の一つの側の中身を求める
pub fn side_content<'a>(path: &str, tree: &FileTree, reads: &'a ContentReads) -> SideContent<'a> {
    if is_absent(tree, path) {
        SideContent::Absent
    } else {
        reads
            .content(path)
            .map_or(SideContent::Unread, SideContent::Read)
    }
}

/// `tree` と読み込みの結果から、`path` の一つの側の種類と中身を求める
pub fn side_state<'a>(path: &str, tree: &FileTree, reads: &'a ContentReads) -> SideState<'a> {
    SideState {
        kind: tree
            .find_node(Path::new(path))
            .map(|node| node.entry_kind()),
        content: side_content(path, tree, reads),
    }
}

/// 三つのツリーのうち `path` のあるものの種類がそろい、ディレクトリでないか。そろわなければ
/// 中身を読まずに扱いが決まり、ディレクトリは中身として読めない
fn contents_can_decide(path: &str, trees: [&FileTree; 3]) -> bool {
    let nodes: Vec<&FileNode> = trees
        .iter()
        .filter_map(|tree| tree.find_node(Path::new(path)))
        .collect();
    !nodes.iter().any(|node| node.is_dir()) && kinds_agree(nodes)
}

/// `tree` の側で中身を読むパス。`trees` は読み込み元・書き込み先・参照先のツリー。--force では
/// 中身はバッジにしか使わないので、三つともに同じ種類であるファイル（`badge_paths`）だけを読む。
/// --force なしでは TBL-cli-009 のために、ないと分かっていないファイルを読む（ないと分かっている
/// 側と、種類のそろわないパスは読まない）
pub fn paths_to_read(
    files: &[String],
    trees: [&FileTree; 3],
    tree: &FileTree,
    badge_paths: &[String],
    force: bool,
) -> Vec<String> {
    if force {
        return badge_paths.to_vec();
    }
    let decidable: Vec<String> = files
        .iter()
        .filter(|path| contents_can_decide(path, trees))
        .cloned()
        .collect();
    not_absent_in(&decidable, tree)
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
        side_state(path, sides.source.0, sides.source.1),
        side_state(path, sides.destination.0, sides.destination.1),
        side_state(path, sides.reference.0, sides.reference.1),
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

    /// 通常ファイルの側（ないときは種類も分からない）
    fn file(content: SideContent<'static>) -> SideState<'static> {
        let kind = (content != Absent).then_some(EntryKind::File);
        SideState { kind, content }
    }

    /// 中身を読まない、ほかの種類の側
    fn other(kind: EntryKind) -> SideState<'static> {
        SideState {
            kind: Some(kind),
            content: Unread,
        }
    }

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
            let treatment = three_way_treatment(file(source), file(destination), file(reference));
            assert_eq!(reference_refusal(treatment, false, false), plain, "{name}");
            assert_eq!(reference_refusal(treatment, false, true), dry_run, "{name}");
        }
    }

    /// 種類で決まる TBL-cli-009 の行と REQ-merge-001 に任せる組み合わせ。中身は読んでいない
    #[test]
    fn a_path_whose_kinds_differ_is_decided_without_its_contents() {
        use EntryKind::{Directory, Symlink};
        use ThreeWayTreatment::{DifferentKinds, Refuse};
        type Row = (
            &'static str,
            [SideState<'static>; 3],
            ThreeWayTreatment<'static>,
            Option<&'static str>,
        );
        let rows: [Row; 7] = [
            (
                "destination a directory",
                [file(Read(CHANGED)), other(Directory), file(Read(BASE))],
                DifferentKinds,
                None,
            ),
            (
                "destination a directory, reference absent",
                [file(Read(CHANGED)), other(Directory), file(Absent)],
                DifferentKinds,
                None,
            ),
            (
                "source a directory, reference of yet another kind",
                [other(Directory), file(Unread), other(Symlink)],
                DifferentKinds,
                None,
            ),
            (
                "source a symlink, destination a file",
                [other(Symlink), file(Read(BASE)), file(Read(BASE))],
                DifferentKinds,
                None,
            ),
            (
                "reference a directory",
                [file(Unread), file(Unread), other(Directory)],
                Refuse(THREE_WAY_CONFLICT),
                Some(THREE_WAY_CONFLICT),
            ),
            (
                "reference a symlink",
                [file(Read(CHANGED)), file(Read(BASE)), other(Symlink)],
                Refuse(THREE_WAY_CONFLICT),
                Some(THREE_WAY_CONFLICT),
            ),
            (
                "reference a directory, destination absent",
                [file(Unread), file(Absent), other(Directory)],
                Refuse(DESTINATION_CHANGED),
                Some(DESTINATION_CHANGED),
            ),
        ];
        for (name, [source, destination, reference], expected, plain) in rows {
            let treatment = three_way_treatment(source, destination, reference);
            assert_eq!(treatment, expected, "{name}");
            assert_eq!(reference_refusal(treatment, false, false), plain, "{name}");
            // --dry-run では読み込みに失敗した行だけを当てる（REQ-cli-092）
            assert_eq!(reference_refusal(treatment, false, true), None, "{name}");
        }
    }

    #[test]
    fn a_symlink_with_only_the_destination_changed_is_written_unless_both_changed() {
        let only_destination =
            three_way_treatment(file(Read(BASE)), file(Read(CHANGED)), file(Read(BASE)));
        let both = three_way_treatment(
            file(Read(CHANGED)),
            file(Read(b"other\n")),
            file(Read(BASE)),
        );

        assert_eq!(reference_refusal(only_destination, true, false), None);
        assert_eq!(
            reference_refusal(both, true, false),
            Some("three-way conflict")
        );
    }

    #[test]
    fn a_forced_merge_reads_only_the_files_needed_for_a_badge() {
        let mut tree = FileTree::new("/root");
        tree.nodes = vec![FileNode::new_file("all"), FileNode::new_file("here")];
        let files = vec!["all".to_string(), "here".to_string(), "missing".to_string()];
        let badge_paths = vec!["all".to_string()];

        let trees = [&tree, &tree, &tree];

        assert_eq!(
            paths_to_read(&files, trees, &tree, &badge_paths, true),
            ["all"]
        );
        assert_eq!(
            paths_to_read(&files, trees, &tree, &badge_paths, false),
            ["all", "here"]
        );
    }

    // 種類のそろわないパスはどの側も読まず、ディレクトリを中身として読まない
    #[test]
    fn a_path_whose_kinds_differ_is_read_on_no_side() {
        let mut files_tree = FileTree::new("/root");
        files_tree.nodes = vec![FileNode::new_file("x"), FileNode::new_file("same")];
        let mut dir_tree = FileTree::new("/root");
        dir_tree.nodes = vec![FileNode::new_dir("x"), FileNode::new_file("same")];
        let files = vec!["x".to_string(), "same".to_string()];

        for trees in [
            [&files_tree, &files_tree, &dir_tree],
            [&files_tree, &dir_tree, &files_tree],
        ] {
            for tree in trees {
                assert_eq!(paths_to_read(&files, trees, tree, &[], false), ["same"]);
            }
        }
    }

    #[test]
    fn the_state_of_a_side_comes_from_the_tree_and_the_reads() {
        let mut tree = FileTree::new("/root");
        tree.nodes = vec![FileNode::new_file("read"), FileNode::new_file("failed")];
        let mut reads = ContentReads::default();
        reads.record_read("read", b"x".to_vec());
        reads.record_failure("failed", "denied");

        assert_eq!(side_content("read", &tree, &reads), Read(b"x"));
        assert_eq!(side_content("failed", &tree, &reads), Unread);
        assert_eq!(side_content("missing", &tree, &reads), Absent);
    }
}
