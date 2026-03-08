//! ref サーバの重複検出。
//!
//! --ref で指定されたサーバが left/right と同一の場合に警告して None を返す。

use crate::app::side::Side;
use crate::service::source_pair::SourcePair;

/// If ref_side duplicates left or right, return None and print warning to stderr.
/// Otherwise return Some(ref_side).
pub fn validate_ref_side(ref_side: Option<Side>, pair: &SourcePair) -> Option<Side> {
    let ref_side = ref_side?;
    // eprintln is used intentionally (not tracing::warn) so the warning
    // is always visible to CLI users regardless of log level settings.
    if ref_side == pair.left {
        eprintln!("Warning: --ref server is the same as left side; ref comparison skipped.");
        None
    } else if ref_side == pair.right {
        eprintln!("Warning: --ref server is the same as right side; ref comparison skipped.");
        None
    } else {
        Some(ref_side)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pair(left: &str, right: &str) -> SourcePair {
        SourcePair {
            left: if left == "local" {
                Side::Local
            } else {
                Side::Remote(left.to_string())
            },
            right: if right == "local" {
                Side::Local
            } else {
                Side::Remote(right.to_string())
            },
        }
    }

    #[test]
    fn ref_same_as_left_returns_none() {
        let pair = make_pair("local", "staging");
        assert!(validate_ref_side(Some(Side::Local), &pair).is_none());
    }

    #[test]
    fn ref_same_as_right_returns_none() {
        let pair = make_pair("local", "staging");
        assert!(validate_ref_side(Some(Side::Remote("staging".to_string())), &pair).is_none());
    }

    #[test]
    fn ref_different_returns_some() {
        let pair = make_pair("local", "staging");
        let result = validate_ref_side(Some(Side::Remote("develop".to_string())), &pair);
        assert_eq!(result, Some(Side::Remote("develop".to_string())));
    }

    #[test]
    fn ref_none_returns_none() {
        let pair = make_pair("local", "staging");
        assert!(validate_ref_side(None, &pair).is_none());
    }
}
