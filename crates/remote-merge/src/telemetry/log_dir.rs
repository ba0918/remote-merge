//! 診断ログなどの記録を置くディレクトリ。

use std::path::PathBuf;

/// 記録を置く既定のディレクトリ（ユーザーのキャッシュ配下）。
///
/// キャッシュの場所が決まらなければ `None`。共有の一時ディレクトリには逃がさない
/// （他の利用者が置いたリンクで書き込み先をすり替えられるため）。
pub fn default_log_dir() -> Option<PathBuf> {
    log_dir_in(dirs::cache_dir())
}

fn log_dir_in(cache: Option<PathBuf>) -> Option<PathBuf> {
    cache.map(|cache| cache.join("remote-merge"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn the_location_is_named_after_the_tool_under_the_cache() {
        let dir = log_dir_in(Some(PathBuf::from("/home/someone/.cache")));
        assert_eq!(
            dir.as_deref(),
            Some(Path::new("/home/someone/.cache/remote-merge"))
        );
    }

    #[test]
    fn there_is_no_location_without_a_cache() {
        assert_eq!(log_dir_in(None), None);
    }
}
