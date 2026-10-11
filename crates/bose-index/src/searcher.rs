//! 查詢執行器
//!
//! Executes search queries against the Tantivy index.

use crate::schema::fields;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tantivy::{
    Index, IndexReader, ReloadPolicy, collector::TopDocs, query::QueryParser, schema::Value,
};

/// 搜尋結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub body: String,
    pub url: String,
    pub source: String,
    pub language: String,
    pub relevance_score: f64,
    pub tags: Vec<String>,
    pub search_score: f32,
}

/// 查詢執行器
pub struct Searcher {
    index: Index,
    reader: IndexReader,
}

impl Searcher {
    /// 創建新的查詢執行器
    ///
    /// # Arguments
    ///
    /// - `index_path`: path to the index directory
    ///
    /// # Errors
    ///
    /// Returns an error if the index cannot be opened or the reader cannot be created.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use bose_index::searcher::Searcher;
    ///
    /// let searcher = Searcher::new("./index").unwrap();
    /// ```
    pub fn new<P: AsRef<Path>>(index_path: P) -> Result<Self> {
        let index = Index::open_in_dir(index_path).context("Failed to open index")?;

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .context("Failed to create index reader")?;

        Ok(Self { index, reader })
    }

    /// 搜尋索引
    ///
    /// # Arguments
    ///
    /// - `query`: search query string
    /// - `limit`: maximum number of results to return
    ///
    /// # Errors
    ///
    /// Returns an error if the query cannot be parsed or the search fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use bose_index::searcher::Searcher;
    ///
    /// let searcher = Searcher::new("./index").unwrap();
    /// let results = searcher.search("rust async", 10).unwrap();
    /// ```
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let schema = self.index.schema();
        let searcher = self.reader.searcher();

        // 創建查詢解析器 (搜尋 title 和 body 欄位)
        let title_field = schema.get_field(fields::TITLE)?;
        let body_field = schema.get_field(fields::BODY)?;
        let query_parser = QueryParser::for_index(&self.index, vec![title_field, body_field]);

        // 解析查詢
        let query = query_parser.parse_query(query).context("Failed to parse query")?;

        // 執行搜尋
        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

        // 轉換結果
        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let retrieved_doc: tantivy::TantivyDocument = searcher.doc(doc_address)?;

            let title = retrieved_doc
                .get_first(title_field)
                .and_then(|v: &tantivy::schema::OwnedValue| v.as_str())
                .unwrap_or("")
                .to_string();

            let body = retrieved_doc
                .get_first(body_field)
                .and_then(|v: &tantivy::schema::OwnedValue| v.as_str())
                .unwrap_or("")
                .to_string();

            let url_field = schema.get_field(fields::URL)?;
            let url = retrieved_doc
                .get_first(url_field)
                .and_then(|v: &tantivy::schema::OwnedValue| v.as_str())
                .unwrap_or("")
                .to_string();

            let source_field = schema.get_field(fields::SOURCE)?;
            let source = retrieved_doc
                .get_first(source_field)
                .and_then(|v: &tantivy::schema::OwnedValue| v.as_str())
                .unwrap_or("")
                .to_string();

            let language_field = schema.get_field(fields::LANGUAGE)?;
            let language = retrieved_doc
                .get_first(language_field)
                .and_then(|v: &tantivy::schema::OwnedValue| v.as_str())
                .unwrap_or("")
                .to_string();

            let relevance_score_field = schema.get_field(fields::RELEVANCE_SCORE)?;
            let relevance_score = retrieved_doc
                .get_first(relevance_score_field)
                .and_then(|v: &tantivy::schema::OwnedValue| v.as_f64())
                .unwrap_or(0.0);

            let tags_field = schema.get_field(fields::TAGS)?;
            let tags: Vec<String> = retrieved_doc
                .get_all(tags_field)
                .filter_map(|v: &tantivy::schema::OwnedValue| {
                    v.as_str().map(|s: &str| s.to_string())
                })
                .collect();

            results.push(SearchResult {
                title,
                body,
                url,
                source,
                language,
                relevance_score,
                tags,
                search_score: score,
            });
        }

        Ok(results)
    }

    /// 獲取索引中的文檔總數
    #[must_use]
    pub fn num_docs(&self) -> u64 {
        self.reader.searcher().num_docs()
    }

    /// Force the reader to pick up externally committed changes.
    ///
    /// Call this after an `Indexer` in the same process writes new documents,
    /// otherwise the reader keeps serving the pre-write segment list.
    ///
    /// # Errors
    ///
    /// Returns an error if the reload fails.
    pub fn reload(&self) -> Result<()> {
        self.reader.reload().context("Failed to reload index reader")
    }
}

/// A lazily-opened, process-lifetime `Searcher`.
///
/// `Searcher::new` costs a directory walk plus reader setup. The server used
/// to call it inside every `local_search`, paying that cost on every single
/// tool call. This handle opens once and keeps the result.
///
/// The index directory may legitimately not exist yet — a fresh install has
/// no index until the first `crawl_knowledge` — so construction is deferred
/// and failure is reported as "not available" rather than an error.
#[derive(Clone)]
pub struct SearcherHandle {
    path: String,
    inner: std::sync::Arc<std::sync::Mutex<Option<std::sync::Arc<Searcher>>>>,
}

impl SearcherHandle {
    /// Create a handle for `path`. Does not touch the filesystem.
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into(), inner: std::sync::Arc::new(std::sync::Mutex::new(None)) }
    }

    /// The cached `Searcher`, opening the index on first use.
    ///
    /// Returns `None` if the index cannot be opened — which on a fresh
    /// install simply means "nothing has been crawled yet".
    #[must_use]
    pub fn get(&self) -> Option<std::sync::Arc<Searcher>> {
        let mut guard = self.lock();
        if let Some(s) = guard.as_ref() {
            return Some(std::sync::Arc::clone(s));
        }
        let searcher = Searcher::new(&self.path).ok()?;
        let arc = std::sync::Arc::new(searcher);
        *guard = Some(std::sync::Arc::clone(&arc));
        Some(arc)
    }

    /// The directory the Tantivy index lives in.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Drop the cached searcher so the next `get` reopens the directory.
    ///
    /// Used when the index is (re)created by this process.
    pub fn invalidate(&self) {
        *self.lock() = None;
    }

    /// Reopen and refresh the cached searcher, if one exists.
    ///
    /// # Errors
    ///
    /// Returns an error if the reader cannot be reloaded.
    pub fn reload(&self) -> anyhow::Result<()> {
        let guard = self.lock();
        if let Some(s) = guard.as_ref() {
            s.reload()?;
        }
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<std::sync::Arc<Searcher>>> {
        // A poisoned mutex here only means a panic happened while we were
        // opening the index; the inner Option is still valid.
        self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "test code")]

    use super::*;
    use crate::indexer::{Document, Indexer};
    use chrono::Utc;
    use tempfile::TempDir;

    fn setup_test_index() -> (TempDir, Searcher) {
        let temp_dir = TempDir::new().unwrap();

        // 創建並填充索引
        {
            let mut indexer = Indexer::new(temp_dir.path(), 50).unwrap();

            let docs = vec![
                Document {
                    title: "Rust Async Programming".to_string(),
                    body: "Learn async programming in Rust with tokio".to_string(),
                    url: "https://example.com/rust-async".to_string(),
                    source: "docs.rs".to_string(),
                    language: "rust".to_string(),
                    crawled_at: Utc::now(),
                    relevance_score: 0.9,
                    tags: vec!["rust".to_string(), "async".to_string()],
                },
                Document {
                    title: "Python Asyncio Guide".to_string(),
                    body: "Async programming in Python using asyncio".to_string(),
                    url: "https://example.com/python-async".to_string(),
                    source: "docs.python.org".to_string(),
                    language: "python".to_string(),
                    crawled_at: Utc::now(),
                    relevance_score: 0.8,
                    tags: vec!["python".to_string(), "async".to_string()],
                },
            ];

            indexer.add_documents(&docs).unwrap();
            indexer.commit().unwrap();
        }

        let searcher = Searcher::new(temp_dir.path()).unwrap();
        (temp_dir, searcher)
    }

    #[test]
    fn test_searcher_new() {
        let temp_dir = TempDir::new().unwrap();
        {
            let mut indexer = Indexer::new(temp_dir.path(), 50).unwrap();
            indexer.commit().unwrap();
        }

        let searcher = Searcher::new(temp_dir.path());
        assert!(searcher.is_ok());
    }

    #[test]
    fn test_search_basic() {
        let (_temp_dir, searcher) = setup_test_index();

        let results = searcher.search("async", 10).unwrap();
        assert!(!results.is_empty());
        assert!(results.len() <= 2);
    }

    #[test]
    fn test_search_rust_specific() {
        let (_temp_dir, searcher) = setup_test_index();

        let results = searcher.search("rust", 10).unwrap();
        assert!(!results.is_empty());

        // 應該找到 Rust 相關的文檔
        let has_rust = results.iter().any(|r| r.language == "rust");
        assert!(has_rust);
    }

    #[test]
    fn test_search_limit() {
        let (_temp_dir, searcher) = setup_test_index();

        let results = searcher.search("async", 1).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_no_results() {
        let (_temp_dir, searcher) = setup_test_index();

        let results = searcher.search("nonexistent_term_xyz", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_num_docs() {
        let (_temp_dir, searcher) = setup_test_index();

        let num_docs = searcher.num_docs();
        assert_eq!(num_docs, 2);
    }

    #[test]
    fn test_search_result_fields() {
        let (_temp_dir, searcher) = setup_test_index();

        let results = searcher.search("rust", 10).unwrap();
        assert!(!results.is_empty());

        let result = &results[0];
        assert!(!result.title.is_empty());
        assert!(!result.body.is_empty());
        assert!(!result.url.is_empty());
        assert!(!result.source.is_empty());
        assert!(!result.language.is_empty());
        assert!(result.search_score > 0.0);
    }
}

#[cfg(test)]
mod handle_tests {
    #![expect(clippy::unwrap_used, reason = "test code")]

    use super::*;
    use crate::indexer::{Document, Indexer};
    use chrono::Utc;
    use tempfile::TempDir;

    fn seed(dir: &TempDir) {
        let mut indexer = Indexer::new(dir.path(), 50).unwrap();
        indexer
            .add_documents(&[Document {
                title: "Handle test".to_string(),
                body: "lazy searcher handle content".to_string(),
                url: "https://example.com/h".to_string(),
                source: "test".to_string(),
                language: "en".to_string(),
                crawled_at: Utc::now(),
                relevance_score: 1.0,
                tags: vec!["handle".to_string()],
            }])
            .unwrap();
        indexer.commit().unwrap();
    }

    #[test]
    fn opens_once_and_reuses() {
        let dir = TempDir::new().unwrap();
        seed(&dir);
        let h = SearcherHandle::new(dir.path().to_str().unwrap());

        let first = h.get().unwrap();
        let second = h.get().unwrap();
        // Same Arc: the index was opened exactly once.
        assert!(std::sync::Arc::ptr_eq(&first, &second));
        assert_eq!(first.num_docs(), 1);
    }

    #[test]
    fn missing_index_is_none_not_a_panic() {
        let dir = TempDir::new().unwrap();
        let h = SearcherHandle::new(dir.path().join("does-not-exist").to_str().unwrap());
        assert!(h.get().is_none());
    }

    #[test]
    fn invalidate_forces_reopen() {
        let dir = TempDir::new().unwrap();
        seed(&dir);
        let h = SearcherHandle::new(dir.path().to_str().unwrap());

        let before = h.get().unwrap();
        h.invalidate();
        let after = h.get().unwrap();
        assert!(!std::sync::Arc::ptr_eq(&before, &after));
    }

    #[test]
    fn sees_documents_written_after_open() {
        let dir = TempDir::new().unwrap();
        let h = SearcherHandle::new(dir.path().to_str().unwrap());

        // Open first (index may not exist yet on a fresh install).
        assert!(h.get().is_none());

        seed(&dir);
        h.invalidate();
        let s = h.get().unwrap();
        assert_eq!(s.num_docs(), 1);
        assert!(!s.search("lazy", 10).unwrap().is_empty());
    }

    #[test]
    fn reload_is_safe_without_a_searcher() {
        let dir = TempDir::new().unwrap();
        let h = SearcherHandle::new(dir.path().to_str().unwrap());
        assert!(h.reload().is_ok());
    }
}
