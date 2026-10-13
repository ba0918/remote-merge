//! 書き込みと symlink の作成。どちらもローカルの経路と同じ `machine_ops` で行い、失敗は種類で返す。
//!
//! 既存のファイルはその場で書き換えるので、所有者と権限はそのまま残る（docs/ir/merge/permissions.md
//! の REQ-merge-013、docs/ir/merge/concurrency.md の REQ-merge-039）。新しく作ったファイルとリンクだけに、
//! エージェントを起動したときの既定の権限と（root で動くときの）所有者を付け、付けられなければ
//! 失敗にする。

use std::io::Write as _;
use std::path::Path;

use remote_merge_core::machine_ops;
use remote_merge_core::op_failure::{from_io_error, OpFailure};
use remote_merge_core::relative_path::EntryPath;

use crate::agent::server::MetadataConfig;

/// 分けて送られる書き込みの一つの分
pub(super) struct Part {
    pub first: bool,
    pub last: bool,
    /// 最初の分を書く前に書き込み先がなかった
    pub created: bool,
}

/// 書き込み先がまだないか（最初の分を書く前に確かめる）
pub(super) fn is_new(root: &Path, path: &EntryPath) -> bool {
    std::fs::symlink_metadata(root.join(path.as_str())).is_err()
}

/// 一つの分を書く。最初の分は親を作って切り詰め、後の分は書き足す。最後の分で、新しく作った
/// ファイルに既定の権限と所有者を付ける
pub(super) fn write_part(
    root: &Path,
    path: &EntryPath,
    content: &[u8],
    part: &Part,
    config: &MetadataConfig,
) -> Result<(), OpFailure> {
    let mut file = machine_ops::open_for_write(root, path, part.first)?;
    file.write_all(content)
        .map_err(|error| from_io_error(&error))?;
    if !(part.last && part.created) {
        return Ok(());
    }
    if let Some(mode) = config.file_permissions {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(mode & 0o7777))
            .map_err(|error| from_io_error(&error))?;
    }
    if is_root() && (config.default_uid.is_some() || config.default_gid.is_some()) {
        std::os::unix::fs::fchown(&file, config.default_uid, config.default_gid)
            .map_err(|error| from_io_error(&error))?;
    }
    Ok(())
}

/// symlink を作る。作成先に何かがあれば置き換えずに失敗する。root で動くときは、作ったリンクの
/// 所有者を既定の利用者にする
pub(super) fn symlink(
    root: &Path,
    path: &EntryPath,
    target: &str,
    config: &MetadataConfig,
) -> Result<(), OpFailure> {
    machine_ops::create_symlink(root, path, target)?;
    if is_root() && (config.default_uid.is_some() || config.default_gid.is_some()) {
        std::os::unix::fs::lchown(
            root.join(path.as_str()),
            config.default_uid,
            config.default_gid,
        )
        .map_err(|error| from_io_error(&error))?;
    }
    Ok(())
}

fn is_root() -> bool {
    // SAFETY: geteuid は引数を取らず、いつも成功して呼び出したプロセスの実効 uid を返す
    unsafe { libc::geteuid() == 0 }
}
