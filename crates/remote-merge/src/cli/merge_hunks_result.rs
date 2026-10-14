//! --hunks の merge の結果の形（JSON の形はファイル全体の merge と同じ MergeOutput）

use super::merge::{MergeCommandOutput, MergeCommandResult};
use crate::service::output::NO_CHANGES;
use crate::service::types::{exit_code, MergeFailure, MergeFileResult, MergeOutput};

/// 書いたか書く予定のファイル。書き込み先への処理を止めていたら（`stopped`）、済んだファイルが
/// あっても終了コード 2 にする（docs/ir/ssh/reconnect.md の REQ-ssh-022）
pub(super) fn written(result: MergeFileResult, stopped: bool) -> MergeCommandResult {
    MergeCommandResult {
        output: MergeCommandOutput::Files(MergeOutput {
            merged: vec![result],
            skipped: Vec::new(),
            deleted: Vec::new(),
            failed: Vec::new(),
            ref_: None,
        }),
        exit_code: if stopped {
            exit_code::ERROR
        } else {
            exit_code::SUCCESS
        },
    }
}

/// 差分のないファイルは書かない（REQ-merge-030。JSON の形は今のまま merged の status で示す）
pub(super) fn no_changes(path: &str) -> MergeCommandResult {
    written(
        MergeFileResult {
            path: path.to_string(),
            status: NO_CHANGES.to_string(),
            ..MergeFileResult::default()
        },
        false,
    )
}

pub(super) fn failed(path: &str, error: String) -> MergeCommandResult {
    MergeCommandResult {
        output: MergeCommandOutput::Files(MergeOutput {
            merged: Vec::new(),
            skipped: Vec::new(),
            deleted: Vec::new(),
            failed: vec![MergeFailure {
                path: path.to_string(),
                error,
            }],
            ref_: None,
        }),
        exit_code: exit_code::ERROR,
    }
}
