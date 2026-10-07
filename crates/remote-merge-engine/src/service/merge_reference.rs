//! 参照先を使う merge で、三者の存在と中身から書くかどうかを決める
//! （docs/ir/cli/merge.md の TBL-cli-009、REQ-cli-092、REQ-cli-051）。

use crate::tree::EntryKind;

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
}
