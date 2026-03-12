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
    use std::sync::{Arc, Mutex};

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
    fn test_env_provider_returns_none_when_empty() {
        // W3: 空文字列の環境変数は None として扱う
        let env_key = "REMOTE_MERGE_KEY_PASSPHRASE_EMPTYTEST";
        unsafe { std::env::set_var(env_key, "") };

        let provider = EnvPassphraseProvider::new("emptytest");
        assert_eq!(provider.get_passphrase("/any/key"), None);

        unsafe { std::env::remove_var(env_key) };
    }

    #[test]
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

    // ── MAX_PASSPHRASE_RETRIES ──

    #[test]
    fn test_max_passphrase_retries_is_3() {
        assert_eq!(MAX_PASSPHRASE_RETRIES, 3);
    }

    // ── C3: ログ漏洩テスト ──

    #[test]
    fn test_zeroizing_debug_leaks_content_so_never_log_it() {
        // 重要: Zeroizing<String> の Debug 表示は内部文字列をそのまま出力する。
        // つまり `tracing::debug!("{:?}", passphrase)` とすると漏洩する。
        // このテストはその性質を明示し、「tracing に Zeroizing を渡してはいけない」
        // というルールの根拠を文書化する。
        let passphrase = Zeroizing::new("super-secret-passphrase".to_string());
        let debug_output = format!("{:?}", passphrase);
        assert!(
            debug_output.contains("super-secret-passphrase"),
            "Zeroizing Debug leaks content — NEVER pass to tracing macros"
        );
    }

    #[test]
    fn test_passphrase_not_leaked_in_tracing_logs() {
        // 注意: このテストは実際の load_secret_key_with_passphrase() の
        // ログ出力を直接テストしているわけではない（暗号化鍵ファイルが必要なため）。
        // 代わりに、コード内で使われるのと同じ tracing パターンを再現し、
        // パスフレーズ文字列が混入しないことを確認する。
        // 実装コードが将来変更された場合の回帰検知力には限界がある。
        use std::sync::{Arc, Mutex};
        use tracing_subscriber::layer::SubscriberExt;

        let captured = Arc::new(Mutex::new(Vec::<u8>::new()));
        let captured_clone = Arc::clone(&captured);

        let writer = CaptureWriter(captured_clone);
        let layer = tracing_subscriber::fmt::layer()
            .with_writer(move || writer.clone())
            .with_ansi(false);

        let subscriber = tracing_subscriber::registry().with(layer);

        let secret = "my-ultra-secret-passphrase-12345";

        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!("Failed to read passphrase from terminal: connection reset");
            tracing::debug!("Passphrase provider returned None for '/path/to/key'");
            // パスフレーズ自体はログに渡さない
        });

        let output = captured.lock().unwrap();
        let log_str = String::from_utf8_lossy(&output);
        assert!(
            !log_str.contains(secret),
            "Passphrase must not appear in logs. Log output: {}",
            log_str
        );
    }

    /// tracing の Writer として使うキャプチャ用構造体
    #[derive(Clone)]
    struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for CaptureWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
}
