//! エージェントのパスの確認（リモートサーバー上で実行）。ローカルと同じ手順で辿り、通信の形にする。
//!
//! パスは受け取り口で入力の形を検査した `RelPath` で受け取る。途中の symlink は root_dir の外を
//! 指していても辿る（docs/ir/paths/operations.md の REQ-paths-013）。

use std::path::Path;

use remote_merge_core::path_resolution::{self, ResolveError, ResolvedPath};
use remote_merge_core::relative_path::RelPath;

use crate::agent::protocol::AgentPathInspection;

pub fn inspect_path(root_dir: &Path, rel_path: &RelPath) -> AgentPathInspection {
    inspection_of(resolve_from_working_directory(
        root_dir,
        rel_path.path_to_resolve(),
    ))
}

/// 相対の root_dir は、リモートでログインした利用者の作業ディレクトリ（エージェントのプロセスの
/// 作業ディレクトリ）の実パスを基準に絶対パスにしてから辿る
fn resolve_from_working_directory(
    root_dir: &Path,
    rel_path: &str,
) -> Result<ResolvedPath, ResolveError> {
    if root_dir.is_absolute() {
        return path_resolution::os::resolve_on_this_machine(root_dir, rel_path);
    }
    let working_directory = std::env::current_dir().map_err(|_| ResolveError::CannotRead)?;
    path_resolution::os::resolve_on_this_machine(&working_directory.join(root_dir), rel_path)
}

/// ローカルと同じ手順で辿った結果を通信の形にする
fn inspection_of(resolved: Result<ResolvedPath, ResolveError>) -> AgentPathInspection {
    match resolved {
        Ok(ResolvedPath::Missing {
            real_parent,
            parent_exists,
        }) => AgentPathInspection::Missing {
            real_parent,
            parent_exists,
        },
        Ok(ResolvedPath::Exists { real_path, kind }) => {
            AgentPathInspection::Exists { real_path, kind }
        }
        Ok(ResolvedPath::Symlink {
            link_target,
            real_path,
            target_kind,
        }) => AgentPathInspection::Symlink {
            link_target,
            real_path,
            target_kind,
        },
        Ok(ResolvedPath::AncestorNotDirectory {
            real_path,
            regular_file,
        }) => AgentPathInspection::AncestorNotDirectory {
            real_path,
            regular_file,
        },
        Err(error) => AgentPathInspection::Unresolved {
            cause: error.cause().into(),
        },
    }
}
