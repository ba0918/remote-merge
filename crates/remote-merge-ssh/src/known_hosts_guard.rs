//! テスト用の組み立てで、テストが利用者本人の known_hosts を使うことを止める見張り
//! （docs/decision/records/2026-10-12-foundation-1.md の B9）。
//!
//! 見張りは、確かめられないとき（アカウントのホームディレクトリを引けないなど）も止める側に倒す。

use std::path::{Component, Path, PathBuf};

/// アカウントの情報を引くときの、最初のバッファの大きさ
const INITIAL_BUFFER: usize = 1024;
/// バッファを広げる上限。これを超えても足りなければ確かめられないとする
const MAX_BUFFER: usize = 1024 * 1024;

/// アカウントの情報を一度引いた結果
#[derive(Debug)]
enum Lookup {
    Found(PathBuf),
    /// バッファが足りない（ERANGE）
    TooSmall,
    /// アカウントがない
    NoEntry,
    /// そのほかの失敗（errno）
    Failed(i32),
}

/// `path` が利用者本人の known_hosts なら止める。確かめられないときも止める
// テスト用の機能を使わない単体テストの組み立てでは、見張りの部品だけを確かめる
#[cfg_attr(not(feature = "test-utils"), allow(dead_code))]
pub(crate) fn refuse_the_users_file(path: &Path) {
    if let Err(reason) = check_not_users_file(path, users_home()) {
        panic!("tests must not use the user's known_hosts: {reason}");
    }
}

/// `path` が、ホームディレクトリ `users_home` の known_hosts と同じ場所でなければ Ok
fn check_not_users_file(path: &Path, users_home: Result<PathBuf, String>) -> Result<(), String> {
    let users_home = users_home.map_err(|reason| {
        format!(
            "{reason}; set SshConfig::known_hosts to a temporary file ({})",
            path.display()
        )
    })?;
    let users_file = located(&users_home.join(".ssh").join("known_hosts"));
    if located(path) == users_file {
        return Err(format!(
            "{}: set SshConfig::known_hosts to a temporary file",
            path.display()
        ));
    }
    Ok(())
}

/// 比べるための場所。存在する最も深い祖先を実パスにし、残りの名前をつなぐ（ファイルがなければ
/// 親ディレクトリで比べる）
fn located(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut rest = Vec::new();
    loop {
        if let Ok(real) = existing.canonicalize() {
            let mut located = real;
            for name in rest.iter().rev() {
                located.push(name);
            }
            return located;
        }
        match (
            existing.file_name().map(|name| name.to_owned()),
            existing.parent(),
        ) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                existing = parent.to_path_buf();
            }
            _ => return lexical(path),
        }
    }
}

/// 実パスを求められないときの、"." の要素を除いた字面の形
fn lexical(path: &Path) -> PathBuf {
    path.components()
        .filter(|component| !matches!(component, Component::CurDir))
        .collect()
}

/// アカウントのホームディレクトリ（環境変数 HOME によらない）
#[cfg_attr(not(feature = "test-utils"), allow(dead_code))]
fn users_home() -> Result<PathBuf, String> {
    home_from_lookup(lookup_with_buffer)
}

/// `lookup` をバッファの大きさを広げながら呼び、ホームディレクトリを求める
fn home_from_lookup(mut lookup: impl FnMut(usize) -> Lookup) -> Result<PathBuf, String> {
    let mut size = INITIAL_BUFFER;
    loop {
        match lookup(size) {
            Lookup::Found(home) => return Ok(home),
            Lookup::TooSmall if size < MAX_BUFFER => size *= 2,
            Lookup::TooSmall => {
                return Err(format!(
                    "the account entry does not fit in {MAX_BUFFER} bytes"
                ))
            }
            Lookup::NoEntry => return Err("the account has no entry".into()),
            Lookup::Failed(errno) => {
                return Err(format!(
                    "the account could not be looked up (errno {errno})"
                ))
            }
        }
    }
}

/// 大きさ `size` のバッファで、実行している利用者のアカウントを一度引く
#[cfg_attr(not(feature = "test-utils"), allow(dead_code))]
fn lookup_with_buffer(size: usize) -> Lookup {
    use std::ffi::CStr;
    use std::os::unix::ffi::OsStrExt;
    // SAFETY: passwd はポインタと整数だけの構造体で、ゼロで埋めた値は有効
    let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
    let mut buffer = vec![0 as libc::c_char; size];
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
    if status == libc::ERANGE {
        return Lookup::TooSmall;
    }
    if status != 0 {
        return Lookup::Failed(status);
    }
    if result.is_null() || entry.pw_dir.is_null() {
        return Lookup::NoEntry;
    }
    // SAFETY: pw_dir は getpwuid_r が buffer の中に書いた NUL 終端の文字列
    let dir = unsafe { CStr::from_ptr(entry.pw_dir) };
    Lookup::Found(PathBuf::from(std::ffi::OsStr::from_bytes(dir.to_bytes())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_buffer_too_small_for_the_account_is_widened_and_asked_again() {
        let mut asked = Vec::new();
        let home = home_from_lookup(|size| {
            asked.push(size);
            if size < 4 * INITIAL_BUFFER {
                Lookup::TooSmall
            } else {
                Lookup::Found(PathBuf::from("/home/someone"))
            }
        });
        assert_eq!(home, Ok(PathBuf::from("/home/someone")));
        assert_eq!(
            asked,
            [INITIAL_BUFFER, 2 * INITIAL_BUFFER, 4 * INITIAL_BUFFER]
        );
    }

    #[test]
    fn an_account_that_cannot_be_looked_up_is_an_error() {
        assert!(home_from_lookup(|_| Lookup::Failed(libc_eio())).is_err());
        assert!(home_from_lookup(|_| Lookup::NoEntry).is_err());
        assert!(home_from_lookup(|_| Lookup::TooSmall).is_err());
    }

    #[test]
    fn the_guard_stops_when_the_users_home_cannot_be_known() {
        let decision = check_not_users_file(
            Path::new("/tmp/fixture/.ssh/known_hosts"),
            Err("no account".into()),
        );
        assert!(decision.is_err(), "{decision:?}");
    }

    #[test]
    fn a_path_reaching_the_users_known_hosts_through_a_link_is_the_users_file() {
        let base = tempfile::TempDir::new().unwrap();
        let home = base.path().join("home");
        std::fs::create_dir(&home).unwrap();
        std::os::unix::fs::symlink(&home, base.path().join("alias")).unwrap();

        // ファイルも .ssh もない: 存在する祖先（リンク）を実パスにして比べる
        let through_link = base.path().join("alias/.ssh/known_hosts");
        assert!(check_not_users_file(&through_link, Ok(home.clone())).is_err());
        let with_dots = base.path().join("alias/./.ssh/known_hosts");
        assert!(check_not_users_file(&with_dots, Ok(home.clone())).is_err());

        // 別の場所のファイルは利用者のファイルではない
        let elsewhere = base.path().join("fixture/.ssh/known_hosts");
        assert!(check_not_users_file(&elsewhere, Ok(home)).is_ok());
    }

    fn libc_eio() -> i32 {
        5
    }
}
