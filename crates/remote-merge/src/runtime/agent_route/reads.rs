//! エージェントの経路の読み取り（中身・ハッシュ・項目のリンク先）。
//! エージェントが答えた失敗の種類は、操作とパスを添えた誤りにする

use remote_merge_core::op_failure::{failure_of, OpFailure, PathResult, SizeLimit};
use remote_merge_core::relative_path::EntryPath;

use super::{with_agent, AGENT_CHUNK_SIZE_LIMIT, AGENT_READ_BATCH_SIZE};
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
    let mut answers = Vec::with_capacity(paths.len());
    for chunk in paths.chunks(AGENT_READ_BATCH_SIZE) {
        answers.extend(with_agent(rt, server_name, |agent| {
            agent.read_files(chunk, AGENT_CHUNK_SIZE_LIMIT, size_limit)
        })?);
    }
    Ok(answers)
}

/// 項目そのものが symlink ならそのリンク先。symlink でないか、ないか、途中がディレクトリでなければ None
pub(crate) fn entry_link(
    rt: &mut CoreRuntime,
    server_name: &str,
    path: &EntryPath,
) -> anyhow::Result<Option<String>> {
    match with_agent(rt, server_name, |agent| agent.read_entry_link(path)) {
        Err(error)
            if matches!(
                failure_of(&error),
                Some(OpFailure::NotFound | OpFailure::NotADirectory)
            ) =>
        {
            Ok(None)
        }
        other => other,
    }
}

/// パスごとの比べる値（symlink はリンク先、それ以外は中身の SHA-256）か失敗の種類
pub(crate) fn hash_entries(
    rt: &mut CoreRuntime,
    server_name: &str,
    paths: &[EntryPath],
) -> anyhow::Result<Vec<PathResult<String>>> {
    let mut answers = Vec::with_capacity(paths.len());
    for chunk in paths.chunks(AGENT_READ_BATCH_SIZE) {
        let results = with_agent(rt, server_name, |agent| agent.hash_files(chunk))?;
        answers.extend(results.into_iter().map(|result| match result {
            FileHashResult::Ok { path, hash } => (path, Ok(hash)),
            FileHashResult::Symlink { path, target } => (path, Ok(target)),
            FileHashResult::Failed { path, failure } => (path, Err(OpFailure::from(failure))),
        }));
    }
    Ok(answers)
}
