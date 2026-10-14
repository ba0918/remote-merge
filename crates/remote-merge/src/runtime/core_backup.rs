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

    /// バックアップのセッションの一覧（新しい順）
    pub fn list_backup_sessions(
        &mut self,
        side: &crate::side::Side,
    ) -> anyhow::Result<Vec<crate::service::types::BackupSession>> {
        self.backup_store.list_sessions(&self.config, side)
    }

    /// `target` の `path` を、セッション `session_id` にバックアップする（テストがセッションを
    /// 作るのに使う）。通常ファイルは中身を、symlink はリンク文字列を残す。`force` は読み込みの
    /// 大きさの上限を外す
    pub fn save_backup(
        &mut self,
        target: &crate::side::Side,
        path: &str,
        session_id: &str,
        force: bool,
    ) -> anyhow::Result<String> {
        use remote_merge_core::path_resolution::{ExistingKind, ResolvedPath};
        let entry = remote_merge_core::relative_path::EntryPath::parse(path)?;
        let mut io = super::target_io::for_side(target, self);
        match io.inspect_path(self, entry.as_rel())? {
            ResolvedPath::Exists {
                real_path,
                kind: ExistingKind::File,
            } => {
                let limit = remote_merge_core::op_failure::SizeLimit::from_force(force);
                let mut io = super::target_io::for_side(target, self);
                let mut read = io.read_entries(self, std::slice::from_ref(&entry), limit)?;
                let content = match read.pop() {
                    Some((_, Ok(content))) => content,
                    Some((_, Err(failure))) => anyhow::bail!("{failure}"),
                    None => anyhow::bail!("the backup source was not read: {path}"),
                };
                self.backup_store.save_file(
                    &self.config,
                    target,
                    session_id,
                    path,
                    std::path::Path::new(&real_path),
                    &content,
                )
            }
            ResolvedPath::Symlink { link_target, .. } => self.backup_store.save_symlink(
                &self.config,
                target,
                session_id,
                path,
                &super::backup_store::SymlinkBackup {
                    link_target: std::path::Path::new(&link_target),
                    expected: None,
                },
            ),
            ResolvedPath::Missing { .. } => anyhow::bail!("backup source does not exist: {path}"),
            _ => anyhow::bail!("backup source is not a regular file or a symlink: {path}"),
        }
    }
}
