use std::path::PathBuf;

/// アプリケーション固有のエラー型
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    // ── 設定エラー ──
    #[error("設定ファイルが見つかりません: {path}")]
    ConfigNotFound { path: PathBuf },

    #[error("設定ファイルのパースに失敗しました: {source}")]
    ConfigParse {
        #[source]
        source: toml::de::Error,
    },

    #[error("設定値が不正です: {field} - {message}")]
    ConfigValidation { field: String, message: String },

    // ── SSH エラー ──
    #[error("{host} への SSH 接続に失敗しました: {message}")]
    SshConnection { host: String, message: String },

    #[error("SSH 認証に失敗しました (ユーザ: {user}@{host})")]
    SshAuth { host: String, user: String },

    #[error("SSH コマンドの実行に失敗しました: {command}")]
    SshExec { command: String },

    #[error("SSH 接続がタイムアウトしました ({timeout_sec}秒): {host}")]
    SshTimeout { host: String, timeout_sec: u64 },

    #[error("SSH 秘密鍵の読み込みに失敗しました: {path}")]
    SshKeyLoad { path: PathBuf },

    // ── ファイルシステムエラー ──
    #[error("パスが見つかりません: {path}")]
    PathNotFound { path: PathBuf },

    #[error("IO エラー: {0}")]
    Io(#[from] std::io::Error),

    // ── リモートコマンドエラー ──
    #[error("リモートコマンドの出力パースに失敗しました: {message}")]
    RemoteParse { message: String },

    #[error("リモートの root_dir が見つかりません: {host}:{path}")]
    RemoteRootNotFound { host: String, path: String },
}

/// アプリ全体で使う Result 型エイリアス
pub type Result<T> = std::result::Result<T, anyhow::Error>;

/// SSH 接続自体が断絶したエラーかどうかを判定する。
///
/// `SshConnection` / `SshTimeout` のみ `true`。
/// `SshExec`（コマンド実行失敗＝ファイル不在等）は `false`。
/// 呼び出し元で `is_connected = false` にすべきかの判定に使う。
pub fn is_connection_error(e: &anyhow::Error) -> bool {
    matches!(
        e.downcast_ref::<AppError>(),
        Some(AppError::SshConnection { .. }) | Some(AppError::SshTimeout { .. })
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssh_connection_error_is_connection() {
        let err: anyhow::Error = AppError::SshConnection {
            host: "example.com".to_string(),
            message: "reset by peer".to_string(),
        }
        .into();
        assert!(is_connection_error(&err));
    }

    #[test]
    fn test_ssh_timeout_is_connection() {
        let err: anyhow::Error = AppError::SshTimeout {
            host: "example.com".to_string(),
            timeout_sec: 30,
        }
        .into();
        assert!(is_connection_error(&err));
    }

    #[test]
    fn test_ssh_exec_is_not_connection() {
        let err: anyhow::Error = AppError::SshExec {
            command: "cat /nonexistent".to_string(),
        }
        .into();
        assert!(!is_connection_error(&err));
    }

    #[test]
    fn test_ssh_auth_is_not_connection() {
        let err: anyhow::Error = AppError::SshAuth {
            host: "example.com".to_string(),
            user: "root".to_string(),
        }
        .into();
        assert!(!is_connection_error(&err));
    }

    #[test]
    fn test_non_app_error_is_not_connection() {
        let err = anyhow::anyhow!("some random error");
        assert!(!is_connection_error(&err));
    }
}
