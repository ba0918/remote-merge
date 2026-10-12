//! SSH の経路の中身とハッシュ。複数のパスをまとめて尋ね、パスごとに値か失敗の種類を返す
//! （`remote_merge_ssh::contents`）。ハッシュを取る道具がないサーバでは中身を読んで手元で取る

use remote_merge_core::machine_ops::sha256_hex;
use remote_merge_core::op_failure::{
    Disconnection, OpError, OpFailure, Operation, PathResult, SizeLimit,
};
use remote_merge_core::relative_path::EntryPath;
use remote_merge_ssh::command_limit::MAX_COMMAND_LEN;
use remote_merge_ssh::contents::{hash_all, read_all, ContentsFailure};

use super::core::CoreRuntime;
use super::route_state::require_ssh_client;

/// 結果のキー（入力されたままのパス）と、リモートのパス（root_dir につないだもの）
fn asked(root: &str, paths: &[EntryPath]) -> Vec<(String, String)> {
    let root = root.trim_end_matches('/');
    paths
        .iter()
        .map(|path| {
            let full = if root.is_empty() {
                path.as_str().to_string()
            } else {
                format!("{root}/{}", path.as_str())
            };
            (path.as_typed().to_string(), full)
        })
        .collect()
}

/// まとめて尋ねられなかった理由を誤りにする。答えを読めなければ接続断
fn failed(
    failure: ContentsFailure<anyhow::Error>,
    operation: Operation,
    first: &str,
) -> anyhow::Error {
    match failure {
        ContentsFailure::Run(error) => error,
        ContentsFailure::Unreadable(detail) => OpError::new(
            operation,
            first,
            OpFailure::Disconnected(Disconnection::NoAnswer(detail)),
        )
        .into(),
    }
}

impl CoreRuntime {
    /// SSH のスクリプトを動かし、0 で終わったときの標準出力を返す
    fn run_ssh_script(&mut self, server: &str, script: &str) -> anyhow::Result<Vec<u8>> {
        let client = require_ssh_client(&mut self.ssh_clients, server)?;
        let (stdout, stderr, exit) = self.rt.block_on(client.run_bytes(script))?;
        if exit != 0 {
            anyhow::bail!(
                "the remote command failed on {server}: exit={exit}, stderr={}",
                String::from_utf8_lossy(&stderr).trim_end()
            );
        }
        Ok(stdout)
    }

    pub(crate) fn ssh_read_entries(
        &mut self,
        server: &str,
        paths: &[EntryPath],
        limit: SizeLimit,
    ) -> anyhow::Result<Vec<PathResult<Vec<u8>>>> {
        let root = self
            .get_server_config(server)?
            .root_dir
            .to_string_lossy()
            .into_owned();
        let asked = asked(&root, paths);
        let limit = match limit {
            SizeLimit::Bytes(bytes) => Some(bytes),
            SizeLimit::Unlimited => None,
        };
        let first = asked.first().map_or(String::new(), |(key, _)| key.clone());
        read_all(&asked, limit, MAX_COMMAND_LEN, |script| {
            self.run_ssh_script(server, script)
        })
        .map_err(|failure| failed(failure, Operation::Read, &first))
    }

    pub(crate) fn ssh_hash_entries(
        &mut self,
        server: &str,
        paths: &[EntryPath],
    ) -> anyhow::Result<Vec<PathResult<String>>> {
        let root = self
            .get_server_config(server)?
            .root_dir
            .to_string_lossy()
            .into_owned();
        let asked = asked(&root, paths);
        let first = asked.first().map_or(String::new(), |(key, _)| key.clone());
        let hashes = hash_all(&asked, MAX_COMMAND_LEN, |script| {
            self.run_ssh_script(server, script)
        })
        .map_err(|failure| failed(failure, Operation::Hash, &first))?;
        // ハッシュを取る道具のないサーバでは、その通常ファイルの中身を読んで手元で取る
        let unhashed: Vec<EntryPath> = paths
            .iter()
            .zip(&hashes)
            .filter(|(_, (_, hash))| matches!(hash, Ok(None)))
            .map(|(path, _)| path.clone())
            .collect();
        let mut digests = std::collections::HashMap::new();
        if !unhashed.is_empty() {
            tracing::info!("No sha256sum or shasum on {server}; hashing the contents read instead");
            for (path, content) in self.ssh_read_entries(server, &unhashed, SizeLimit::Unlimited)? {
                digests.insert(path, content.map(|bytes| sha256_hex(&bytes)));
            }
        }
        Ok(hashes
            .into_iter()
            .map(|(path, hash)| {
                let value = match hash {
                    Ok(Some(value)) => Ok(value),
                    Ok(None) => digests.remove(&path).expect("every unhashed path was read"),
                    Err(failure) => Err(failure),
                };
                (path, value)
            })
            .collect())
    }
}
