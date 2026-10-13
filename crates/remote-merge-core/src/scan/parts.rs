//! 一覧に載せる項目の形と、走査の誤りの作り方
use std::collections::HashMap;

use super::outcome::{ListedEntry, ListedKind, ScanError, ScanFailure};
use super::raw::{DirId, RawEntry, RawTime, ReadCause, Subtree};
use crate::unreadable_name::escape_non_utf8;

pub(crate) fn join(base: &[u8], rel: &[u8]) -> Vec<u8> {
    match (base.is_empty(), rel.is_empty()) {
        (true, _) => rel.to_vec(),
        (_, true) => base.to_vec(),
        _ => [base, b"/", rel].concat(),
    }
}

pub(super) fn shown(path: &[u8]) -> String {
    escape_non_utf8(path)
}

pub(super) fn unreadable_directory(path: &[u8], cause: super::raw::ReadCause) -> ScanFailure {
    if path.is_empty() {
        ScanFailure::RootUnreadable { cause }
    } else {
        ScanFailure::UnreadableDirectory {
            path: shown(path),
            cause,
        }
    }
}

/// 走査の途中で項目が変わり、問いの答えが前の答えと合わない
pub(super) fn changed(path: &[u8]) -> ScanError {
    ScanFailure::ChangedDuringScan { path: shown(path) }.into()
}

/// 起点からの `path` の親までの各ディレクトリの番号（起点そのものを含む）
pub(super) fn parent_ids(path: &[u8], dir_ids: &HashMap<&[u8], DirId>) -> Vec<DirId> {
    let mut ids = vec![dir_ids[&b""[..]]];
    for (end, _) in path.iter().enumerate().filter(|(_, b)| **b == b'/') {
        if let Some(id) = dir_ids.get(&path[..end]) {
            ids.push(*id);
        }
    }
    ids
}

pub(super) fn listed_entry(
    path: &str,
    entry: &RawEntry,
    kind: ListedKind,
) -> Result<ListedEntry, ScanError> {
    let RawTime::Read {
        secs,
        nanos,
        digits,
    } = entry.mtime
    else {
        return Err(ScanFailure::UnreadableTime {
            path: path.to_string(),
        }
        .into());
    };
    Ok(ListedEntry {
        path: path.to_string(),
        kind,
        size: entry.size,
        mtime_secs: secs,
        mtime_nanos: nanos,
        mtime_digits: digits,
        permissions: entry.mode & 0o7777,
    })
}

/// 辿るかをまだ決めていない symlink
pub(super) struct PendingLink {
    pub path: Vec<u8>,
    /// root_dir からこのリンクの親までのディレクトリの番号（循環の判定に使う）
    pub ancestors: Vec<DirId>,
    pub counter: usize,
    /// 一覧に載せたときの位置（読めない名前なら None）
    pub listed: Option<usize>,
}

impl PendingLink {
    /// 一覧に載せていない symlink（指定したディレクトリの起点）
    pub fn new(path: Vec<u8>, ancestors: Vec<DirId>, counter: usize) -> Self {
        Self {
            path,
            ancestors,
            counter,
            listed: None,
        }
    }
}

/// 相対パスの親（最後の "/" より前。なければ起点そのもの）
pub(super) fn parent_of(path: &[u8]) -> &[u8] {
    path.iter()
        .rposition(|byte| *byte == b'/')
        .map_or(&path[..0], |at| &path[..at])
}

/// 起点の下の並びの一つ
pub(super) enum Item<'a> {
    Entry(&'a RawEntry),
    /// 情報を読めなかった項目
    Unread(ReadCause),
    /// 一覧に出た後で変わった項目
    Changed,
}

impl Item<'_> {
    fn is_changed(&self) -> bool {
        matches!(self, Self::Changed)
    }
}

/// 起点の下の項目と、情報を読めなかった項目と、変わった項目を、パスの順に並べる。同じパスでは
/// 変わったことを先に置く
pub(super) fn in_path_order(tree: &Subtree) -> Vec<(&[u8], Item<'_>)> {
    let mut sorted: Vec<(&[u8], Item)> = tree
        .entries
        .iter()
        .map(|entry| (entry.path.as_slice(), Item::Entry(entry)))
        .chain(
            tree.unreadable_entries
                .iter()
                .map(|item| (item.path.as_slice(), Item::Unread(item.cause))),
        )
        .chain(tree.changed.iter().map(|p| (p.as_slice(), Item::Changed)))
        .collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0).then(b.1.is_changed().cmp(&a.1.is_changed())));
    sorted
}
