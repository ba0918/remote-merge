//! runtime の単体テストが共有する、試験用のサーバの設定

use super::core::CoreRuntime;
use crate::config::{AuthMethod, ServerConfig};

/// サーバ `name`（root_dir は `root`。`sudo` は sudo を使うか）を足した、どこにもつないでいない runtime
pub(super) fn runtime_with_server(name: &str, root: &str, sudo: bool) -> CoreRuntime {
    let mut rt = CoreRuntime::new_for_test();
    rt.config
        .servers
        .insert(name.to_string(), server_config(root, sudo));
    rt
}

/// root_dir が `root` の、試験用のサーバの設定（ホスト 10.0.0.1 の利用者 deploy で鍵の認証）
pub(super) fn server_config(root: &str, sudo: bool) -> ServerConfig {
    ServerConfig {
        host: "10.0.0.1".to_string(),
        port: 22,
        user: "deploy".to_string(),
        auth: AuthMethod::Key,
        password: None,
        key: None,
        root_dir: std::path::PathBuf::from(root),
        ssh_options: None,
        sudo,
        file_permissions: None,
        dir_permissions: None,
    }
}
