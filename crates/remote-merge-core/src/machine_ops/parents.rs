//! 書き込みと symlink の作成が作る親ディレクトリ。作った後で書き込み先を作れなければ、作った親を
//! 下から消して何も残さない（docs/ir/merge/permissions.md の REQ-merge-042、
//! docs/decision/records/2026-10-12-foundation-3.md の D36）

use std::path::{Path, PathBuf};

use super::failed;
use crate::op_failure::{OpFailure, os_reason};

/// `path` の親のうち、root_dir `root` より下でまだないもの（上から順）
fn missing_parents(root: &Path, path: &Path) -> Vec<PathBuf> {
    let mut missing: Vec<PathBuf> = path
        .ancestors()
        .skip(1)
        .take_while(|ancestor| {
            *ancestor != root
                && !ancestor.as_os_str().is_empty()
                && std::fs::symlink_metadata(ancestor).is_err()
        })
        .map(Path::to_path_buf)
        .collect();
    missing.reverse();
    missing
}

/// root_dir `root` の下のない親ディレクトリを作り、作ったもの（上から順）を返す。途中の symlink は
/// 辿る（REQ-merge-004）。作ろうとした親がすでにあってディレクトリでなければ、途中の要素がディレクトリで
/// ない失敗にする（REQ-paths-020）。root_dir がない（ディレクトリでない）ときは、root_dir もその親も
/// 作らずに root_dir がない失敗にする（D47）
pub(super) fn create_parents(root: &Path, path: &Path) -> Result<Vec<PathBuf>, OpFailure> {
    create_parents_with(root, path, |dir| std::fs::create_dir(dir))
}

/// [`create_parents`] の、一つのディレクトリを作る手段 `mkdir` を受け取るもの。作ったものに数えるのは
/// `mkdir` が成功したものだけで、作る前に別の誰かが作ったディレクトリは数えない（後で所有者を付けたり
/// 消したりしない。docs/decision/records/2026-10-12-foundation-3.md の D39）
pub(super) fn create_parents_with(
    root: &Path,
    path: &Path,
    mut mkdir: impl FnMut(&Path) -> std::io::Result<()>,
) -> Result<Vec<PathBuf>, OpFailure> {
    if let Some(failure) = root_failure(root) {
        return Err(failure);
    }
    let Some(parent) = path.parent() else {
        return Ok(Vec::new());
    };
    let mut made = Vec::new();
    for dir in missing_parents(root, path) {
        match mkdir(&dir) {
            Ok(()) => made.push(dir),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && dir.is_dir() => {}
            Err(error) => {
                return Err(undo_parents(
                    &made,
                    parent_failure(&dir, &error),
                    remove_dir,
                ));
            }
        }
    }
    if parent.as_os_str().is_empty() || parent.is_dir() {
        return Ok(made);
    }
    let error = std::io::Error::from(std::io::ErrorKind::NotADirectory);
    Err(undo_parents(
        &made,
        parent_failure(parent, &error),
        remove_dir,
    ))
}

/// root_dir `root` を使えないときの失敗。ない・ディレクトリでない（途中を含む）ときは root_dir がない、
/// 上を辿れないときは権限がない（D48）
fn root_failure(root: &Path) -> Option<OpFailure> {
    match std::fs::metadata(root) {
        Ok(meta) if meta.is_dir() => None,
        Ok(_) => Some(OpFailure::RootGone),
        Err(error) => Some(match error.kind() {
            std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory => OpFailure::RootGone,
            _ => failed(&error),
        }),
    }
}

/// 親 `dir` を作れなかった失敗。`dir` かその祖先がすでにあって（リンク切れの symlink を含む）
/// ディレクトリでなければ、途中の要素がディレクトリでない失敗にする（D35）
fn parent_failure(dir: &Path, error: &std::io::Error) -> OpFailure {
    let blocked = dir.ancestors().any(|ancestor| {
        !ancestor.as_os_str().is_empty()
            && std::fs::symlink_metadata(ancestor).is_ok()
            && !ancestor.is_dir()
    });
    if blocked {
        OpFailure::NotADirectory
    } else {
        failed(error)
    }
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
                .map(|error| format!("{} ({})", dir.display(), os_reason(&error)))
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

/// 作ったディレクトリを消す。どの経路も同じ判断にする（D43）：消えていたものは消せたものとし、
/// ディレクトリでないもの（作った後に通常ファイルや symlink に替わったもの）は消さずに、消せずに
/// 残ったものとして誤りを返す
pub fn remove_dir(dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir(dir) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}
