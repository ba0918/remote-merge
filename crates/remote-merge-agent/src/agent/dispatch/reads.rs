//! 読み取りの要求（中身・ハッシュ・大きさと更新時刻・項目のリンク先）。複数のパスの要求は、
//! パスごとに成功の値か失敗の種類を返す

use std::path::Path;

use remote_merge_core::machine_ops::{self, EntryLink};
use remote_merge_core::op_failure::SizeLimit;
use remote_merge_core::relative_path::EntryPath;

use super::wire;
use crate::agent::protocol::{AgentEntryLink, AgentResponse, FileHashResult, FileReadResult};
use crate::agent::receipt::split_entries;

/// 一つの FileContents に載せる中身の大きさの目安。フレームの上限（16MB）に余裕を持たせる
const RESPONSE_BUDGET: usize = 8 * 1024 * 1024;

/// 中身をパスごとに読む。`size_limit` を超えるファイルは読まずに上限を超える失敗にし、
/// 大きい中身は `chunk_size_limit` ずつに分け、応答は大きさの目安で分ける
pub(super) fn read_files(
    root: &Path,
    paths: &[String],
    chunk_size_limit: usize,
    size_limit: Option<u64>,
) -> Vec<AgentResponse> {
    let mut responses = Vec::new();
    read_files_into(root, paths, chunk_size_limit, size_limit, &mut |response| {
        responses.push(response);
        Ok(())
    })
    .expect("collecting the answers does not fail");
    responses
}

/// [`read_files`] と同じに読み、応答を一つ作るたびに `send` に渡す。読んだ中身を、まとめて求められた
/// 分だけ持たないようにする（送るのは応答の大きさの目安ごと）
pub(super) fn read_files_into(
    root: &Path,
    paths: &[String],
    chunk_size_limit: usize,
    size_limit: Option<u64>,
    send: &mut dyn FnMut(AgentResponse) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let (entries, refused) = split_entries(paths);
    let limit = size_limit.map_or(SizeLimit::Unlimited, SizeLimit::Bytes);
    let mut pending: Vec<FileReadResult> = Vec::new();
    let mut pending_size = 0;
    for entry in &entries {
        for result in read_one(root, entry, chunk_size_limit, limit) {
            let size = result_size(&result);
            if !pending.is_empty() && pending_size + size > RESPONSE_BUDGET {
                send(contents(std::mem::take(&mut pending), false, Vec::new()))?;
                pending_size = 0;
            }
            pending_size += size;
            pending.push(result);
        }
    }
    send(contents(pending, true, refused))
}

fn contents(
    results: Vec<FileReadResult>,
    is_last: bool,
    refused: Vec<remote_merge_core::relative_path::RelPathError>,
) -> AgentResponse {
    AgentResponse::FileContents {
        results,
        is_last,
        refused,
    }
}

/// 一つのファイルを読んだ結果。中身は `chunk_size_limit` ずつに分ける（0 なら分けない）
fn read_one(
    root: &Path,
    entry: &EntryPath,
    chunk_size_limit: usize,
    limit: SizeLimit,
) -> Vec<FileReadResult> {
    let path = entry.as_typed().to_string();
    let content = match machine_ops::read(root, entry, limit) {
        Ok(content) => content,
        Err(failure) => {
            return vec![FileReadResult::Failed {
                path,
                failure: wire(&failure),
            }]
        }
    };
    let size = if chunk_size_limit == 0 {
        content.len().max(1)
    } else {
        chunk_size_limit
    };
    let pieces: Vec<&[u8]> = if content.is_empty() {
        vec![&[]]
    } else {
        content.chunks(size).collect()
    };
    let last = pieces.len() - 1;
    pieces
        .into_iter()
        .enumerate()
        .map(|(index, piece)| FileReadResult::Ok {
            path: path.clone(),
            content: piece.to_vec(),
            more_to_follow: index < last,
        })
        .collect()
}

/// 応答に載せる結果の大きさの見積もり
fn result_size(result: &FileReadResult) -> usize {
    match result {
        FileReadResult::Ok { path, content, .. } => path.len() + content.len() + 64,
        FileReadResult::Failed { path, .. } => path.len() + 64,
    }
}

/// パスごとの比べる値。symlink はリンク先、それ以外は中身の SHA-256
pub(super) fn hash_files(root: &Path, paths: &[String]) -> AgentResponse {
    let (entries, refused) = split_entries(paths);
    let results = entries
        .iter()
        .map(|entry| {
            let path = entry.as_typed().to_string();
            let hashed = machine_ops::entry_link(root, entry).and_then(|link| match link {
                EntryLink::Link(target) => Ok(FileHashResult::Symlink {
                    path: path.clone(),
                    target,
                }),
                EntryLink::NotALink => {
                    machine_ops::hash(root, entry).map(|hash| FileHashResult::Ok {
                        path: path.clone(),
                        hash,
                    })
                }
            });
            hashed.unwrap_or_else(|failure| FileHashResult::Failed {
                path,
                failure: wire(&failure),
            })
        })
        .collect();
    AgentResponse::FileHashes {
        results,
        is_last: true,
        refused,
    }
}

/// パスごとの中身の SHA-256。末尾の symlink も辿った先の中身のハッシュを取る
pub(super) fn hash_contents(root: &Path, paths: &[String]) -> AgentResponse {
    let (entries, refused) = split_entries(paths);
    let results = entries
        .iter()
        .map(|entry| {
            let path = entry.as_typed().to_string();
            match machine_ops::hash(root, entry) {
                Ok(hash) => FileHashResult::Ok { path, hash },
                Err(failure) => FileHashResult::Failed {
                    path,
                    failure: wire(&failure),
                },
            }
        })
        .collect();
    AgentResponse::FileHashes {
        results,
        is_last: true,
        refused,
    }
}

/// 項目そのもの（最後の要素のリンクは辿らない）のリンク先
pub(super) fn read_entry_link(root: &Path, path: &EntryPath) -> AgentResponse {
    let result = match machine_ops::entry_link(root, path) {
        Ok(EntryLink::Link(target)) => AgentEntryLink::Symlink { target },
        Ok(EntryLink::NotALink) => AgentEntryLink::NotSymlink,
        Err(failure) => AgentEntryLink::Failed {
            failure: wire(&failure),
        },
    };
    AgentResponse::EntryLink { result }
}
