//! 診断ログなどの記録を置くディレクトリ。

/// 記録を置く既定のディレクトリ（ユーザーのキャッシュ配下）
pub fn default_log_dir() -> std::path::PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("remote-merge")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_log_dir_is_named_after_the_tool() {
        let dir = default_log_dir();
        assert!(dir.ends_with("remote-merge"));
    }
}
