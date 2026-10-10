//! バックアップが無効な rollback で、書き戻す前の書き戻す先の状態をまとめて取る
//! （docs/ir/ssh/reconnect.md の REQ-ssh-022。決定 A40・A41）。
//!
//! 書き戻しの最中の `接続断` の後の確かめ（`confirm`）が「書く前」に使う。取ったものは保存しない。
//! エージェントでは HashFiles の要求一回、SSH ではまとめてハッシュを取るコマンド（コマンドの長さで
//! 分けることはある）で取り、SSH でハッシュのコマンドがないサーバでは中身をまとめて読む。

use std::collections::HashMap;

use crate::agent::protocol::FileHashResult;
use crate::service::rollback::{BackupPathRecord, CurrentRestorePath};
use crate::ssh::batch_hash::BatchHashes;

use super::change_check::Before;
use super::core::CoreRuntime;

/// 書き戻す前の状態を取るパスか。書き戻す先にある通常のファイルへ、中身を書き戻すときだけ
pub(crate) fn needs_state_before(record: &BackupPathRecord, current: &CurrentRestorePath) -> bool {
    matches!(
        (record, current),
        (
            BackupPathRecord::File { .. },
            CurrentRestorePath::Present { .. }
        )
    )
}

/// エージェントのハッシュの結果のうち、通常のファイルのハッシュ
fn hashes_before(results: Vec<FileHashResult>) -> HashMap<String, Before> {
    results
        .into_iter()
        .filter_map(|result| match result {
            FileHashResult::Ok { path, hash } => Some((path, Before::Hash(hash))),
            FileHashResult::Symlink { .. } | FileHashResult::Error { .. } => None,
        })
        .collect()
}

impl CoreRuntime {
    /// リモートの `paths` の、書き戻す前の状態。通常のファイルでないか読めなかったパスは入らない。
    /// 読み取りの `接続断` は同じ経路で一度だけやり直す
    pub(crate) fn remote_states_before_restore(
        &mut self,
        server_name: &str,
        paths: &[String],
    ) -> anyhow::Result<HashMap<String, Before>> {
        if paths.is_empty() {
            return Ok(HashMap::new());
        }
        if let Some(results) = self.try_agent_hash_files(server_name, paths) {
            return Ok(hashes_before(results?));
        }
        self.check_sudo_fallback(server_name)?;
        match self.ssh_read(server_name, |rt| {
            rt.hash_remote_files_batch(server_name, paths)
        })? {
            BatchHashes::Hashes(hashes) => Ok(hashes
                .into_iter()
                .map(|(path, hash)| (path, Before::Hash(hash)))
                .collect()),
            BatchHashes::Unsupported => {
                tracing::info!(
                    "No sha256sum or shasum on {}; reading the files to restore instead",
                    server_name
                );
                let contents = self.ssh_read(server_name, |rt| {
                    rt.read_remote_files_batch_bytes(server_name, paths)
                })?;
                Ok(contents
                    .into_iter()
                    .map(|(path, content)| (path, Before::Content(content)))
                    .collect())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn only_a_file_restored_over_a_present_entry_needs_its_state_before() {
        let file = BackupPathRecord::File {
            real_path: PathBuf::from("/r/a"),
        };
        let present = CurrentRestorePath::Present {
            real_path: PathBuf::from("/r/a"),
        };
        let missing = CurrentRestorePath::Missing { real_parent: None };
        let link = CurrentRestorePath::Symlink {
            link_target: PathBuf::from("t"),
            real_parent: PathBuf::from("/r"),
        };
        let update = BackupPathRecord::SymlinkUpdate {
            expected_target: PathBuf::from("t"),
            real_parent: PathBuf::from("/r"),
        };
        assert!(needs_state_before(&file, &present));
        assert!(!needs_state_before(&file, &missing));
        assert!(!needs_state_before(&file, &link));
        assert!(!needs_state_before(&update, &present));
        assert!(!needs_state_before(&BackupPathRecord::Symlink, &present));
    }

    #[test]
    fn only_regular_file_hashes_from_the_agent_are_kept() {
        let results = vec![
            FileHashResult::Ok {
                path: "a".into(),
                hash: "h".into(),
            },
            FileHashResult::Symlink {
                path: "l".into(),
                target: "t".into(),
            },
            FileHashResult::Error {
                path: "e".into(),
                reason: "denied".into(),
            },
        ];
        assert_eq!(
            hashes_before(results),
            HashMap::from([("a".to_string(), Before::Hash("h".into()))])
        );
    }
}
