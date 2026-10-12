//! エージェントの経路の読み取り（中身・大きさ・更新時刻・ハッシュ・項目のリンク先）

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use remote_merge_core::relative_path::EntryPath;

use super::{with_agent, AGENT_CHUNK_SIZE_LIMIT, AGENT_READ_BATCH_SIZE};
use crate::agent::client::AgentRefused;
use crate::agent::protocol::{AgentFileStat, FileReadResult};
use crate::runtime::core::CoreRuntime;
use crate::runtime::size_limit::FileSize;

/// 分けて返った中身をパスごとにつなぐ。読めなかったパスがあれば、その失敗にする
fn joined(results: Vec<FileReadResult>) -> anyhow::Result<HashMap<String, Vec<u8>>> {
    let mut contents: HashMap<String, Vec<u8>> = HashMap::new();
    for result in results {
        match result {
            FileReadResult::Ok { path, content, .. } => {
                contents.entry(path).or_default().extend(content)
            }
            FileReadResult::Error { path, message } => {
                return Err(AgentRefused(format!("cannot read {path}: {message}")).into())
            }
        }
    }
    Ok(contents)
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
            agent.read_files(chunk, AGENT_CHUNK_SIZE_LIMIT)
        })?;
        contents.extend(joined(results)?);
    }
    Ok(contents)
}

pub(crate) fn read_one(
    rt: &mut CoreRuntime,
    server_name: &str,
    path: &EntryPath,
) -> anyhow::Result<Vec<u8>> {
    read(rt, server_name, std::slice::from_ref(path))?
        .remove(path.as_typed())
        .ok_or_else(|| {
            AgentRefused(format!(
                "the agent returned no content for {}",
                path.as_typed()
            ))
            .into()
        })
}

/// 一度に送るパスの数の上限ごとに分けて尋ねる。どれかの回が失敗したら全体の失敗にする
fn stat_in_batches(
    rt: &mut CoreRuntime,
    server_name: &str,
    paths: &[EntryPath],
) -> anyhow::Result<Vec<AgentFileStat>> {
    let mut stats = Vec::new();
    for batch in paths.chunks(AGENT_READ_BATCH_SIZE) {
        stats.extend(with_agent(rt, server_name, |agent| {
            agent.stat_files(batch)
        })?);
    }
    Ok(stats)
}

/// パスごとの大きさ。エージェントが答えなかったパスは、SSH で尋ね直さずに尋ねられなかったことにする
pub(crate) fn sizes(
    rt: &mut CoreRuntime,
    server_name: &str,
    paths: &[EntryPath],
) -> anyhow::Result<HashMap<String, FileSize>> {
    let stats = stat_in_batches(rt, server_name, paths)?;
    let known: HashMap<String, u64> = stats
        .into_iter()
        .map(|stat| (stat.path, stat.size))
        .collect();
    Ok(paths
        .iter()
        .map(|path| {
            let size = match known.get(path.as_typed()) {
                Some(size) => FileSize::Known(*size),
                None => FileSize::Unavailable(format!(
                    "the agent did not answer the size of {}",
                    path.as_typed()
                )),
            };
            (path.as_typed().to_string(), size)
        })
        .collect())
}

/// パスごとの更新時刻。答えのないパスは None
pub(crate) fn stat(
    rt: &mut CoreRuntime,
    server_name: &str,
    paths: &[EntryPath],
) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>> {
    let stats = stat_in_batches(rt, server_name, paths)?;
    let known: HashMap<String, Option<DateTime<Utc>>> = stats
        .into_iter()
        .map(|stat| {
            let modified = DateTime::from_timestamp(stat.mtime_secs, stat.mtime_nanos);
            (stat.path, modified)
        })
        .collect();
    Ok(paths
        .iter()
        .map(|path| {
            let modified = known.get(path.as_typed()).copied().flatten();
            (path.as_typed().to_string(), modified)
        })
        .collect())
}

pub(crate) fn entry_link(
    rt: &mut CoreRuntime,
    server_name: &str,
    path: &EntryPath,
) -> anyhow::Result<Option<String>> {
    with_agent(rt, server_name, |agent| agent.read_entry_link(path))
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
