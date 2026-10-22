//! 起点ごとの答えを一覧に載せる規則（include の値・起点そのもの・指定したディレクトリの下）

use super::drive::{missing_subtree, unreached_start};
use super::outcome::ScanError;
use super::raw::{Answer, DirId, RawKind};
use super::start_paths::StartPath;
use super::walk::{PendingLink, Walk};

/// include の値そのものと、その下を載せる（docs/ir/config/include.md）
pub(super) fn take_value(
    walk: &mut Walk,
    path: &[u8],
    answer: &Answer,
    root: DirId,
    counter: usize,
) -> Result<(), ScanError> {
    let entry = match &answer.entry {
        Err(why) => return unreached_start(path, *why),
        Ok(entry) => entry,
    };
    if entry.kind == RawKind::Directory {
        let tree = answer.subtree.as_ref().ok_or_else(missing_subtree)?;
        return walk.take_subtree(path, tree, &[root], counter);
    }
    // 値そのものが symlink なら、同じ問いの答えでそのリンク先を決める
    match walk.take_leaf(path, entry, vec![root], counter)? {
        Some(link) => walk.take_link(&link, answer),
        None => Ok(()),
    }
}

/// 起点そのものだけを載せる。ディレクトリは器として載せ、symlink は辿らずにリンク先の種類を決める。
/// ディレクトリへのリンクも器なので、ディレクトリと同じく件数に数えない（REQ-scan-010）
pub(super) fn take_itself(
    walk: &mut Walk,
    path: &[u8],
    answer: &Answer,
    root: DirId,
    counter: usize,
) -> Result<(), ScanError> {
    let entry = match &answer.entry {
        Err(why) => return unreached_start(path, *why),
        Ok(entry) => entry,
    };
    if entry.kind == RawKind::Directory {
        return walk.list_directory(path, entry);
    }
    let container = answer.link == Some(super::raw::LinkTarget::Directory);
    let counter = (!container).then_some(counter);
    match walk.take_leaf_counted(path, entry, vec![root], counter)? {
        Some(link) => walk.point_link(&link, answer),
        None => Ok(()),
    }
}

/// 指定したディレクトリの下だけを載せる（TBL-scan-001）。起点がディレクトリ（かディレクトリへの
/// リンク）なら、空でも分かるようにそれ自身を器のディレクトリとして載せる
pub(super) fn take_below(
    walk: &mut Walk,
    start: &StartPath,
    answer: &Answer,
    root: DirId,
) -> Result<(), ScanError> {
    let path = start.path.as_slice();
    let entry = match &answer.entry {
        Err(_) if start.excluded => return Ok(()),
        Err(why) => return unreached_start(path, *why),
        Ok(entry) => entry,
    };
    let is_dir = match entry.kind {
        RawKind::Directory => true,
        RawKind::Symlink { .. } => {
            if answer.link != Some(super::raw::LinkTarget::Directory) {
                if start.excluded {
                    return Ok(());
                }
                let link = PendingLink::new(path.to_vec(), vec![root], start.group);
                return walk.take_link(&link, answer);
            }
            true
        }
        RawKind::File | RawKind::Other => false,
    };
    if !is_dir {
        return Ok(());
    }
    let tree = answer.subtree.as_ref();
    walk.list_directory(path, tree.map_or(entry, |tree| &tree.start))?;
    match (start.descend, tree) {
        (false, _) => Ok(()),
        (true, Some(tree)) => walk.take_subtree(path, tree, &[root], start.group),
        (true, None) => Err(missing_subtree()),
    }
}
