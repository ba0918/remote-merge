//! SshOptions から russh::Preferred を構築するモジュール。
//!
//! レガシーSSHサーバ向けに、ユーザー設定のアルゴリズムを
//! Preferred に反映する。不明なアルゴリズム名は警告ログを出して無視する。

use std::borrow::Cow;

use russh::{cipher, kex, mac, Preferred};
use tracing::warn;

use crate::config::SshOptions;

/// SshOptions から Preferred を構築する。
///
/// 設定されていないフィールドはデフォルト値を使用する。
/// ユーザー指定のアルゴリズムはデフォルトの **先頭** に追加される
/// （優先度が最も高くなる）。デフォルトに既に含まれるものは重複排除する。
pub fn build_preferred(opts: &SshOptions) -> Preferred {
    let mut preferred = Preferred::default();

    if let Some(ref kex_list) = opts.kex_algorithms {
        if let Some(names) = resolve_kex_names(kex_list) {
            preferred.kex = Cow::Owned(names);
        }
    }

    if let Some(ref cipher_list) = opts.ciphers {
        if let Some(names) = resolve_cipher_names(cipher_list) {
            preferred.cipher = Cow::Owned(names);
        }
    }

    if let Some(ref mac_list) = opts.mac_algorithms {
        if let Some(names) = resolve_mac_names(mac_list) {
            preferred.mac = Cow::Owned(names);
        }
    }

    // host_key_algorithms は ssh_key::Algorithm 型で、文字列→Algorithm の
    // 汎用変換が russh に存在しないため、Phase 3 で対応する。

    preferred
}

/// ユーザー指定の kex アルゴリズム名を解決し、デフォルトの先頭に追加する。
///
/// 全て不明な場合は None を返す（デフォルトを維持）。
fn resolve_kex_names(user_list: &[String]) -> Option<Vec<kex::Name>> {
    let default_kex: &[kex::Name] = Preferred::DEFAULT.kex.as_ref();

    let user_names: Vec<kex::Name> = user_list
        .iter()
        .filter_map(|s| {
            kex::Name::try_from(s.as_str()).ok().or_else(|| {
                warn!(algorithm = %s, "Unknown kex algorithm in config, ignoring");
                None
            })
        })
        .collect();

    if user_names.is_empty() {
        return None;
    }

    // ユーザー指定を先頭に、デフォルトから重複を除いて末尾に追加
    let mut result = user_names.clone();
    for &name in default_kex {
        if !result.contains(&name) {
            result.push(name);
        }
    }

    Some(result)
}

/// ユーザー指定の cipher アルゴリズム名を解決する。
fn resolve_cipher_names(user_list: &[String]) -> Option<Vec<cipher::Name>> {
    let default_cipher: &[cipher::Name] = Preferred::DEFAULT.cipher.as_ref();

    let user_names: Vec<cipher::Name> = user_list
        .iter()
        .filter_map(|s| {
            cipher::Name::try_from(s.as_str()).ok().or_else(|| {
                warn!(algorithm = %s, "Unknown cipher algorithm in config, ignoring");
                None
            })
        })
        .collect();

    if user_names.is_empty() {
        return None;
    }

    let mut result = user_names.clone();
    for &name in default_cipher {
        if !result.contains(&name) {
            result.push(name);
        }
    }

    Some(result)
}

/// ユーザー指定の MAC アルゴリズム名を解決する。
fn resolve_mac_names(user_list: &[String]) -> Option<Vec<mac::Name>> {
    let default_mac: &[mac::Name] = Preferred::DEFAULT.mac.as_ref();

    let user_names: Vec<mac::Name> = user_list
        .iter()
        .filter_map(|s| {
            mac::Name::try_from(s.as_str()).ok().or_else(|| {
                warn!(algorithm = %s, "Unknown MAC algorithm in config, ignoring");
                None
            })
        })
        .collect();

    if user_names.is_empty() {
        return None;
    }

    let mut result = user_names.clone();
    for &name in default_mac {
        if !result.contains(&name) {
            result.push(name);
        }
    }

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_preferred_default_when_no_options() {
        let opts = SshOptions::default();
        let preferred = build_preferred(&opts);
        // デフォルトと同じになる
        assert_eq!(preferred.kex.as_ref(), Preferred::DEFAULT.kex.as_ref());
        assert_eq!(
            preferred.cipher.as_ref(),
            Preferred::DEFAULT.cipher.as_ref()
        );
        assert_eq!(preferred.mac.as_ref(), Preferred::DEFAULT.mac.as_ref());
    }

    #[test]
    fn test_build_preferred_kex_prepended() {
        let opts = SshOptions {
            kex_algorithms: Some(vec![
                "diffie-hellman-group-exchange-sha1".to_string(),
                "diffie-hellman-group14-sha1".to_string(),
            ]),
            ..Default::default()
        };
        let preferred = build_preferred(&opts);
        let kex_list = preferred.kex.as_ref();

        // ユーザー指定が先頭にある
        assert_eq!(kex_list[0], kex::DH_GEX_SHA1);
        assert_eq!(kex_list[1], kex::DH_G14_SHA1);

        // デフォルトのアルゴリズムも含まれている
        assert!(kex_list.contains(&kex::CURVE25519));
    }

    #[test]
    fn test_build_preferred_unknown_kex_ignored() {
        let opts = SshOptions {
            kex_algorithms: Some(vec!["totally-fake-algorithm".to_string()]),
            ..Default::default()
        };
        let preferred = build_preferred(&opts);
        // 全て不明ならデフォルトのまま
        assert_eq!(preferred.kex.as_ref(), Preferred::DEFAULT.kex.as_ref());
    }

    #[test]
    fn test_build_preferred_cipher_prepended() {
        let opts = SshOptions {
            ciphers: Some(vec!["aes128-cbc".to_string()]),
            ..Default::default()
        };
        let preferred = build_preferred(&opts);
        let cipher_list = preferred.cipher.as_ref();
        assert_eq!(cipher_list[0], cipher::AES_128_CBC);
    }

    #[test]
    fn test_build_preferred_mac_prepended() {
        let opts = SshOptions {
            mac_algorithms: Some(vec!["hmac-sha1".to_string()]),
            ..Default::default()
        };
        let preferred = build_preferred(&opts);
        let mac_list = preferred.mac.as_ref();
        assert_eq!(mac_list[0], mac::HMAC_SHA1);
    }

    #[test]
    fn test_build_preferred_no_duplicates() {
        let opts = SshOptions {
            kex_algorithms: Some(vec!["curve25519-sha256".to_string()]),
            ..Default::default()
        };
        let preferred = build_preferred(&opts);
        let kex_list = preferred.kex.as_ref();

        // curve25519-sha256 は1回だけ
        let count = kex_list.iter().filter(|&&n| n == kex::CURVE25519).count();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_build_preferred_mixed_valid_invalid() {
        let opts = SshOptions {
            kex_algorithms: Some(vec![
                "diffie-hellman-group14-sha1".to_string(),
                "invalid-algo".to_string(),
            ]),
            ..Default::default()
        };
        let preferred = build_preferred(&opts);
        let kex_list = preferred.kex.as_ref();

        // 有効なものだけ先頭に追加
        assert_eq!(kex_list[0], kex::DH_G14_SHA1);
        // デフォルトも続く
        assert!(kex_list.len() > 1);
    }
}
