//! merge と sync が書いた後の権限（docs/ir/merge/permissions.md の REQ-merge-012・013・036・037）。
//! 経路の権限の変更の結果は `settle_change` に渡し、`接続断` の後は書き込み先を確かめる

use remote_merge_core::compare::{Role, SideState};
use remote_merge_core::relative_path::EntryPath;

use super::change_check::Change;
use super::core::CoreRuntime;
use super::merge_apply::{ApplyOptions, ChangeKind, PlannedChange};
use super::merge_inspect::Inspected;
use crate::config::{resolve_dir_permissions, resolve_file_permissions};
use crate::side::Side;

/// 読み込み元の権限が分からないときの error（中身は書いた。docs/ir/merge/permissions.md の
/// REQ-merge-037。文言は IR で決めない）
const PERMISSIONS_UNKNOWN: &str =
    "content written, but permissions not copied because the source permissions are unknown";
/// 読み込み元の権限に 0o777 を超えるビットがあるときの error
const SPECIAL_BITS: &str = "content written, but permissions not copied because the source has \
     setuid, setgid or sticky bits";

impl CoreRuntime {
    /// 書いた後の権限。作ったディレクトリは書き込み先に設定された権限にし（REQ-merge-012）、書いた
    /// ファイルは --with-permissions なら読み込み元の走査で得た権限を写し（REQ-merge-036・037）、
    /// そうでなければ新しく作ったときだけ設定された権限にする（既存のファイルの権限は保つ。REQ-merge-013）
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
        let comparison = change.comparison;
        let mode = if options.with_permissions {
            let Some(&permissions) = comparison.states.permissions.get(&Role::Left) else {
                return Err(PERMISSIONS_UNKNOWN.to_string());
            };
            if permissions & !0o777 != 0 {
                return Err(SPECIAL_BITS.to_string());
            }
            permissions
        } else if matches!(
            comparison.states.states.get(&Role::Right),
            Some(SideState::Absent)
        ) {
            file_mode
        } else {
            return Ok(());
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
