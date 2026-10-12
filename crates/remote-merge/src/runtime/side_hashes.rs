//! status のハッシュでの比較。どの側も選んだ経路でハッシュを取る

use std::collections::HashMap;

use remote_merge_core::relative_path::EntryPath;

use super::core::CoreRuntime;
use crate::side::Side;

/// 左右のハッシュ（パス → 比べる値）
pub type SideHashes = (HashMap<String, String>, HashMap<String, String>);

impl CoreRuntime {
    /// `paths` の左右のハッシュ。どちらかの側にハッシュを取る道具がなければ None（中身で比べる）。
    /// `接続断` は中身の比べ方に移らずに Err を返す
    pub fn try_hash_compare(
        &mut self,
        left: &Side,
        right: &Side,
        paths: &[String],
    ) -> anyhow::Result<Option<SideHashes>> {
        let entries = paths
            .iter()
            .map(|path| EntryPath::parse(path))
            .collect::<Result<Vec<_>, _>>()?;
        let Some(left_hashes) = self.side_hashes(left, &entries)? else {
            return Ok(None);
        };
        let Some(right_hashes) = self.side_hashes(right, &entries)? else {
            return Ok(None);
        };
        Ok(Some((left_hashes, right_hashes)))
    }

    fn side_hashes(
        &mut self,
        side: &Side,
        entries: &[EntryPath],
    ) -> anyhow::Result<Option<HashMap<String, String>>> {
        if let Some(stopped) = self.stopped_error_for(side) {
            return Err(stopped);
        }
        let mut io = super::target_io::for_side(side, self);
        io.hashes(self, entries)
    }
}
