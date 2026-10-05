//! root_dir からの相対パスとして受け取る値の検査。
//!
//! 引数の検査と経路ごとの境界の検査が、同じ規則と同じ文言で拒否するための唯一の置き場所。

use std::path::Path;

/// root_dir の外を指しうる値（絶対パスか ".." の要素を含むパス）を拒否する。
///
/// 絶対パスであり ".." も含む値は、絶対パスとして拒否する。
pub fn check_rel_path(path: &str) -> anyhow::Result<()> {
    if Path::new(path).is_absolute() {
        anyhow::bail!("absolute subpath not allowed: {path}");
    }
    if path.split('/').any(|component| component == "..") {
        anyhow::bail!("path traversal not allowed: {path}");
    }
    Ok(())
}
