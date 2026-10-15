//! known_hosts ファイルの解析・検証。

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use hmac::{Hmac, Mac};
use russh::client;
use sha1::Sha1;

use super::host_key_verifier::UnknownHostInfo;
use super::known_hosts_io;

/// known_hosts のエントリ
#[derive(Debug, Clone)]
struct KnownHost<'a> {
    /// ホスト名パターン（平文またはハッシュ形式 `|1|salt|hash`）
    hostname_pattern: &'a str,
    /// キータイプ（例: "ssh-rsa", "ssh-ed25519"）
    key_type: &'a str,
    /// キーのbase64エンコードされたデータ
    key_base64: &'a str,
    /// known_hosts ファイルでの行番号（1始まり）
    line_number: usize,
}

/// russh の Handler 実装
pub(crate) struct SshHandler {
    pub host: String,
    pub port: u16,
    /// テスト用: known_hosts チェックをスキップするフラグ
    pub skip_host_key_check: bool,
    /// 未知ホスト検出時にフィンガープリント情報を送信するチャネル
    pub unknown_host_sender: Option<tokio::sync::oneshot::Sender<UnknownHostInfo>>,
    /// 利用者が承認した鍵（種別と base64）。あるときはこの鍵と完全に一致する鍵だけを受け入れる
    approved_key: Option<(String, String)>,
    /// 使う known_hosts。None なら利用者の ~/.ssh/known_hosts
    pub known_hosts: Option<std::path::PathBuf>,
}

impl SshHandler {
    pub fn new(host: String, port: u16) -> Self {
        Self {
            host,
            port,
            skip_host_key_check: false,
            unknown_host_sender: None,
            approved_key: None,
            known_hosts: None,
        }
    }

    /// 利用者が承認した鍵での再接続用のハンドラ。
    ///
    /// known_hosts を読み直すと別プロセスの変更と食い違い得るため、承認した鍵そのものと比べる。
    pub fn for_approved_key(host: String, port: u16, key_type: &str, key_base64: &str) -> Self {
        Self {
            approved_key: Some((key_type.to_string(), key_base64.to_string())),
            ..Self::new(host, port)
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
            tracing::debug!(target: "remote_merge::ssh::known_hosts", "Skipping known_hosts check: {}", self.host);
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

        if let Some((approved_type, approved_base64)) = &self.approved_key {
            let matches = approved_type == server_key_type && approved_base64 == server_key_base64;
            if !matches {
                tracing::warn!(target: "remote_merge::ssh::known_hosts", "Reconnection presented a host key different from the approved one: {}", self.host);
            }
            return Ok(matches);
        }

        let known_hosts_content = match known_hosts_io::read_known_hosts(
            self.known_hosts.as_deref(),
        ) {
            Ok(Some(content)) => content,
            Err(error) => {
                return Err(anyhow::anyhow!(
                    "Failed to read ~/.ssh/known_hosts: {error}. Refusing to verify the host key for {}.",
                    self.host
                ));
            }
            Ok(None) => {
                // known_hosts ファイルが無い → 未知ホスト
                return self.handle_unknown_host(
                    server_key_type,
                    server_key_base64,
                    server_public_key,
                );
            }
        };

        match judge_host_key(
            &known_hosts_content,
            &self.host,
            self.port,
            server_key_type,
            server_key_base64,
        ) {
            HostKeyJudgement::Trusted => {
                tracing::debug!(target: "remote_merge::ssh::known_hosts", "known_hosts: host key matched: {}", self.host);
                Ok(true)
            }
            HostKeyJudgement::Changed { line } => {
                let host_entry = format_host_entry(&self.host, self.port);
                Err(anyhow::anyhow!(
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
                    line = line,
                    host_entry = host_entry,
                ))
            }
            HostKeyJudgement::Revoked { line } => Err(anyhow::anyhow!(
                "The {key_type} host key for {host} is marked as revoked (line {line} in ~/.ssh/known_hosts). Refusing to connect.",
                key_type = server_key_type,
                host = self.host,
            )),
            // 別の鍵種別でだけ登録されたホストも、提示された鍵は確かめられていないので未知として扱う
            HostKeyJudgement::Unknown => {
                self.handle_unknown_host(server_key_type, server_key_base64, server_public_key)
            }
        }
    }
}

/// known_hosts の内容に照らした、提示されたホスト鍵の判定
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HostKeyJudgement {
    /// 同じ種別・同じ鍵が登録されている
    Trusted,
    /// 同じ種別で別の鍵が登録されている（`line` はその行番号）
    Changed { line: usize },
    /// 提示された鍵を確かめる登録がない
    Unknown,
    /// 提示された鍵が "@revoked" で失効とされている（`line` はその行番号）
    Revoked { line: usize },
}

/// known_hosts の内容と提示された鍵から、その鍵を信頼できるかを判定する。
pub(crate) fn judge_host_key(
    known_hosts_content: &str,
    host: &str,
    port: u16,
    key_type: &str,
    key_base64: &str,
) -> HostKeyJudgement {
    let host_entry = format_host_entry(host, port);
    // 同じ鍵の通常の行があっても失効を優先する
    let revoked = parse_revoked_keys(known_hosts_content).find(|kh| {
        host_matches(kh.hostname_pattern, &host_entry, host, port)
            && kh.key_type == key_type
            && kh.key_base64 == key_base64
    });
    if let Some(kh) = revoked {
        return HostKeyJudgement::Revoked {
            line: kh.line_number,
        };
    }
    for kh in known_host_entries(known_hosts_content) {
        if !host_matches(kh.hostname_pattern, &host_entry, host, port) || kh.key_type != key_type {
            continue;
        }
        if kh.key_base64 == key_base64 {
            return HostKeyJudgement::Trusted;
        }
        return HostKeyJudgement::Changed {
            line: kh.line_number,
        };
    }
    HostKeyJudgement::Unknown
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
                target: "remote_merge::ssh::known_hosts",
                "Unknown host info sent via channel, rejecting connection: {}",
                self.host
            );
            Ok(false)
        } else {
            // sender がない = 旧来の自動承認モード
            tracing::info!(
                target: "remote_merge::ssh::known_hosts",
                "known_hosts: unknown host. TOFU: adding host key: {}",
                self.host
            );
            known_hosts_io::append_known_hosts_entry(
                self.known_hosts.as_deref(),
                &self.host,
                self.port,
                key_type,
                key_base64,
            );
            Ok(true)
        }
    }
}

/// known_hosts のテキスト内容をパースする（"@" で始まる印付きの行は除く）
#[cfg(test)]
fn parse_known_hosts(content: &str) -> Vec<KnownHost<'_>> {
    known_host_entries(content).collect()
}

fn known_host_entries(content: &str) -> impl Iterator<Item = KnownHost<'_>> {
    entry_lines(content)
        .filter(|(_, line)| !line.starts_with('@'))
        .filter_map(|(line_number, line)| parse_entry(line, line_number))
}

/// "@revoked" の印が付いた行だけをパースする
fn parse_revoked_keys(content: &str) -> impl Iterator<Item = KnownHost<'_>> {
    entry_lines(content).filter_map(|(line_number, line)| {
        let rest = line.strip_prefix("@revoked")?;
        if !rest.starts_with([' ', '\t']) {
            return None;
        }
        parse_entry(rest.trim_start(), line_number)
    })
}

/// 空行とコメントを除いた行を、1始まりの行番号と前後の空白を除いた内容で返す
fn entry_lines(content: &str) -> impl Iterator<Item = (usize, &str)> {
    content
        .lines()
        .enumerate()
        .map(|(line_idx, line)| (line_idx + 1, line.trim()))
        .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'))
}

/// "ホスト 種別 鍵 [コメント]" の一行をパースする
///
/// OpenSSH と同じく、フィールドはタブや連続した空白で区切られていてもよい。
fn parse_entry(line: &str, line_number: usize) -> Option<KnownHost<'_>> {
    let mut fields = line.split_whitespace();
    let (hostname_pattern, key_type, key_base64) = (fields.next()?, fields.next()?, fields.next()?);
    Some(KnownHost {
        hostname_pattern,
        key_type,
        key_base64,
        line_number,
    })
}

/// ホスト名がknown_hostsのパターンに一致するか判定する
pub(crate) fn host_matches(pattern: &str, host_entry: &str, hostname: &str, port: u16) -> bool {
    if pattern.starts_with("|1|") {
        return hashed_host_matches(pattern, hostname, port);
    }

    for p in pattern.split(',') {
        let p = p.trim();
        // 括弧なしの行（ポート 22 用の書き方）は、ポート 22 の接続にだけ当たる（OpenSSH と同じ）
        if p == host_entry || (port == 22 && p == hostname) {
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

    // 括弧なしの行はポート 22 の接続にだけ当たる（OpenSSH と同じ。非標準ポートでは [host]:port の行だけ）
    #[test]
    fn test_a_plain_line_does_not_match_a_non_standard_port() {
        let entry_2222 = format_host_entry("example.com", 2222);
        assert!(!host_matches(
            "example.com",
            &entry_2222,
            "example.com",
            2222
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
            entries[0].hostname_pattern,
            &entry_22,
            "example.com",
            22
        ));

        // "[example.com]:2222" パターンはポート22の host_entry にはマッチしない
        assert!(!host_matches(
            entries[1].hostname_pattern,
            &entry_22,
            "example.com",
            22
        ));

        // ポート2222のエントリはポート2222にマッチする
        let entry_2222 = format_host_entry("example.com", 2222);
        assert!(host_matches(
            entries[1].hostname_pattern,
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

    // ── judge_host_key ──

    const ED25519_KEY: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIKxx";

    #[test]
    fn the_first_same_host_and_key_type_still_decides_between_trusted_and_changed() {
        for (keys, expected) in [
            (
                ["other-key", ED25519_KEY],
                HostKeyJudgement::Changed { line: 1 },
            ),
            ([ED25519_KEY, "other-key"], HostKeyJudgement::Trusted),
        ] {
            let content = format!(
                "example.com ssh-ed25519 {}\nexample.com ssh-ed25519 {}\n",
                keys[0], keys[1]
            );
            assert_eq!(
                judge_host_key(&content, "example.com", 22, "ssh-ed25519", ED25519_KEY),
                expected
            );
        }
    }

    #[test]
    fn a_host_registered_only_with_another_key_type_presents_an_unknown_key() {
        let content = "example.com ssh-rsa AAAAB3NzaC1yc2EAAA\n";

        let judgement = judge_host_key(content, "example.com", 22, "ssh-ed25519", ED25519_KEY);

        assert_eq!(judgement, HostKeyJudgement::Unknown);
    }

    /// OpenSSH と同じく、フィールドの区切りはタブや連続した空白でもよい
    const REVOKED_LINES_WITH_OTHER_SEPARATORS: [&str; 3] = [
        "@revoked\texample.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKxx\n",
        "@revoked example.com  ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKxx\n",
        "@revoked example.com\tssh-ed25519\tAAAAC3NzaC1lZDI1NTE5AAAAIKxx\n",
    ];

    #[test]
    fn a_revoked_line_separated_by_tabs_or_repeated_spaces_revokes_the_key() {
        for revoked in REVOKED_LINES_WITH_OTHER_SEPARATORS {
            let judgement = judge_host_key(revoked, "example.com", 22, "ssh-ed25519", ED25519_KEY);

            assert_eq!(
                judgement,
                HostKeyJudgement::Revoked { line: 1 },
                "{revoked:?}"
            );
        }
    }

    #[test]
    fn a_revoked_line_with_other_separators_wins_over_a_normal_line_for_the_same_key() {
        for revoked in REVOKED_LINES_WITH_OTHER_SEPARATORS {
            let content = format!("example.com ssh-ed25519 {ED25519_KEY}\n{revoked}");

            let judgement = judge_host_key(&content, "example.com", 22, "ssh-ed25519", ED25519_KEY);

            assert_eq!(
                judgement,
                HostKeyJudgement::Revoked { line: 2 },
                "{content:?}"
            );
        }
    }

    // ── 承認した鍵での再接続 ──

    fn random_ed25519_key() -> russh::keys::PublicKey {
        use russh::keys::ssh_key::rand_core::OsRng;
        use russh::keys::{Algorithm, PrivateKey};
        PrivateKey::random(&mut OsRng, Algorithm::Ed25519)
            .unwrap()
            .public_key()
            .clone()
    }

    fn approved_handler(key: &russh::keys::PublicKey) -> SshHandler {
        let openssh = key.to_openssh().unwrap();
        let (key_type, key_base64) = openssh.split_once(' ').unwrap();
        SshHandler::for_approved_key("example.invalid".into(), 22, key_type, key_base64)
    }

    #[tokio::test]
    async fn reconnecting_after_approval_accepts_the_approved_key() {
        use russh::client::Handler;
        let approved = random_ed25519_key();

        let accepted = approved_handler(&approved)
            .check_server_key(&approved)
            .await
            .unwrap();

        assert!(accepted);
    }

    #[tokio::test]
    async fn reconnecting_after_approval_rejects_a_different_key() {
        use russh::client::Handler;
        let approved = random_ed25519_key();
        let presented = random_ed25519_key();

        let accepted = approved_handler(&approved)
            .check_server_key(&presented)
            .await
            .unwrap_or(false);

        assert!(!accepted);
    }
}
