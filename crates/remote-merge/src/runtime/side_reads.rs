//! Side ごとの中身の読み込み。どの経路もパスごとに中身か失敗の種類を返す

use remote_merge_core::op_failure::{OpError, Operation};
use remote_merge_core::relative_path::EntryPath;

use super::connection::label_read;
use super::core::CoreRuntime;
use super::size_limit::SizeLimit;
use crate::side::Side;

/// 一つのパスの読み込みの結果。失敗は操作とパスと種類を持つ誤り
pub type ReadResult = (String, anyhow::Result<Vec<u8>>);

impl CoreRuntime {
    /// `rel_paths` の中身をまとめて読む。上限を当てるときは、上限を超えるファイルを読まずに上限を
    /// 超える失敗にする。尋ねる操作そのものが失敗したら（やり直しても接続断など）Err
    pub fn read_files(
        &mut self,
        side: &Side,
        rel_paths: &[String],
        limit: SizeLimit,
    ) -> anyhow::Result<Vec<ReadResult>> {
        let entries = rel_paths
            .iter()
            .map(|path| EntryPath::parse(path))
            .collect::<Result<Vec<_>, _>>()?;
        let mut io = super::target_io::for_side(side, self);
        let results = io
            .read_entries(self, &entries, limit.bytes())
            .map_err(label_read)?;
        Ok(results
            .into_iter()
            .map(|(path, content)| {
                let content = content.map_err(|failure| {
                    anyhow::Error::new(OpError::new(Operation::Read, path.as_str(), failure))
                });
                (path, content)
            })
            .collect())
    }

    /// Side に基づいてバイト列を読み込む（バイナリファイル対応）。`force` が false なら上限を
    /// 超えるファイルは読まずに上限を超える失敗にする
    pub fn read_file_bytes(
        &mut self,
        side: &Side,
        rel_path: &str,
        force: bool,
    ) -> anyhow::Result<Vec<u8>> {
        let limit = SizeLimit::from_force(force);
        let mut results = self.read_files(side, &[rel_path.to_string()], limit)?;
        let (_, content) = results.pop().expect("every route answers each path asked");
        content
    }
}
