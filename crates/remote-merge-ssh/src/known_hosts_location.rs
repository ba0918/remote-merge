//! 接続に使う known_hosts の場所。
//!
//! 製品は利用者の `~/.ssh/known_hosts` を使う。テストは `SshConfig::known_hosts` に一時ディレクトリの
//! ファイルを差し込み、利用者のファイルに触れない（docs/decision/records/2026-10-12-foundation-1.md の
//! B9）。テスト用の機能（test-utils）を有効にした組み立てでは、差し込まずに利用者のファイルを
//! 使おうとすると panic して、テストが利用者のファイルに書き足すことを止める。

use std::path::{Path, PathBuf};

/// 利用者の known_hosts（ホームディレクトリの `.ssh/known_hosts`）
pub(crate) fn default_known_hosts_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ssh").join("known_hosts"))
}

/// 接続に使う known_hosts の場所。`configured` がなければ利用者の known_hosts
pub(crate) fn known_hosts_location(configured: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = configured {
        return Some(path.to_path_buf());
    }
    let path = default_known_hosts_path();
    #[cfg(all(feature = "test-utils", unix))]
    if let Some(path) = &path {
        super::known_hosts_guard::refuse_the_users_file(path);
    }
    path
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    #[test]
    fn a_configured_file_is_used_as_it_is() {
        assert_eq!(
            known_hosts_location(Some(Path::new("/tmp/test/known_hosts"))),
            Some(PathBuf::from("/tmp/test/known_hosts"))
        );
    }

    #[test]
    fn the_default_is_the_known_hosts_under_the_home_directory() {
        if let Some(path) = default_known_hosts_path() {
            assert!(path.ends_with(".ssh/known_hosts"), "{path:?}");
        }
    }
}
