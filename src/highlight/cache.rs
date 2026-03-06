//! ハイライト結果のキャッシュ。
//! ファイルパスをキーとして HighlightedFile を保持する。

use std::collections::HashMap;

use crate::highlight::engine::HighlightedFile;

/// ハイライト結果のキャッシュ。
#[derive(Debug, Default)]
pub struct HighlightCache {
    /// ファイルパス -> ハイライト結果
    entries: HashMap<String, HighlightedFile>,
}

impl HighlightCache {
    /// 新しい空のキャッシュを作成する。
    pub fn new() -> Self {
        Self::default()
    }

    /// キャッシュにエントリを追加する。
    pub fn insert(&mut self, path: String, highlighted: HighlightedFile) {
        self.entries.insert(path, highlighted);
    }

    /// キャッシュからエントリを取得する。
    pub fn get(&self, path: &str) -> Option<&HighlightedFile> {
        self.entries.get(path)
    }

    /// 指定パスのキャッシュを削除する。
    pub fn remove(&mut self, path: &str) {
        self.entries.remove(path);
    }

    /// キャッシュを全クリアする。
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// キャッシュのエントリ数を返す。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// キャッシュが空かどうか。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::highlight::engine::StyledSegment;
    use ratatui::style::Modifier;

    fn sample_highlighted() -> HighlightedFile {
        vec![vec![StyledSegment {
            text: "hello".to_string(),
            fg: None,
            modifier: Modifier::empty(),
        }]]
    }

    #[test]
    fn test_cache_insert_and_get() {
        let mut cache = HighlightCache::new();
        cache.insert("test.rs".to_string(), sample_highlighted());
        assert!(cache.get("test.rs").is_some());
        assert!(cache.get("other.rs").is_none());
    }

    #[test]
    fn test_cache_remove() {
        let mut cache = HighlightCache::new();
        cache.insert("test.rs".to_string(), sample_highlighted());
        cache.remove("test.rs");
        assert!(cache.get("test.rs").is_none());
    }

    #[test]
    fn test_cache_clear() {
        let mut cache = HighlightCache::new();
        cache.insert("a.rs".to_string(), sample_highlighted());
        cache.insert("b.rs".to_string(), sample_highlighted());
        assert_eq!(cache.len(), 2);
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_cache_len() {
        let mut cache = HighlightCache::new();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        cache.insert("test.rs".to_string(), sample_highlighted());
        assert_eq!(cache.len(), 1);
        assert!(!cache.is_empty());
    }

    #[test]
    fn test_cache_overwrite() {
        let mut cache = HighlightCache::new();
        cache.insert("test.rs".to_string(), sample_highlighted());
        let new_data = vec![vec![StyledSegment {
            text: "world".to_string(),
            fg: None,
            modifier: Modifier::empty(),
        }]];
        cache.insert("test.rs".to_string(), new_data);
        let entry = cache.get("test.rs").unwrap();
        assert_eq!(entry[0][0].text, "world");
    }
}
