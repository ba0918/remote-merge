//! リモートファイルの読み書き操作。

use crate::merge::executor;

use super::TuiRuntime;

impl TuiRuntime {
    /// リモートファイル内容を取得する
    pub fn read_remote_file(
        &mut self,
        server_name: &str,
        rel_path: &str,
    ) -> anyhow::Result<String> {
        let server_config = self.get_server_config(server_name)?;
        let remote_root = server_config.root_dir.to_string_lossy().to_string();
        let full_path = executor::validate_remote_path(&remote_root, rel_path)?;

        let client = self
            .ssh_client
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("SSH not connected"))?;

        self.rt.block_on(client.read_file(&full_path))
    }

    /// リモートファイルに書き込む
    pub fn write_remote_file(
        &mut self,
        server_name: &str,
        rel_path: &str,
        content: &str,
    ) -> anyhow::Result<()> {
        let server_config = self.get_server_config(server_name)?;
        let remote_root = server_config.root_dir.to_string_lossy().to_string();
        let full_path = executor::validate_remote_path(&remote_root, rel_path)?;

        let client = self
            .ssh_client
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("SSH not connected"))?;

        self.rt.block_on(client.write_file(&full_path, content))
    }
}
