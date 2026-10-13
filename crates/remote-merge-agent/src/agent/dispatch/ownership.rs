//! sudo のエージェントが新しく作ったもの（ファイル・symlink・親ディレクトリ）の所有者を、ログイン
//! した利用者として作るほかの経路と同じになるよう既定の利用者にする（docs/ir/merge/permissions.md の
//! REQ-merge-042）。
//!
//! 所有者を付けられなければ、作ったものを消して何も変えずに失敗にする。消せなければ、残ったものを
//! 示す失敗にする。

use std::io;
use std::path::{Path, PathBuf};

use remote_merge_core::op_failure::{from_io_error, OpFailure};
use remote_merge_core::relative_path::EntryPath;

/// 作ったものへの所有者の操作と、消す操作
pub(super) trait OwnerOps {
    fn chown_dir(&mut self, dir: &Path) -> io::Result<()>;
    fn chown_entry(&mut self) -> io::Result<()>;
    fn remove_entry(&mut self) -> io::Result<()>;
    fn remove_dir(&mut self, dir: &Path) -> io::Result<()>;
}

/// `root` の下の `path` の親のうち、まだないもの（上から順）。書き込みか symlink の作成の前に求める
pub(super) fn missing_ancestors(root: &Path, path: &EntryPath) -> Vec<PathBuf> {
    let mut missing = Vec::new();
    let mut current = root.join(path.as_str());
    while let Some(parent) = current.parent() {
        if parent == root || !parent.starts_with(root) || std::fs::symlink_metadata(parent).is_ok()
        {
            break;
        }
        missing.push(parent.to_path_buf());
        current = parent.to_path_buf();
    }
    missing.reverse();
    missing
}

/// 作った親ディレクトリ（上から順）と項目に所有者を付ける。付けられなければ、項目と親を下から
/// 消して、所有者を付けられなかった種類の失敗にする。消せないものがあれば、それを示す I/O の失敗
pub(super) fn own_created(
    dirs: &[PathBuf],
    entry_shown: &str,
    ops: &mut dyn OwnerOps,
) -> Result<(), OpFailure> {
    let owned = dirs
        .iter()
        .try_for_each(|dir| ops.chown_dir(dir))
        .and_then(|()| ops.chown_entry());
    let Err(error) = owned else {
        return Ok(());
    };
    let reason = from_io_error(&error);
    let mut left = Vec::new();
    if let Err(removal) = ops.remove_entry() {
        left.push(format!("{entry_shown} ({removal})"));
    }
    for dir in dirs.iter().rev() {
        if let Err(removal) = ops.remove_dir(dir) {
            left.push(format!("{} ({removal})", dir.display()));
        }
    }
    if left.is_empty() {
        return Err(reason);
    }
    Err(OpFailure::Io(format!(
        "cannot set the owner ({reason}); these created entries were left because they could not be removed: {}",
        left.join(", ")
    )))
}

#[cfg(test)]
#[path = "ownership_tests.rs"]
mod tests;
