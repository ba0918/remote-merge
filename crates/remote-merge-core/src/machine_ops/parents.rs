//! 書き込みと symlink の作成が作る親ディレクトリ。作った後で書き込み先を作れなければ、作った親を
//! 下から消して何も残さない（docs/ir/merge/permissions.md の REQ-merge-042、
//! docs/decision/records/2026-10-12-foundation-3.md の D36）

use std::path::{Path, PathBuf};

use super::failed;
use crate::op_failure::OpFailure;

/// `path` の親のうち、まだないもの（上から順）
fn missing_parents(path: &Path) -> Vec<PathBuf> {
    let mut missing: Vec<PathBuf> = path
        .ancestors()
        .skip(1)
        .take_while(|ancestor| {
            !ancestor.as_os_str().is_empty() && std::fs::symlink_metadata(ancestor).is_err()
        })
        .map(Path::to_path_buf)
        .collect();
    missing.reverse();
    missing
}

/// ない親ディレクトリを作り、作ったもの（上から順）を返す。途中の symlink は辿る（REQ-merge-004）。
/// 作ろうとした親がすでにあってディレクトリでなければ、途中の要素がディレクトリでない失敗にする
/// （REQ-paths-020）
pub(super) fn create_parents(path: &Path) -> Result<Vec<PathBuf>, OpFailure> {
    let Some(parent) = path.parent() else {
        return Ok(Vec::new());
    };
    let missing = missing_parents(path);
    let failure = match std::fs::create_dir_all(parent) {
        Ok(()) => return Ok(missing),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => OpFailure::NotADirectory,
        Err(error) => failed(error),
    };
    // 途中まで作れていたら、作ったところを消す
    Err(undo_parents(&missing, failure, remove_dir))
}

/// 作った親 `dirs`（上から順）を下から消して、`failure` を返す。消せなかったものがあれば、それを示す
/// I/O の失敗にする
pub fn undo_parents(
    dirs: &[PathBuf],
    failure: OpFailure,
    mut remove: impl FnMut(&Path) -> std::io::Result<()>,
) -> OpFailure {
    let left: Vec<String> = dirs
        .iter()
        .rev()
        .filter_map(|dir| {
            remove(dir)
                .err()
                .map(|error| format!("{} ({error})", dir.display()))
        })
        .collect();
    if left.is_empty() {
        return failure;
    }
    OpFailure::Io(format!(
        "{failure}; these created directories were left because they could not be removed: {}",
        left.join(", ")
    ))
}

/// 作った親を消すいつもの手段。作れずにないものは消せたものとする
pub(super) fn remove_dir(dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir(dir) {
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            Ok(())
        }
        other => other,
    }
}
