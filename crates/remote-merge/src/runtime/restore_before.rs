//! バックアップが無効な rollback で、書き戻す前の書き戻す先の状態をまとめて取る
//! （docs/ir/ssh/reconnect.md の REQ-ssh-022。決定 A40・A41）。
//!
//! 書き戻しの最中の `接続断` の後の確かめ（`confirm`）が「書く前」に使う。取ったものは保存しない。
//! 選んだ経路でまとめてハッシュを取り、ハッシュの道具がないサーバでは中身をまとめて読む。

use remote_merge_core::relative_path::EntryPath;
use std::collections::HashMap;

use super::change_check::Before;
use super::core::CoreRuntime;

impl CoreRuntime {
    /// リモートの `paths` の、書き戻す前の状態（書き戻す先にある通常のファイルだけを尋ねる）。
    /// ハッシュを取れなかったパスは入らず、確かめでは「書く前を持っていない」になる。ハッシュの
    /// 道具がないサーバでは中身を読んで取る。読み取りの `接続断` は同じ経路で一度だけやり直す
    pub(crate) fn remote_states_before_restore(
        &mut self,
        server_name: &str,
        paths: &[EntryPath],
    ) -> anyhow::Result<HashMap<String, Before>> {
        if paths.is_empty() {
            return Ok(HashMap::new());
        }
        let hashes =
            self.read_on_route(server_name, |rt| rt.remote_hash_entries(server_name, paths))?;
        Ok(hashes
            .into_iter()
            .filter_map(|(path, hash)| hash.ok().map(|hash| (path, Before::Hash(hash))))
            .collect())
    }
}
