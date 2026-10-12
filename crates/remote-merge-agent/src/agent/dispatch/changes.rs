//! 変更の要求（symlink の作成・削除・権限の変更）

use std::path::Path;

use remote_merge_core::machine_ops::{self, PermissionMode};
use remote_merge_core::op_failure::{from_error, OpFailure};
use remote_merge_core::relative_path::EntryPath;

use super::wire;
use crate::agent::file_io;
use crate::agent::protocol::AgentResponse;
use crate::agent::server::MetadataConfig;

fn changed(result: Result<(), OpFailure>) -> AgentResponse {
    AgentResponse::Changed {
        failure: result.err().as_ref().map(wire),
    }
}

/// symlink を作る。作成先に何かがあれば置き換えずに失敗する。sudo のエージェントは作ったリンクの
/// 所有者を既定の利用者にする
pub(super) fn symlink(
    root: &Path,
    path: &EntryPath,
    target: &str,
    config: &MetadataConfig,
) -> AgentResponse {
    let created = file_io::create_symlink_with_metadata(
        root,
        path,
        target,
        config.default_uid,
        config.default_gid,
    );
    changed(created.map_err(|error| from_error(&error)))
}

pub(super) fn remove(root: &Path, path: &EntryPath) -> AgentResponse {
    changed(machine_ops::remove(root, path))
}

/// 権限を変える。0o777 を超える値は変えずに失敗にする
pub(super) fn chmod(root: &Path, path: &EntryPath, mode: u32) -> AgentResponse {
    changed(match PermissionMode::new(mode) {
        Ok(mode) => machine_ops::chmod(root, path, mode),
        Err(too_wide) => Err(OpFailure::Io(too_wide.to_string())),
    })
}
