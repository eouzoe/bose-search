use sha2::{Digest, Sha256};
use std::collections::HashSet;

/// 內容去重器
///
/// Uses SHA-256 for content-addressable storage, ensuring each piece of content is stored once.
///
/// # Examples
///
/// ```
/// use bose_common::dedup::Deduplicator;
///
/// let mut dedup = Deduplicator::new();
///
/// // 第一次添加
/// let hash1 = dedup.add_content("Hello, world!");
/// assert_eq!(dedup.len(), 1);
///
/// // 相同內容，返回相同 hash
/// let hash2 = dedup.add_content("Hello, world!");
/// assert_eq!(hash1, hash2);
/// assert_eq!(dedup.len(), 1); // Still only one
///
/// // 不同內容
/// let hash3 = dedup.add_content("Different content");
/// assert_ne!(hash1, hash3);
/// assert_eq!(dedup.len(), 2);
/// ```
#[derive(Debug, Clone, Default)]
pub struct Deduplicator {
    /// 已見過的內容 hash
    seen: HashSet<String>,
}

impl Deduplicator {
    /// 建立新的去重器
    pub fn new() -> Self {
        Self {
            seen: HashSet::new(),
        }
    }

    /// Add content and return its SHA-256 hash.
    ///
    /// 如果內容已存在，返回現有的 hash。
    ///
    /// # Arguments
    ///
    /// * `content` — content to deduplicate
    ///
    /// # Returns
    ///
    /// Returns the SHA-256 hash of the content (hex-encoded).
    pub fn add_content(&mut self, content: &str) -> String {
        let hash = compute_sha256(content);
        self.seen.insert(hash.clone());
        hash
    }

    /// 檢查內容是否已存在
    ///
    /// # Arguments
    ///
    /// * `content` — content to check
    ///
    /// # Returns
    ///
    /// Returns `true` if the content already exists, `false` otherwise.
    pub fn contains(&self, content: &str) -> bool {
        let hash = compute_sha256(content);
        self.seen.contains(&hash)
    }

    /// 檢查 hash 是否已存在
    pub fn contains_hash(&self, hash: &str) -> bool {
        self.seen.contains(hash)
    }

    /// 返回已去重的內容數量
    pub fn len(&self) -> usize {
        self.seen.len()
    }

    /// 檢查是否為空
    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }

    /// 清空去重器
    pub fn clear(&mut self) {
        self.seen.clear();
    }
}

/// Compute the SHA-256 hash of content.
///
/// # Arguments
///
/// * `content` — content to hash
///
/// # Returns
///
/// Returns the SHA-256 hash as a 64-character hex string.
///
/// # Examples
///
/// ```
/// use bose_common::dedup::compute_sha256;
///
/// let hash = compute_sha256("Hello, world!");
/// assert_eq!(hash.len(), 64); // SHA-256 = 32 bytes = 64 hex chars
/// ```
pub fn compute_sha256(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)
}

/// 批量去重
///
/// # Arguments
///
/// * `contents` — list of content items
///
/// # Returns
///
/// 返回去重後的內容列表（保持原始順序，移除重複）
///
/// # Examples
///
/// ```
/// use bose_common::dedup::deduplicate;
///
/// let contents = vec!["a", "b", "a", "c", "b"];
/// let deduped = deduplicate(contents);
/// assert_eq!(deduped, vec!["a", "b", "c"]);
/// ```
pub fn deduplicate<T: AsRef<str>>(contents: Vec<T>) -> Vec<String> {
    let mut dedup = Deduplicator::new();
    let mut result = Vec::new();

    for content in contents {
        let content_str = content.as_ref();
        if !dedup.contains(content_str) {
            dedup.add_content(content_str);
            result.push(content_str.to_string());
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_deterministic() {
        // SHA-256 應該是確定性的
        let hash1 = compute_sha256("test content");
        let hash2 = compute_sha256("test content");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_sha256_different_content() {
        // 不同內容應該產生不同 hash
        let hash1 = compute_sha256("content 1");
        let hash2 = compute_sha256("content 2");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_sha256_length() {
        // SHA-256 = 32 bytes = 64 hex characters
        let hash = compute_sha256("any content");
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_deduplicator_basic() {
        let mut dedup = Deduplicator::new();

        // 第一次添加
        let hash1 = dedup.add_content("Hello");
        assert_eq!(dedup.len(), 1);
        assert!(dedup.contains("Hello"));

        // 相同內容
        let hash2 = dedup.add_content("Hello");
        assert_eq!(hash1, hash2);
        assert_eq!(dedup.len(), 1); // Still only one

        // 不同內容
        let hash3 = dedup.add_content("World");
        assert_ne!(hash1, hash3);
        assert_eq!(dedup.len(), 2);
    }

    #[test]
    fn test_deduplicator_contains() {
        let mut dedup = Deduplicator::new();

        assert!(!dedup.contains("test"));

        dedup.add_content("test");
        assert!(dedup.contains("test"));
        assert!(!dedup.contains("other"));
    }

    #[test]
    fn test_deduplicator_contains_hash() {
        let mut dedup = Deduplicator::new();

        let hash = dedup.add_content("test");
        assert!(dedup.contains_hash(&hash));
        assert!(!dedup.contains_hash("invalid_hash"));
    }

    #[test]
    fn test_deduplicator_empty() {
        let dedup = Deduplicator::new();
        assert!(dedup.is_empty());
        assert_eq!(dedup.len(), 0);
    }

    #[test]
    fn test_deduplicator_clear() {
        let mut dedup = Deduplicator::new();
        dedup.add_content("test1");
        dedup.add_content("test2");
        assert_eq!(dedup.len(), 2);

        dedup.clear();
        assert!(dedup.is_empty());
        assert_eq!(dedup.len(), 0);
    }

    #[test]
    fn test_deduplicate_function() {
        let contents = vec!["a", "b", "a", "c", "b", "d"];
        let deduped = deduplicate(contents);

        assert_eq!(deduped.len(), 4);
        assert_eq!(deduped, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn test_deduplicate_preserves_order() {
        // 應該保持第一次出現的順序
        let contents = vec!["z", "a", "z", "b", "a"];
        let deduped = deduplicate(contents);

        assert_eq!(deduped, vec!["z", "a", "b"]);
    }

    #[test]
    fn test_deduplicate_empty() {
        let contents: Vec<&str> = vec![];
        let deduped = deduplicate(contents);
        assert_eq!(deduped.len(), 0);
    }

    #[test]
    fn test_deduplicate_all_unique() {
        let contents = vec!["a", "b", "c"];
        let deduped = deduplicate(contents);
        assert_eq!(deduped, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_deduplicate_all_same() {
        let contents = vec!["a", "a", "a"];
        let deduped = deduplicate(contents);
        assert_eq!(deduped, vec!["a"]);
    }

    #[test]
    fn test_whitespace_matters() {
        // 空白字元應該影響 hash
        let hash1 = compute_sha256("hello");
        let hash2 = compute_sha256("hello ");
        let hash3 = compute_sha256(" hello");

        assert_ne!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_ne!(hash2, hash3);
    }

    #[test]
    fn test_case_sensitive() {
        // 大小寫應該影響 hash
        let hash1 = compute_sha256("Hello");
        let hash2 = compute_sha256("hello");

        assert_ne!(hash1, hash2);
    }
}
