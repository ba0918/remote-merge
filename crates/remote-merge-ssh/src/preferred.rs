//! SshOptions から russh::Preferred を構築するモジュール。
//!
//! レガシーSSHサーバ向けに、ユーザー設定のアルゴリズムを
//! Preferred に反映する。このツールが使えないアルゴリズム名（不明な名前と、ホスト鍵の ssh-dss）は
//! 利用者に向けた警告（target が remote_merge::warning。記録の細かさによらず標準エラーに出る）を出して
//! 無視する。

use std::borrow::Cow;

use russh::keys::Algorithm;
use russh::{Preferred, cipher, kex, mac};
use tracing::warn;

use crate::config::SshOptions;

/// SshOptions から Preferred を構築する。
///
/// 設定されていないフィールドはデフォルト値を使用する。
/// ユーザー指定のアルゴリズムはデフォルトの **先頭** に追加される
/// （優先度が最も高くなる）。デフォルトに既に含まれるものは重複排除する。
pub fn build_preferred(opts: &SshOptions) -> Preferred {
    let mut preferred = Preferred {
        mac: Cow::Owned(default_macs()),
        ..Preferred::default()
    };

    if let Some(ref kex_list) = opts.kex_algorithms
        && let Some(names) = resolve_kex_names(kex_list)
    {
        preferred.kex = Cow::Owned(names);
    }

    if let Some(ref cipher_list) = opts.ciphers
        && let Some(names) = resolve_cipher_names(cipher_list)
    {
        preferred.cipher = Cow::Owned(names);
    }

    if let Some(ref mac_list) = opts.mac_algorithms
        && let Some(names) = resolve_mac_names(mac_list)
    {
        preferred.mac = Cow::Owned(names);
    }

    if let Some(ref key_list) = opts.host_key_algorithms
        && let Some(names) = resolve_key_names(key_list)
    {
        preferred.key = Cow::Owned(names);
    }

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
                warn!(target: "remote_merge::warning", algorithm = %s, "Unknown kex algorithm in config, ignoring");
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

/// このツールがホスト鍵の方式の名前を使えない理由
pub(crate) enum UnusableHostKey {
    /// russh が知らない名前。russh は知らない名前も `Algorithm::Other` として読む
    Unknown,
    /// russh は ssh-dss を知っているが、使う ssh-key は dsa の機能なしで作られ、DSA の署名を確かめられない。
    /// 申し出ると方式は合っても "Wrong server signature" で失敗する
    DsaSignatureUnverifiable,
}

/// ホスト鍵の方式の名前を、このツールが接続に使える方式として読む
pub(crate) fn usable_host_key(name: &str) -> Result<Algorithm, UnusableHostKey> {
    match name.parse::<Algorithm>() {
        Ok(Algorithm::Other(_)) | Err(_) => Err(UnusableHostKey::Unknown),
        Ok(Algorithm::Dsa) => Err(UnusableHostKey::DsaSignatureUnverifiable),
        Ok(algorithm) => Ok(algorithm),
    }
}

/// ユーザー指定のホスト鍵の方式を解決し、デフォルトの先頭に追加する。使えない方式は警告して除く。
/// 全て使えない場合は None を返す
fn resolve_key_names(user_list: &[String]) -> Option<Vec<Algorithm>> {
    let user_names: Vec<Algorithm> = user_list
        .iter()
        .filter_map(|s| match usable_host_key(s) {
            Ok(algorithm) => Some(algorithm),
            Err(UnusableHostKey::Unknown) => {
                warn!(target: "remote_merge::warning", algorithm = %s, "Unknown host key algorithm in config, ignoring");
                None
            }
            Err(UnusableHostKey::DsaSignatureUnverifiable) => {
                warn!(target: "remote_merge::warning", algorithm = %s, "Unsupported host key algorithm in config (DSA signatures cannot be verified), ignoring");
                None
            }
        })
        .collect();
    if user_names.is_empty() {
        return None;
    }
    let mut result = user_names;
    for name in Preferred::DEFAULT.key.iter() {
        if !result.contains(name) {
            result.push(name.clone());
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
                warn!(target: "remote_merge::warning", algorithm = %s, "Unknown cipher algorithm in config, ignoring");
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

/// 既定の MAC。russh の既定の後ろに SHA-1 の MAC を置く。
///
/// russh は既定から SHA-1 の MAC を外したが、OpenSSH 5.9 より前のサーバは SHA-2 の MAC を持たず、
/// 外したままだと MAC を設定しないとつながらない。SHA-2 を持つサーバでは選ばれないよう最後に置く
fn default_macs() -> Vec<mac::Name> {
    let mut macs = Preferred::DEFAULT.mac.to_vec();
    for name in [mac::HMAC_SHA1_ETM, mac::HMAC_SHA1] {
        if !macs.contains(&name) {
            macs.push(name);
        }
    }
    macs
}

/// ユーザー指定の MAC アルゴリズム名を解決する。
fn resolve_mac_names(user_list: &[String]) -> Option<Vec<mac::Name>> {
    let default_mac = default_macs();

    let user_names: Vec<mac::Name> = user_list
        .iter()
        .filter_map(|s| {
            mac::Name::try_from(s.as_str()).ok().or_else(|| {
                warn!(target: "remote_merge::warning", algorithm = %s, "Unknown MAC algorithm in config, ignoring");
                None
            })
        })
        .collect();

    if user_names.is_empty() {
        return None;
    }

    let mut result = user_names.clone();
    for name in default_mac {
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
    }

    // SHA-2 の MAC を持たない古いサーバ（OpenSSH 5.9 より前）へも、MAC を設定せずにつなげる
    #[test]
    fn default_macs_fall_back_to_sha1_after_the_safe_ones() {
        let preferred = build_preferred(&SshOptions::default());
        let macs = preferred.mac.as_ref();

        let safe = Preferred::DEFAULT.mac.as_ref();
        assert_eq!(&macs[..safe.len()], safe);
        assert_eq!(&macs[safe.len()..], &[mac::HMAC_SHA1_ETM, mac::HMAC_SHA1]);
    }

    #[test]
    fn configured_macs_come_before_the_default_ones_including_sha1() {
        let opts = SshOptions {
            mac_algorithms: Some(vec!["hmac-sha2-256".to_string()]),
            ..Default::default()
        };
        let preferred = build_preferred(&opts);
        let macs = preferred.mac.as_ref();

        assert_eq!(macs[0], mac::HMAC_SHA256);
        assert_eq!(macs.iter().filter(|&&n| n == mac::HMAC_SHA256).count(), 1);
        assert_eq!(macs.last(), Some(&mac::HMAC_SHA1));
    }

    // 設定したホスト鍵の方式を、既定の前に置いて接続に使う（docs/ir/ssh/compatibility.md の REQ-ssh-006）
    // @kotowari[REQ-ssh-006]
    #[test]
    fn host_key_algorithms_are_prepended_and_unknown_names_are_ignored() {
        let opts = SshOptions {
            host_key_algorithms: Some(vec!["ssh-rsa".to_string(), "no-such-key".to_string()]),
            ..Default::default()
        };
        let preferred = build_preferred(&opts);
        let keys = preferred.key.as_ref();
        assert_eq!(
            keys[0],
            "ssh-rsa".parse::<russh::keys::Algorithm>().unwrap()
        );
        assert!(keys.len() > 1, "{keys:?}");
        assert!(
            !keys.iter().any(|key| key.as_str() == "no-such-key"),
            "{keys:?}"
        );
    }

    // ssh-dss を申し出ると、DSA のホスト鍵のサーバとは方式が合っても署名を確かめられずに失敗する
    // @kotowari[REQ-ssh-006]
    #[test]
    fn a_dsa_host_key_algorithm_in_config_is_not_offered() {
        let opts = SshOptions {
            host_key_algorithms: Some(vec!["ssh-dss".to_string()]),
            ..Default::default()
        };
        let preferred = build_preferred(&opts);
        assert_eq!(preferred.key.as_ref(), Preferred::DEFAULT.key.as_ref());
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
