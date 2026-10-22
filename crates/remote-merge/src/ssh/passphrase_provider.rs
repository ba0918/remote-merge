//! 鍵のパスフレーズを端末から尋ねるプロバイダ。

pub use remote_merge_ssh::passphrase_provider::PassphraseProvider;
use zeroize::Zeroizing;

/// CLI（ターミナル）からマスク入力でパスフレーズを取得する。
///
/// `rpassword::prompt_password` を使用。
/// 端末が利用不可の場合は `None` を返す。
/// 環境変数のパスフレーズは、鍵を読み込むときにサーバ名から先に確かめるので、ここでは読まない。
pub(crate) struct CliPassphraseProvider;

impl PassphraseProvider for CliPassphraseProvider {
    fn get_passphrase(&self, key_path: &str) -> Option<Zeroizing<String>> {
        let prompt = format!("Enter passphrase for key '{}': ", key_path);
        match rpassword::prompt_password(&prompt) {
            Ok(pass) if !pass.is_empty() => Some(Zeroizing::new(pass)),
            Ok(_) => None,
            Err(e) => {
                tracing::debug!(target: "remote_merge::ssh::passphrase_provider", "Failed to read passphrase from terminal: {}", e);
                tracing::warn!(
                    target: "remote_merge::warning",
                    "Could not read the passphrase from the terminal."
                );
                None
            }
        }
    }
}
