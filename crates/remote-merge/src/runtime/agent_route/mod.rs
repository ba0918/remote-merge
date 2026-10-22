//! エージェントの経路の操作。一度だけ行い、やり直しは呼び出し側（`route_retry`）が決める。
//! エージェントが失敗を返しても SSH の経路には回さない（REQ-ssh-010）

mod listing;
mod reads;

pub(crate) use listing::{list_children, scan_listing};
pub(crate) use reads::{hash_contents, hash_entries, read_entries};

use remote_merge_core::op_failure::{Disconnection, OpError, OpFailure, Operation};
use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::relative_path::{EntryPath, RelPath};

use super::core::{BoxedAgentClient, CoreRuntime};

/// エージェントへの一つの要求で送るパスの数の上限（読み込み・ハッシュ・パスの確認）
pub(crate) const AGENT_READ_BATCH_SIZE: usize = 2000;
/// エージェントが中身を分けて返す大きさ（4 MB）。フレームの上限（16 MB）を超えないようにする
pub(crate) const AGENT_CHUNK_SIZE_LIMIT: usize = 4 * 1024 * 1024;

/// 選んだエージェントで一度だけ行う。エージェントが動いていなければ、要求を送れなかった接続断
pub(crate) fn with_agent<T>(
    rt: &mut CoreRuntime,
    server_name: &str,
    operation: impl FnOnce(&mut BoxedAgentClient) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let Some(agent) = rt.agent_clients.get_mut(server_name) else {
        return Err(not_running(server_name));
    };
    operation(&mut agent.client)
}

fn not_running(server_name: &str) -> anyhow::Error {
    OpError::new(
        Operation::Read,
        server_name,
        OpFailure::Disconnected(Disconnection::NotSent(format!(
            "the agent for {server_name} is not running"
        ))),
    )
    .into()
}

pub(crate) fn inspect(
    rt: &mut CoreRuntime,
    server_name: &str,
    path: &RelPath,
) -> anyhow::Result<ResolvedPath> {
    let inspection = with_agent(rt, server_name, |agent| agent.inspect_path(path))?;
    crate::agent::inspection::resolved_from_agent(inspection)
}

/// `paths` をまとめて確かめる。一つの要求で送るのは [`AGENT_READ_BATCH_SIZE`] 件までにし、
/// 答えは尋ねた順
pub(crate) fn inspect_paths(
    rt: &mut CoreRuntime,
    server_name: &str,
    paths: &[RelPath],
) -> anyhow::Result<Vec<anyhow::Result<ResolvedPath>>> {
    let mut answers = Vec::with_capacity(paths.len());
    for chunk in paths.chunks(AGENT_READ_BATCH_SIZE) {
        let inspections = with_agent(rt, server_name, |agent| agent.inspect_paths(chunk))?;
        answers.extend(
            inspections.into_iter().map(|inspection| {
                inspection.and_then(crate::agent::inspection::resolved_from_agent)
            }),
        );
    }
    Ok(answers)
}

pub(crate) fn write(
    rt: &mut CoreRuntime,
    server_name: &str,
    path: &EntryPath,
    content: &[u8],
    is_binary: bool,
) -> anyhow::Result<()> {
    with_agent(rt, server_name, |agent| {
        agent.write_file(path, content, is_binary)
    })
}

pub(crate) fn chmod(
    rt: &mut CoreRuntime,
    server_name: &str,
    path: &EntryPath,
    mode: u32,
) -> anyhow::Result<()> {
    with_agent(rt, server_name, |agent| agent.chmod(path, mode))
}

pub(crate) fn remove(
    rt: &mut CoreRuntime,
    server_name: &str,
    path: &EntryPath,
) -> anyhow::Result<()> {
    with_agent(rt, server_name, |agent| agent.remove(path))
}

pub(crate) fn symlink(
    rt: &mut CoreRuntime,
    server_name: &str,
    path: &EntryPath,
    target: &str,
) -> anyhow::Result<()> {
    with_agent(rt, server_name, |agent| agent.symlink(path, target))
}

#[cfg(all(test, unix))]
#[path = "batch_tests.rs"]
mod batch_tests;
