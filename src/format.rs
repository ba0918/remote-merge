//! フォーマットユーティリティ関数。
//! レイヤー横断で使われる純粋な文字列変換を提供する。

/// バイト数を人間が読みやすい形式に変換する（スペースなし）
pub fn format_size(bytes: u64) -> String {
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

/// Option<u64> 版。None の場合は "-" を返す
pub fn format_size_or_dash(size: Option<u64>) -> String {
    match size {
        Some(bytes) => format_size(bytes),
        None => "-".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size_zero() {
        assert_eq!(format_size(0), "0B");
    }

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(1), "1B");
        assert_eq!(format_size(512), "512B");
        assert_eq!(format_size(1023), "1023B");
    }

    #[test]
    fn test_format_size_kb() {
        assert_eq!(format_size(1024), "1.0KB");
        assert_eq!(format_size(1536), "1.5KB");
        assert_eq!(format_size(10240), "10.0KB");
    }

    #[test]
    fn test_format_size_mb_boundary() {
        assert_eq!(format_size(1048575), "1024.0KB"); // MB 境界直前
        assert_eq!(format_size(1048576), "1.0MB");
    }

    #[test]
    fn test_format_size_mb() {
        assert_eq!(format_size(5 * 1048576), "5.0MB");
    }

    #[test]
    fn test_format_size_gb_boundary() {
        assert_eq!(format_size(1073741823), "1024.0MB"); // GB 境界直前
        assert_eq!(format_size(1073741824), "1.0GB");
    }

    #[test]
    fn test_format_size_tb_displays_as_gb() {
        assert_eq!(format_size(1099511627776), "1024.0GB"); // 1TB — TB 単位未対応のため GB 表示
    }

    #[test]
    fn test_format_size_u64_max_no_panic() {
        let _ = format_size(u64::MAX); // パニックしないこと
    }

    #[test]
    fn test_format_size_or_dash_none() {
        assert_eq!(format_size_or_dash(None), "-");
    }

    #[test]
    fn test_format_size_or_dash_some() {
        assert_eq!(format_size_or_dash(Some(1024)), "1.0KB");
        assert_eq!(format_size_or_dash(Some(0)), "0B");
    }
}
