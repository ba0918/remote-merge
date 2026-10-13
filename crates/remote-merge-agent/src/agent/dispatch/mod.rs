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
use super::protocol::{AgentRequest, AgentResponse, WireFailure};
use super::server::MetadataConfig;

pub struct Dispatcher {
    root_dir: PathBuf,
    metadata_config: MetadataConfig,
    /// 分けて送られる書き込みで最初の分を書き終えたパスと、最初の分を書く前に書き込み先が
    /// なかったか
    pub(crate) writing: HashMap<String, bool>,
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

    /// 書き込みの一つの分を書く。分けて送られるときは、最初の分の前に書き込み先がなかったかを
    /// 最後の分まで覚える
    fn write(&mut self, path: &EntryPath, content: &[u8], more_to_follow: bool) -> AgentResponse {
        let key = path.as_typed().to_string();
        let part = match self.writing.get(&key) {
            Some(created) => writes::Part {
                first: false,
                created: *created,
            },
            None => writes::Part {
                first: true,
                created: writes::is_new(&self.root_dir, path),
            },
        };
        let written =
            writes::write_part(&self.root_dir, path, content, &part, &self.metadata_config);
        if written.is_ok() && more_to_follow {
            self.writing.insert(key, part.created);
        } else {
            self.forget(&key);
        }
        changes::changed(written)
    }

    fn forget(&mut self, key: &str) {
        self.writing.remove(key);
    }
}

#[cfg(test)]
mod tests;
