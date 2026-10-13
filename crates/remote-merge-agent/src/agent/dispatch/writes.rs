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

use super::ownership::{own_created, OwnerOps};
use crate::agent::server::MetadataConfig;

/// 最初の分を書く。書き込み先を開き（ないときは新しいファイルとして O_EXCL で作る）、root で動くとき
/// は、新しく作ったファイルと親ディレクトリの所有者を、中身を書く前に既定の利用者にする
/// （[`own_created`]）。返したファイルに後の分を書き足し、パスで開き直さない（開き直すと、分の間に
/// 差し替えられたファイルに書く）。権限は、どの経路でも書いた後に merge が設定の値で付ける
pub(super) fn write_first(
    root: &Path,
    path: &EntryPath,
    content: &[u8],
    config: &MetadataConfig,
) -> Result<std::fs::File, OpFailure> {
    let opened = machine_ops::open_for_write(root, path)?;
    let mut file = opened.file;
    if opened.created && owner_to_set(config) {
        let full = root.join(path.as_str());
        let mut ops = Owner {
            config,
            entry: &full,
            file: Some(&file),
        };
        own_created(&opened.new_dirs, path.as_typed(), &mut ops)?;
    }
    write_more(&mut file, content)?;
    Ok(file)
}

/// 最初の分で開いたファイルに、後の分を書き足す
pub(super) fn write_more(file: &mut std::fs::File, content: &[u8]) -> Result<(), OpFailure> {
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
    let new_dirs = machine_ops::create_symlink_creating(root, path, target)?;
    if !owner_to_set(config) {
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
pub(super) struct Owner<'a> {
    pub(super) config: &'a MetadataConfig,
    pub(super) entry: &'a Path,
    pub(super) file: Option<&'a std::fs::File>,
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
    /// 作ったファイルを消す。パスにあるものが開いたファイルと別のもの（作った後に差し替えられた）なら
    /// 消さない
    fn remove_entry(&mut self) -> std::io::Result<()> {
        if let Some(file) = self.file {
            use std::os::unix::fs::MetadataExt;
            let (opened, now) = (file.metadata()?, std::fs::symlink_metadata(self.entry)?);
            if (opened.dev(), opened.ino()) != (now.dev(), now.ino()) {
                return Err(std::io::Error::other(
                    "the path no longer holds the created file",
                ));
            }
        }
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
