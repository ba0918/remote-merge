//! 経路の操作の失敗の種類（docs/ir/paths/operations.md の REQ-paths-020）。
//!
//! ローカルとエージェントの経路は自分の結果をこの型に訳すだけにする。OS の誤りは [`from_io_error`]、
//! 大きさの上限は [`check_size`] の一つずつの関数が決める。

mod message;
mod size_limit;

pub use message::{OpError, Operation, Prefixed, failure_of, op_error_of};
pub use size_limit::{MAX_FILE_SIZE, SizeLimit, check_size};

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
    /// それ以外の失敗。OS かリモートが返した理由を持つ
    Io(String),
    /// 要求を送り始めた後に結果を受け取れなかった（docs/ir/CONTEXT.md の `接続断`）
    Disconnected(Disconnection),
    /// 変更の途中で root_dir がなくなっていた（ディレクトリでなくなったものを含む）。走査の
    /// RootChanged と同じく、root_dir の誤りとして示す（docs/decision/records/2026-10-12-foundation-3.md
    /// の D47）
    RootGone,
}

impl OpFailure {
    pub fn is_disconnection(&self) -> bool {
        matches!(self, Self::Disconnected(_))
    }
}

/// 接続断のうち、要求を送れなかったか、送った後に結果を受け取れなかったか
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disconnection {
    NotSent(String),
    NoAnswer(String),
}

/// OS の誤りを種類に訳す。ローカルとエージェントが同じものを使う
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "std::io::ErrorKind は外部の enum で、他の種類は I/O の失敗として運ぶ"
)]
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
pub fn os_reason(error: &std::io::Error) -> String {
    let shown = error.to_string();
    match (error.raw_os_error(), shown.rfind(" (os error ")) {
        (Some(_), Some(at)) => shown[..at].to_string(),
        _ => shown,
    }
}

#[cfg(all(test, unix))]
mod tests;
