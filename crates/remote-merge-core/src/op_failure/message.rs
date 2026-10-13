//! 操作の失敗を利用者に示す文言（docs/ir/paths/operations.md の REQ-paths-016）。文言はここだけで作る

use std::fmt;

use super::{Disconnection, OpFailure, Tool};

/// 失敗した操作
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Read,
    Write,
    Remove,
    Chmod,
    CreateSymlink,
    Stat,
    Size,
    Hash,
    ReadEntryLink,
    ListChildren,
    /// root_dir の下の走査（パスは root_dir）
    Scan,
}

impl Operation {
    fn verb(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Remove => "remove",
            Self::Chmod => "change permissions of",
            Self::CreateSymlink => "create symlink",
            Self::Stat => "stat",
            Self::Size => "get the size of",
            Self::Hash => "hash",
            Self::ReadEntryLink => "read the link of",
            Self::ListChildren => "list the children of",
            Self::Scan => "scan",
        }
    }
}

/// 一つのパスへの操作の失敗
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpError {
    pub operation: Operation,
    pub path: String,
    pub failure: OpFailure,
}

impl OpError {
    pub fn new(operation: Operation, path: impl Into<String>, failure: OpFailure) -> Self {
        Self {
            operation,
            path: path.into(),
            failure,
        }
    }
}

impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cannot {} {}: {}",
            self.operation.verb(),
            self.path,
            self.failure
        )
    }
}

impl std::error::Error for OpError {}

impl fmt::Display for OpFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str("no such file or directory"),
            Self::NotADirectory => f.write_str("not a directory"),
            Self::PermissionDenied => f.write_str("permission denied"),
            Self::TooLarge { size, limit } => write!(
                f,
                "file too large ({size} bytes, the limit is {limit} bytes; use --force to read it)"
            ),
            Self::ToolMissing(tool) => write!(f, "{tool}"),
            Self::Io(reason) => f.write_str(reason),
            Self::Disconnected(Disconnection::NotSent(detail)) => {
                write!(f, "connection lost before the request was sent: {detail}")
            }
            Self::Disconnected(Disconnection::NoAnswer(detail)) => {
                write!(f, "connection lost before the result arrived: {detail}")
            }
        }
    }
}

impl fmt::Display for Tool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FindPrintf => f.write_str(
                "the remote find does not accept -printf and -exec ... {} +; \
                 a find that accepts both (GNU findutils 4.2.27 or later) is required, \
                 or enable the agent ([agent] enabled = true) to handle this server",
            ),
            Self::LsDirectory => f.write_str("ls is not available on the remote"),
            Self::Stat => f.write_str("neither stat -c nor ls -ldn works on the remote"),
            Self::Readlink => f.write_str("readlink not available on the remote"),
            Self::OpensslBase64 => {
                f.write_str("openssl is not available on the remote to decode the content")
            }
        }
    }
}

/// anyhow の誤りの連なりの中の操作の失敗の種類
pub fn failure_of(error: &anyhow::Error) -> Option<&OpFailure> {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<OpError>())
        .map(|error| &error.failure)
}
