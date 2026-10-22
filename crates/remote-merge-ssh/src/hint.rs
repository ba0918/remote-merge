//! サーバと共通の SSH の方式がなく接続に失敗したときの、設定のヒントを作る。

use russh::{AlgorithmKind, cipher, kex, mac};

use crate::preferred;

/// 接続の失敗が、サーバと共通の方式がないことによるなら、ssh_options に書く設定の例を返す。
///
/// 例には、サーバが申し出た方式のうちこのツールが使えるものを並べる。どの方式が合わなかったかは
/// russh の誤りの種類で見る（表示の文言は版で変わりうる）。使えるものがないとき、方式の設定が
/// ないとき（圧縮）、ほかの失敗のときは `None` を返す。
pub(crate) fn ssh_algorithm_hint(error: &anyhow::Error) -> Option<String> {
    let Some(russh::Error::NoCommonAlgo { kind, theirs, .. }) =
        error.downcast_ref::<russh::Error>()
    else {
        return None;
    };
    let (setting, usable): (&str, fn(&str) -> bool) = match kind {
        AlgorithmKind::Kex => ("kex_algorithms", usable_kex),
        AlgorithmKind::Key => ("host_key_algorithms", usable_host_key),
        AlgorithmKind::Cipher => ("ciphers", usable_cipher),
        AlgorithmKind::Mac => ("mac_algorithms", usable_mac),
        AlgorithmKind::Compression => return None,
    };
    let names: Vec<String> = theirs
        .iter()
        .filter(|name| usable(name))
        .map(|name| format!("{name:?}"))
        .collect();
    if names.is_empty() {
        return None;
    }

    Some(format!(
        "Hint: The remote server may require legacy SSH algorithms.\n\
         Add the following to your config.toml under [servers.<name>]:\n\n\
         [servers.<name>.ssh_options]\n  {setting} = [{}]\n",
        names.join(", ")
    ))
}

/// 鍵交換をしない none は、共有の秘密がなく鍵を誰でも求められ、ホスト鍵の署名も確かめないので勧めない
fn usable_kex(name: &str) -> bool {
    kex::Name::try_from(name).is_ok_and(|name| name != kex::NONE)
}

/// 設定に書くと無視される方式は勧めない
fn usable_host_key(name: &str) -> bool {
    preferred::usable_host_key(name).is_ok()
}

/// 暗号をかけない none は、つながっても中身が平文で流れるので勧めない
fn usable_cipher(name: &str) -> bool {
    cipher::Name::try_from(name).is_ok_and(|name| name != cipher::NONE && name != cipher::CLEAR)
}

/// none は改ざんを検知できないので勧めない
fn usable_mac(name: &str) -> bool {
    mac::Name::try_from(name).is_ok_and(|name| name != mac::NONE)
}

#[cfg(test)]
mod tests {
    use russh::AlgorithmKind;

    use super::*;

    /// サーバと共通の方式がなく、サーバが `theirs` を申し出たときの接続の誤り
    fn no_common(kind: AlgorithmKind, theirs: &[&str]) -> anyhow::Error {
        russh::Error::NoCommonAlgo {
            kind,
            ours: vec!["curve25519-sha256".into()],
            theirs: theirs.iter().map(|name| name.to_string()).collect(),
        }
        .into()
    }

    // @kotowari[REQ-ssh-007]
    #[test]
    fn a_key_exchange_mismatch_suggests_the_servers_key_exchanges() {
        let hint = ssh_algorithm_hint(&no_common(
            AlgorithmKind::Kex,
            &[
                "diffie-hellman-group14-sha1",
                "kex-strict-s-v00@openssh.com",
            ],
        ))
        .unwrap();
        assert!(hint.contains("[servers.<name>.ssh_options]"), "{hint}");
        assert!(
            hint.contains("kex_algorithms = [\"diffie-hellman-group14-sha1\"]"),
            "{hint}"
        );
    }

    // @kotowari[REQ-ssh-007]
    #[test]
    fn a_cipher_mismatch_suggests_only_the_servers_ciphers_this_tool_can_use() {
        let hint = ssh_algorithm_hint(&no_common(
            AlgorithmKind::Cipher,
            &["blowfish-cbc", "aes128-cbc", "none"],
        ))
        .unwrap();
        assert!(hint.contains("ciphers = [\"aes128-cbc\"]"), "{hint}");
    }

    // 既定のホスト鍵の方式は "ssh-rsa" まで含むので、ホスト鍵が合わないのは DSA の鍵だけの古いサーバで、
    // このツールはその署名を確かめられない
    // @kotowari[REQ-ssh-007]
    #[test]
    fn no_hint_when_the_servers_only_host_key_is_dsa() {
        let error = no_common(AlgorithmKind::Key, &["ssh-dss"]);
        assert_eq!(ssh_algorithm_hint(&error), None);
    }

    // @kotowari[REQ-ssh-007]
    #[test]
    fn no_hint_when_the_only_key_exchange_offered_is_none() {
        let error = no_common(AlgorithmKind::Kex, &["none"]);
        assert_eq!(ssh_algorithm_hint(&error), None);
    }

    #[test]
    fn no_hint_when_the_server_offers_nothing_this_tool_can_use() {
        let error = no_common(AlgorithmKind::Mac, &["hmac-md5", "none"]);
        assert_eq!(ssh_algorithm_hint(&error), None);
    }

    #[test]
    fn no_hint_for_a_compression_mismatch_since_there_is_no_setting_for_it() {
        let error = no_common(AlgorithmKind::Compression, &["zlib"]);
        assert_eq!(ssh_algorithm_hint(&error), None);
    }

    #[test]
    fn no_hint_for_an_unrelated_failure() {
        let error =
            anyhow::Error::from(std::io::Error::from(std::io::ErrorKind::ConnectionRefused));
        assert_eq!(ssh_algorithm_hint(&error), None);
    }
}
