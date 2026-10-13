//! 書き込みと symlink の作成。どちらもローカルの経路と同じ `machine_ops` で行い、失敗は種類で返す。
//!
//! 既存のファイルはその場で書き換えるので、所有者と権限はそのまま残る（docs/ir/merge/permissions.md
//! の REQ-merge-013、docs/ir/merge/concurrency.md の REQ-merge-039）。新しいファイルの権限は、ほかの
//! 経路と同じく書いた後に merge が設定の値で付ける。root で動く（sudo の）エージェントだけは、新しく
//! 作ったファイル・リンク・親ディレクトリの所有者を、ログインした利用者として作るほかの経路と同じに
//! なるよう既定の利用者にする（`ownership`）。

use std::io::Write as _;
use std::path::Path;

use remote_merge_core::machine_ops;
use remote_merge_core::op_failure::{from_io_error, OpFailure};
use remote_merge_core::relative_path::EntryPath;

use super::ownership::{missing_ancestors, own_created, OwnerOps};
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
/// ファイルと親ディレクトリの所有者を、中身を書く前に既定の利用者にする（[`own_created`]）。権限は、
/// どの経路でも書いた後に merge が設定の値で付ける
pub(super) fn write_part(
    root: &Path,
    path: &EntryPath,
    content: &[u8],
    part: &Part,
    config: &MetadataConfig,
) -> Result<(), OpFailure> {
    let owning = part.first && part.created && owner_to_set(config);
    let new_dirs = if owning {
        missing_ancestors(root, path)
    } else {
        Vec::new()
    };
    let mut file = machine_ops::open_for_write(root, path, part.first)?;
    if owning {
        let full = root.join(path.as_str());
        let mut ops = Owner {
            config,
            entry: &full,
            file: Some(&file),
        };
        own_created(&new_dirs, path.as_typed(), &mut ops)?;
    }
    file.write_all(content)
        .map_err(|error| from_io_error(&error))
}

/// symlink を作る。作成先に何かがあれば置き換えずに失敗する。root で動くときは、作ったリンクと親
/// ディレクトリの所有者を既定の利用者にする
pub(super) fn symlink(
    root: &Path,
    path: &EntryPath,
    target: &str,
    config: &MetadataConfig,
) -> Result<(), OpFailure> {
    let owning = owner_to_set(config);
    let new_dirs = if owning {
        missing_ancestors(root, path)
    } else {
        Vec::new()
    };
    machine_ops::create_symlink(root, path, target)?;
    if !owning {
        return Ok(());
    }
    let link = root.join(path.as_str());
    let mut ops = Owner {
        config,
        entry: &link,
        file: None,
    };
    own_created(&new_dirs, path.as_typed(), &mut ops)
}

/// 本物の所有者の操作。開いたファイルがあればそれに（パスの差し替えに惑わされないよう fchown で）、
/// なければ項目そのもの（リンクは辿らない）に付ける
struct Owner<'a> {
    config: &'a MetadataConfig,
    entry: &'a Path,
    file: Option<&'a std::fs::File>,
}

impl OwnerOps for Owner<'_> {
    fn chown_dir(&mut self, dir: &Path) -> std::io::Result<()> {
        std::os::unix::fs::lchown(dir, self.config.default_uid, self.config.default_gid)
    }
    fn chown_entry(&mut self) -> std::io::Result<()> {
        let (uid, gid) = (self.config.default_uid, self.config.default_gid);
        match self.file {
            Some(file) => std::os::unix::fs::fchown(file, uid, gid),
            None => std::os::unix::fs::lchown(self.entry, uid, gid),
        }
    }
    fn remove_entry(&mut self) -> std::io::Result<()> {
        std::fs::remove_file(self.entry)
    }
    fn remove_dir(&mut self, dir: &Path) -> std::io::Result<()> {
        std::fs::remove_dir(dir)
    }
}

/// root で動き、既定の利用者が決まっているか（sudo のエージェント）
fn owner_to_set(config: &MetadataConfig) -> bool {
    is_root() && (config.default_uid.is_some() || config.default_gid.is_some())
}

fn is_root() -> bool {
    // SAFETY: geteuid は引数を取らず、いつも成功して呼び出したプロセスの実効 uid を返す
    unsafe { libc::geteuid() == 0 }
}
