//! 索引寫入器
//!
//! Writes documents into the Tantivy index.

use crate::schema::{create_schema, fields};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tantivy::{Index, IndexWriter, TantivyDocument, doc};

/// 要索引的文檔
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub title: String,
    pub body: String,
    pub url: String,
    pub source: String,
    pub language: String,
    pub crawled_at: DateTime<Utc>,
    pub relevance_score: f64,
    pub tags: Vec<String>,
}

/// 索引寫入器
pub struct Indexer {
    index: Index,
    writer: IndexWriter,
}

impl Indexer {
    /// 創建新的索引寫入器
    ///
    /// # Arguments
    ///
    /// - `index_path`: path to the index directory
    /// - `heap_size_mb`: writer heap size in MB
    ///
    /// # Errors
    ///
    /// Returns an error if the index directory cannot be created or opened,
    /// or if the index writer cannot be initialized.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use bose_index::indexer::Indexer;
    ///
    /// let indexer = Indexer::new("./index", 50).unwrap();
    /// ```
    pub fn new<P: AsRef<Path>>(index_path: P, heap_size_mb: usize) -> Result<Self> {
        let schema = create_schema();
        let index_path = index_path.as_ref();

        // 創建或打開索引
        let index = if index_path.join("meta.json").exists() {
            Index::open_in_dir(index_path).context("Failed to open existing index")?
        } else {
            std::fs::create_dir_all(index_path).context("Failed to create index directory")?;
            Index::create_in_dir(index_path, schema.clone()).context("Failed to create index")?
        };

        // 創建寫入器
        let heap_size = heap_size_mb * 1024 * 1024;
        let writer = index.writer(heap_size).context("Failed to create index writer")?;

        Ok(Self { index, writer })
    }

    /// 添加單個文檔到索引
    ///
    /// # Arguments
    ///
    /// - `doc`: document to index
    ///
    /// # Errors
    ///
    /// Returns an error if the schema field lookup or document write fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use bose_index::indexer::{Indexer, Document};
    /// use chrono::Utc;
    ///
    /// let mut indexer = Indexer::new("./index", 50).unwrap();
    /// let doc = Document {
    ///     title: "Rust async".to_string(),
    ///     body: "Async programming in Rust".to_string(),
    ///     url: "https://example.com".to_string(),
    ///     source: "docs.rs".to_string(),
    ///     language: "rust".to_string(),
    ///     crawled_at: Utc::now(),
    ///     relevance_score: 0.9,
    ///     tags: vec!["rust".to_string(), "async".to_string()],
    /// };
    /// indexer.add_document(&doc).unwrap();
    /// ```
    pub fn add_document(&mut self, doc: &Document) -> Result<()> {
        let schema = self.index.schema();

        let title_field = schema.get_field(fields::TITLE)?;
        let body_field = schema.get_field(fields::BODY)?;
        let url_field = schema.get_field(fields::URL)?;
        let source_field = schema.get_field(fields::SOURCE)?;
        let language_field = schema.get_field(fields::LANGUAGE)?;
        let crawled_at_field = schema.get_field(fields::CRAWLED_AT)?;
        let relevance_score_field = schema.get_field(fields::RELEVANCE_SCORE)?;
        let tags_field = schema.get_field(fields::TAGS)?;

        // 轉換 chrono::DateTime 到 tantivy::DateTime
        let tantivy_datetime = tantivy::DateTime::from_timestamp_secs(doc.crawled_at.timestamp());

        let mut tantivy_doc = TantivyDocument::new();
        tantivy_doc.add_text(title_field, &doc.title);
        tantivy_doc.add_text(body_field, &doc.body);
        tantivy_doc.add_text(url_field, &doc.url);
        tantivy_doc.add_text(source_field, &doc.source);
        tantivy_doc.add_text(language_field, &doc.language);
        tantivy_doc.add_date(crawled_at_field, tantivy_datetime);
        tantivy_doc.add_f64(relevance_score_field, doc.relevance_score);

        for tag in &doc.tags {
            tantivy_doc.add_text(tags_field, tag);
        }

        self.writer.add_document(tantivy_doc)?;
        Ok(())
    }

    /// 批量添加文檔
    ///
    /// # Errors
    ///
    /// Returns an error if any document fails to be added.
    pub fn add_documents(&mut self, docs: &[Document]) -> Result<()> {
        for doc in docs {
            self.add_document(doc)?;
        }
        Ok(())
    }

    /// Remove every document whose `url` matches, then insert `doc`.
    ///
    /// URL is the identity of a crawled page, so re-crawling the same URL
    /// must replace its entry rather than append a second copy. Without this
    /// the second `crawl_knowledge` for the same query duplicates every
    /// result, which is what `local_search` was returning.
    ///
    /// # Errors
    ///
    /// Returns an error if the delete or the write fails.
    pub fn upsert_document(&mut self, doc: &Document) -> Result<()> {
        self.delete_by_url(&doc.url)?;
        self.add_document(doc)
    }

    /// Delete all documents carrying `url`.
    ///
    /// # Errors
    ///
    /// Returns an error if the field lookup or the delete fails.
    pub fn delete_by_url(&mut self, url: &str) -> Result<()> {
        let schema = self.index.schema();
        let url_field = schema.get_field(fields::URL)?;
        let term = tantivy::Term::from_field_text(url_field, url);
        self.writer.delete_term(term);
        Ok(())
    }

    /// Number of documents currently stored for `url`.
    ///
    /// Used to tell a fresh insert from a replacement.
    ///
    /// # Errors
    ///
    /// Returns an error if the reader cannot be obtained.
    pub fn count_by_url(&self, url: &str) -> Result<usize> {
        use tantivy::Searcher;
        use tantivy::collector::DocSetCollector;
        use tantivy::query::TermQuery;
        use tantivy::schema::IndexRecordOption;

        let schema = self.index.schema();
        let url_field = schema.get_field(fields::URL)?;
        let term = tantivy::Term::from_field_text(url_field, url);

        // A committed reader is required for a correct count; fall back to 0
        // when the index is brand new and nothing has been committed yet.
        let reader = self.index.reader().context("Failed to create reader")?;
        let searcher: Searcher = reader.searcher();
        let query = TermQuery::new(term, IndexRecordOption::Basic);
        Ok(searcher.search(&query, &DocSetCollector)?.len())
    }

    /// 提交所有待處理的文檔
    ///
    /// # Errors
    ///
    /// Returns an error if the commit operation fails.
    pub fn commit(&mut self) -> Result<()> {
        self.writer.commit()?;
        Ok(())
    }

    /// 獲取索引引用
    #[must_use]
    pub fn index(&self) -> &Index {
        &self.index
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "test code")]

    use super::*;
    use tempfile::TempDir;

    fn create_test_document() -> Document {
        Document {
            title: "Test Document".to_string(),
            body: "This is a test document body".to_string(),
            url: "https://example.com/test".to_string(),
            source: "test".to_string(),
            language: "rust".to_string(),
            crawled_at: Utc::now(),
            relevance_score: 0.8,
            tags: vec!["test".to_string(), "rust".to_string()],
        }
    }

    #[test]
    fn test_indexer_new() {
        let temp_dir = TempDir::new().unwrap();
        let indexer = Indexer::new(temp_dir.path(), 50);
        assert!(indexer.is_ok());
    }

    #[test]
    fn test_add_document() {
        let temp_dir = TempDir::new().unwrap();
        let mut indexer = Indexer::new(temp_dir.path(), 50).unwrap();
        let doc = create_test_document();

        let result = indexer.add_document(&doc);
        assert!(result.is_ok());
    }

    #[test]
    fn test_add_and_commit() {
        let temp_dir = TempDir::new().unwrap();
        let mut indexer = Indexer::new(temp_dir.path(), 50).unwrap();
        let doc = create_test_document();

        indexer.add_document(&doc).unwrap();
        let result = indexer.commit();
        assert!(result.is_ok());
    }

    #[test]
    fn test_add_multiple_documents() {
        let temp_dir = TempDir::new().unwrap();
        let mut indexer = Indexer::new(temp_dir.path(), 50).unwrap();

        let docs = vec![
            create_test_document(),
            Document {
                title: "Second Document".to_string(),
                body: "Another test document".to_string(),
                url: "https://example.com/test2".to_string(),
                source: "test".to_string(),
                language: "python".to_string(),
                crawled_at: Utc::now(),
                relevance_score: 0.7,
                tags: vec!["test".to_string()],
            },
        ];

        let result = indexer.add_documents(&docs);
        assert!(result.is_ok());

        let commit_result = indexer.commit();
        assert!(commit_result.is_ok());
    }

    #[test]
    fn test_reopen_index() {
        let temp_dir = TempDir::new().unwrap();

        // 創建並寫入
        {
            let mut indexer = Indexer::new(temp_dir.path(), 50).unwrap();
            let doc = create_test_document();
            indexer.add_document(&doc).unwrap();
            indexer.commit().unwrap();
        }

        // 重新打開
        let indexer = Indexer::new(temp_dir.path(), 50);
        assert!(indexer.is_ok());
    }
}

#[cfg(test)]
mod upsert_tests {
    #![expect(clippy::unwrap_used, reason = "test code")]

    use super::*;
    use crate::searcher::Searcher;
    use tempfile::TempDir;

    fn doc_with(url: &str, body: &str) -> Document {
        Document {
            title: format!("Doc for {url}"),
            body: body.to_string(),
            url: url.to_string(),
            source: "test".to_string(),
            language: "en".to_string(),
            crawled_at: Utc::now(),
            relevance_score: 1.0,
            tags: vec!["test".to_string()],
        }
    }

    fn count_for(dir: &TempDir, url: &str) -> usize {
        let indexer = Indexer::new(dir.path(), 50).unwrap();
        indexer.count_by_url(url).unwrap()
    }

    #[test]
    fn add_document_twice_duplicates() {
        // The behaviour we are moving away from: add_document is a plain
        // append, so the same URL stored twice is two searchable rows.
        let dir = TempDir::new().unwrap();
        {
            let mut i = Indexer::new(dir.path(), 50).unwrap();
            i.add_document(&doc_with("https://e.com/a", "v1")).unwrap();
            i.add_document(&doc_with("https://e.com/a", "v2")).unwrap();
            i.commit().unwrap();
        }
        assert_eq!(count_for(&dir, "https://e.com/a"), 2);
    }

    #[test]
    fn upsert_replaces_instead_of_appending() {
        let dir = TempDir::new().unwrap();
        {
            let mut i = Indexer::new(dir.path(), 50).unwrap();
            i.upsert_document(&doc_with("https://e.com/a", "v1")).unwrap();
            i.upsert_document(&doc_with("https://e.com/a", "v2")).unwrap();
            i.commit().unwrap();
        }
        assert_eq!(count_for(&dir, "https://e.com/a"), 1);
    }

    #[test]
    fn upsert_keeps_other_urls() {
        let dir = TempDir::new().unwrap();
        {
            let mut i = Indexer::new(dir.path(), 50).unwrap();
            i.upsert_document(&doc_with("https://e.com/a", "a")).unwrap();
            i.upsert_document(&doc_with("https://e.com/b", "b")).unwrap();
            i.upsert_document(&doc_with("https://e.com/a", "a2")).unwrap();
            i.commit().unwrap();
        }
        assert_eq!(count_for(&dir, "https://e.com/a"), 1);
        assert_eq!(count_for(&dir, "https://e.com/b"), 1);
    }

    #[test]
    fn upsert_updates_the_stored_body() {
        let dir = TempDir::new().unwrap();
        {
            let mut i = Indexer::new(dir.path(), 50).unwrap();
            i.upsert_document(&doc_with("https://e.com/a", "oldbody")).unwrap();
            i.commit().unwrap();
        }
        {
            let mut i = Indexer::new(dir.path(), 50).unwrap();
            i.upsert_document(&doc_with("https://e.com/a", "newbody uniqueword")).unwrap();
            i.commit().unwrap();
        }
        let s = Searcher::new(dir.path()).unwrap();
        let found = s.search("uniqueword", 10).unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].body.contains("newbody"));
    }

    #[test]
    fn delete_by_url_removes_everything_for_that_url() {
        let dir = TempDir::new().unwrap();
        {
            let mut i = Indexer::new(dir.path(), 50).unwrap();
            i.add_document(&doc_with("https://e.com/a", "x")).unwrap();
            i.add_document(&doc_with("https://e.com/a", "y")).unwrap();
            i.commit().unwrap();
        }
        {
            let mut i = Indexer::new(dir.path(), 50).unwrap();
            i.delete_by_url("https://e.com/a").unwrap();
            i.commit().unwrap();
        }
        assert_eq!(count_for(&dir, "https://e.com/a"), 0);
    }
}
