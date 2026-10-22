//! 未知のホスト鍵を端末で確かめる verifier と、設定の方針からの選び方。
//!
//! 確認の文言と答えの読み方は SSH の crate の [`CliVerifier`] が持ち、ここでは標準入力と
//! 標準エラーにつなぐ。

pub use remote_merge_ssh::host_key_verifier::{CliVerifier, HostKeyVerifier};

use crate::config::StrictHostKeyChecking;

/// [`CliVerifier`] を、標準入力で答えを読み標準エラーに確認を出すようにつないだもの
struct TerminalVerifier(CliVerifier);

impl HostKeyVerifier for TerminalVerifier {
    fn verify_host_key(&self, host: &str, port: u16, key_type: &str, fingerprint: &str) -> bool {
        self.0.verify_host_key_with_input(
            host,
            port,
            key_type,
            fingerprint,
            &mut std::io::stdin().lock(),
            &mut std::io::stderr(),
        )
    }
}

/// StrictHostKeyChecking 設定からデフォルトの HostKeyVerifier を選択する
///
/// Ask は標準入力で確認する。答えを読めない（入力の終わり）ときは拒否するので、
/// 確認手段のない接続も未知の鍵を受け入れない。
pub fn verifier_from_policy(
    policy: StrictHostKeyChecking,
    auto_yes: bool,
) -> Box<dyn HostKeyVerifier> {
    remote_merge_ssh::host_key_verifier::choose_verifier(
        policy,
        Box::new(TerminalVerifier(CliVerifier { auto_yes })),
    )
}
