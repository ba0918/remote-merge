//! 書き込みと symlink の作成。どちらもローカルの経路と同じ `machine_ops` で行い、失敗は種類で返す。
//!
//! 既存のファイルはその場で書き換えるので、所有者と権限はそのまま残る（docs/ir/merge/permissions.md
//! の REQ-merge-013、docs/ir/merge/concurrency.md の REQ-merge-039）。新しいファイルの権限は、ほかの
//! 経路と同じく書いた後に merge が設定の値で付ける。root で動く（sudo の）エージェントだけは、新しく
//! 作ったファイルとリンクの所有者を、ログインした利用者として作るほかの経路と同じになるよう既定の
//! 利用者にし、付けられなければ作ったものを消して何も変えずに失敗にする。

use std::io::Write as _;
use std::path::Path;

use remote_merge_core::machine_ops;
use remote_merge_core::op_failure::{from_io_error, OpFailure};
use remote_merge_core::relative_path::EntryPath;

use crate::agent::server::MetadataConfig;

/// 分けて送られる書き込みの一つの分
pub(super) struct Part {
    pub first: bool,
    /// 最初の分を書く前に書き込み先がなかった
    pub created: bool,
}

/// 書き込み先がまだないか（最初の分を書く前に確かめる）
pub(super) fn is_new(root: &Path, path: &EntryPath) -> bool {
    std::fs::symlink_metadata(root.join(path.as_str())).is_err()
}

/// 一つの分を書く。最初の分は親を作って切り詰め、後の分は書き足す。root で動くときは、新しく作った
/// ファイルの所有者を、中身を書く前に既定の利用者にする（付けられなければ作ったファイルを消して、
/// 何も書かずに失敗にする）。権限は、どの経路でも書いた後に merge が設定の値で付ける
pub(super) fn write_part(
    root: &Path,
    path: &EntryPath,
    content: &[u8],
    part: &Part,
    config: &MetadataConfig,
) -> Result<(), OpFailure> {
    let mut file = machine_ops::open_for_write(root, path, part.first)?;
    if part.first && part.created && owner_to_set(config) {
        if let Err(error) = std::os::unix::fs::fchown(&file, config.default_uid, config.default_gid)
        {
            drop(file);
            let _ = std::fs::remove_file(root.join(path.as_str()));
            return Err(from_io_error(&error));
        }
    }
    file.write_all(content)
        .map_err(|error| from_io_error(&error))
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
    if !owner_to_set(config) {
        return Ok(());
    }
    let link = root.join(path.as_str());
    if let Err(error) = std::os::unix::fs::lchown(&link, config.default_uid, config.default_gid) {
        let _ = std::fs::remove_file(&link);
        return Err(from_io_error(&error));
    }
    Ok(())
}

/// root で動き、既定の利用者が決まっているか（sudo のエージェント）
fn owner_to_set(config: &MetadataConfig) -> bool {
    is_root() && (config.default_uid.is_some() || config.default_gid.is_some())
}

fn is_root() -> bool {
    // SAFETY: geteuid は引数を取らず、いつも成功して呼び出したプロセスの実効 uid を返す
    unsafe { libc::geteuid() == 0 }
}
