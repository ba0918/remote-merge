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

/// 削除・権限の変更・子の一覧に渡すパス。末尾の "/" を除く前の値を [`check_rel_path`] で検査し、
/// 除いた値を返す。"/" は絶対パスとして拒否し、root_dir 自身は空のパスで表す
pub fn operation_rel_path(path: &str) -> anyhow::Result<&str> {
    check_rel_path(path)?;
    Ok(path.trim_end_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_operation_path_is_judged_before_its_trailing_slash_is_removed() {
        assert_eq!(operation_rel_path("dir/").unwrap(), "dir");
        assert_eq!(operation_rel_path("").unwrap(), "");
        assert_eq!(
            operation_rel_path("/").unwrap_err().to_string(),
            "absolute subpath not allowed: /"
        );
        assert_eq!(
            operation_rel_path("/a/").unwrap_err().to_string(),
            "absolute subpath not allowed: /a/"
        );
        assert_eq!(
            operation_rel_path("a/../").unwrap_err().to_string(),
            "path traversal not allowed: a/../"
        );
        assert_eq!(
            operation_rel_path("../").unwrap_err().to_string(),
            "path traversal not allowed: ../"
        );
    }
}
