//! エージェントが運ぶ操作の失敗の種類（docs/ir/paths/operations.md の REQ-paths-020）。
//!
//! エージェントはリモートのマシンの上のローカルなので、OS の誤りを訳した種類（ない・ディレクトリで
//! ない・権限がない・上限を超える・I/O）だけを答える。道具がないと接続断は手元で決まる種類で、
//! 通信には載らない。

use remote_merge_core::op_failure::OpFailure;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WireFailure {
    NotFound,
    NotADirectory,
    PermissionDenied,
    TooLarge {
        size: u64,
        limit: u64,
    },
    Io {
        reason: String,
    },
    /// 変更の途中で root_dir がなくなっていた（v13）
    RootGone,
}

impl From<&OpFailure> for WireFailure {
    fn from(failure: &OpFailure) -> Self {
        match failure {
            OpFailure::NotFound => Self::NotFound,
            OpFailure::NotADirectory => Self::NotADirectory,
            OpFailure::PermissionDenied => Self::PermissionDenied,
            OpFailure::TooLarge { size, limit } => Self::TooLarge {
                size: *size,
                limit: *limit,
            },
            OpFailure::Io(reason) => Self::Io {
                reason: reason.clone(),
            },
            OpFailure::RootGone => Self::RootGone,
            OpFailure::ToolMissing(_) | OpFailure::Disconnected(_) => Self::Io {
                reason: failure.to_string(),
            },
        }
    }
}

impl From<WireFailure> for OpFailure {
    fn from(failure: WireFailure) -> Self {
        match failure {
            WireFailure::NotFound => Self::NotFound,
            WireFailure::NotADirectory => Self::NotADirectory,
            WireFailure::PermissionDenied => Self::PermissionDenied,
            WireFailure::TooLarge { size, limit } => Self::TooLarge { size, limit },
            WireFailure::Io { reason } => Self::Io(reason),
            WireFailure::RootGone => Self::RootGone,
        }
    }
}
