//! CoreRuntime のバックアップの操作（予約・保存・片付け）

use super::core::CoreRuntime;

impl CoreRuntime {
    /// バックアップのセッションを予約し、書き込み先 `targets` ごとの領域を書き込む前に作る
    pub fn reserve_backup_session(
        &mut self,
        targets: &[&crate::side::Side],
    ) -> anyhow::Result<String> {
        let session_id = self.backup_store.reserve_session()?;
        for target in targets {
            self.backup_store
                .prepare_target(&self.config, target, &session_id)?;
        }
        Ok(session_id)
    }

    /// テストだけ: rollback が書き戻す前の内容を退避した直後に呼ぶ関数を置く
    #[cfg(feature = "test-utils")]
    pub fn set_after_pre_rollback_save(&mut self, hook: super::core::PathHook) {
        self.after_pre_rollback_save = Some(hook);
    }

    pub fn finish_backup_session(&self, session_id: &str) {
        self.backup_store.finish_session(session_id);
    }

    pub fn cleanup_expired_backups(&self) -> anyhow::Result<usize> {
        self.backup_store.cleanup_expired(&self.config)
    }

    pub fn save_backup(
        &mut self,
        target: &crate::side::Side,
        path: &str,
        session_id: &str,
        force: bool,
    ) -> anyhow::Result<String> {
        self.save_backup_if_exists(target, path, session_id, force)?
            .ok_or_else(|| anyhow::anyhow!("backup source does not exist: {path}"))
    }

    pub fn save_backup_if_exists(
        &mut self,
        target: &crate::side::Side,
        path: &str,
        session_id: &str,
        force: bool,
    ) -> anyhow::Result<Option<String>> {
        let inspected = self.inspect_path(target, path)?;
        match inspected {
            super::target_io::TargetPath::File { real_path } => {
                let content = self.read_file_bytes(target, path, force)?;
                self.backup_store
                    .save_file(&self.config, target, session_id, path, &real_path, &content)
                    .map(Some)
            }
            super::target_io::TargetPath::Symlink { link_target, .. } => self
                .backup_store
                .save_symlink(
                    &self.config,
                    target,
                    session_id,
                    path,
                    super::backup_store::SymlinkBackup {
                        link_target: &link_target,
                        expected: None,
                    },
                )
                .map(Some),
            super::target_io::TargetPath::Missing { .. } => Ok(None),
        }
    }

    pub fn save_symlink_update(
        &mut self,
        target: &crate::side::Side,
        path: &str,
        session_id: &str,
        expected_target: &str,
    ) -> anyhow::Result<Option<String>> {
        let inspected = self.inspect_path(target, path)?;
        let super::target_io::TargetPath::Symlink { link_target, .. } = inspected else {
            return match inspected {
                super::target_io::TargetPath::Missing { .. } => Ok(None),
                _ => anyhow::bail!("destination is no longer a symlink: {path}"),
            };
        };
        let parent = std::path::Path::new(path)
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""));
        let real_parent = match self.inspect_path(target, &parent.to_string_lossy())? {
            super::target_io::TargetPath::File { real_path }
            | super::target_io::TargetPath::Symlink { real_path, .. } => real_path,
            super::target_io::TargetPath::Missing { .. } => {
                anyhow::bail!("parent directory no longer exists: {path}")
            }
        };
        self.backup_store
            .save_symlink(
                &self.config,
                target,
                session_id,
                path,
                super::backup_store::SymlinkBackup {
                    link_target: &link_target,
                    expected: Some((std::path::Path::new(expected_target), &real_parent)),
                },
            )
            .map(Some)
    }
}
