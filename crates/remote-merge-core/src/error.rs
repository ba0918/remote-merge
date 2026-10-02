use std::path::PathBuf;

/// アプリケーション固有のエラー型
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    // ── 設定エラー ──
    #[error("Config file not found.\n  Searched:\n  - {project_path}\n  - {global_path}")]
    ConfigNotFound {
        project_path: PathBuf,
        global_path: PathBuf,
    },

    #[error("Failed to parse config file: {source}")]
    ConfigParse {
        #[source]
        source: toml::de::Error,
    },

    #[error("Invalid config value: {field} - {message}")]
    ConfigValidation { field: String, message: String },

    // ── SSH errors ──
    #[error("SSH connection failed to {host}: {message}")]
    SshConnection { host: String, message: String },

    #[error("SSH authentication failed (user: {user}@{host})")]
    SshAuth { host: String, user: String },

    #[error("SSH command execution failed: {command}")]
    SshExec { command: String },

    #[error("SSH connection timed out ({timeout_sec}s): {host}")]
    SshTimeout { host: String, timeout_sec: u64 },

    #[error("Failed to load SSH private key: {path}")]
    SshKeyLoad { path: PathBuf },

    // ── Filesystem errors ──
    #[error("Path not found: {path}")]
    PathNotFound { path: PathBuf },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    // ── Remote command errors ──
    #[error("Failed to parse remote command output: {message}")]
    RemoteParse { message: String },

    #[error("Remote root_dir not found: {host}:{path}")]
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

    // ── Step 5: config error message contains both paths ──

    #[test]
    fn test_config_not_found_error_message_contains_both_paths() {
        let err = AppError::ConfigNotFound {
            project_path: PathBuf::from("/home/user/project/.remote-merge.toml"),
            global_path: PathBuf::from("/home/user/.config/remote-merge/config.toml"),
        };
        let msg = format!("{}", err);
        assert!(
            msg.contains(".remote-merge.toml"),
            "should contain project path"
        );
        assert!(msg.contains("config.toml"), "should contain global path");
        assert!(
            msg.contains("Searched:"),
            "should contain 'Searched:' label"
        );
    }
}
