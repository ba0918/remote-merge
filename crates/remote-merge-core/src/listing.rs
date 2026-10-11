//! ディレクトリの子の一覧の結果と、このマシンで一覧する手順（ローカルとエージェントの経路）。
//!
//! 一覧を止めるエラー（一覧の中の symlink のリンク先か更新時刻を読めない）は `Err` で返し、
//! [`ChildListing`] には続けられる三つの結果を置く。

use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::filter;
use crate::rel_path::operation_rel_path;
use crate::tree::FileNode;

/// 子の一覧の結果
#[derive(Debug, Clone)]
pub enum ChildListing {
    /// ディレクトリかディレクトリへの symlink の直下の子
    Children(Vec<FileNode>),
    /// 通常ファイル、ディレクトリでないものへの symlink、リンク切れ、ないパス
    NotDirectory,
    /// ディレクトリかもしれないのに中身が分からない（読めないディレクトリなど）。値は理由
    Unlistable(String),
}

/// 一覧の中身を読む手順。リンク先の読み取りだけを差し替えられる
pub trait ListingReader {
    fn read_link(&self, path: &Path) -> io::Result<PathBuf>;
}

/// このマシンのファイルシステムをそのまま読む
pub struct ThisMachine;

impl ListingReader for ThisMachine {
    fn read_link(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::read_link(path)
    }
}

/// root_dir `root` の下の `path` の直下の子を、このマシンで一覧する。
///
/// `exclude` は直下の子の root_dir からのパスに当てる。子が `max_entries` を超えたら打ち切る。
/// 途中の symlink は root_dir の外を指していても辿る
pub fn list_children(
    root: &Path,
    path: &str,
    exclude: &[String],
    max_entries: usize,
) -> anyhow::Result<ChildListing> {
    list_children_with(&ThisMachine, root, path, exclude, max_entries)
}

/// [`list_children`] の、中身の読み方を `reader` にしたもの
pub fn list_children_with(
    reader: &impl ListingReader,
    root: &Path,
    path: &str,
    exclude: &[String],
    max_entries: usize,
) -> anyhow::Result<ChildListing> {
    let rel = operation_rel_path(path)?;
    let dir = root.join(rel);
    match std::fs::metadata(&dir) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => return Ok(ChildListing::NotDirectory),
        Err(error) if is_missing(&error) => return Ok(ChildListing::NotDirectory),
        Err(error) => return Ok(unlistable(rel, &error)),
    }
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) => return Ok(unlistable(rel, &error)),
    };
    let mut nodes = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => return Ok(unlistable(rel, &error)),
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let child_rel = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        if filter::is_path_excluded(&child_rel, exclude) {
            continue;
        }
        if nodes.len() >= max_entries {
            break;
        }
        let meta = match entry.path().symlink_metadata() {
            Ok(meta) => meta,
            Err(error) => return Ok(unlistable(rel, &error)),
        };
        nodes.push(child_node(reader, &entry.path(), &child_rel, name, &meta)?);
    }
    Ok(ChildListing::Children(nodes))
}

/// 一覧の一項目。リンク先か更新時刻を読めなければ、既定の値で埋めずにエラーにする
fn child_node(
    reader: &impl ListingReader,
    full: &Path,
    child_rel: &str,
    name: String,
    meta: &std::fs::Metadata,
) -> anyhow::Result<FileNode> {
    let mut node = if meta.file_type().is_symlink() {
        let target = reader.read_link(full).map_err(|error| {
            anyhow::anyhow!("failed to list {child_rel:?}: cannot read the link target: {error}")
        })?;
        let mut node = FileNode::new_symlink(name, target.to_string_lossy().into_owned());
        node.link_is_dir = full.is_dir();
        node
    } else if meta.is_dir() {
        FileNode::new_dir(name)
    } else {
        FileNode::new_file(name)
    };
    node.size = Some(meta.len());
    node.permissions = Some(permission_bits(meta));
    node.mtime = Some(modified(meta).map_err(|error| {
        anyhow::anyhow!("failed to list {child_rel:?}: cannot read the modification time: {error}")
    })?);
    Ok(node)
}

/// 0o7777 の範囲の権限のビット（特別なビットを含む）
#[cfg(unix)]
fn permission_bits(meta: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt;
    meta.mode() & 0o7777
}

#[cfg(not(unix))]
fn permission_bits(meta: &std::fs::Metadata) -> u32 {
    if meta.permissions().readonly() {
        0o444
    } else {
        0o644
    }
}

/// ナノ秒までの更新時刻。1970 年より前は、秒を負の値に切り下げ、ナノ秒を正の値にする
pub fn modified(meta: &std::fs::Metadata) -> io::Result<DateTime<Utc>> {
    let (seconds, nanos) = match meta.modified()?.duration_since(std::time::UNIX_EPOCH) {
        Ok(after) => (after.as_secs() as i64, after.subsec_nanos()),
        Err(before) => {
            let before = before.duration();
            match before.subsec_nanos() {
                0 => (-(before.as_secs() as i64), 0),
                nanos => (-(before.as_secs() as i64) - 1, 1_000_000_000 - nanos),
            }
        }
    };
    DateTime::from_timestamp(seconds, nanos)
        .ok_or_else(|| io::Error::other("modification time out of range"))
}

/// ないパス（途中がディレクトリでないものを含む）
pub(crate) fn is_missing(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::NotFound || error.raw_os_error() == Some(not_a_directory())
}

#[cfg(unix)]
fn not_a_directory() -> i32 {
    libc::ENOTDIR
}

#[cfg(not(unix))]
fn not_a_directory() -> i32 {
    -1
}

fn unlistable(rel: &str, error: &io::Error) -> ChildListing {
    ChildListing::Unlistable(format!("failed to list {rel:?}: {error}"))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    fn listed(listing: ChildListing) -> Vec<FileNode> {
        match listing {
            ChildListing::Children(mut nodes) => {
                nodes.sort_by(|a, b| a.name.cmp(&b.name));
                nodes
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_file_a_link_to_a_file_a_dangling_link_and_a_missing_path_are_not_directories() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("f"), "x").unwrap();
        symlink("f", root.path().join("to_file")).unwrap();
        symlink("nowhere", root.path().join("dangling")).unwrap();
        for path in ["f", "to_file", "dangling", "missing", "f/below"] {
            let listing = list_children(root.path(), path, &[], 10).unwrap();
            assert!(
                matches!(listing, ChildListing::NotDirectory),
                "{path}: {listing:?}"
            );
        }
    }

    #[test]
    fn an_unreadable_directory_is_unlistable_with_its_path_and_cause() {
        let root = tempfile::tempdir().unwrap();
        let locked = root.path().join("locked");
        std::fs::create_dir(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let listing = list_children(root.path(), "locked", &[], 10).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
        if std::fs::read_dir(&locked).is_ok() && unsafe { libc::geteuid() } == 0 {
            return; // root はモードによらず読める
        }
        let ChildListing::Unlistable(reason) = listing else {
            panic!("{listing:?}");
        };
        assert!(
            reason.contains("locked") && reason.contains("ermission"),
            "{reason}"
        );
    }

    #[test]
    fn a_link_to_a_directory_lists_the_children_of_its_target() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("d")).unwrap();
        std::fs::write(root.path().join("d/a.txt"), "x").unwrap();
        symlink("d", root.path().join("to_dir")).unwrap();
        let names: Vec<String> = listed(list_children(root.path(), "to_dir/", &[], 10).unwrap())
            .into_iter()
            .map(|node| node.name)
            .collect();
        assert_eq!(names, ["a.txt"]);
    }

    #[test]
    fn a_time_before_1970_is_read_with_whole_seconds_rounded_down() {
        use std::time::{Duration, UNIX_EPOCH};
        let root = tempfile::tempdir().unwrap();
        let file = std::fs::File::create(root.path().join("old")).unwrap();
        file.set_modified(UNIX_EPOCH - Duration::from_millis(1500))
            .unwrap();
        let nodes = listed(list_children(root.path(), "", &[], 10).unwrap());
        let mtime = nodes[0].mtime.unwrap();
        assert_eq!(
            (mtime.timestamp(), mtime.timestamp_subsec_nanos()),
            (-2, 500_000_000)
        );
    }

    #[test]
    fn the_listing_stops_at_the_entry_limit() {
        let root = tempfile::tempdir().unwrap();
        for name in ["a", "b", "c"] {
            std::fs::write(root.path().join(name), "x").unwrap();
        }
        assert_eq!(
            listed(list_children(root.path(), "", &[], 2).unwrap()).len(),
            2
        );
    }

    #[test]
    fn exclude_applies_to_the_path_of_each_child_from_the_root_dir() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("a/vendor")).unwrap();
        std::fs::create_dir_all(root.path().join("vendor")).unwrap();
        let exclude = vec!["vendor/**".to_string()];
        let names = |path: &str| -> Vec<String> {
            listed(list_children(root.path(), path, &exclude, 10).unwrap())
                .into_iter()
                .map(|node| node.name)
                .collect()
        };
        assert_eq!(names(""), ["a"]);
        assert_eq!(names("a"), ["vendor"]);
    }
}
