//! サイズ制限付きキャッシュ。
//! 挿入順を記録し、上限に達したら古いエントリを削除する。

use std::collections::{HashMap, VecDeque};

/// テキストコンテンツキャッシュの最大エントリ数
pub const MAX_TEXT_CACHE_ENTRIES: usize = 500;

/// バイナリ情報キャッシュの最大エントリ数
pub const MAX_BINARY_CACHE_ENTRIES: usize = 1000;

/// サイズ制限付きキャッシュ。
/// 挿入順を `Vec` で記録し、上限到達時に最古のエントリを除去する。
/// 厳密な LRU ではないが、無制限の肥大化を防ぐには十分。
#[derive(Debug, Clone)]
pub struct BoundedCache<V> {
    map: HashMap<String, V>,
    insertion_order: VecDeque<String>,
    max_entries: usize,
}

impl<V> BoundedCache<V> {
    /// 指定の上限でキャッシュを作成する。
    pub fn new(max_entries: usize) -> Self {
        Self {
            map: HashMap::new(),
            insertion_order: VecDeque::new(),
            max_entries,
        }
    }

    /// キーに対応する値への参照を返す。
    pub fn get(&self, key: &str) -> Option<&V> {
        self.map.get(key)
    }

    /// エントリを挿入する。上限に達している場合は最古のエントリを除去する。
    #[allow(clippy::map_entry)]
    pub fn insert(&mut self, key: String, value: V) {
        if self.map.contains_key(&key) {
            // 既存キーの上書き: insertion_order はそのまま
            self.map.insert(key, value);
            return;
        }
        self.evict_if_full();
        self.insertion_order.push_back(key.clone());
        self.map.insert(key, value);
    }

    /// 指定キーのエントリを削除する。
    pub fn remove(&mut self, key: &str) {
        if self.map.remove(key).is_some() {
            self.insertion_order.retain(|k| k != key);
        }
    }

    /// 指定キーが存在するか。
    pub fn contains_key(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    /// 全エントリを削除する。
    pub fn clear(&mut self) {
        self.map.clear();
        self.insertion_order.clear();
    }

    /// キャッシュが空かどうか。
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// エントリ数を返す。
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// 上限に達している場合、最古のエントリを 1 つ除去する。
    fn evict_if_full(&mut self) {
        if self.map.len() >= self.max_entries {
            // insertion_order の先頭から、まだ map に存在するキーを探して除去
            while let Some(key) = self.insertion_order.pop_front() {
                if self.map.remove(&key).is_some() {
                    break;
                }
            }
        }
    }
}

impl<V> Default for BoundedCache<V> {
    fn default() -> Self {
        Self::new(MAX_TEXT_CACHE_ENTRIES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_insert_and_get() {
        let mut cache = BoundedCache::new(10);
        cache.insert("a".to_string(), 1);
        cache.insert("b".to_string(), 2);
        assert_eq!(cache.get("a"), Some(&1));
        assert_eq!(cache.get("b"), Some(&2));
        assert_eq!(cache.get("c"), None);
    }

    #[test]
    fn test_overwrite_existing_key() {
        let mut cache = BoundedCache::new(10);
        cache.insert("a".to_string(), 1);
        cache.insert("a".to_string(), 99);
        assert_eq!(cache.get("a"), Some(&99));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_remove() {
        let mut cache = BoundedCache::new(10);
        cache.insert("a".to_string(), 1);
        cache.remove("a");
        assert!(cache.get("a").is_none());
        assert!(cache.is_empty());
    }

    #[test]
    fn test_remove_nonexistent() {
        let mut cache: BoundedCache<i32> = BoundedCache::new(10);
        cache.remove("x"); // should not panic
        assert!(cache.is_empty());
    }

    #[test]
    fn test_clear() {
        let mut cache = BoundedCache::new(10);
        cache.insert("a".to_string(), 1);
        cache.insert("b".to_string(), 2);
        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_contains_key() {
        let mut cache = BoundedCache::new(10);
        cache.insert("a".to_string(), 1);
        assert!(cache.contains_key("a"));
        assert!(!cache.contains_key("b"));
    }

    #[test]
    fn test_eviction_at_capacity() {
        let mut cache = BoundedCache::new(3);
        cache.insert("a".to_string(), 1);
        cache.insert("b".to_string(), 2);
        cache.insert("c".to_string(), 3);
        // At capacity, inserting a new key should evict the oldest ("a")
        cache.insert("d".to_string(), 4);
        assert!(cache.get("a").is_none(), "oldest entry should be evicted");
        assert_eq!(cache.get("b"), Some(&2));
        assert_eq!(cache.get("d"), Some(&4));
        assert_eq!(cache.len(), 3);
    }

    #[test]
    fn test_eviction_preserves_overwritten_keys() {
        let mut cache = BoundedCache::new(3);
        cache.insert("a".to_string(), 1);
        cache.insert("b".to_string(), 2);
        cache.insert("c".to_string(), 3);
        // Overwrite "a" — should not add duplicate to insertion_order
        cache.insert("a".to_string(), 10);
        // Now insert "d" — oldest in insertion_order is "a"
        cache.insert("d".to_string(), 4);
        // "a" was the oldest tracked entry, so it should be evicted
        assert!(cache.get("a").is_none());
        assert_eq!(cache.len(), 3);
    }

    #[test]
    fn test_eviction_after_remove() {
        let mut cache = BoundedCache::new(3);
        cache.insert("a".to_string(), 1);
        cache.insert("b".to_string(), 2);
        cache.insert("c".to_string(), 3);
        cache.remove("a");
        // Now only 2 entries, should be able to insert without eviction
        cache.insert("d".to_string(), 4);
        assert_eq!(cache.len(), 3);
        assert!(cache.contains_key("b"));
        assert!(cache.contains_key("c"));
        assert!(cache.contains_key("d"));
    }

    #[test]
    fn test_default_uses_text_cache_limit() {
        let cache: BoundedCache<i32> = BoundedCache::default();
        assert_eq!(cache.max_entries, MAX_TEXT_CACHE_ENTRIES);
    }
}
