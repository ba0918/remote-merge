//! エージェントの経路の読み取り（中身・大きさ・更新時刻・ハッシュ・項目のリンク先）。
//! エージェントが答えた失敗の種類は、操作とパスを添えた誤りにする

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use remote_merge_core::op_failure::{failure_of, OpError, OpFailure, Operation};
use remote_merge_core::relative_path::EntryPath;

use super::{with_agent, AGENT_CHUNK_SIZE_LIMIT, AGENT_READ_BATCH_SIZE};
use crate::agent::protocol::StatResult;
use crate::runtime::core::CoreRuntime;
use crate::runtime::size_limit::FileSize;

fn failed(operation: Operation, path: &str, failure: OpFailure) -> anyhow::Error {
    OpError::new(operation, path, failure).into()
}

/// 中身をまとめて読む。読めなかったパスがあれば誤り
pub(crate) fn read(
    rt: &mut CoreRuntime,
    server_name: &str,
    paths: &[EntryPath],
) -> anyhow::Result<HashMap<String, Vec<u8>>> {
    let mut contents = HashMap::new();
    for chunk in paths.chunks(AGENT_READ_BATCH_SIZE) {
        let results = with_agent(rt, server_name, |agent| {
            agent.read_files(chunk, AGENT_CHUNK_SIZE_LIMIT, None)
        })?;
        for (path, result) in results {
            let content = result.map_err(|failure| failed(Operation::Read, &path, failure))?;
            contents.insert(path, content);
        }
    }
    Ok(contents)
}

pub(crate) fn read_one(
    rt: &mut CoreRuntime,
    server_name: &str,
    path: &EntryPath,
) -> anyhow::Result<Vec<u8>> {
    let mut contents = read(rt, server_name, std::slice::from_ref(path))?;
    Ok(contents
        .remove(path.as_typed())
        .expect("the client checks that every path asked is answered"))
}

/// 一度に送るパスの数の上限ごとに分けて尋ねる。どれかの回が失敗したら全体の失敗にする
fn stat_in_batches(
    rt: &mut CoreRuntime,
    server_name: &str,
    paths: &[EntryPath],
) -> anyhow::Result<Vec<StatResult>> {
    let mut stats = Vec::new();
    for batch in paths.chunks(AGENT_READ_BATCH_SIZE) {
        stats.extend(with_agent(rt, server_name, |agent| {
            agent.stat_files(batch)
        })?);
    }
    Ok(stats)
}

/// パスごとの大きさ。パスごとの失敗は分からない大きさにし、読み込みでその失敗を受け取る
pub(crate) fn sizes(
    rt: &mut CoreRuntime,
    server_name: &str,
    paths: &[EntryPath],
) -> anyhow::Result<HashMap<String, FileSize>> {
    Ok(stat_in_batches(rt, server_name, paths)?
        .into_iter()
        .map(|result| match result {
            StatResult::Ok(stat) => (stat.path, FileSize::Known(stat.size)),
            StatResult::Failed { path, .. } => (path, FileSize::Unknown),
        })
        .collect())
}

/// パスごとの更新時刻。ないパスは None、そのほかの失敗は誤り
pub(crate) fn stat(
    rt: &mut CoreRuntime,
    server_name: &str,
    paths: &[EntryPath],
) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>> {
    stat_in_batches(rt, server_name, paths)?
        .into_iter()
        .map(|result| match result {
            StatResult::Ok(stat) => {
                let modified = DateTime::from_timestamp(stat.mtime_secs, stat.mtime_nanos);
                Ok((stat.path, modified))
            }
            StatResult::Failed { path, failure } => match OpFailure::from(failure) {
                OpFailure::NotFound => Ok((path, None)),
                failure => Err(failed(Operation::Stat, &path, failure)),
            },
        })
        .collect()
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

/// パスごとの比べる値（symlink はリンク先）
pub(crate) fn hashes(
    rt: &mut CoreRuntime,
    server_name: &str,
    paths: &[EntryPath],
) -> anyhow::Result<HashMap<String, String>> {
    let results = with_agent(rt, server_name, |agent| agent.hash_files(paths))?;
    Ok(crate::runtime::side_io::hash_results_to_map(&results))
}
