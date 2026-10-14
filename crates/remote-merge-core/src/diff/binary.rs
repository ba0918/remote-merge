//! バイナリファイルの比較ロジック。
//! SHA-256ハッシュとファイルサイズによる同一性判定を行う。

use sha2::{Digest, Sha256};

/// バイナリファイルの情報（サイズ + SHA-256ハッシュ）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryInfo {
    /// ファイルサイズ（バイト）
    pub size: u64,
    /// SHA-256 ハッシュ（16進数文字列）
    pub sha256: String,
}

impl BinaryInfo {
    /// バイト列から BinaryInfo を生成する
    pub fn from_bytes(content: &[u8]) -> Self {
        Self {
            size: content.len() as u64,
            sha256: compute_sha256(content),
        }
    }

    /// SHA-256ハッシュが一致するかどうかを判定する
    pub fn is_same_content(&self, other: &Self) -> bool {
        self.sha256 == other.sha256
    }

    /// SHA-256ハッシュを短縮表示する（UI用）
    pub fn short_hash(&self) -> String {
        if self.sha256.len() > 16 {
            format!("{}...", &self.sha256[..16])
        } else {
            self.sha256.clone()
        }
    }
}

/// バイナリファイルの比較結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinaryComparison {
    /// ハッシュが一致（同一）
    Equal,
    /// ハッシュが異なる（差分あり）
    Different,
}

/// 2つの BinaryInfo を比較する
pub fn compare(left: &BinaryInfo, right: &BinaryInfo) -> BinaryComparison {
    if left.is_same_content(right) {
        BinaryComparison::Equal
    } else {
        BinaryComparison::Different
    }
}

/// SHA-256ハッシュを計算する（16進数文字列）
pub fn compute_sha256(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_known_value() {
        // "hello" の SHA-256 は既知の値
        let hash = compute_sha256(b"hello");
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_sha256_empty() {
        let hash = compute_sha256(b"");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_binary_info_from_bytes() {
        let info = BinaryInfo::from_bytes(b"hello world");
        assert_eq!(info.size, 11);
        assert!(!info.sha256.is_empty());
    }

    #[test]
    fn test_binary_comparison_equal() {
        let a = BinaryInfo::from_bytes(b"same content");
        let b = BinaryInfo::from_bytes(b"same content");
        assert_eq!(compare(&a, &b), BinaryComparison::Equal);
    }

    #[test]
    fn test_binary_comparison_different() {
        let a = BinaryInfo::from_bytes(b"content a");
        let b = BinaryInfo::from_bytes(b"content b");
        assert_eq!(compare(&a, &b), BinaryComparison::Different);
    }

    #[test]
    fn test_binary_comparison_different_size_same_would_not_happen() {
        // サイズが違えばハッシュも違う
        let a = BinaryInfo::from_bytes(b"short");
        let b = BinaryInfo::from_bytes(b"longer content here");
        assert_eq!(compare(&a, &b), BinaryComparison::Different);
    }

    // format_size テストは crate::format::tests に集約済み
}
