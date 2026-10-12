//! このマシンでの削除と権限の変更（ローカルとエージェントの経路）。
//!
//! 途中の symlink は root_dir の外を指していても辿る。失敗の文言は操作・パス・原因を含む。

use std::path::Path;

use crate::relative_path::RelPath;

/// 権限の変更で付けてよい値の上限。setuid・setgid・sticky は付けない
pub const MAX_PERMISSION_MODE: u32 = 0o777;

/// root_dir `root` の下の `path` を消す。末尾の symlink ならリンクだけを消し、ディレクトリと
/// ないパスは消さずに失敗する
pub fn remove(root: &Path, path: &RelPath) -> anyhow::Result<()> {
    let rel = path.as_str();
    std::fs::remove_file(root.join(rel))
        .map_err(|error| anyhow::anyhow!("failed to remove {rel:?}: {error}"))
}

/// root_dir `root` の下の `path`（末尾の symlink はリンク先）の権限を `mode` にする。
/// ディレクトリはすでに付いている setuid・setgid を残す（[`resulting_mode`]）
pub fn chmod(root: &Path, path: &RelPath, mode: u32) -> anyhow::Result<()> {
    let rel = path.as_str();
    check_mode(rel, mode)?;
    set_mode(&root.join(rel), mode).map_err(|error| {
        anyhow::anyhow!("failed to change permissions of {rel:?} to {mode:o}: {error}")
    })
}

/// 今の権限 `current` のものを `requested` に変えた後の権限。ディレクトリはすでに付いている
/// setuid・setgid を残し、0o777 の範囲のビットと sticky を `requested` にする（SSH の経路の
/// coreutils の chmod と同じ）。ファイルは `requested` そのものにする
pub fn resulting_mode(current: u32, requested: u32, directory: bool) -> u32 {
    if directory {
        (current & 0o6000) | (requested & 0o1777)
    } else {
        requested
    }
}

/// 0o777 を超える値（特別なビットを含む）を拒否する。`rel` は文言に出すパス
pub fn check_mode(rel: &str, mode: u32) -> anyhow::Result<()> {
    if mode > MAX_PERMISSION_MODE {
        anyhow::bail!(
            "failed to change permissions of {rel:?} to {mode:o}: modes above 777 (setuid, setgid or sticky) are not set"
        );
    }
    Ok(())
}

#[cfg(unix)]
fn set_mode(full: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(full)?;
    let mode = resulting_mode(meta.permissions().mode() & 0o7777, mode, meta.is_dir());
    std::fs::set_permissions(full, std::fs::Permissions::from_mode(mode))
}

/// Unix 以外では権限を変えない
#[cfg(not(unix))]
fn set_mode(full: &Path, _mode: u32) -> std::io::Result<()> {
    std::fs::metadata(full).map(|_| ())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn rel(path: &str) -> crate::relative_path::RelPath {
        crate::relative_path::RelPath::parse(path).unwrap()
    }
    use std::os::unix::fs::{symlink, PermissionsExt};

    fn mode_of(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    #[test]
    fn a_directory_keeps_setuid_and_setgid_and_a_file_gets_the_mode_itself() {
        let cases = [
            (0o2775, 0o775, true, 0o2775),
            (0o4755, 0o700, true, 0o4700),
            (0o3775, 0o775, true, 0o2775),
            (0o1777, 0o755, true, 0o755),
            (0o2775, 0o775, false, 0o775),
            (0o4755, 0o644, false, 0o644),
        ];
        for (current, requested, directory, expected) in cases {
            assert_eq!(
                resulting_mode(current, requested, directory),
                expected,
                "{current:o} {requested:o} {directory}"
            );
        }
    }

    #[test]
    fn a_mode_above_777_is_refused_without_change() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a.sh"), "x").unwrap();
        std::fs::set_permissions(
            root.path().join("a.sh"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        let error = chmod(root.path(), &rel("a.sh"), 0o4755)
            .unwrap_err()
            .to_string();
        assert!(error.contains("a.sh") && error.contains("4755"), "{error}");
        assert_eq!(mode_of(&root.path().join("a.sh")), 0o644);
    }

    #[test]
    fn a_trailing_slash_is_removed_before_the_operation() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("t.txt"), "x").unwrap();
        symlink("t.txt", root.path().join("l")).unwrap();
        remove(root.path(), &rel("l/")).unwrap();
        assert!(root.path().join("t.txt").exists());
        assert!(root.path().join("l").symlink_metadata().is_err());
    }
}
