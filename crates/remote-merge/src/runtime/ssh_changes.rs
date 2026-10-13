//! SSH の経路の削除・権限の変更・symlink の作成。コマンドが 0 以外で終わったときは、標準エラーの
//! 文言の終わりから失敗の種類を決める（docs/ir/paths/operations.md の REQ-paths-020、
//! docs/decision/records/2026-10-12-foundation-3.md の D2）

use remote_merge_core::machine_ops::PermissionMode;
use remote_merge_core::op_failure::{from_tool_message, OpError, OpFailure, Operation};
use remote_merge_core::relative_path::EntryPath;
use remote_merge_ssh::change_scripts;
use remote_merge_ssh::tree_parser::{build_remove_command, shell_escape};

use super::core::CoreRuntime;
use super::route_state::require_ssh_client;
use crate::merge::executor::remote_full_path;

impl CoreRuntime {
    /// ファイルか symlink を消す。symlink はリンクそのものを消す
    pub(crate) fn ssh_remove(&mut self, server: &str, path: &EntryPath) -> anyhow::Result<()> {
        let full = self.ssh_full_path(server, path)?;
        self.run_ssh_change(
            server,
            Operation::Remove,
            path,
            &build_remove_command(&full),
        )
    }

    /// 権限を変える。0o777 を超える値は送らずに断る（REQ-paths-015）
    pub(crate) fn ssh_chmod(
        &mut self,
        server: &str,
        path: &EntryPath,
        mode: u32,
    ) -> anyhow::Result<()> {
        let mode = PermissionMode::new(mode).map_err(|error| {
            anyhow::anyhow!("cannot change permissions of {}: {error}", path.as_typed())
        })?;
        let full = self.ssh_full_path(server, path)?;
        let command = chmod_command(mode, &full);
        self.run_ssh_change(server, Operation::Chmod, path, &command)
    }

    /// 親ディレクトリを作ってから symlink を作る
    pub(crate) fn ssh_create_symlink(
        &mut self,
        server: &str,
        path: &EntryPath,
        target: &str,
    ) -> anyhow::Result<()> {
        let full = self.ssh_full_path(server, path)?;
        let command = change_scripts::symlink(target, &full);
        let client = require_ssh_client(&mut self.ssh_clients, server)?;
        let (_, stderr, exit) = self.rt.block_on(client.run_bytes(&command))?;
        match exit {
            0 => Ok(()),
            _ => Err(changed(
                Operation::CreateSymlink,
                path,
                change_scripts::failure(&stderr),
            )),
        }
    }

    /// 親ディレクトリを作ってから、書き込み先をその場で書き換える（REQ-merge-039）
    pub(crate) fn ssh_write(
        &mut self,
        server: &str,
        path: &EntryPath,
        content: &[u8],
        binary: bool,
    ) -> anyhow::Result<()> {
        let full = self.ssh_full_path(server, path)?;
        let command = change_scripts::write(&full, binary);
        let input = change_scripts::write_input(content, binary);
        let client = require_ssh_client(&mut self.ssh_clients, server)?;
        let (_, stderr, exit) = self.rt.block_on(client.run_with_input(&command, &input))?;
        match exit {
            0 => Ok(()),
            _ => Err(changed(
                Operation::Write,
                path,
                change_scripts::failure(&stderr),
            )),
        }
    }

    fn ssh_full_path(&self, server: &str, path: &EntryPath) -> anyhow::Result<String> {
        let root = self
            .get_server_config(server)?
            .root_dir
            .to_string_lossy()
            .into_owned();
        Ok(remote_full_path(&root, path.as_rel()))
    }

    /// 変更のコマンドを動かす。接続の失敗はそのまま返し、0 以外で終わったら失敗の種類にする
    fn run_ssh_change(
        &mut self,
        server: &str,
        operation: Operation,
        path: &EntryPath,
        command: &str,
    ) -> anyhow::Result<()> {
        let client = require_ssh_client(&mut self.ssh_clients, server)?;
        let (_, stderr, exit) = self.rt.block_on(client.run_bytes(command))?;
        if exit == 0 {
            return Ok(());
        }
        Err(changed(operation, path, from_tool_message(&stderr)))
    }
}

/// 権限を変えるコマンド。"-" で始まるパスをオプションに読ませない
fn chmod_command(mode: PermissionMode, full: &str) -> String {
    format!("chmod -- {:o} {}", mode.bits(), shell_escape(full))
}

fn changed(operation: Operation, path: &EntryPath, failure: OpFailure) -> anyhow::Error {
    OpError::new(operation, path.as_typed(), failure).into()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_path_starting_with_a_dash_is_not_read_as_an_option_of_chmod() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("-r"), "x").unwrap();
        let status = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(chmod_command(PermissionMode::new(0o600).unwrap(), "-r"))
            .current_dir(temp.path())
            .status()
            .unwrap();
        assert!(status.success());
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(temp.path().join("-r"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
