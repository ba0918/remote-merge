//! ファイルメタデータのフォーマット関数。
//!
//! 純粋関数のみで構成。UI表示用の文字列変換を提供する。

use chrono::{DateTime, Local, Utc};

/// mtime を "2024-01-15 14:00:23" 形式にフォーマットする。
/// ローカルタイムゾーンで表示。
pub fn format_mtime(mtime: Option<DateTime<Utc>>) -> String {
    match mtime {
        Some(dt) => {
            let local: DateTime<Local> = dt.into();
            local.format("%Y-%m-%d %H:%M:%S").to_string()
        }
        None => "-".to_string(),
    }
}

/// Unix パーミッション (例: 0o755) を "rwxr-xr-x" 形式にフォーマットする。
pub fn format_permissions(permissions: Option<u32>) -> String {
    match permissions {
        Some(mode) => {
            let mut s = String::with_capacity(9);
            let flags = [
                (0o400, 'r'),
                (0o200, 'w'),
                (0o100, 'x'),
                (0o040, 'r'),
                (0o020, 'w'),
                (0o010, 'x'),
                (0o004, 'r'),
                (0o002, 'w'),
                (0o001, 'x'),
            ];
            for (bit, ch) in flags {
                s.push(if mode & bit != 0 { ch } else { '-' });
            }
            s
        }
        None => "-".to_string(),
    }
}

/// ファイルサイズを人間が読みやすい形式にフォーマットする。
/// 例: 0 → "0B", 1023 → "1023B", 1024 → "1.0KB", 1536 → "1.5KB"
pub fn format_size(size: Option<u64>) -> String {
    match size {
        Some(bytes) => {
            const KB: u64 = 1024;
            const MB: u64 = 1024 * KB;
            const GB: u64 = 1024 * MB;

            if bytes >= GB {
                format!("{:.1}GB", bytes as f64 / GB as f64)
            } else if bytes >= MB {
                format!("{:.1}MB", bytes as f64 / MB as f64)
            } else if bytes >= KB {
                format!("{:.1}KB", bytes as f64 / KB as f64)
            } else {
                format!("{}B", bytes)
            }
        }
        None => "-".to_string(),
    }
}

/// FileNode のメタデータを1行にフォーマットする。
/// 例: "2024-01-15 14:00:23  rwxr-xr-x  1.2KB"
pub fn format_metadata_line(
    mtime: Option<DateTime<Utc>>,
    permissions: Option<u32>,
    size: Option<u64>,
) -> String {
    format!(
        "{}  {}  {}",
        format_mtime(mtime),
        format_permissions(permissions),
        format_size(size),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_format_mtime_none() {
        assert_eq!(format_mtime(None), "-");
    }

    #[test]
    fn test_format_mtime_some() {
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 5, 0, 23).unwrap();
        let result = format_mtime(Some(dt));
        // ローカルタイムゾーン依存なので、フォーマットのパターンだけ確認
        assert!(result.contains("2024"));
        assert!(result.contains(":"));
    }

    #[test]
    fn test_format_permissions_standard() {
        assert_eq!(format_permissions(Some(0o755)), "rwxr-xr-x");
        assert_eq!(format_permissions(Some(0o644)), "rw-r--r--");
        assert_eq!(format_permissions(Some(0o777)), "rwxrwxrwx");
        assert_eq!(format_permissions(Some(0o000)), "---------");
        assert_eq!(format_permissions(Some(0o600)), "rw-------");
    }

    #[test]
    fn test_format_permissions_none() {
        assert_eq!(format_permissions(None), "-");
    }

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(Some(0)), "0B");
        assert_eq!(format_size(Some(1)), "1B");
        assert_eq!(format_size(Some(1023)), "1023B");
    }

    #[test]
    fn test_format_size_kb() {
        assert_eq!(format_size(Some(1024)), "1.0KB");
        assert_eq!(format_size(Some(1536)), "1.5KB");
    }

    #[test]
    fn test_format_size_mb() {
        assert_eq!(format_size(Some(1024 * 1024)), "1.0MB");
        assert_eq!(format_size(Some(1024 * 1024 * 5)), "5.0MB");
    }

    #[test]
    fn test_format_size_gb() {
        assert_eq!(format_size(Some(1024 * 1024 * 1024)), "1.0GB");
    }

    #[test]
    fn test_format_size_none() {
        assert_eq!(format_size(None), "-");
    }

    #[test]
    fn test_format_metadata_line() {
        let result = format_metadata_line(None, Some(0o644), Some(1024));
        assert_eq!(result, "-  rw-r--r--  1.0KB");
    }
}
