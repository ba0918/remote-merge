//! 中身を読むときの大きさの上限（docs/ir/cli/size-limit.md の REQ-cli-079）。上限を超えるかは
//! core の `op_failure::check_size` が経路の答えた大きさから決める

pub use crate::merge::executor::MAX_BINARY_FILE_SIZE;

/// コマンドが読み込みに当てる上限
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeLimit {
    /// 上限を超えるファイルを読まない
    Enforced,
    /// --force の指定で、上限を当てずに読む
    Lifted,
}

impl SizeLimit {
    pub fn from_force(force: bool) -> Self {
        if force {
            Self::Lifted
        } else {
            Self::Enforced
        }
    }

    /// 経路に渡す上限
    pub fn bytes(self) -> remote_merge_core::op_failure::SizeLimit {
        remote_merge_core::op_failure::SizeLimit::from_force(self == Self::Lifted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use remote_merge_core::op_failure::{SizeLimit as Bytes, MAX_FILE_SIZE};

    #[test]
    fn force_lifts_the_limit() {
        assert_eq!(SizeLimit::from_force(true).bytes(), Bytes::Unlimited);
        assert_eq!(
            SizeLimit::from_force(false).bytes(),
            Bytes::Bytes(MAX_FILE_SIZE)
        );
        assert_eq!(MAX_FILE_SIZE, MAX_BINARY_FILE_SIZE as u64);
    }
}
