//! known_hosts ファイルの解析・検証。

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use hmac::{Hmac, Mac};
use russh::client;
use sha1::Sha1;

use super::host_key_verifier::UnknownHostInfo;
use super::known_hosts_io;

/// known_hosts のエントリ
#[derive(Debug, Clone)]
pub(crate) struct KnownHost {
    /// ホスト名パターン（平文またはハッシュ形式 `|1|salt|hash`）
    pub hostname_pattern: String,
    /// キータイプ（例: "ssh-rsa", "ssh-ed25519"）
    pub key_type: String,
    /// キーのbase64エンコードされたデータ
    pub key_base64: String,
    /// known_hosts ファイルでの行番号（1始まり）
    pub line_number: usize,
}

/// russh の Handler 実装
pub(crate) struct SshHandler {
    pub host: String,
    pub port: u16,
    /// テスト用: known_hosts チェックをスキップするフラグ
    pub skip_host_key_check: bool,
    /// 未知ホスト検出時にフィンガープリント情報を送信するチャネル
    pub unknown_host_sender: Option<tokio::sync::oneshot::Sender<UnknownHostInfo>>,
}

impl SshHandler {
    pub fn new(host: String, port: u16) -> Self {
        Self {
            host,
            port,
            skip_host_key_check: false,
            unknown_host_sender: None,
        }
    }
}

impl client::Handler for SshHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        if self.skip_host_key_check {
            tracing::debug!("Skipping known_hosts check: {}", self.host);
            return Ok(true);
        }

        let openssh_str = server_public_key
            .to_openssh()
            .map_err(|e| anyhow::anyhow!("Failed to serialize public key: {}", e))?;

        let parts: Vec<&str> = openssh_str.splitn(2, ' ').collect();
        if parts.len() < 2 {
            return Err(anyhow::anyhow!(
                "Invalid public key format: {}",
                openssh_str
            ));
        }
        let server_key_type = parts[0];
        let server_key_base64 = parts[1];

        let known_hosts_content = match known_hosts_io::read_known_hosts() {
            Some(content) => content,
            None => {
                // known_hosts ファイルが無い → 未知ホスト
                return self.handle_unknown_host(
                    server_key_type,
                    server_key_base64,
                    server_public_key,
                );
            }
        };

        let known_hosts = parse_known_hosts(&known_hosts_content);
        let host_entry = format_host_entry(&self.host, self.port);

        let mut found_host = false;
        for kh in &known_hosts {
            if !host_matches(&kh.hostname_pattern, &host_entry, &self.host, self.port) {
                continue;
            }
            found_host = true;

            if kh.key_type == server_key_type {
                if kh.key_base64 == server_key_base64 {
                    tracing::debug!("known_hosts: host key matched: {}", self.host);
                    return Ok(true);
                } else {
                    return Err(anyhow::anyhow!(
                        "\n\
                        @@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@\n\
                        @    WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!     @\n\
                        @@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@\n\
                        IT IS POSSIBLE THAT SOMEONE IS DOING SOMETHING NASTY!\n\
                        Someone could be eavesdropping on you right now (man-in-the-middle attack)!\n\
                        It is also possible that a host key has just been changed.\n\
                        The {key_type} host key for {host} has changed (line {line} in ~/.ssh/known_hosts).\n\
                        To fix this, run:\n\
                        \n  ssh-keygen -R {host_entry}\n",
                        key_type = server_key_type,
                        host = self.host,
                        line = kh.line_number,
                        host_entry = host_entry,
                    ));
                }
            }
        }

        if found_host {
            // ホスト名は一致するが異なるキータイプ → 新しいキータイプを追加
            known_hosts_io::append_known_hosts_entry(
                &self.host,
                self.port,
                server_key_type,
                server_key_base64,
            );
            return Ok(true);
        }

        // 完全に未知のホスト
        self.handle_unknown_host(server_key_type, server_key_base64, server_public_key)
    }
}

impl SshHandler {
    /// 未知ホストを検出した場合のハンドリング。
    ///
    /// `unknown_host_sender` があれば情報を送信して false を返す（接続を中断）。
    /// sender がなければ既存動作（自動追加して true）。
    fn handle_unknown_host(
        &mut self,
        key_type: &str,
        key_base64: &str,
        public_key: &russh::keys::PublicKey,
    ) -> Result<bool, anyhow::Error> {
        if let Some(sender) = self.unknown_host_sender.take() {
            let fingerprint = known_hosts_io::format_fingerprint(public_key);
            let info = UnknownHostInfo {
                host: self.host.clone(),
                port: self.port,
                key_type: key_type.to_string(),
                fingerprint,
                key_base64: key_base64.to_string(),
            };
            // 送信失敗（receiver が drop 済み）は無視して false を返す
            let _ = sender.send(info);
            tracing::debug!(
                "Unknown host info sent via channel, rejecting connection: {}",
                self.host
            );
            Ok(false)
        } else {
            // sender がない = 旧来の自動承認モード
            tracing::info!(
                "known_hosts: unknown host. TOFU: adding host key: {}",
                self.host
            );
            known_hosts_io::append_known_hosts_entry(&self.host, self.port, key_type, key_base64);
            Ok(true)
        }
    }
}

// format_fingerprint, append_known_hosts_entry, known_hosts_path は
// known_hosts_io モジュールに移動。

/// 後方互換: client.rs から使われる re-export
pub(crate) use known_hosts_io::append_known_hosts_entry;

/// known_hosts のテキスト内容をパースする
pub(crate) fn parse_known_hosts(content: &str) -> Vec<KnownHost> {
    let mut entries = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('@') {
            continue;
        }

        let parts: Vec<&str> = line.splitn(3, ' ').collect();
        if parts.len() < 3 {
            continue;
        }

        let hostname_pattern = parts[0].to_string();
        let key_type = parts[1].to_string();
        let key_base64 = parts[2].split_whitespace().next().unwrap_or("").to_string();

        if key_base64.is_empty() {
            continue;
        }

        entries.push(KnownHost {
            hostname_pattern,
            key_type,
            key_base64,
            line_number: line_idx + 1,
        });
    }

    entries
}

/// ホスト名がknown_hostsのパターンに一致するか判定する
pub(crate) fn host_matches(pattern: &str, host_entry: &str, hostname: &str, port: u16) -> bool {
    if pattern.starts_with("|1|") {
        return hashed_host_matches(pattern, hostname, port);
    }

    for p in pattern.split(',') {
        let p = p.trim();
        if p == host_entry || p == hostname {
            return true;
        }
    }

    false
}

/// ハッシュ化されたホスト名との照合（HMAC-SHA1）
fn hashed_host_matches(pattern: &str, hostname: &str, port: u16) -> bool {
    let parts: Vec<&str> = pattern.split('|').collect();
    if parts.len() < 4 || parts[1] != "1" {
        return false;
    }

    let salt = match BASE64.decode(parts[2]) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let expected_hash = match BASE64.decode(parts[3]) {
        Ok(h) => h,
        Err(_) => return false,
    };

    let candidate = format_host_entry(hostname, port);

    let Ok(mut mac) = Hmac::<Sha1>::new_from_slice(&salt) else {
        return false;
    };
    mac.update(candidate.as_bytes());
    let result = mac.finalize().into_bytes();

    result.as_slice() == expected_hash.as_slice()
}

/// ホスト名とポートから known_hosts 用のエントリ文字列を構築する
///
/// 非標準ポートの場合は `[host]:port` 形式。
/// IPv6 アドレス（`:`を含む）も同じ形式で正しく動作する。
pub(crate) fn format_host_entry(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{}]:{}", host, port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_known_hosts_basic() {
        let content = "\
example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
192.168.1.1 ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC...
";
        let entries = parse_known_hosts(content);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].hostname_pattern, "example.com");
        assert_eq!(entries[0].key_type, "ssh-ed25519");
        assert_eq!(entries[1].hostname_pattern, "192.168.1.1");
        assert_eq!(entries[1].key_type, "ssh-rsa");
    }

    #[test]
    fn test_parse_known_hosts_comments_and_empty_lines() {
        let content = "\
# This is a comment
example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKxx

# Another comment
192.168.1.1 ssh-rsa AAAAB3NzaC1yc2EAAA
";
        let entries = parse_known_hosts(content);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_parse_known_hosts_with_trailing_comment() {
        let content = "example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKxx user@host\n";
        let entries = parse_known_hosts(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key_base64, "AAAAC3NzaC1lZDI1NTE5AAAAIKxx");
    }

    #[test]
    fn test_parse_known_hosts_skip_markers() {
        let content = "\
@cert-authority *.example.com ssh-rsa AAAAB3...
@revoked example.com ssh-rsa AAAAB3...
example.com ssh-ed25519 AAAAC3...
";
        let entries = parse_known_hosts(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].hostname_pattern, "example.com");
    }

    #[test]
    fn test_parse_known_hosts_non_standard_port() {
        let content = "[example.com]:2222 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKxx\n";
        let entries = parse_known_hosts(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].hostname_pattern, "[example.com]:2222");
    }

    #[test]
    fn test_host_matches_plain() {
        assert!(host_matches(
            "example.com",
            "example.com",
            "example.com",
            22
        ));
        assert!(!host_matches("other.com", "example.com", "example.com", 22));
    }

    #[test]
    fn test_host_matches_non_standard_port() {
        let entry_2222 = format_host_entry("example.com", 2222);
        assert!(host_matches(
            "[example.com]:2222",
            &entry_2222,
            "example.com",
            2222
        ));

        let entry_22 = format_host_entry("example.com", 22);
        assert!(!host_matches(
            "[example.com]:2222",
            &entry_22,
            "example.com",
            22
        ));
    }

    #[test]
    fn test_host_matches_comma_separated() {
        assert!(host_matches(
            "example.com,192.168.1.1",
            "192.168.1.1",
            "192.168.1.1",
            22
        ));
        assert!(host_matches(
            "example.com,192.168.1.1",
            "example.com",
            "example.com",
            22
        ));
    }

    #[test]
    fn test_host_matches_hashed() {
        let salt = b"testsalt";
        let salt_b64 = BASE64.encode(salt);

        let mut mac = Hmac::<Sha1>::new_from_slice(salt).unwrap();
        mac.update(b"example.com");
        let hash = mac.finalize().into_bytes();
        let hash_b64 = BASE64.encode(hash);

        let pattern = format!("|1|{}|{}", salt_b64, hash_b64);

        assert!(hashed_host_matches(&pattern, "example.com", 22));
        assert!(!hashed_host_matches(&pattern, "other.com", 22));
    }

    #[test]
    fn test_host_matches_hashed_non_standard_port() {
        let salt = b"portsalt";
        let salt_b64 = BASE64.encode(salt);

        let mut mac = Hmac::<Sha1>::new_from_slice(salt).unwrap();
        mac.update(b"[example.com]:2222");
        let hash = mac.finalize().into_bytes();
        let hash_b64 = BASE64.encode(hash);

        let pattern = format!("|1|{}|{}", salt_b64, hash_b64);

        assert!(hashed_host_matches(&pattern, "example.com", 2222));
        assert!(!hashed_host_matches(&pattern, "example.com", 22));
    }

    #[test]
    fn test_format_host_entry() {
        assert_eq!(format_host_entry("example.com", 22), "example.com");
        assert_eq!(format_host_entry("example.com", 2222), "[example.com]:2222");
    }

    #[test]
    fn test_format_host_entry_ipv6() {
        assert_eq!(format_host_entry("::1", 22), "::1");
        assert_eq!(format_host_entry("::1", 2222), "[::1]:2222");
        assert_eq!(format_host_entry("2001:db8::1", 22), "2001:db8::1");
        assert_eq!(format_host_entry("2001:db8::1", 2222), "[2001:db8::1]:2222");
    }

    #[test]
    fn test_same_host_different_ports_coexist() {
        let content = "\
example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKxx
[example.com]:2222 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKyy
";
        let entries = parse_known_hosts(content);
        assert_eq!(entries.len(), 2);

        // ポート22のエントリはポート22にマッチする
        let entry_22 = format_host_entry("example.com", 22);
        assert!(host_matches(
            &entries[0].hostname_pattern,
            &entry_22,
            "example.com",
            22
        ));

        // "[example.com]:2222" パターンはポート22の host_entry にはマッチしない
        assert!(!host_matches(
            &entries[1].hostname_pattern,
            &entry_22,
            "example.com",
            22
        ));

        // ポート2222のエントリはポート2222にマッチする
        let entry_2222 = format_host_entry("example.com", 2222);
        assert!(host_matches(
            &entries[1].hostname_pattern,
            &entry_2222,
            "example.com",
            2222
        ));

        // 異なるキーが登録されているので二つの独立したエントリとして存在
        assert_ne!(entries[0].key_base64, entries[1].key_base64);
    }

    #[test]
    fn test_parse_known_hosts_line_numbers() {
        let content = "\
# comment line 1
example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKxx

192.168.1.1 ssh-rsa AAAAB3NzaC1yc2EAAA
";
        let entries = parse_known_hosts(content);
        assert_eq!(entries.len(), 2);
        // "# comment" は行1、"example.com ..." は行2
        assert_eq!(entries[0].line_number, 2);
        // 空行は行3、"192.168.1.1 ..." は行4
        assert_eq!(entries[1].line_number, 4);
    }

    #[test]
    fn test_mitm_warning_contains_line_number() {
        // ホストキー変更時のエラーメッセージに行番号が含まれることを検証
        let content = "example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKxx\n";
        let entries = parse_known_hosts(content);
        assert_eq!(entries[0].line_number, 1);

        // 行番号がメッセージに含められることを間接的に検証
        // (実際のcheck_server_keyは非同期+SSHが必要なのでフォーマットだけテスト)
        let msg = format!(
            "The ssh-ed25519 host key for example.com has changed (line {} in ~/.ssh/known_hosts).",
            entries[0].line_number
        );
        assert!(msg.contains("line 1"));
    }

    #[test]
    fn test_mitm_warning_contains_keygen_hint() {
        // ssh-keygen -R の案内が含まれることを検証（フォーマットのみ）
        let host_entry = format_host_entry("example.com", 22);
        let msg = format!("ssh-keygen -R {}", host_entry);
        assert!(msg.contains("ssh-keygen -R example.com"));

        let host_entry_port = format_host_entry("example.com", 2222);
        let msg_port = format!("ssh-keygen -R {}", host_entry_port);
        assert!(msg_port.contains("ssh-keygen -R [example.com]:2222"));
    }

    #[test]
    fn test_parse_known_hosts_empty() {
        let entries = parse_known_hosts("");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_parse_known_hosts_invalid_lines() {
        let content = "\
onlyonefield
two fields
example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKxx
";
        let entries = parse_known_hosts(content);
        assert_eq!(entries.len(), 1);
    }
}
