//! merge と sync が書いた後の権限（docs/ir/merge/permissions.md の REQ-merge-012・013・036・037）。
//! 経路の権限の変更の結果は `settle_change` に渡し、`接続断` の後は書き込み先を確かめる

use remote_merge_core::change_check::Change;
use remote_merge_core::relative_path::EntryPath;
use remote_merge_engine::treat::merge_permissions::{FilePermissions, written_file};

use super::core::CoreRuntime;
use super::merge_apply::{ApplyOptions, ChangeKind, PlannedChange};
use super::merge_inspect::Inspected;
use crate::config::{resolve_dir_permissions, resolve_file_permissions};
use crate::side::Side;

impl CoreRuntime {
    /// 書いた後の権限。作ったディレクトリは書き込み先に設定された権限にし（REQ-merge-012）、書いた
    /// ファイルの権限は treat の `merge_permissions` が決める（REQ-merge-013・036・037）
    pub(super) fn set_permissions(
        &mut self,
        target: &Side,
        entry: &EntryPath,
        change: &PlannedChange<'_>,
        inspected: &Inspected,
        options: &ApplyOptions<'_>,
    ) -> Result<(), String> {
        let (file_mode, dir_mode) = self.configured_modes(target);
        for directory in &inspected.missing_ancestors {
            self.chmod(target, directory, dir_mode).map_err(|error| {
                format!(
                    "content written, but permissions of the created directory {} not changed: \
                     {error:#}",
                    directory.as_typed()
                )
            })?;
        }
        if change.kind != ChangeKind::WriteFile {
            return Ok(());
        }
        let states = &change.comparison.states;
        let mode = match written_file(states, options.with_permissions, file_mode) {
            FilePermissions::Keep => return Ok(()),
            FilePermissions::Set(mode) => mode,
            FilePermissions::Fail(error) => return Err(error.to_string()),
        };
        self.chmod(target, entry, mode)
            .map_err(|error| format!("content written, but permissions not changed: {error:#}"))
    }

    fn chmod(&mut self, target: &Side, entry: &EntryPath, mode: u32) -> anyhow::Result<()> {
        let mut io = super::target_io::for_side(target, self);
        let changed = io.chmod_file(self, entry, mode);
        self.settle_change(target, entry, Change::Chmod { mode }, changed)
    }

    /// 書き込み先に設定された新しいファイルとディレクトリの権限（サーバの設定、なければ defaults）
    fn configured_modes(&self, target: &Side) -> (u32, u32) {
        let defaults = &self.config.defaults;
        match target {
            Side::Remote(name) => match self.config.servers.get(name) {
                Some(server) => (
                    resolve_file_permissions(server, defaults),
                    resolve_dir_permissions(server, defaults),
                ),
                None => (defaults.file_permissions, defaults.dir_permissions),
            },
            Side::Local => (defaults.file_permissions, defaults.dir_permissions),
        }
    }
}
