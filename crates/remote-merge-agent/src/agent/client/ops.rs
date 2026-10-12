//! エージェントへの操作の要求。複数のパスの要求は、尋ねたパスのすべてに一つずつ答えがあることを
//! 確かめ、欠けや余りは要求と合わない応答（接続断）にする

use std::collections::HashSet;
use std::io::{Read, Write};

use anyhow::Result;
use remote_merge_core::op_failure::{OpError, OpFailure, Operation};
use remote_merge_core::relative_path::{entries_as_typed, EntryPath, RelPath};

use super::{mismatch, unexpected, AgentClient, AgentRefused, WRITE_CHUNK_SIZE};
use crate::agent::protocol::{
    AgentEntryLink, AgentPathInspection, AgentRequest, AgentResponse, RelPathError, WireFailure,
};

/// エージェントが入力の形の検査で拒んだパスがあれば、その理由の誤り
pub(super) fn check_refused(refused: Vec<RelPathError>) -> Result<()> {
    match refused.into_iter().next() {
        Some(refusal) => Err(refusal.into()),
        None => Ok(()),
    }
}

/// 尋ねたパスと答えのパスが一つずつ対応するか
pub(super) fn check_answered<'a>(
    request: &str,
    asked: &[EntryPath],
    answered: impl Iterator<Item = &'a str>,
) -> Result<()> {
    let asked: HashSet<String> = entries_as_typed(asked).into_iter().collect();
    let answered: HashSet<&str> = answered.collect();
    if answered.len() == asked.len() && answered.iter().all(|path| asked.contains(*path)) {
        return Ok(());
    }
    Err(mismatch(request))
}

/// 変更の結果。失敗ならその種類の誤り
fn changed(operation: Operation, path: &EntryPath, response: AgentResponse) -> Result<()> {
    match response {
        AgentResponse::Changed { failure: None } => Ok(()),
        AgentResponse::Changed {
            failure: Some(failure),
        } => Err(OpError::new(operation, path.as_typed(), OpFailure::from(failure)).into()),
        AgentResponse::PathRefused { refusal } => Err(refusal.into()),
        AgentResponse::Error { message } => Err(AgentRefused(message).into()),
        other => Err(unexpected(&format!("{operation:?}"), &other)),
    }
}

/// 一つのパスの読み込みの答え（パスと、中身か失敗の種類）
pub type PathContent = (String, std::result::Result<Vec<u8>, OpFailure>);

impl<R: Read, W: Write> AgentClient<R, W> {
    /// 中身を書く。大きい中身は分けて送る
    pub fn write_file(&mut self, path: &EntryPath, content: &[u8], is_binary: bool) -> Result<()> {
        let chunks: Vec<&[u8]> = if content.is_empty() {
            vec![&[]]
        } else {
            content.chunks(WRITE_CHUNK_SIZE).collect()
        };
        let last = chunks.len() - 1;
        for (index, chunk) in chunks.into_iter().enumerate() {
            let response = self.request(&AgentRequest::WriteFile {
                path: path.as_typed().to_string(),
                content: chunk.to_vec(),
                is_binary,
                more_to_follow: index < last,
            })?;
            changed(Operation::Write, path, response)?;
        }
        Ok(())
    }

    pub fn inspect_path(&mut self, path: &RelPath) -> Result<AgentPathInspection> {
        match self.request(&AgentRequest::InspectPath {
            path: path.as_typed().to_string(),
        })? {
            AgentResponse::PathInspection { result } => Ok(result),
            AgentResponse::PathRefused { refusal } => Err(refusal.into()),
            AgentResponse::Error { message } => Err(AgentRefused(message).into()),
            other => Err(unexpected("InspectPath", &other)),
        }
    }

    pub fn symlink(&mut self, path: &EntryPath, target: &str) -> Result<()> {
        let response = self.request(&AgentRequest::Symlink {
            path: path.as_typed().to_string(),
            target: target.to_string(),
        })?;
        changed(Operation::CreateSymlink, path, response)
    }

    pub fn remove(&mut self, path: &EntryPath) -> Result<()> {
        let response = self.request(&AgentRequest::Remove {
            path: path.as_typed().to_string(),
        })?;
        changed(Operation::Remove, path, response)
    }

    pub fn chmod(&mut self, path: &EntryPath, mode: u32) -> Result<()> {
        let response = self.request(&AgentRequest::Chmod {
            path: path.as_typed().to_string(),
            mode,
        })?;
        changed(Operation::Chmod, path, response)
    }

    /// 項目そのものが symlink ならリンク先。symlink でなければ None
    pub fn read_entry_link(&mut self, path: &EntryPath) -> Result<Option<String>> {
        match self.request(&AgentRequest::ReadEntryLink {
            path: path.as_typed().to_string(),
        })? {
            AgentResponse::EntryLink { result } => match result {
                AgentEntryLink::Symlink { target } => Ok(Some(target)),
                AgentEntryLink::NotSymlink => Ok(None),
                AgentEntryLink::Failed { failure } => Err(failed(path, failure)),
            },
            AgentResponse::PathRefused { refusal } => Err(refusal.into()),
            AgentResponse::Error { message } => Err(AgentRefused(message).into()),
            other => Err(unexpected("ReadEntryLink", &other)),
        }
    }
}

fn failed(path: &EntryPath, failure: WireFailure) -> anyhow::Error {
    OpError::new(
        Operation::ReadEntryLink,
        path.as_typed(),
        OpFailure::from(failure),
    )
    .into()
}
