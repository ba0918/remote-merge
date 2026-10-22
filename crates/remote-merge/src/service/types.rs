pub use remote_merge_engine::service::types::*;

/// merge がファイルの結果を作らずに終わった理由（ファイルの結果は cli の `MergeCommandOutput::Files`）
#[derive(Debug, Clone)]
pub enum MergeOutcome {
    /// 指定パスにマージ対象ファイルがない
    NoFilesToMerge,
    /// remote-to-remote マージがブロックされた（--force で解除可能）
    R2rBlocked { left: String, right: String },
}
