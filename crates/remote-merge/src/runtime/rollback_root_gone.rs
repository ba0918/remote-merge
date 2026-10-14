//! rollback の書き戻す前のまとめた問いで、戻す先の root_dir がなくなったと分かったときの結果
//! （docs/ir/merge/concurrency.md の REQ-merge-044）

use std::collections::{BTreeMap, HashMap};

use remote_merge_core::compare::SideState;

use super::core::CoreRuntime;
use super::rollback_apply::{RestoreOutcome, Restoring};
use crate::side::Side;

impl CoreRuntime {
    /// 書き戻す前のまとめた問い（退避のための読み込みか、書き戻す前のハッシュ）で root_dir がなくなった
    /// と分かってその側を止めたときの、書き戻すパスの結果。merge・sync と同じく、そのまとめの最初の
    /// ファイルにだけ root_dir がない error を出し、残りは手を付けていない error にする（REQ-merge-044）
    pub(super) fn root_gone_before_restore<T, U>(
        &self,
        target: &Side,
        restoring: &BTreeMap<String, Restoring>,
        contents: &Result<T, String>,
        befores: &Result<U, String>,
    ) -> Option<HashMap<String, RestoreOutcome>> {
        let prefix = match (contents, befores) {
            (Err(_), _) => "backup failed: read failed: ",
            (_, Err(_)) => "read failed: target: ",
            _ => return None,
        };
        let paths: Vec<&str> = restoring.keys().map(String::as_str).collect();
        let found = restoring
            .iter()
            .find(|(_, restore)| matches!(restore.state, SideState::File { .. }))
            .map(|(path, _)| path.as_str())?;
        let errors = self.root_gone_before_changes(target, &paths, found, prefix)?;
        Some(
            paths
                .into_iter()
                .zip(errors)
                .map(|(path, error)| (path.to_string(), RestoreOutcome::Failed(error)))
                .collect(),
        )
    }
}
