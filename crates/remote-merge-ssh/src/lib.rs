#![warn(clippy::unwrap_used)]
#![deny(clippy::print_stdout, clippy::print_stderr)]
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

/// リモートが黙っていてよい時間の下限（120 秒）を当てた時間切れ。ssh の接続の時間切れ
/// （`timeout_sec`）が短くても、keepalive の間隔と、エージェントが黙って計算する時間（大きな
/// 走査・ハッシュ）で切らないようにする。セッションの inactivity_timeout と、手元がエージェントの
/// 答えを読むときの時間切れで同じ値を使う（docs/ir/ssh/reconnect.md の REQ-ssh-025）
pub fn silence_timeout(timeout_sec: u64) -> std::time::Duration {
    std::time::Duration::from_secs(timeout_sec.max(120))
}
