//! エージェントの要求の受け取り口。パスを入力の形の検査に通し、このマシンの操作（ローカルの経路と
//! 同じ `machine_ops`）を行い、失敗を種類（`WireFailure`）で返す

mod changes;
mod reads;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use remote_merge_core::op_failure::OpFailure;
use remote_merge_core::relative_path::{EntryPath, RelPath, RelPathError};

use super::file_io::{self, SavedMetadata};
use super::protocol::{AgentRequest, AgentResponse, WireFailure};
use super::server::MetadataConfig;

pub struct Dispatcher {
    root_dir: PathBuf,
    metadata_config: MetadataConfig,
    /// 分けて送られる書き込みの、最初の分を書く前の書き込み先の所有者と権限
    pub(crate) saved_metadata: HashMap<String, Option<SavedMetadata>>,
    /// 分けて送られる書き込みで、最初の分を書き終えたパス
    pub(crate) written_paths: HashSet<String>,
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
            saved_metadata: HashMap::new(),
            written_paths: HashSet::new(),
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
            AgentRequest::StatFiles { paths } => vec![reads::stat_files(root, &paths)],
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
                Ok(path) => vec![changes::symlink(
                    root,
                    &path,
                    &target,
                    &self.metadata_config,
                )],
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

    /// 書き込みの一つの分を書く。分けて送られるときは、最初の分で取った書き込み先の所有者と
    /// 権限を最後の分まで持つ
    fn write(&mut self, path: &EntryPath, content: &[u8], more_to_follow: bool) -> AgentResponse {
        let key = path.as_typed().to_string();
        let first = !self.written_paths.contains(&key);
        let saved = self.saved_metadata.get(&key).cloned().flatten();
        let written = file_io::write_file_with_metadata(
            &self.root_dir,
            path,
            content,
            first,
            !more_to_follow,
            saved.as_ref(),
            &self.metadata_config,
        );
        match written {
            Ok(taken) if more_to_follow => {
                if first {
                    self.saved_metadata.insert(key.clone(), taken);
                    self.written_paths.insert(key);
                }
                AgentResponse::Changed { failure: None }
            }
            Ok(_) => {
                self.forget(&key);
                AgentResponse::Changed { failure: None }
            }
            Err(error) => {
                self.forget(&key);
                let failure = remote_merge_core::op_failure::from_error(&error);
                AgentResponse::Changed {
                    failure: Some(wire(&failure)),
                }
            }
        }
    }

    fn forget(&mut self, key: &str) {
        self.saved_metadata.remove(key);
        self.written_paths.remove(key);
    }
}

#[cfg(test)]
mod tests;
