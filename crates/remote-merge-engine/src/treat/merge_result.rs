//! merge と sync の一つの書き込み先の結果を、出力の merged・skipped・deleted・failed に詰める
//! （docs/ir/cli/merge.md の REQ-cli-048、docs/ir/merge/deletion.md の REQ-merge-024・026・034）

use crate::service::types::{
    DeleteFileResult, DeleteStatus, MergeFailure, MergeFileResult, MergeSkipped,
};

/// 一つのパスの結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// 書いた（バックアップがあればその場所）
    Merged {
        backup: Option<String>,
        ref_badge: Option<String>,
    },
    /// --dry-run で書く予定
    WouldMerge {
        ref_badge: Option<String>,
    },
    Deleted {
        backup: Option<String>,
    },
    /// --dry-run で消す予定
    WouldDelete,
    /// スキップした理由
    Skipped(String),
    /// 書かず消さなかった理由
    Failed(String),
    /// 書き込み先を変えた後で失敗した理由（中身は書いたが権限を変えられなかった、symlink を置き換える
    /// ために古いリンクを消した後に作れなかった）。failed に出す
    FailedAfterChange(String),
}

/// 一つの書き込み先の結果の欄
#[derive(Debug, Clone, Default)]
pub struct TargetLists {
    pub merged: Vec<MergeFileResult>,
    pub skipped: Vec<MergeSkipped>,
    pub deleted: Vec<DeleteFileResult>,
    pub failed: Vec<MergeFailure>,
    /// failed のどれかが、書き込み先を変えた後で失敗したものか（sync の status。TBL-cli-007）
    pub changed_but_failed: bool,
}

/// パスの順の結果を欄に分ける
pub fn target_lists(mut outcomes: Vec<(String, Outcome)>) -> TargetLists {
    outcomes.sort_by(|a, b| a.0.cmp(&b.0));
    let mut lists = TargetLists::default();
    for (path, outcome) in outcomes {
        match outcome {
            Outcome::Merged { backup, ref_badge } => lists.merged.push(MergeFileResult {
                path,
                status: "ok".to_string(),
                backup,
                ref_badge,
                hunk_info: None,
            }),
            Outcome::WouldMerge { ref_badge } => lists.merged.push(MergeFileResult {
                path,
                status: "would merge".to_string(),
                backup: None,
                ref_badge,
                hunk_info: None,
            }),
            Outcome::Deleted { backup } => lists.deleted.push(DeleteFileResult {
                path,
                status: DeleteStatus::Ok,
                backup,
            }),
            Outcome::WouldDelete => lists.deleted.push(DeleteFileResult {
                path,
                status: DeleteStatus::WouldDelete,
                backup: None,
            }),
            Outcome::Skipped(reason) => lists.skipped.push(MergeSkipped { path, reason }),
            Outcome::Failed(error) => lists.failed.push(MergeFailure { path, error }),
            Outcome::FailedAfterChange(error) => {
                lists.changed_but_failed = true;
                lists.failed.push(MergeFailure { path, error });
            }
        }
    }
    lists
}

#[cfg(test)]
#[path = "merge_result_tests.rs"]
mod tests;
