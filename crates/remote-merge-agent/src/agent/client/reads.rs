//! エージェントへの読み取りの要求（走査・中身・ハッシュ・大きさと更新時刻）

use std::collections::HashMap;
use std::io::{Read, Write};

use anyhow::Result;
use remote_merge_core::op_failure::OpFailure;
use remote_merge_core::relative_path::{entries_as_typed, EntryPath};
use remote_merge_core::scan::outcome::{Listing, ScanFailure, ScanRequest};

use super::ops::{check_answered, check_refused, PathContent};
use super::{mismatch, unexpected, AgentClient, AgentRefused};
use crate::agent::protocol::{AgentRequest, AgentResponse, FileHashResult, FileReadResult};

impl<R: Read, W: Write> AgentClient<R, W> {
    /// 走査の規則で走査する。走査を止めた誤りは内側の Err
    pub fn list_tree(
        &mut self,
        request: &ScanRequest,
        exclude: &[String],
    ) -> Result<std::result::Result<Listing, ScanFailure>> {
        self.send(&AgentRequest::ListTree {
            request: request.clone(),
            exclude: exclude.to_vec(),
        })?;
        let mut listing = Listing::default();
        loop {
            match self.receive()? {
                AgentResponse::TreeChunk {
                    entries,
                    is_last,
                    unreadable_names,
                    truncated,
                    cycle,
                } => {
                    listing.entries.extend(entries);
                    if is_last {
                        listing.unreadable_names = unreadable_names;
                        listing.truncated = truncated;
                        listing.cycle = cycle;
                        return Ok(Ok(listing));
                    }
                }
                AgentResponse::TreeFailed { failure } => return Ok(Err(failure)),
                AgentResponse::PathRefused { refusal } => return Err(refusal.into()),
                AgentResponse::Error { message } => return Err(AgentRefused(message).into()),
                other => return Err(unexpected("ListTree", &other)),
            }
        }
    }

    /// 中身をパスごとに読む。`size_limit` を超えるものは上限を超える失敗になる
    pub fn read_files(
        &mut self,
        paths: &[EntryPath],
        chunk_size_limit: usize,
        size_limit: Option<u64>,
    ) -> Result<Vec<PathContent>> {
        self.send(&AgentRequest::ReadFiles {
            paths: entries_as_typed(paths),
            chunk_size_limit,
            size_limit,
        })?;
        let mut order: Vec<String> = Vec::new();
        let mut answers: HashMap<String, std::result::Result<Vec<u8>, OpFailure>> = HashMap::new();
        loop {
            match self.receive()? {
                AgentResponse::FileContents {
                    results,
                    is_last,
                    refused,
                } => {
                    for result in results {
                        let (path, answer) = match result {
                            FileReadResult::Ok { path, content, .. } => (path, Ok(content)),
                            FileReadResult::Failed { path, failure } => {
                                (path, Err(OpFailure::from(failure)))
                            }
                        };
                        match answers.get_mut(&path) {
                            Some(Ok(joined)) => match answer {
                                Ok(more) => joined.extend(more),
                                Err(_) => return Err(mismatch("ReadFiles")),
                            },
                            Some(Err(_)) => return Err(mismatch("ReadFiles")),
                            None => {
                                order.push(path.clone());
                                answers.insert(path, answer);
                            }
                        }
                    }
                    if is_last {
                        check_refused(refused)?;
                        break;
                    }
                }
                AgentResponse::Error { message } => return Err(AgentRefused(message).into()),
                other => return Err(unexpected("ReadFiles", &other)),
            }
        }
        check_answered("ReadFiles", paths, order.iter().map(String::as_str))?;
        Ok(order
            .into_iter()
            .map(|path| {
                let answer = answers
                    .remove(&path)
                    .expect("every path in order has an answer");
                (path, answer)
            })
            .collect())
    }

    /// パスごとの比べる値（symlink はリンク先）
    pub fn hash_files(&mut self, paths: &[EntryPath]) -> Result<Vec<FileHashResult>> {
        let request = AgentRequest::HashFiles {
            paths: entries_as_typed(paths),
        };
        self.hashes(&request, "HashFiles", paths)
    }

    /// パスごとの中身の SHA-256（末尾の symlink も辿った先の中身）
    pub fn hash_contents(&mut self, paths: &[EntryPath]) -> Result<Vec<FileHashResult>> {
        let request = AgentRequest::HashContents {
            paths: entries_as_typed(paths),
        };
        self.hashes(&request, "HashContents", paths)
    }

    /// ハッシュの要求を送り、FileHashes の応答を最後まで受ける
    fn hashes(
        &mut self,
        request: &AgentRequest,
        name: &str,
        paths: &[EntryPath],
    ) -> Result<Vec<FileHashResult>> {
        self.send(request)?;
        let mut all = Vec::new();
        loop {
            match self.receive()? {
                AgentResponse::FileHashes {
                    results,
                    is_last,
                    refused,
                } => {
                    all.extend(results);
                    if is_last {
                        check_refused(refused)?;
                        break;
                    }
                }
                AgentResponse::Error { message } => return Err(AgentRefused(message).into()),
                other => return Err(unexpected(name, &other)),
            }
        }
        let answered = all.iter().map(|result| match result {
            FileHashResult::Ok { path, .. }
            | FileHashResult::Symlink { path, .. }
            | FileHashResult::Failed { path, .. } => path.as_str(),
        });
        check_answered(name, paths, answered)?;
        Ok(all)
    }
}
