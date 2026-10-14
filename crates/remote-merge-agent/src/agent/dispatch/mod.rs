//! エージェントの要求の受け取り口。パスを入力の形の検査に通し、このマシンの操作（ローカルの経路と
//! 同じ `machine_ops`）を行い、失敗を種類（`WireFailure`）で返す

mod changes;
mod ownership;
mod reads;
mod writes;

use std::collections::HashMap;
use std::path::PathBuf;

use remote_merge_core::op_failure::OpFailure;
use remote_merge_core::relative_path::{EntryPath, RelPath, RelPathError};

use super::file_io;
use super::protocol::{AgentPathCheck, AgentRequest, AgentResponse, WireFailure};
use super::server::MetadataConfig;

pub struct Dispatcher {
    root_dir: PathBuf,
    metadata_config: MetadataConfig,
    /// 分けて送られる書き込みで最初の分を書き終えたパスと、最初の分で開いたファイル
    pub(crate) writing: HashMap<String, std::fs::File>,
}

/// 一つのパスの要求で、パスを拒んだときの応答
fn refused(refusal: RelPathError) -> Vec<AgentResponse> {
    vec![AgentResponse::PathRefused { refusal }]
}

pub(crate) fn wire(failure: &OpFailure) -> WireFailure {
    WireFailure::from(failure)
}

impl Dispatcher {
    pub fn new(root_dir: PathBuf, metadata_config: MetadataConfig) -> Self {
        Self {
            root_dir,
            metadata_config,
            writing: HashMap::new(),
        }
    }

    /// 要求に答え、応答を一つずつ `send` に渡す。中身の読み込みは応答を作るたびに渡す。Shutdown なら
    /// false
    pub fn dispatch_into(
        &mut self,
        request: AgentRequest,
        send: &mut dyn FnMut(AgentResponse) -> anyhow::Result<()>,
    ) -> anyhow::Result<bool> {
        if let AgentRequest::ReadFiles {
            paths,
            chunk_size_limit,
            size_limit,
        } = &request
        {
            reads::read_files_into(&self.root_dir, paths, *chunk_size_limit, *size_limit, send)?;
            return Ok(true);
        }
        match self.dispatch(request) {
            Some(responses) => {
                for response in responses {
                    send(response)?;
                }
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// 要求に答える。Shutdown なら None
    pub fn dispatch(&mut self, request: AgentRequest) -> Option<Vec<AgentResponse>> {
        let root = &self.root_dir;
        Some(match request {
            AgentRequest::ListTree { request, exclude } => {
                super::tree_listing::list_tree(root, &request, &exclude)
            }
            AgentRequest::ReadFiles {
                paths,
                chunk_size_limit,
                size_limit,
            } => reads::read_files(root, &paths, chunk_size_limit, size_limit),
            AgentRequest::HashFiles { paths } => vec![reads::hash_files(root, &paths)],
            AgentRequest::HashContents { paths } => vec![reads::hash_contents(root, &paths)],
            AgentRequest::InspectPath { path } => match RelPath::parse(&path) {
                Ok(path) => vec![AgentResponse::PathInspection {
                    result: file_io::inspect_path(root, &path),
                }],
                Err(refusal) => refused(refusal),
            },
            AgentRequest::InspectPaths { paths } => vec![AgentResponse::PathInspections {
                results: paths
                    .iter()
                    .map(|path| match RelPath::parse(path) {
                        Ok(path) => AgentPathCheck::Inspected(file_io::inspect_path(root, &path)),
                        Err(refusal) => AgentPathCheck::Refused(refusal),
                    })
                    .collect(),
            }],
            AgentRequest::ReadEntryLink { path } => match EntryPath::parse(&path) {
                Ok(path) => vec![reads::read_entry_link(root, &path)],
                Err(refusal) => refused(refusal),
            },
            AgentRequest::WriteFile {
                path,
                content,
                is_binary: _,
                more_to_follow,
            } => match EntryPath::parse(&path) {
                Ok(path) => vec![self.write(&path, &content, more_to_follow)],
                Err(refusal) => refused(refusal),
            },
            AgentRequest::Symlink { path, target } => match EntryPath::parse(&path) {
                Ok(path) => vec![changes::changed(writes::symlink(
                    root,
                    &path,
                    &target,
                    &self.metadata_config,
                ))],
                Err(refusal) => refused(refusal),
            },
            AgentRequest::Remove { path } => match EntryPath::parse(&path) {
                Ok(path) => vec![changes::remove(root, &path)],
                Err(refusal) => refused(refusal),
            },
            AgentRequest::Chmod { path, mode } => match EntryPath::parse(&path) {
                Ok(path) => vec![changes::chmod(root, &path, mode)],
                Err(refusal) => refused(refusal),
            },
            AgentRequest::Ping => vec![AgentResponse::Pong],
            AgentRequest::Shutdown => return None,
        })
    }

    /// 書き込みの一つの分を書く。分けて送られるときは、最初の分で開いたファイルを最後の分まで持ち、
    /// 後の分はそれに書き足す
    fn write(&mut self, path: &EntryPath, content: &[u8], more_to_follow: bool) -> AgentResponse {
        let key = path.as_typed().to_string();
        let written = match self.writing.remove(&key) {
            Some(mut file) => writes::write_more(&mut file, content).map(|()| file),
            None => writes::write_first(&self.root_dir, path, content, &self.metadata_config),
        };
        let written = written.map(|file| {
            if more_to_follow {
                self.writing.insert(key, file);
            }
        });
        changes::changed(written)
    }
}

#[cfg(test)]
mod tests;
