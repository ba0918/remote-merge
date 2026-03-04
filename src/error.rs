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
