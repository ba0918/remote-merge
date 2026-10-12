//! diff が名前で指定したディレクトリの配下の一覧。全体の走査と同じ走査（起点をそのディレクトリに
//! したもの）で側ごとに一度集め、その木から子を答える（docs/ir/scan/listing.md の REQ-scan-020）。
//!
//! 配下に読めないディレクトリがあれば、その走査全体を止める（REQ-scan-014）。そのときは
//! 指定したパス全体を一覧を取れなかったものにする（docs/ir/cli/directory-paths.md の REQ-cli-096、
//! docs/decision/records/2026-10-12-foundation-2.md の C12）。

use remote_merge_core::listing::ChildListing;
use remote_merge_core::scan::outcome::ScanFailure;
use remote_merge_engine::local_io::ScanTruncated;
use remote_merge_engine::scan_result::PartialScan;

use crate::runtime::connection::classify;
use crate::runtime::CoreRuntime;
use crate::service::types::DiffError;
use crate::service::unreadable_names::requested_unreadable_names;
use crate::side::Side;
use crate::tree::{FileNode, FileTree};

/// 指定したディレクトリごと・側ごとの走査の結果
pub(crate) struct Expansions {
    max_entries: usize,
    scans: Vec<Scanned>,
}

struct Scanned {
    side: Side,
    root: String,
    /// 走査した木か、一覧を取れなかった理由
    result: Result<FileTree, String>,
    /// 循環で止めた走査の誤り（読めたところまでの木は `result` にある。REQ-cli-021）
    incomplete: Option<DiffError>,
    noticed: bool,
}

impl Expansions {
    pub(crate) fn new(max_entries: usize) -> Self {
        Self {
            max_entries,
            scans: Vec::new(),
        }
    }

    /// `side` の `path` の子の一覧。`path` かその祖先を起点にした走査から答え、なければ走査する
    pub(crate) fn list(
        &mut self,
        core: &mut CoreRuntime,
        side: &Side,
        path: &str,
    ) -> anyhow::Result<ChildListing> {
        let path = normalized(path);
        let found = self
            .scans
            .iter()
            .position(|scan| &scan.side == side && is_within(&path, &scan.root));
        let index = match found {
            Some(index) => index,
            None => {
                let (result, incomplete) = scan_below(core, side, &path, self.max_entries)?;
                self.scans.push(Scanned {
                    side: side.clone(),
                    root: path.clone(),
                    result,
                    incomplete,
                    noticed: false,
                });
                self.scans.len() - 1
            }
        };
        Ok(match &self.scans[index].result {
            Err(reason) => ChildListing::Unlistable(reason.clone()),
            Ok(tree) => children_in(tree, &path),
        })
    }

    /// まだ出していない、走査の打ち切り（REQ-cli-084）と読めない名前（REQ-scan-012）
    pub(crate) fn take_notices(&mut self) -> Vec<DiffError> {
        let mut notices = Vec::new();
        for scan in self.scans.iter_mut().filter(|scan| !scan.noticed) {
            scan.noticed = true;
            notices.extend(scan.incomplete.take());
            let Ok(tree) = &scan.result else { continue };
            if tree.truncated {
                notices.push(DiffError {
                    path: scan.root.clone(),
                    reason: ScanTruncated {
                        max_entries: self.max_entries,
                    }
                    .to_string(),
                });
            }
            let names = requested_unreadable_names(
                std::slice::from_ref(&scan.root),
                &[(scan.side.display_name(), tree)],
            );
            notices.extend(
                names
                    .into_iter()
                    .map(|(path, reason)| DiffError { path, reason }),
            );
        }
        notices
    }
}

/// 走査の結果と、循環で止めたときのその誤り
type ScanOfPath = (Result<FileTree, String>, Option<DiffError>);

/// `side` の `path` の下を走査する。配下を読めない・辿れないリンク・`接続断` は一覧を取れなかった
/// 理由にし（C12）、循環は読めたところまでの木と循環の誤りにし（REQ-cli-021）、途中がディレクトリ
/// でないパスは中身のないものにする。root_dir がないときと、リンク先や更新時刻を読めないとき
/// （REQ-scan-015）は diff を止める
fn scan_below(
    core: &mut CoreRuntime,
    side: &Side,
    path: &str,
    max_entries: usize,
) -> anyhow::Result<ScanOfPath> {
    let error = match core.fetch_tree_below(side, &[path.to_string()], max_entries, false) {
        Ok(tree) => return Ok((Ok(tree), None)),
        Err(error) => error,
    };
    if classify(&error).is_disconnection() {
        return Ok((Err(format!("{error:#}")), None));
    }
    let error = match error.downcast::<PartialScan>() {
        Ok(partial) => {
            let reason = partial.failure.to_string();
            let path = match &partial.failure {
                ScanFailure::Cycle { path } => path.clone(),
                _ => path.to_string(),
            };
            return Ok((Ok(partial.tree), Some(DiffError { path, reason })));
        }
        Err(error) => error,
    };
    match error.downcast_ref::<ScanFailure>() {
        Some(ScanFailure::StartBlocked { .. }) => Ok((Ok(FileTree::default()), None)),
        Some(
            ScanFailure::UnreadableDirectory { .. }
            | ScanFailure::StartUnreadable { .. }
            | ScanFailure::UnresolvableLink { .. },
        ) => Ok((Err(error.to_string()), None)),
        _ => Err(error),
    }
}

/// 木の `path` の直下の子。`path` がディレクトリかディレクトリへのリンクでなければ、ディレクトリ
/// ではないもの
fn children_in(tree: &FileTree, path: &str) -> ChildListing {
    match tree.find_node(path) {
        Some(node) if node.is_dir() || (node.is_symlink() && node.link_is_dir) => {
            let children: Vec<FileNode> = node
                .children
                .as_ref()
                .map(|children| children.values().cloned().collect())
                .unwrap_or_default();
            ChildListing::Children(children)
        }
        _ => ChildListing::NotDirectory,
    }
}

fn normalized(path: &str) -> String {
    path.split('/')
        .filter(|element| !element.is_empty() && *element != ".")
        .collect::<Vec<_>>()
        .join("/")
}

/// `path` が `root` そのものか、その下にあるか
fn is_within(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
#[path = "diff_expansion_tests.rs"]
mod tests;
