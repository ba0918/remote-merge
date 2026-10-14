//! 子の一覧（docs/ir/paths/operations.md の REQ-paths-013・017）。走査の集め方を深さ一つにして
//! 生の項目を集め、走査と同じ規則で一覧に載せる（docs/decision/records/2026-10-12-foundation-3.md
//! の D6）。子のリンクは辿らず、リンク先がディレクトリかだけを決める。

use super::drive::{ask, missing_subtree, root_identity, unreached};
use super::exclude::Exclude;
use super::outcome::{LinkPoint, ListedEntry, ListedKind, Listing, ScanError, ScanFailure};
use super::raw::{Answer, Collector, Depth, LinkTarget, Question, RawKind};
use super::walk::{PendingLink, Walk};

/// 子の一覧の結果
#[derive(Debug, Clone)]
pub enum ChildListing {
    /// ディレクトリかディレクトリへの symlink の直下の子（パスは root_dir からのもの）
    Children(Vec<ListedEntry>),
    /// 通常ファイル、ディレクトリでないものへの symlink、リンク切れ、ないパス
    NotDirectory,
    /// ディレクトリかもしれないのに中身が分からない（読めないディレクトリなど）。値は利用者に
    /// 示す理由（docs/ir/cli/directory-paths.md の REQ-cli-096）
    Unlistable(String),
}

/// root_dir からの `path` の直下の子を集めて一覧にする。ディレクトリ（かディレクトリへのリンク）
/// なら、それ自身を器のディレクトリとして載せる
pub(super) fn scan_children(
    collector: &mut dyn Collector,
    exclude: &Exclude,
    path: &str,
    max_entries: usize,
) -> Result<Listing, ScanError> {
    let path = super::starts::normalize(path).into_bytes();
    let mut questions = vec![Question {
        path: Vec::new(),
        depth: if path.is_empty() {
            Depth::Children
        } else {
            Depth::Itself
        },
    }];
    if !path.is_empty() {
        questions.push(Question {
            path: path.clone(),
            depth: Depth::Children,
        });
    }
    let answers = ask(collector, &questions, exclude)?;
    root_identity(&answers[0])?;
    let mut walk = Walk::new(exclude, max_entries, 1);
    walk.follow_links = false;
    if path.is_empty() {
        let tree = answers[0].subtree.as_ref().ok_or_else(missing_subtree)?;
        walk.take_subtree(b"", tree, &[], 0)?;
    } else {
        take_directory(&mut walk, &path, &answers[1])?;
    }
    while !walk.pending.is_empty() && !walk.stopped() {
        let pending = std::mem::take(&mut walk.pending);
        let questions: Vec<Question> = pending
            .iter()
            .map(|link| Question {
                path: link.path.clone(),
                depth: Depth::Itself,
            })
            .collect();
        let answers = ask(collector, &questions, exclude)?;
        for (link, answer) in pending.into_iter().zip(&answers) {
            walk.take_link(link, answer)?;
        }
    }
    Ok(walk.finish())
}

/// 一覧するパスの答えを載せる。ディレクトリでなければ何も載せない
fn take_directory(walk: &mut Walk, path: &[u8], answer: &Answer) -> Result<(), ScanError> {
    let entry = match &answer.entry {
        Err(why) => return unreached(path, *why),
        Ok(entry) => entry,
    };
    match entry.kind {
        RawKind::Directory => {}
        RawKind::Symlink { .. } if answer.link == Some(LinkTarget::Directory) => {}
        RawKind::Symlink { .. } => {
            return walk.take_link(PendingLink::new(path.to_vec(), Vec::new(), 0), answer);
        }
        RawKind::File | RawKind::Other => return Ok(()),
    }
    let tree = answer.subtree.as_ref().ok_or_else(missing_subtree)?;
    walk.list_container(path, &tree.start)?;
    walk.take_subtree(path, tree, &[], 0)
}

/// 子の一覧の走査の一覧 `listing` の、`path` の直下の子。`path` がディレクトリかディレクトリへの
/// リンクでなければ、ディレクトリではないもの。空の `path` は root_dir
pub fn children_in(listing: &Listing, path: &str) -> ChildListing {
    let path = super::starts::normalize(path);
    if !path.is_empty() {
        let container = listing.entries.iter().find(|entry| entry.path == path);
        let is_directory = matches!(
            container.map(|entry| &entry.kind),
            Some(
                ListedKind::Directory
                    | ListedKind::Symlink {
                        points_to: LinkPoint::Directory,
                        ..
                    }
            )
        );
        if !is_directory {
            return ChildListing::NotDirectory;
        }
    }
    let prefix = if path.is_empty() {
        String::new()
    } else {
        format!("{path}/")
    };
    let children = listing
        .entries
        .iter()
        .filter(|entry| {
            entry
                .path
                .strip_prefix(&prefix)
                .is_some_and(|name| !name.is_empty() && !name.contains('/'))
        })
        .cloned()
        .collect();
    ChildListing::Children(children)
}

/// 子の一覧や配下の展開を止めた走査の誤りの扱い
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListingFailure {
    /// 途中がディレクトリでない：ディレクトリではないもの
    NotDirectory,
    /// その名前の部分木についての事実：一覧を取れなかったもの（REQ-cli-096）
    Unlistable,
    /// root_dir を読めない・項目の情報を読めない（REQ-paths-017・REQ-scan-015）・集め方の失敗
    Stop,
}

/// 走査の誤りを、一覧を続けられるものと止めるものに分ける（docs/decision/records/
/// 2026-10-12-foundation-2.md の C12・C13・C15）。種類を足したらここで決め直すよう、すべて挙げる
pub fn listing_failure(failure: &ScanFailure) -> ListingFailure {
    match failure {
        ScanFailure::StartBlocked { .. } => ListingFailure::NotDirectory,
        ScanFailure::UnreadableDirectory { .. }
        | ScanFailure::StartUnreadable { .. }
        | ScanFailure::UnresolvableLink { .. }
        | ScanFailure::ChangedDuringScan { .. } => ListingFailure::Unlistable,
        ScanFailure::RootMissing
        | ScanFailure::RootUnreadable { .. }
        | ScanFailure::RootNotDirectory
        | ScanFailure::RootChanged
        | ScanFailure::UnreadableLinkTarget { .. }
        | ScanFailure::UnreadableTime { .. }
        | ScanFailure::InconsistentAnswer { .. }
        | ScanFailure::CollectFailed { .. } => ListingFailure::Stop,
    }
}

#[cfg(test)]
#[path = "children_tests.rs"]
mod tests;
