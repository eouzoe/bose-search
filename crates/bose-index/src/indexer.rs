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
