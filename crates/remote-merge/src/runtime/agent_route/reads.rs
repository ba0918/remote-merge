//! エージェントの経路の読み取り（中身・ハッシュ・項目のリンク先）。
//! エージェントが答えた失敗の種類は、操作とパスを添えた誤りにする

use remote_merge_core::op_failure::{Disconnection, OpFailure, PathResult, SizeLimit};
use remote_merge_core::relative_path::EntryPath;

use super::{AGENT_CHUNK_SIZE_LIMIT, in_batches};
use crate::agent::protocol::FileHashResult;
use crate::runtime::core::CoreRuntime;

/// パスごとの中身か失敗の種類。上限を超えるファイルはエージェントが中身を送らない
pub(crate) fn read_entries(
    rt: &mut CoreRuntime,
    server_name: &str,
    paths: &[EntryPath],
    limit: SizeLimit,
) -> anyhow::Result<Vec<PathResult<Vec<u8>>>> {
    let size_limit = match limit {
        SizeLimit::Bytes(bytes) => Some(bytes),
        SizeLimit::Unlimited => None,
    };
    in_batches(rt, server_name, paths, |agent, chunk| {
        agent.read_files(chunk, AGENT_CHUNK_SIZE_LIMIT, size_limit)
    })
}

/// パスごとの中身の SHA-256（symlink も辿った先の中身）か失敗の種類。リンク文字列の答えは問いと
/// 合わないので接続断にする
pub(crate) fn hash_contents(
    rt: &mut CoreRuntime,
    server_name: &str,
    paths: &[EntryPath],
) -> anyhow::Result<Vec<PathResult<String>>> {
    let results = in_batches(rt, server_name, paths, |agent, chunk| {
        agent.hash_contents(chunk)
    })?;
    Ok(results
        .into_iter()
        .map(|result| match result {
            FileHashResult::Ok { path, hash } => (path, Ok(hash)),
            FileHashResult::Failed { path, failure } => (path, Err(OpFailure::from(failure))),
            FileHashResult::Symlink { path, .. } => (
                path,
                Err(OpFailure::Disconnected(Disconnection::NoAnswer(
                    "the agent answered a link to a content hash".into(),
                ))),
            ),
        })
        .collect())
}

/// パスごとの比べる値（symlink はリンク先、それ以外は中身の SHA-256）か失敗の種類
pub(crate) fn hash_entries(
    rt: &mut CoreRuntime,
    server_name: &str,
    paths: &[EntryPath],
) -> anyhow::Result<Vec<PathResult<String>>> {
    let results = in_batches(rt, server_name, paths, |agent, chunk| {
        agent.hash_files(chunk)
    })?;
    Ok(results
        .into_iter()
        .map(|result| match result {
            FileHashResult::Ok { path, hash } => (path, Ok(hash)),
            FileHashResult::Symlink { path, target } => (path, Ok(target)),
            FileHashResult::Failed { path, failure } => (path, Err(OpFailure::from(failure))),
        })
        .collect())
}
