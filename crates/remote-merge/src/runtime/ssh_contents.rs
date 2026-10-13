//! SSH の経路の中身とハッシュ。複数のパスをまとめて尋ね、パスごとに値か失敗の種類を返す
//! （`remote_merge_ssh::contents`）。ハッシュを取る道具がないサーバでは、中身を一つずつ少しずつ受けて
//! 手元で取る（REQ-cli-095）

use remote_merge_core::op_failure::{
    Disconnection, OpError, OpFailure, Operation, PathResult, SizeLimit,
};
use remote_merge_core::relative_path::EntryPath;
use remote_merge_ssh::command_limit::MAX_COMMAND_LEN;
use remote_merge_ssh::contents::{digest_all, fill_unhashed, hash_all, read_all, ContentsFailure};
use remote_merge_ssh::contents_stream::RawDigests;

use super::core::CoreRuntime;
use super::route_state::require_ssh_client;

/// 結果のキー（入力されたままのパス）と、リモートのパス（root_dir につないだもの）
fn asked(root: &str, paths: &[EntryPath]) -> Vec<(String, String)> {
    paths
        .iter()
        .map(|path| {
            let full = crate::merge::executor::remote_full_path(root, path.as_rel());
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
        read_all(
            &asked,
            limit,
            &fresh_boundary(),
            MAX_COMMAND_LEN,
            |script| self.run_ssh_script(server, script),
        )
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
        // ハッシュを取る道具のないサーバでは、その通常ファイルの中身を少しずつ受けて手元で取る
        if hashes.iter().any(|(_, hash)| matches!(hash, Ok(None))) {
            tracing::info!("No sha256sum or shasum on {server}; hashing the contents read instead");
        }
        let first = first.clone();
        fill_unhashed(&asked, hashes, |fulls| {
            let boundary = fresh_boundary();
            digest_all(fulls, &boundary, MAX_COMMAND_LEN, |script, digests| {
                self.run_ssh_streaming(server, script, digests)
            })
            .map_err(|failure| failed(failure, Operation::Hash, &first))
        })
    }

    /// 中身をそのまま出すスクリプトを動かし、標準出力を受けた順に `digests` へ渡す
    fn run_ssh_streaming(
        &mut self,
        server: &str,
        script: &str,
        digests: &mut RawDigests,
    ) -> anyhow::Result<()> {
        let client = require_ssh_client(&mut self.ssh_clients, server)?;
        let (stderr, exit) = self
            .rt
            .block_on(client.run_streaming(script, &mut |piece: &[u8]| digests.feed(piece)))?;
        if exit != 0 {
            anyhow::bail!(
                "the remote command failed on {server}: exit={exit}, stderr={}",
                String::from_utf8_lossy(&stderr).trim_end()
            );
        }
        Ok(())
    }
}

/// 中身の終わりを見分ける区切り。読むファイルの中身から推し量れないよう、問いごとに作る
fn fresh_boundary() -> String {
    use std::hash::{BuildHasher, Hasher};
    let state = std::collections::hash_map::RandomState::new();
    let half = |salt: u64| {
        let mut hasher = state.build_hasher();
        hasher.write_u64(salt);
        hasher.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos()),
        );
        hasher.finish()
    };
    format!("{:016x}{:016x}", half(1), half(2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_root_dir_of_slash_asks_absolute_paths() {
        let paths = [EntryPath::parse("a/b.txt").unwrap()];
        assert_eq!(
            asked("/", &paths),
            [("a/b.txt".to_string(), "/a/b.txt".to_string())]
        );
        assert_eq!(
            asked("/srv/app/", &paths),
            [("a/b.txt".to_string(), "/srv/app/a/b.txt".to_string())]
        );
    }
}
