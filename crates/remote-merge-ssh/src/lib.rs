#![warn(clippy::unwrap_used)]
pub mod client;
pub mod hint;
pub mod host_key_verifier;
pub mod passphrase_provider;
pub mod quote;

pub(crate) mod known_hosts;
#[cfg(all(unix, any(test, feature = "test-utils")))]
pub(crate) mod known_hosts_guard;
pub(crate) mod known_hosts_io;
pub(crate) mod known_hosts_location;
pub(crate) mod login_shell;
pub(crate) mod preferred;

pub(crate) use remote_merge_core::error;

mod config {
    pub(crate) use remote_merge_config::expand_tilde;
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
