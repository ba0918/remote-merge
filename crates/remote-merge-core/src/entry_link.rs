//! パスの項目そのもの（最後の要素のリンクは辿らない）が symlink なら、そのリンク先を読む手順
//! （ローカルの経路）。親ディレクトリを一覧せずに一つの項目だけを見る。
//!
//! 結果の分け方は、親ディレクトリの子の一覧からリンク先を取り出したときと同じにする:
//! symlink ならリンク先の文字列（読んだ文字列そのまま）、symlink でないか、ないか、途中が
//! ディレクトリでないなら None、それ以外で読めなければ Err。

use std::path::Path;

use crate::listing::is_missing;
use crate::relative_path::RelPath;

/// root_dir `root` の下の `path` の項目が symlink ならそのリンク先の文字列を返す
pub fn link_target_of_entry(root: &Path, path: &RelPath) -> anyhow::Result<Option<String>> {
    let path = path.as_str();
    let full = root.join(path);
    let meta = match full.symlink_metadata() {
        Ok(meta) => meta,
        Err(error) if is_missing(&error) => return Ok(None),
        Err(error) => anyhow::bail!("failed to read {path:?}: {error}"),
    };
    if !meta.file_type().is_symlink() {
        return Ok(None);
    }
    let target = std::fs::read_link(&full).map_err(|error| {
        anyhow::anyhow!("failed to read {path:?}: cannot read the link target: {error}")
    })?;
    Ok(Some(target.to_string_lossy().into_owned()))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn rel(path: &str) -> crate::relative_path::RelPath {
        crate::relative_path::RelPath::parse(path).unwrap()
    }
    use std::os::unix::fs::symlink;

    fn root_with_entries() -> tempfile::TempDir {
        let root = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(root.path().join("dir")).unwrap();
        std::fs::write(root.path().join("file"), "x").unwrap();
        symlink(" target ", root.path().join("spaced")).unwrap();
        symlink("sub\\b.txt", root.path().join("backslash")).unwrap();
        symlink("dir", root.path().join("dirlink")).unwrap();
        root
    }

    #[test]
    fn a_symlink_gives_its_link_target_as_read_without_following_it() {
        let root = root_with_entries();
        let target = |path: &str| link_target_of_entry(root.path(), &rel(path)).unwrap();
        assert_eq!(target("spaced").as_deref(), Some(" target "));
        assert_eq!(target("backslash").as_deref(), Some("sub\\b.txt"));
        assert_eq!(target("dirlink").as_deref(), Some("dir"));
    }

    #[test]
    fn an_entry_that_is_not_a_symlink_or_is_absent_has_no_link_target() {
        let root = root_with_entries();
        for path in [
            "file",
            "dir",
            "absent",
            "absent/link",
            "file/link",
            "dirlink/absent",
        ] {
            assert_eq!(
                link_target_of_entry(root.path(), &rel(path)).unwrap(),
                None,
                "{path}"
            );
        }
    }

    #[test]
    fn an_entry_below_an_unsearchable_directory_fails() {
        use std::os::unix::fs::PermissionsExt;
        let root = root_with_entries();
        let locked = root.path().join("locked");
        std::fs::create_dir(&locked).unwrap();
        symlink("t", locked.join("link")).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o600)).unwrap();
        let result = link_target_of_entry(root.path(), &rel("locked/link"));
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
        // root では権限に関わらず読めるので、読めたときは確かめない
        if running_as_root() {
            return;
        }
        assert!(result.is_err(), "{result:?}");
    }

    fn running_as_root() -> bool {
        // SAFETY: geteuid は引数を取らず、失敗しない
        unsafe { libc::geteuid() == 0 }
    }
}
