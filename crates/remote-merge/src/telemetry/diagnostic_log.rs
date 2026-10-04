//! CLI の実行が書き足す診断ログ（debug.log）のファイルを用意する。

use std::fs::File;
use std::path::Path;

use super::truncate::truncate_file_bytes;

/// 診断ログのファイル名
pub const DIAGNOSTIC_LOG_FILE: &str = "debug.log";

/// 診断ログの大きさの上限（実行の始めに超えていれば古い行を捨てる）
pub const MAX_DIAGNOSTIC_LOG_BYTES: u64 = 10 * 1024 * 1024;

/// `dir` の診断ログを追記用に開く。
///
/// 上限を超えていれば先に古い行を切り詰める。置き場を作れないか開けないときは
/// 保存をあきらめて `None` を返す（診断ログのために本来の操作を止めない）。
/// 同時に動く実行どうしの排他はしない。
pub fn open_diagnostic_log(dir: &Path, max_bytes: u64) -> Option<File> {
    std::fs::create_dir_all(dir).ok()?;
    let path = dir.join(DIAGNOSTIC_LOG_FILE);
    let _ = truncate_file_bytes(&path, max_bytes);
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn creates_the_location_and_appends_to_the_log() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path().join("nested").join("remote-merge");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(DIAGNOSTIC_LOG_FILE), "old\n").unwrap();
        std::fs::remove_dir_all(tmp.path().join("nested")).unwrap();

        let mut file = open_diagnostic_log(&dir, 1024).expect("log must open");
        writeln!(file, "first").unwrap();
        drop(file);
        let mut file = open_diagnostic_log(&dir, 1024).expect("log must open");
        writeln!(file, "second").unwrap();

        let saved = std::fs::read_to_string(dir.join(DIAGNOSTIC_LOG_FILE)).unwrap();
        assert_eq!(saved, "first\nsecond\n");
    }

    #[test]
    fn drops_the_oldest_lines_when_the_log_is_over_the_limit() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join(DIAGNOSTIC_LOG_FILE);
        std::fs::write(&path, "aaaa\nbbbb\ncccc\n").unwrap();

        let file = open_diagnostic_log(tmp.path(), 10).expect("log must open");
        drop(file);

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "bbbb\ncccc\n");
    }

    #[test]
    fn gives_up_when_the_location_cannot_be_created() {
        let tmp = tempfile::TempDir::new().unwrap();
        let blocker = tmp.path().join("remote-merge");
        std::fs::write(&blocker, "not a directory").unwrap();

        assert!(open_diagnostic_log(&blocker, 1024).is_none());
    }
}
