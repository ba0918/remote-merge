//! ホストキー確認を UI 層に委譲するための trait と実装。
//!
//! russh の `check_server_key()` はコールバック内で同期的な UI 操作ができないため、
//! oneshot チャネルで情報を送出し、接続側で HostKeyVerifier を呼ぶ設計にしている。

use std::io::Write as _;

/// ホストキー確認を UI 層に委譲するための trait
pub trait HostKeyVerifier: Send + Sync {
    /// 未知ホストのキーを受け入れるか判定する。
    ///
    /// `true` を返すと known_hosts に保存して接続を続行する。
    /// `false` を返すと接続を中止する。
    fn verify_host_key(&self, host: &str, port: u16, key_type: &str, fingerprint: &str) -> bool;
}

/// 未知ホスト検出時の情報
#[derive(Debug, Clone)]
pub struct UnknownHostInfo {
    pub host: String,
    pub port: u16,
    pub key_type: String,
    pub fingerprint: String,
    pub key_base64: String,
}

/// 自動承認（strict_host_key_checking = "no" / Agent 接続）
pub struct AutoAcceptVerifier;

impl HostKeyVerifier for AutoAcceptVerifier {
    fn verify_host_key(&self, host: &str, port: u16, _key_type: &str, _fingerprint: &str) -> bool {
        tracing::info!("Auto-accepting host key for {}:{}", host, port,);
        true
    }
}

/// 自動拒否（strict_host_key_checking = "yes"）
pub struct RejectVerifier;

impl HostKeyVerifier for RejectVerifier {
    fn verify_host_key(&self, host: &str, port: u16, key_type: &str, fingerprint: &str) -> bool {
        tracing::warn!(
            "Rejecting unknown host key: {}:{} ({} {})",
            host,
            port,
            key_type,
            fingerprint,
        );
        false
    }
}

/// CLI 用（stderr に表示して stdin で確認 / --yes で自動承認）
pub struct CliVerifier {
    pub auto_yes: bool,
}

impl HostKeyVerifier for CliVerifier {
    fn verify_host_key(&self, host: &str, port: u16, key_type: &str, fingerprint: &str) -> bool {
        if self.auto_yes {
            eprintln!(
                "The authenticity of host '{}' (port {}) can't be established.",
                host, port,
            );
            eprintln!("{} key fingerprint is {}.", key_type, fingerprint);
            eprintln!("Automatically accepted (--yes).");
            return true;
        }

        eprintln!(
            "The authenticity of host '{}' (port {}) can't be established.",
            host, port,
        );
        eprintln!("{} key fingerprint is {}.", key_type, fingerprint);
        eprint!("Are you sure you want to continue connecting (yes/no)? ");
        let _ = std::io::stderr().flush();

        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_err() {
            return false;
        }

        let answer = input.trim().to_lowercase();
        answer == "yes" || answer == "y"
    }
}

/// StrictHostKeyChecking 設定からデフォルトの HostKeyVerifier を選択する
///
/// - `is_tui = true`: TUI モードでは stdin を読めないため、
///   Ask → AutoAcceptVerifier（TOFU 自動承認）を使う。
///   将来的に TUI ダイアログ経由の確認を実装予定。
/// - `is_tui = false`: CLI モードでは CliVerifier で stdin 確認。
pub fn verifier_from_policy(
    policy: crate::config::StrictHostKeyChecking,
    auto_yes: bool,
    is_tui: bool,
) -> Box<dyn HostKeyVerifier> {
    use crate::config::StrictHostKeyChecking;
    match policy {
        StrictHostKeyChecking::No => Box::new(AutoAcceptVerifier),
        StrictHostKeyChecking::Yes => Box::new(RejectVerifier),
        StrictHostKeyChecking::Ask => {
            if is_tui {
                // TUI モードでは stdin ベースの確認ができないため自動承認
                // TODO: TUI ダイアログ経由のホストキー確認は将来のサイクルで実装
                Box::new(AutoAcceptVerifier)
            } else {
                Box::new(CliVerifier { auto_yes })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_accept_verifier_returns_true() {
        let v = AutoAcceptVerifier;
        assert!(v.verify_host_key("example.com", 22, "ssh-ed25519", "SHA256:abc"));
    }

    #[test]
    fn test_reject_verifier_returns_false() {
        let v = RejectVerifier;
        assert!(!v.verify_host_key("example.com", 22, "ssh-ed25519", "SHA256:abc"));
    }

    #[test]
    fn test_cli_verifier_auto_yes_returns_true() {
        let v = CliVerifier { auto_yes: true };
        assert!(v.verify_host_key("example.com", 22, "ssh-ed25519", "SHA256:abc"));
    }

    #[test]
    fn test_verifier_from_policy_no() {
        use crate::config::StrictHostKeyChecking;
        let v = verifier_from_policy(StrictHostKeyChecking::No, false, false);
        assert!(v.verify_host_key("host", 22, "ssh-ed25519", "SHA256:abc"));
    }

    #[test]
    fn test_verifier_from_policy_yes() {
        use crate::config::StrictHostKeyChecking;
        let v = verifier_from_policy(StrictHostKeyChecking::Yes, false, false);
        assert!(!v.verify_host_key("host", 22, "ssh-ed25519", "SHA256:abc"));
    }

    #[test]
    fn test_verifier_from_policy_ask_with_auto_yes() {
        use crate::config::StrictHostKeyChecking;
        let v = verifier_from_policy(StrictHostKeyChecking::Ask, true, false);
        assert!(v.verify_host_key("host", 22, "ssh-ed25519", "SHA256:abc"));
    }

    #[test]
    fn test_verifier_from_policy_ask_tui_uses_auto_accept() {
        use crate::config::StrictHostKeyChecking;
        // TUI モードでは Ask ポリシーで AutoAcceptVerifier が使われる
        let v = verifier_from_policy(StrictHostKeyChecking::Ask, false, true);
        assert!(v.verify_host_key("host", 22, "ssh-ed25519", "SHA256:abc"));
    }

    #[test]
    fn test_verifier_from_policy_yes_tui_still_rejects() {
        use crate::config::StrictHostKeyChecking;
        // TUI モードでも Yes ポリシーは RejectVerifier
        let v = verifier_from_policy(StrictHostKeyChecking::Yes, false, true);
        assert!(!v.verify_host_key("host", 22, "ssh-ed25519", "SHA256:abc"));
    }

    #[test]
    fn test_verifier_from_policy_no_tui_auto_accepts() {
        use crate::config::StrictHostKeyChecking;
        // TUI モードでも No ポリシーは AutoAcceptVerifier
        let v = verifier_from_policy(StrictHostKeyChecking::No, false, true);
        assert!(v.verify_host_key("host", 22, "ssh-ed25519", "SHA256:abc"));
    }
}
