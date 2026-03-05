//! known_hosts ファイルの解析・検証。

use std::io::Write as _;
use std::path::PathBuf;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use hmac::{Hmac, Mac};
use russh::client;
use sha1::Sha1;

/// known_hosts のエントリ
#[derive(Debug, Clone)]
pub(crate) struct KnownHost {
    /// ホスト名パターン（平文またはハッシュ形式 `|1|salt|hash`）
    pub hostname_pattern: String,
    /// キータイプ（例: "ssh-rsa", "ssh-ed25519"）
    pub key_type: String,
    /// キーのbase64エンコードされたデータ
    pub key_base64: String,
}

/// russh の Handler 実装
pub(crate) struct SshHandler {
    pub host: String,
    pub port: u16,
}

impl SshHandler {
    pub fn new(host: String, port: u16) -> Self {
        Self { host, port }
    }

    /// known_hosts ファイルのパスを取得する
    fn known_hosts_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".ssh").join("known_hosts"))
    }

    /// known_hosts ファイルの内容を読み込む
    fn read_known_hosts() -> Option<String> {
        let path = Self::known_hosts_path()?;
        std::fs::read_to_string(&path).ok()
    }

    /// known_hosts ファイルにエントリを追加する
    fn append_known_hosts_entry(&self, key_type: &str, key_base64: &str) {
        let Some(path) = Self::known_hosts_path() else {
            tracing::warn!("known_hosts のパスを取得できませんでした");
            return;
        };

        // .ssh ディレクトリが存在しない場合は作成
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    tracing::warn!("~/.ssh ディレクトリの作成に失敗: {}", e);
                    return;
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ =
                        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
                }
            }
        }

        let host_entry = format_host_entry(&self.host, self.port);
        let line = format!("{} {} {}\n", host_entry, key_type, key_base64);

        let result = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut f| f.write_all(line.as_bytes()));

        if let Err(e) = result {
            tracing::warn!("known_hosts への書き込みに失敗: {}", e);
            return;
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }

        tracing::info!(
            "known_hosts に新しいホストキーを追加しました: {}",
            host_entry
        );
    }
}

impl client::Handler for SshHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        let openssh_str = server_public_key
            .to_openssh()
            .map_err(|e| anyhow::anyhow!("公開鍵のシリアライズに失敗: {}", e))?;

        let parts: Vec<&str> = openssh_str.splitn(2, ' ').collect();
        if parts.len() < 2 {
            return Err(anyhow::anyhow!(
                "公開鍵のフォーマットが不正です: {}",
                openssh_str
            ));
        }
        let server_key_type = parts[0];
        let server_key_base64 = parts[1];

        let known_hosts_content = match Self::read_known_hosts() {
            Some(content) => content,
            None => {
                tracing::info!(
                    "known_hosts ファイルが存在しません。TOFU: ホストキーを追加します: {}",
                    self.host
                );
                self.append_known_hosts_entry(server_key_type, server_key_base64);
                return Ok(true);
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
                    tracing::debug!("known_hosts: ホストキーが一致しました: {}", self.host);
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
                        The {} host key for {} has changed.\n\
                        If this is expected, remove the old entry from ~/.ssh/known_hosts\n\
                        and try again.",
                        server_key_type, self.host
                    ));
                }
            }
        }

        if found_host {
            self.append_known_hosts_entry(server_key_type, server_key_base64);
            return Ok(true);
        }

        tracing::info!(
            "known_hosts: 未知のホストです。TOFU: ホストキーを追加します: {}",
            self.host
        );
        self.append_known_hosts_entry(server_key_type, server_key_base64);
        Ok(true)
    }
}

/// known_hosts のテキスト内容をパースする
pub(crate) fn parse_known_hosts(content: &str) -> Vec<KnownHost> {
    let mut entries = Vec::new();

    for line in content.lines() {
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
