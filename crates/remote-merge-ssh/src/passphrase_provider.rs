//! パスフレーズ取得を UI 層に委譲するための trait。端末から尋ねる実装は製品が持つ。
//!
//! SSH 鍵がパスフレーズで保護されている場合、russh の `load_secret_key` に
//! パスフレーズを渡す必要がある。取得方法を trait で抽象化し、
//! テスタビリティと拡張性を確保する。

use zeroize::Zeroizing;

/// パスフレーズ取得を UI 層に委譲するための trait。
///
/// 戻り値は `Zeroizing<String>` でラップされ、
/// ドロップ時にメモリがゼロ化される。
pub trait PassphraseProvider: Send + Sync {
    /// 指定された鍵パスに対するパスフレーズを取得する。
    ///
    /// `None` を返すとパスフレーズ入力をスキップ（= 認証失敗）。
    fn get_passphrase(&self, key_path: &str) -> Option<Zeroizing<String>>;
}

/// サーバ名からパスフレーズ用の環境変数名を生成する。
/// 例: "my-server" → "REMOTE_MERGE_KEY_PASSPHRASE_MY_SERVER"
pub(crate) fn passphrase_env_key(server_name: &str) -> String {
    crate::server_env::server_env_name("REMOTE_MERGE_KEY_PASSPHRASE_", server_name)
}

/// パスフレーズ入力のリトライ上限
pub(crate) const MAX_PASSPHRASE_RETRIES: u32 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_env_name_is_the_server_name_uppercased_with_hyphens_and_dots_as_underscores() {
        for (server_name, expected) in [
            ("develop", "REMOTE_MERGE_KEY_PASSPHRASE_DEVELOP"),
            ("my-server", "REMOTE_MERGE_KEY_PASSPHRASE_MY_SERVER"),
            ("web.prod", "REMOTE_MERGE_KEY_PASSPHRASE_WEB_PROD"),
            (
                "app-1.example.com",
                "REMOTE_MERGE_KEY_PASSPHRASE_APP_1_EXAMPLE_COM",
            ),
            (
                "my-cool-server",
                "REMOTE_MERGE_KEY_PASSPHRASE_MY_COOL_SERVER",
            ),
            ("PROD", "REMOTE_MERGE_KEY_PASSPHRASE_PROD"),
            ("MyServer", "REMOTE_MERGE_KEY_PASSPHRASE_MYSERVER"),
            ("my_server", "REMOTE_MERGE_KEY_PASSPHRASE_MY_SERVER"),
            ("", "REMOTE_MERGE_KEY_PASSPHRASE_"),
        ] {
            assert_eq!(passphrase_env_key(server_name), expected, "{server_name:?}");
        }
    }
}
