//! 比べた結果から status の一覧・errors・参照先の集計を作る（docs/ir/cli/status.md の TBL-cli-001、
//! status-targets.md の REQ-cli-035、unreadable.md の REQ-cli-088）

use std::collections::BTreeMap;

use remote_merge_core::compare::{PathComparison, Role, SideState, Verdict};

pub use super::status::RefCounts;
use super::status::{RefMark, StatusEntry, ref_counts, ref_mark, status_entry};
use crate::service::types::{FileStatus, UnreadableFile};

/// status の一覧
#[derive(Debug, Clone)]
pub struct StatusListing {
    /// 一覧に載せるファイル（--all で絞る前のすべて）
    pub files: Vec<FileStatus>,
    /// 判定できなかったパスと理由
    pub errors: Vec<UnreadableFile>,
    /// 参照先を使うときの集計
    pub ref_counts: Option<RefCounts>,
}

/// パスごとの比べた結果を一覧にする。`with_ref` は参照先を使ったか
pub fn status_listing(paths: &BTreeMap<String, PathComparison>, with_ref: bool) -> StatusListing {
    let mut files = Vec::new();
    let mut errors = Vec::new();
    let mut marks = Vec::new();
    let mut only_in_ref = 0;
    for (path, comparison) in paths {
        match status_entry(comparison) {
            StatusEntry::Listed(status) => {
                let mark = with_ref.then(|| ref_mark(comparison));
                let ref_badge = match &mark {
                    Some(RefMark::Badge(badge)) => Some(badge.as_str().to_string()),
                    Some(RefMark::Unreadable(reason)) => {
                        errors.push(UnreadableFile {
                            path: path.clone(),
                            reason: reason.clone(),
                        });
                        None
                    }
                    None => None,
                };
                marks.extend(mark);
                files.push(FileStatus {
                    path: path.clone(),
                    status,
                    hunks: None,
                    ref_badge,
                });
            }
            StatusEntry::Unreadable(reason) => errors.push(UnreadableFile {
                path: path.clone(),
                reason,
            }),
            StatusEntry::NotListed => {
                if with_ref && only_in_the_ref(comparison) {
                    only_in_ref += 1;
                }
            }
        }
    }
    StatusListing {
        files,
        errors,
        ref_counts: with_ref.then(|| ref_counts(&marks, only_in_ref)),
    }
}

/// 左右になく参照先にだけあるディレクトリでない項目か
fn only_in_the_ref(comparison: &PathComparison) -> bool {
    let both_absent = comparison
        .pair(Role::Left, Role::Right)
        .is_some_and(|pair| pair.verdict == Verdict::BothAbsent);
    let in_ref = comparison
        .states
        .states
        .get(&Role::Ref)
        .is_some_and(|state| !matches!(state, SideState::Absent | SideState::Directory));
    both_absent && in_ref
}

#[cfg(test)]
#[path = "status_listing_tests.rs"]
mod tests;
