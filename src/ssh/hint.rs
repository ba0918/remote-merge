//! SSH接続エラーからレガシーアルゴリズムのヒントを生成する。

/// SSH接続エラーメッセージを解析し、ssh_options の設定ヒントを返す。
///
/// russh が古いサーバのアルゴリズムに対応しておらず接続に失敗した場合、
/// config.toml に追加すべき ssh_options の例を提示する。
///
/// 該当パターンがない場合は `None` を返す。
pub fn ssh_algorithm_hint(error_message: &str) -> Option<String> {
    let lower = error_message.to_lowercase();
    let mut hints: Vec<&str> = Vec::new();

    if lower.contains("no matching key exchange") || lower.contains("kex") {
        hints.push(
            "  kex_algorithms = \"curve25519-sha256,diffie-hellman-group14-sha256,diffie-hellman-group14-sha1\"",
        );
    }

    if lower.contains("no matching host key") || lower.contains("host key algorithm") {
        hints.push("  host_key_algorithms = \"ssh-ed25519,rsa-sha2-256,rsa-sha2-512,ssh-rsa\"");
    }

    if lower.contains("no matching cipher") || lower.contains("encryption") {
        hints.push("  ciphers = \"aes256-ctr,aes128-ctr,chacha20-poly1305@openssh.com\"");
    }

    if lower.contains("no matching mac") || lower.contains("message authentication code") {
        hints.push("  mac_algorithms = \"hmac-sha2-256,hmac-sha2-512,hmac-sha1\"");
    }

    if hints.is_empty() {
        return None;
    }

    let mut result = String::from(
        "Hint: The remote server may require legacy SSH algorithms.\n\
         Add the following to your config.toml under [servers.<name>]:\n\n\
         [servers.<name>.ssh_options]\n",
    );
    for hint in &hints {
        result.push_str(hint);
        result.push('\n');
    }

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kex_hint() {
        let hint = ssh_algorithm_hint("no matching key exchange method found");
        assert!(hint.is_some());
        let text = hint.unwrap();
        assert!(text.contains("kex_algorithms"));
        assert!(text.contains("ssh_options"));
    }

    #[test]
    fn test_host_key_hint() {
        let hint = ssh_algorithm_hint("no matching host key type found");
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("host_key_algorithms"));
    }

    #[test]
    fn test_cipher_hint() {
        let hint = ssh_algorithm_hint("no matching cipher found");
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("ciphers"));
    }

    #[test]
    fn test_mac_hint() {
        let hint = ssh_algorithm_hint("no matching MAC found");
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("mac_algorithms"));
    }

    #[test]
    fn test_no_hint_for_unrelated_error() {
        let hint = ssh_algorithm_hint("connection refused");
        assert!(hint.is_none());
    }

    #[test]
    fn test_hint_format_contains_config_example() {
        let hint = ssh_algorithm_hint("no matching key exchange").unwrap();
        assert!(hint.contains("[servers.<name>.ssh_options]"));
        assert!(hint.contains("Hint:"));
    }

    #[test]
    fn test_multiple_patterns_match() {
        let hint = ssh_algorithm_hint("no matching key exchange and no matching cipher found");
        assert!(hint.is_some());
        let text = hint.unwrap();
        assert!(text.contains("kex_algorithms"));
        assert!(text.contains("ciphers"));
    }

    #[test]
    fn test_case_insensitive() {
        let hint = ssh_algorithm_hint("No Matching Key Exchange Method");
        assert!(hint.is_some());
    }
}
