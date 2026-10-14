//! 読み込みの大きさの上限（docs/ir/cli/size-limit.md の REQ-cli-079）

use super::OpFailure;

/// 中身を読むファイルの大きさの上限（バイト）
pub const MAX_FILE_SIZE: u64 = 104_857_600;

/// 読み込みに当てる上限
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeLimit {
    Bytes(u64),
    /// --force を指定したとき
    Unlimited,
}

impl SizeLimit {
    pub fn from_force(force: bool) -> Self {
        if force {
            Self::Unlimited
        } else {
            Self::Bytes(MAX_FILE_SIZE)
        }
    }
}

/// 経路が答えた大きさと上限から、上限を超えるかを決める
pub fn check_size(size: u64, limit: SizeLimit) -> Result<(), OpFailure> {
    match limit {
        SizeLimit::Bytes(limit) if size > limit => Err(OpFailure::TooLarge { size, limit }),
        SizeLimit::Bytes(_) | SizeLimit::Unlimited => Ok(()),
    }
}
