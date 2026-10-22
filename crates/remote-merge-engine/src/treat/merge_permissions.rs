//! merge と sync が書いたファイルに付ける権限（docs/ir/merge/permissions.md の REQ-merge-013・036・
//! 037）

use remote_merge_core::compare::{PathStates, Role, SideState};

/// 読み込み元の権限が分からないときの error（中身は書いた。REQ-merge-037。文言は IR で決めない）
pub const PERMISSIONS_UNKNOWN: &str =
    "content written, but permissions not copied because the source permissions are unknown";
/// 読み込み元の権限に 0o777 を超えるビットがあるときの error
pub const SPECIAL_BITS: &str = "content written, but permissions not copied because the source has \
     setuid, setgid or sticky bits";

/// 書いたファイルの権限の扱い
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilePermissions {
    /// 書き込み先の今の権限を保つ
    Keep,
    /// この権限にする
    Set(u32),
    /// 権限を変えずに、この error にする
    Fail(&'static str),
}

/// 比べたときの状態 `states` のパスに書いたファイルの権限。--with-permissions なら読み込み元（左）の
/// 走査で得た権限を写し（REQ-merge-036・037）、そうでなければ新しく作ったときだけ書き込み先に設定された
/// 権限 `configured` にする（既存のファイルの権限は保つ。REQ-merge-013）
pub fn written_file(
    states: &PathStates,
    with_permissions: bool,
    configured: u32,
) -> FilePermissions {
    if with_permissions {
        return match states.permissions.get(&Role::Left) {
            None => FilePermissions::Fail(PERMISSIONS_UNKNOWN),
            Some(&permissions) if permissions & !0o777 != 0 => FilePermissions::Fail(SPECIAL_BITS),
            Some(&permissions) => FilePermissions::Set(permissions),
        };
    }
    if matches!(states.states.get(&Role::Right), Some(SideState::Absent)) {
        FilePermissions::Set(configured)
    } else {
        FilePermissions::Keep
    }
}

#[cfg(test)]
#[path = "merge_permissions_tests.rs"]
mod tests;
