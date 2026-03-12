//! known_hosts ファイルの I/O 操作（読み込み・書き込み・フォーマット）。

use std::io::Write as _;
use std::path::PathBuf;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

use super::known_hosts::format_host_entry;

/// 公開鍵の SHA-256 フィンガープリントを `SHA256:xxx` 形式で返す
pub(crate) fn format_fingerprint(public_key: &russh::keys::PublicKey) -> String {
    use sha2::{Digest, Sha256};

    let openssh_str = match public_key.to_openssh() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("Failed to serialize public key to OpenSSH format: {}", e);
            return "SHA256:<unknown>".to_string();
        }
    };
    let parts: Vec<&str> = openssh_str.splitn(2, ' ').collect();
    if parts.len() < 2 {
        tracing::warn!(
            "Invalid public key format (expected 'type base64'): {}",
            openssh_str
        );
        return "SHA256:<unknown>".to_string();
    }
    let key_bytes = match BASE64.decode(parts[1]) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("Failed to decode public key base64: {}", e);
            return "SHA256:<unknown>".to_string();
        }
    };

    let hash = Sha256::digest(&key_bytes);
    let encoded = BASE64.encode(hash);
    // ssh-keygen -lf と同様、末尾の `=` パディングを削除
    let trimmed = encoded.trim_end_matches('=');
    format!("SHA256:{}", trimmed)
}

/// known_hosts ファイルのパスを取得する
pub(crate) fn known_hosts_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".ssh").join("known_hosts"))
}

/// known_hosts ファイルの内容を読み込む
pub(crate) fn read_known_hosts() -> Option<String> {
    let path = known_hosts_path()?;
    std::fs::read_to_string(&path).ok()
}

/// known_hosts ファイルにエントリを追加する
pub(crate) fn append_known_hosts_entry(host: &str, port: u16, key_type: &str, key_base64: &str) {
    let Some(path) = known_hosts_path() else {
        tracing::warn!("Failed to get known_hosts path");
        return;
    };

    // .ssh ディレクトリが存在しない場合は作成
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!("Failed to create ~/.ssh directory: {}", e);
                return;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
            }
        }
    }

    let host_entry = format_host_entry(host, port);
    let line = format!("{} {} {}\n", host_entry, key_type, key_base64);

    let result = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| f.write_all(line.as_bytes()));

    if let Err(e) = result {
        tracing::warn!("Failed to write to known_hosts: {}", e);
        return;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }

    tracing::info!("Added new host key to known_hosts: {}", host_entry);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_hosts_path_returns_some() {
        // ホームディレクトリが取得できる環境では Some を返す
        if dirs::home_dir().is_some() {
            let path = known_hosts_path().unwrap();
            assert!(path.ends_with(".ssh/known_hosts"));
        }
    }
}
