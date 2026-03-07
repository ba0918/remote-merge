//! ログ/イベントファイルのトランケーション。
//!
//! 起動時に呼び出して、古いログ/イベントを破棄する。
//! - `debug.log`: 10MB 上限
//! - `events.jsonl`: 10,000 行上限

use std::io::{self, BufRead, Write};
use std::path::Path;

/// ファイルの行数上限でトランケートする。
///
/// 上限を超えている場合、末尾 `max_lines` 行のみ残す。
/// ファイルが存在しない場合は何もしない。
pub fn truncate_file_lines(path: &Path, max_lines: usize) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().collect();

    if lines.len() <= max_lines {
        return Ok(());
    }

    // 末尾 max_lines 行だけ残す
    let keep = &lines[lines.len() - max_lines..];
    let truncated = keep.join("\n");

    let mut file = std::fs::File::create(path)?;
    file.write_all(truncated.as_bytes())?;
    if !truncated.is_empty() {
        file.write_all(b"\n")?;
    }

    Ok(())
}

/// ファイルのバイトサイズ上限でトランケートする。
///
/// 上限を超えている場合、末尾の `max_bytes` 相当の行のみ残す。
/// 行の途中で切ることはしない。
pub fn truncate_file_bytes(path: &Path, max_bytes: u64) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let metadata = std::fs::metadata(path)?;
    if metadata.len() <= max_bytes {
        return Ok(());
    }

    // 行単位で末尾から残す
    let file = std::fs::File::open(path)?;
    let reader = io::BufReader::new(file);
    let all_lines: Vec<String> = reader.lines().collect::<io::Result<Vec<_>>>()?;

    let mut kept_lines = Vec::new();
    let mut total_bytes: u64 = 0;

    for line in all_lines.iter().rev() {
        let line_bytes = (line.len() + 1) as u64; // +1 for newline
        if total_bytes + line_bytes > max_bytes {
            break;
        }
        total_bytes += line_bytes;
        kept_lines.push(line.as_str());
    }

    kept_lines.reverse();
    let truncated = kept_lines.join("\n");

    let mut file = std::fs::File::create(path)?;
    file.write_all(truncated.as_bytes())?;
    if !truncated.is_empty() {
        file.write_all(b"\n")?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_file_lines_under_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("small.log");
        std::fs::write(&path, "line1\nline2\nline3\n").unwrap();

        truncate_file_lines(&path, 10).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "line1\nline2\nline3\n");
    }

    #[test]
    fn test_truncate_file_lines_over_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.log");
        let lines: Vec<String> = (0..100).map(|i| format!("line{}", i)).collect();
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();

        truncate_file_lines(&path, 10).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let result_lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(result_lines.len(), 10);
        assert_eq!(result_lines[0], "line90");
        assert_eq!(result_lines[9], "line99");
    }

    #[test]
    fn test_truncate_file_lines_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.log");

        let result = truncate_file_lines(&path, 10);
        assert!(result.is_ok());
    }

    #[test]
    fn test_truncate_file_lines_exact_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("exact.log");
        std::fs::write(&path, "a\nb\nc").unwrap();

        truncate_file_lines(&path, 3).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("a"));
        assert!(content.contains("b"));
        assert!(content.contains("c"));
    }

    #[test]
    fn test_truncate_file_bytes_under_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("small.log");
        std::fs::write(&path, "short content\n").unwrap();

        truncate_file_bytes(&path, 1024).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "short content\n");
    }

    #[test]
    fn test_truncate_file_bytes_over_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.log");

        // 各行20バイト × 100行 = 2000バイト
        let lines: Vec<String> = (0..100).map(|i| format!("line {:>15}", i)).collect();
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();

        // 200バイト上限 → 末尾の数行だけ残る
        truncate_file_bytes(&path, 200).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let result_lines: Vec<&str> = content.trim().lines().collect();
        // 各行は "line              N" + "\n" ≈ 21バイト → 200/21 ≈ 9行程度
        assert!(result_lines.len() < 15);
        assert!(result_lines.len() > 5);
        // 最後の行が含まれていること
        assert!(result_lines.last().unwrap().contains("99"));
    }

    #[test]
    fn test_truncate_file_bytes_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.log");

        let result = truncate_file_bytes(&path, 1024);
        assert!(result.is_ok());
    }

    #[test]
    fn test_truncate_file_lines_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.log");
        std::fs::write(&path, "").unwrap();

        truncate_file_lines(&path, 10).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "");
    }
}
