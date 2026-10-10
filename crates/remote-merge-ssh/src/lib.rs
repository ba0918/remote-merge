pub mod batch_read;
pub mod child_listing;
pub mod client;
pub mod hint;
pub mod host_key_verifier;
pub mod passphrase_provider;
pub mod tree_parser;

pub(crate) mod known_hosts;
pub(crate) mod known_hosts_io;
pub(crate) mod login_shell;
pub(crate) mod preferred;

pub(crate) use remote_merge_core::{error, filter, tree};

mod config {
    pub(crate) use remote_merge_config::{expand_tilde, DEFAULT_MAX_DIR_ENTRIES};
    pub(crate) use remote_merge_settings::{
        AuthMethod, ServerConfig, SshConfig, SshOptions, StrictHostKeyChecking,
    };
}

pub async fn exec_strict(
    client: &mut client::SshClient,
    command: &str,
) -> remote_merge_core::error::Result<String> {
    client.exec_strict(command).await
}

/// [`exec_strict`] と同じく実行し、標準出力を文字列に変えずにバイト列のまま返す
pub async fn exec_strict_bytes(
    client: &mut client::SshClient,
    command: &str,
) -> remote_merge_core::error::Result<Vec<u8>> {
    client.exec_strict_bytes(command).await
}
