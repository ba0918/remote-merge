//! known_hosts ファイルの I/O 操作（読み込み・書き込み・フォーマット）。

use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use super::known_hosts::format_host_entry;
use super::known_hosts_location::known_hosts_location;

/// 公開鍵の SHA-256 フィンガープリントを `SHA256:xxx` 形式で返す
pub(crate) fn format_fingerprint(public_key: &russh::keys::PublicKey) -> String {
    use sha2::{Digest, Sha256};

    let openssh_str = match public_key.to_openssh() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(target: "remote_merge::ssh::known_hosts_io", "Failed to serialize public key to OpenSSH format: {}", e);
            return "SHA256:<unknown>".to_string();
        }
    };
    let parts: Vec<&str> = openssh_str.splitn(2, ' ').collect();
    if parts.len() < 2 {
        tracing::warn!(
            target: "remote_merge::ssh::known_hosts_io",
            "Invalid public key format (expected 'type base64'): {}",
            openssh_str
        );
        return "SHA256:<unknown>".to_string();
    }
    let key_bytes = match BASE64.decode(parts[1]) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(target: "remote_merge::ssh::known_hosts_io", "Failed to decode public key base64: {}", e);
            return "SHA256:<unknown>".to_string();
        }
    };

    let hash = Sha256::digest(&key_bytes);
    let encoded = BASE64.encode(hash);
    // ssh-keygen -lf と同様、末尾の `=` パディングを削除
    let trimmed = encoded.trim_end_matches('=');
    format!("SHA256:{}", trimmed)
}

/// known_hosts ファイルの内容を読み込む。
///
/// ファイル（またはホームディレクトリ）がないときは `Ok(None)`。
/// それ以外の読み込みの失敗を「ファイルなし」と同じにすると、鍵が変わったホストが
/// 未知のホストとして承認され得るため、エラーとして返す。
pub(crate) fn read_known_hosts(configured: Option<&Path>) -> std::io::Result<Option<String>> {
    let Some(path) = known_hosts_location(configured) else {
        return Ok(None);
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// known_hosts ファイルにエントリを追加する
pub(crate) fn append_known_hosts_entry(
    configured: Option<&Path>,
    host: &str,
    port: u16,
    key_type: &str,
    key_base64: &str,
) {
    let Some(path) = known_hosts_location(configured) else {
        tracing::warn!(target: "remote_merge::ssh::known_hosts_io", "Failed to get known_hosts path");
        return;
    };

    // .ssh ディレクトリが存在しない場合は作成
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!(target: "remote_merge::ssh::known_hosts_io", "Failed to create ~/.ssh directory: {}", e);
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
        }
    }

    let host_entry = format_host_entry(host, port);
    let line = format!("{} {} {}\n", host_entry, key_type, key_base64);

    let result = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| {
            if f.metadata()?.len() > 0 {
                f.seek(SeekFrom::End(-1))?;
                let mut last = [0];
                f.read_exact(&mut last)?;
                if last[0] != b'\n' {
                    f.write_all(b"\n")?;
                }
            }
            f.write_all(line.as_bytes())
        });

    if let Err(e) = result {
        tracing::warn!(target: "remote_merge::ssh::known_hosts_io", "Failed to write to known_hosts: {}", e);
        return;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }

    tracing::info!(target: "remote_merge::ssh::known_hosts_io", "Added new host key to known_hosts: {}", host_entry);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known_hosts::{HostKeyJudgement, judge_host_key};

    #[test]
    fn appending_preserves_and_separates_an_unterminated_host_entry() {
        assert_append_preserves_entries("old.example ssh-ed25519 old-key");
    }

    #[test]
    fn appending_separates_an_unterminated_final_comment() {
        assert_append_preserves_entries("old.example ssh-ed25519 old-key\n# final comment");
    }

    #[test]
    fn appending_does_not_add_an_unnecessary_separator() {
        for original in ["", "old.example ssh-ed25519 old-key\n"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("known_hosts");
            std::fs::write(&path, original).unwrap();
            append_known_hosts_entry(Some(&path), "new.example", 22, "ssh-ed25519", "new-key");
            assert_eq!(
                std::fs::read_to_string(path).unwrap(),
                format!("{original}new.example ssh-ed25519 new-key\n")
            );
        }
    }

    fn assert_append_preserves_entries(original: &str) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        std::fs::write(&path, original).unwrap();
        append_known_hosts_entry(Some(&path), "new.example", 22, "ssh-ed25519", "new-key");
        let content = std::fs::read_to_string(path).unwrap();
        assert_eq!(
            content,
            format!("{original}\nnew.example ssh-ed25519 new-key\n")
        );
        for (host, key) in [("old.example", "old-key"), ("new.example", "new-key")] {
            assert_eq!(
                judge_host_key(&content, host, 22, "ssh-ed25519", key),
                HostKeyJudgement::Trusted
            );
        }
    }
}
