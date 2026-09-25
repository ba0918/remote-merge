//! パスフレーズ取得を UI 層に委譲するための trait と実装。
//!
//! SSH 鍵がパスフレーズで保護されている場合、russh の `load_secret_key` に
//! パスフレーズを渡す必要がある。取得方法（CLI入力 / 環境変数）を
//! trait で抽象化し、テスタビリティと拡張性を確保する。

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

/// パスフレーズなし（既存動作互換）。
///
/// `get_passphrase` は常に `None` を返す。
pub struct NoneProvider;

impl PassphraseProvider for NoneProvider {
    fn get_passphrase(&self, _key_path: &str) -> Option<Zeroizing<String>> {
        None
    }
}

/// 環境変数からパスフレーズを取得する。
///
/// 環境変数名: `REMOTE_MERGE_KEY_PASSPHRASE_{NORMALIZED_SERVER_NAME}`
/// サーバ名の正規化: ハイフン → アンダースコア、大文字化。
pub struct EnvPassphraseProvider {
    server_name: String,
}

impl EnvPassphraseProvider {
    pub fn new(server_name: &str) -> Self {
        Self {
            server_name: server_name.to_string(),
        }
    }
}

impl PassphraseProvider for EnvPassphraseProvider {
    fn get_passphrase(&self, _key_path: &str) -> Option<Zeroizing<String>> {
        let env_key = passphrase_env_key(&self.server_name);
        std::env::var(&env_key)
            .ok()
            .filter(|s| !s.is_empty())
            .map(Zeroizing::new)
    }
}

/// CLI（ターミナル）からマスク入力でパスフレーズを取得する。
///
/// `rpassword::prompt_password` を使用。
/// 端末が利用不可の場合は `None` を返す。
pub struct CliPassphraseProvider;

impl PassphraseProvider for CliPassphraseProvider {
    fn get_passphrase(&self, key_path: &str) -> Option<Zeroizing<String>> {
        let prompt = format!("Enter passphrase for key '{}': ", key_path);
        match rpassword::prompt_password(&prompt) {
            Ok(pass) if !pass.is_empty() => Some(Zeroizing::new(pass)),
            Ok(_) => None,
            Err(e) => {
                tracing::debug!("Failed to read passphrase from terminal: {}", e);
                eprintln!("Error: Could not read passphrase from terminal.");
                None
            }
        }
    }
}

/// サーバ名からパスフレーズ用の環境変数名を生成する。
///
/// 正規化ルール: ハイフン → アンダースコア、大文字化。
/// 例: "my-server" → "REMOTE_MERGE_KEY_PASSPHRASE_MY_SERVER"
pub fn passphrase_env_key(server_name: &str) -> String {
    let normalized = server_name.replace('-', "_").to_uppercase();
    format!("REMOTE_MERGE_KEY_PASSPHRASE_{}", normalized)
}

/// パスフレーズ入力のリトライ上限
pub const MAX_PASSPHRASE_RETRIES: u32 = 3;

/// CLI/TUI 共通のデフォルトプロバイダを作成する。
///
/// ターミナルからのマスク入力でパスフレーズを取得する。
/// 環境変数チェックは `load_secret_key_with_passphrase` 内で
/// サーバ名を使って先行チェックするため、プロバイダには含めない。
pub fn build_default_provider() -> CliPassphraseProvider {
    CliPassphraseProvider
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── passphrase_env_key テスト ──

    #[test]
    fn test_passphrase_env_key_simple() {
        assert_eq!(
            passphrase_env_key("develop"),
            "REMOTE_MERGE_KEY_PASSPHRASE_DEVELOP"
        );
    }

    #[test]
    fn test_passphrase_env_key_hyphen_to_underscore() {
        assert_eq!(
            passphrase_env_key("my-server"),
            "REMOTE_MERGE_KEY_PASSPHRASE_MY_SERVER"
        );
    }

    #[test]
    fn test_passphrase_env_key_multiple_hyphens() {
        assert_eq!(
            passphrase_env_key("my-cool-server"),
            "REMOTE_MERGE_KEY_PASSPHRASE_MY_COOL_SERVER"
        );
    }

    #[test]
    fn test_passphrase_env_key_already_uppercase() {
        assert_eq!(
            passphrase_env_key("PROD"),
            "REMOTE_MERGE_KEY_PASSPHRASE_PROD"
        );
    }

    #[test]
    fn test_passphrase_env_key_mixed_case() {
        assert_eq!(
            passphrase_env_key("MyServer"),
            "REMOTE_MERGE_KEY_PASSPHRASE_MYSERVER"
        );
    }

    #[test]
    fn test_passphrase_env_key_underscore_preserved() {
        assert_eq!(
            passphrase_env_key("my_server"),
            "REMOTE_MERGE_KEY_PASSPHRASE_MY_SERVER"
        );
    }

    #[test]
    fn test_passphrase_env_key_empty() {
        assert_eq!(passphrase_env_key(""), "REMOTE_MERGE_KEY_PASSPHRASE_");
    }

    // ── NoneProvider テスト ──

    #[test]
    fn test_none_provider_always_returns_none() {
        let provider = NoneProvider;
        assert_eq!(provider.get_passphrase("/path/to/key"), None);
        assert_eq!(provider.get_passphrase(""), None);
    }

    // ── EnvPassphraseProvider テスト ──

    #[test]
    #[serial_test::serial]
    fn test_env_provider_returns_value_when_set() {
        let env_key = "REMOTE_MERGE_KEY_PASSPHRASE_ENVTEST";
        // safety: テスト用に環境変数をセット（シリアル実行前提）
        unsafe { std::env::set_var(env_key, "test-passphrase") };

        let provider = EnvPassphraseProvider::new("envtest");
        assert_eq!(
            provider.get_passphrase("/any/key"),
            Some(Zeroizing::new("test-passphrase".to_string()))
        );

        unsafe { std::env::remove_var(env_key) };
    }

    #[test]
    fn test_env_provider_returns_none_when_not_set() {
        // 環境変数が存在しない場合
        let provider = EnvPassphraseProvider::new("nonexistent-env-test-server");
        assert_eq!(provider.get_passphrase("/any/key"), None);
    }

    #[test]
    #[serial_test::serial]
    fn test_env_provider_returns_none_when_empty() {
        // W3: 空文字列の環境変数は None として扱う
        let env_key = "REMOTE_MERGE_KEY_PASSPHRASE_EMPTYTEST";
        unsafe { std::env::set_var(env_key, "") };

        let provider = EnvPassphraseProvider::new("emptytest");
        assert_eq!(provider.get_passphrase("/any/key"), None);

        unsafe { std::env::remove_var(env_key) };
    }

    #[test]
    #[serial_test::serial]
    fn test_env_provider_normalizes_server_name() {
        let env_key = "REMOTE_MERGE_KEY_PASSPHRASE_MY_SERVER";
        unsafe { std::env::set_var(env_key, "secret") };

        let provider = EnvPassphraseProvider::new("my-server");
        assert_eq!(
            provider.get_passphrase("/any/key"),
            Some(Zeroizing::new("secret".to_string()))
        );

        unsafe { std::env::remove_var(env_key) };
    }
}
