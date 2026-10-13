//! 経路の操作の失敗の種類（docs/ir/paths/operations.md の REQ-paths-020）。
//!
//! 三つの経路は自分の結果をこの型に訳すだけにする。OS の誤りは [`from_io_error`]、リモートの道具の
//! 文言は [`from_tool_message`]、大きさの上限は [`check_size`] の一つずつの関数が決める。

mod message;
mod size_limit;

pub use message::{failure_of, op_error_of, OpError, Operation, Prefixed};
pub use size_limit::{check_size, SizeLimit, MAX_FILE_SIZE};

/// 一つのパスの結果（結果のキーにするパスと、値か失敗の種類）
pub type PathResult<T> = (String, Result<T, OpFailure>);

/// 操作の失敗の種類
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpFailure {
    /// パスの最後の要素がない
    NotFound,
    /// 途中の要素がディレクトリでない、またはディレクトリを求めた相手がディレクトリでない
    NotADirectory,
    PermissionDenied,
    /// 大きさが読み込みの上限を超える
    TooLarge {
        size: u64,
        limit: u64,
    },
    /// 操作に要る道具がリモートにない
    ToolMissing(Tool),
    /// それ以外の失敗。OS かリモートが返した理由を持つ
    Io(String),
    /// 要求を送り始めた後に結果を受け取れなかった（docs/ir/CONTEXT.md の `接続断`）
    Disconnected(Disconnection),
}

impl OpFailure {
    pub fn is_disconnection(&self) -> bool {
        matches!(self, Self::Disconnected(_))
    }
}

/// エージェントを使わないときにリモートに要る道具（docs/ir/ssh/compatibility.md の TBL-ssh-002）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    /// -printf と "-exec … {} +" を受け付ける find
    FindPrintf,
    /// ないか入れない理由を見分ける ls -d
    LsDirectory,
    /// 大きさを尋ねる stat（なければ ls -ldn）
    Stat,
    /// バイナリの中身を書くための openssl の base64 の復号
    OpensslBase64,
    /// symlink のリンク文字列を読む readlink（パスの確認の "readlink not available" と同じ道具）
    Readlink,
}

/// 接続断のうち、要求を送れなかったか、送った後に結果を受け取れなかったか
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disconnection {
    NotSent(String),
    NoAnswer(String),
}

/// OS の誤りを種類に訳す。ローカルとエージェントが同じものを使う
pub fn from_io_error(error: &std::io::Error) -> OpFailure {
    match error.kind() {
        std::io::ErrorKind::NotFound => OpFailure::NotFound,
        std::io::ErrorKind::NotADirectory => OpFailure::NotADirectory,
        std::io::ErrorKind::PermissionDenied => OpFailure::PermissionDenied,
        _ => OpFailure::Io(os_reason(error)),
    }
}

/// 誤りの連なりの種類。操作の失敗の型があればそれ、OS の誤りがあればその訳、どちらもなければ
/// 文言を持つ I/O
pub fn from_error(error: &anyhow::Error) -> OpFailure {
    for cause in error.chain() {
        if let Some(op) = cause.downcast_ref::<OpError>() {
            return op.failure.clone();
        }
        if let Some(prefixed) = cause.downcast_ref::<Prefixed>() {
            return prefixed.error.failure.clone();
        }
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            return from_io_error(io);
        }
    }
    OpFailure::Io(format!("{error:#}"))
}

/// std の表示に付く " (os error N)" を除いた OS の理由
fn os_reason(error: &std::io::Error) -> String {
    let shown = error.to_string();
    match (error.raw_os_error(), shown.rfind(" (os error ")) {
        (Some(_), Some(at)) => shown[..at].to_string(),
        _ => shown,
    }
}

/// リモートの道具が標準エラーに出した文言を、C ロケールの OS の文言の終わりで種類に訳す
/// （docs/remote-tools.md の「失敗の理由の見分け方」）。終わりが当たらなければ文言を持つ I/O
pub fn from_tool_message(stderr: &[u8]) -> OpFailure {
    let text = String::from_utf8_lossy(stderr);
    let text = text.trim_end();
    if text.ends_with("No such file or directory") {
        OpFailure::NotFound
    } else if text.ends_with("Not a directory") {
        OpFailure::NotADirectory
    } else if text.ends_with("Permission denied") || text.ends_with("Operation not permitted") {
        OpFailure::PermissionDenied
    } else if text.is_empty() {
        OpFailure::Io("the remote command failed without a message".to_string())
    } else {
        OpFailure::Io(text.to_string())
    }
}

#[cfg(all(test, unix))]
mod tests;
