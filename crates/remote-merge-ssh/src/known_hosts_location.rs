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
    #[cfg(feature = "test-utils")]
    refuse_the_users_file_in_tests(path.as_deref());
    path
}

/// `path` が、利用者のホームディレクトリ `users_home` の known_hosts か
#[cfg(any(test, feature = "test-utils"))]
fn is_users_known_hosts(path: &Path, users_home: &Path) -> bool {
    path == users_home.join(".ssh").join("known_hosts")
}

/// テスト用の組み立てで、利用者本人の known_hosts を使おうとしたら止める。HOME を一時ディレクトリに
/// した子プロセス（CLI を動かすテスト）は、利用者のファイルではないので通す
#[cfg(feature = "test-utils")]
fn refuse_the_users_file_in_tests(path: Option<&Path>) {
    let (Some(path), Some(users_home)) = (path, users_home_from_the_account()) else {
        return;
    };
    assert!(
        !is_users_known_hosts(path, &users_home),
        "tests must not use the user's known_hosts ({}): set SshConfig::known_hosts to a temporary file",
        path.display()
    );
}

/// 実行している利用者のアカウントのホームディレクトリ（環境変数 HOME によらない）
#[cfg(all(feature = "test-utils", unix))]
fn users_home_from_the_account() -> Option<PathBuf> {
    use std::ffi::CStr;
    use std::os::unix::ffi::OsStrExt;
    let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
    let mut buffer = vec![0 as libc::c_char; 16 * 1024];
    let mut result: *mut libc::passwd = std::ptr::null_mut();
    // SAFETY: entry・buffer・result は呼び出しの間生きており、buffer の長さを正しく渡す
    let status = unsafe {
        libc::getpwuid_r(
            libc::geteuid(),
            &mut entry,
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut result,
        )
    };
    if status != 0 || result.is_null() || entry.pw_dir.is_null() {
        return None;
    }
    // SAFETY: pw_dir は getpwuid_r が buffer の中に書いた NUL 終端の文字列
    let dir = unsafe { CStr::from_ptr(entry.pw_dir) };
    Some(PathBuf::from(std::ffi::OsStr::from_bytes(dir.to_bytes())))
}

#[cfg(all(feature = "test-utils", not(unix)))]
fn users_home_from_the_account() -> Option<PathBuf> {
    None
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

    // 利用者本人の known_hosts を使おうとしたら止める（ファイルには触れない）
    #[cfg(all(feature = "test-utils", unix))]
    #[test]
    #[should_panic(expected = "tests must not use the user's known_hosts")]
    fn tests_are_stopped_before_using_the_users_own_known_hosts() {
        let home = users_home_from_the_account().expect("the account has a home directory");
        refuse_the_users_file_in_tests(Some(&home.join(".ssh").join("known_hosts")));
    }

    #[test]
    fn only_the_known_hosts_of_the_users_own_home_is_the_users_file() {
        let home = Path::new("/home/someone");
        assert!(is_users_known_hosts(
            Path::new("/home/someone/.ssh/known_hosts"),
            home
        ));
        assert!(!is_users_known_hosts(
            Path::new("/tmp/fixture/.ssh/known_hosts"),
            home
        ));
        assert!(!is_users_known_hosts(
            Path::new("/home/someone/.ssh/other"),
            home
        ));
    }
}
