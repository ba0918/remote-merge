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
                    unresolvable_names,
                    counted,
                } => {
                    listing.entries.extend(entries);
                    if is_last {
                        listing.unreadable_names = unreadable_names;
                        listing.truncated = truncated;
                        listing.cycle = cycle;
                        listing.unresolvable_names = unresolvable_names;
                        listing.counted = counted;
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
        // 続きがあると言った中身のパス。続かないまま終わるか、終わった中身に続きが来れば要求と
        // 合わない応答にする
        let mut open: std::collections::HashSet<String> = std::collections::HashSet::new();
        loop {
            match self.receive()? {
                AgentResponse::FileContents {
                    results,
                    is_last,
                    refused,
                } => {
                    for result in results {
                        let (path, answer, more_to_follow) = match result {
                            FileReadResult::Ok {
                                path,
                                content,
                                more_to_follow,
                            } => (path, Ok(content), more_to_follow),
                            FileReadResult::Failed { path, failure } => {
                                (path, Err(OpFailure::from(failure)), false)
                            }
                        };
                        let continues = open.remove(&path);
                        match (answers.get_mut(&path), answer) {
                            (Some(Ok(joined)), Ok(more)) if continues => joined.extend(more),
                            (Some(_), _) => return Err(mismatch("ReadFiles")),
                            (None, answer) => {
                                order.push(path.clone());
                                answers.insert(path.clone(), answer);
                            }
                        }
                        if more_to_follow {
                            open.insert(path);
                        }
                    }
                    if is_last {
                        if !open.is_empty() {
                            return Err(mismatch("ReadFiles"));
                        }
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
