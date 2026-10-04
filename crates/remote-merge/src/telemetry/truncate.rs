//! ログファイルのトランケーション。
//!
//! 実行の始めに呼び出して、上限を超えた古いログを破棄する。

use std::io::{self, BufRead, Write};
use std::path::Path;

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
}
