//! エージェントの、一つの項目を辿らずに読む要求（ReadEntryLink）。ローカルの経路と同じ手順
//! （remote_merge_core::entry_link）で読み、通信の形式にする。

use std::path::Path;

use super::protocol::{AgentEntryLink, AgentResponse};

/// root_dir からの `path` の項目そのものを読んだ応答
pub fn read_entry_link(root: &Path, path: &str) -> AgentResponse {
    let result = match remote_merge_core::entry_link::link_target_of_entry(root, path) {
        Ok(Some(target)) => AgentEntryLink::Symlink { target },
        Ok(None) => AgentEntryLink::NotSymlink,
        Err(error) => AgentEntryLink::Error {
            message: error.to_string(),
        },
    };
    AgentResponse::EntryLink { result }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    fn result(root: &Path, path: &str) -> AgentEntryLink {
        match read_entry_link(root, path) {
            AgentResponse::EntryLink { result } => result,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_symlink_gives_its_link_target_as_read_without_following_it() {
        let root = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(root.path().join("dir")).unwrap();
        symlink(" t\\x ", root.path().join("link")).unwrap();
        symlink("dir", root.path().join("dirlink")).unwrap();
        for (path, target) in [("link", " t\\x "), ("dirlink", "dir")] {
            assert_eq!(
                result(root.path(), path),
                AgentEntryLink::Symlink {
                    target: target.into()
                }
            );
        }
    }

    #[test]
    fn an_entry_that_is_not_a_symlink_or_is_absent_is_not_a_symlink() {
        let root = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(root.path().join("dir")).unwrap();
        std::fs::write(root.path().join("file"), "x").unwrap();
        for path in ["file", "dir", "absent", "file/x", "absent/x"] {
            assert_eq!(
                result(root.path(), path),
                AgentEntryLink::NotSymlink,
                "{path}"
            );
        }
    }

    #[test]
    fn an_entry_in_a_search_only_directory_is_read() {
        let root = tempfile::TempDir::new().unwrap();
        let dir = root.path().join("dir");
        std::fs::create_dir(&dir).unwrap();
        symlink("t", dir.join("link")).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o300)).unwrap();
        let read = result(root.path(), "dir/link");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(read, AgentEntryLink::Symlink { target: "t".into() });
    }

    #[test]
    fn a_path_outside_the_root_is_refused_with_the_wording_of_the_other_routes() {
        let root = tempfile::TempDir::new().unwrap();
        for path in ["../x", "/etc/passwd"] {
            let expected = remote_merge_core::entry_link::link_target_of_entry(root.path(), path)
                .unwrap_err()
                .to_string();
            assert_eq!(
                result(root.path(), path),
                AgentEntryLink::Error { message: expected },
                "{path}"
            );
        }
    }
}
