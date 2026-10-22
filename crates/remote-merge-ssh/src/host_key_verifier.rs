//! ホストキー確認を UI 層に委譲するための trait と実装。
//!
//! russh の `check_server_key()` はコールバック内で同期的な UI 操作ができないため、
//! oneshot チャネルで情報を送出し、接続側で HostKeyVerifier を呼ぶ設計にしている。

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
pub(crate) struct UnknownHostInfo {
    pub host: String,
    pub port: u16,
    pub key_type: String,
    pub fingerprint: String,
    pub key_base64: String,
}

/// 自動承認（strict_host_key_checking = "no" / Agent 接続）
pub(crate) struct AutoAcceptVerifier;

impl HostKeyVerifier for AutoAcceptVerifier {
    fn verify_host_key(&self, host: &str, port: u16, _key_type: &str, _fingerprint: &str) -> bool {
        tracing::info!(target: "remote_merge::ssh::host_key_verifier", "Auto-accepting host key for {}:{}", host, port,);
        true
    }
}

/// 自動拒否（strict_host_key_checking = "yes"）
pub(crate) struct RejectVerifier;

impl HostKeyVerifier for RejectVerifier {
    fn verify_host_key(&self, host: &str, port: u16, key_type: &str, fingerprint: &str) -> bool {
        tracing::warn!(
            target: "remote_merge::ssh::host_key_verifier",
            "Rejecting unknown host key: {}:{} ({} {})",
            host,
            port,
            key_type,
            fingerprint,
        );
        false
    }
}

/// CLI 用の確認（--yes なら尋ねずに承認する）。端末につなぐのは製品
pub struct CliVerifier {
    pub auto_yes: bool,
}

impl CliVerifier {
    /// 表示先と入力を呼び出し側から受け取る（ライブラリは自分では出力しない）
    pub fn verify_host_key_with_input(
        &self,
        host: &str,
        port: u16,
        key_type: &str,
        fingerprint: &str,
        input: &mut impl std::io::BufRead,
        output: &mut impl std::io::Write,
    ) -> bool {
        let _ = writeln!(
            output,
            "The authenticity of host '{host}' (port {port}) can't be established.",
        );
        let _ = writeln!(output, "{key_type} key fingerprint is {fingerprint}.");
        if self.auto_yes {
            let _ = writeln!(output, "Automatically accepted (--yes).");
            return true;
        }

        let _ = write!(
            output,
            "Are you sure you want to continue connecting (yes/no)? "
        );
        let _ = output.flush();

        let mut input_line = String::new();
        if input.read_line(&mut input_line).is_err() {
            return false;
        }

        let answer = input_line.trim().to_lowercase();
        answer == "yes" || answer == "y"
    }
}

/// StrictHostKeyChecking 設定から HostKeyVerifier を選ぶ。Ask は呼び出し側の確認 `ask` を使う
pub fn choose_verifier(
    policy: crate::config::StrictHostKeyChecking,
    ask: Box<dyn HostKeyVerifier>,
) -> Box<dyn HostKeyVerifier> {
    use crate::config::StrictHostKeyChecking;
    match policy {
        StrictHostKeyChecking::No => Box::new(AutoAcceptVerifier),
        StrictHostKeyChecking::Yes => Box::new(RejectVerifier),
        StrictHostKeyChecking::Ask => ask,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_verifier_rejects_when_input_ends_without_an_answer() {
        let v = CliVerifier { auto_yes: false };
        let mut input = "".as_bytes();
        let mut output = Vec::new();
        assert!(!v.verify_host_key_with_input(
            "host",
            22,
            "ssh-ed25519",
            "SHA256:abc",
            &mut input,
            &mut output
        ));
        let shown = String::from_utf8_lossy(&output);
        assert!(shown.contains("SHA256:abc"), "確認の表示: {shown}");
    }

    #[test]
    fn test_cli_verifier_accepts_yes_and_y() {
        let v = CliVerifier { auto_yes: false };
        for answer in ["yes\n", "y\n"] {
            let mut input = answer.as_bytes();
            assert!(v.verify_host_key_with_input(
                "host",
                22,
                "ssh-ed25519",
                "SHA256:abc",
                &mut input,
                &mut std::io::sink()
            ));
        }
    }
}
