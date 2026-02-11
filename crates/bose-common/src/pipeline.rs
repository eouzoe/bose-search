//! 知識收集管線
//!
//! 將搜尋結果轉換為可索引的知識文檔。
//! 流程: SearXNG 搜尋 → 內容提取 → 去重 → 文檔建立

use crate::dedup::Deduplicator;
use crate::extractor::extract_text;
use serde::{Deserialize, Serialize};

/// 知識文檔 (準備寫入 Tantivy 索引)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDoc {
    pub title: String,
    pub body: String,
    pub url: String,
    pub source: String,
    pub language: String,
    pub tags: Vec<String>,
}

/// 原始抓取結果
#[derive(Debug, Clone)]
pub struct RawPage {
    pub url: String,
    pub title: String,
    pub html: String,
    pub source: String,
}

/// 知識收集管線
///
/// # 範例
///
/// ```
/// use bose_common::pipeline::{Pipeline, RawPage};
///
/// let mut pipeline = Pipeline::new("rust", &["async", "tokio"]);
/// let page = RawPage {
///     url: "https://example.com".to_string(),
///     title: "Rust Async".to_string(),
///     html: "<p>Learn async programming in Rust with the tokio runtime for high performance</p>".to_string(),
///     source: "docs.rs".to_string(),
/// };
/// let docs = pipeline.process(vec![page]);
/// assert_eq!(docs.len(), 1);
/// ```
pub struct Pipeline {
    language: String,
    tags: Vec<String>,
    deduplicator: Deduplicator,
    min_body_length: usize,
}

impl Pipeline {
    /// 創建新的管線
    pub fn new(language: &str, tags: &[&str]) -> Self {
        Self {
            language: language.to_string(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            deduplicator: Deduplicator::new(),
            min_body_length: 50,
        }
    }

    /// 設定最小內容長度 (過濾太短的頁面)
    pub fn with_min_body_length(mut self, len: usize) -> Self {
        self.min_body_length = len;
        self
    }

    /// 處理原始頁面，返回去重後的知識文檔
    pub fn process(&mut self, pages: Vec<RawPage>) -> Vec<KnowledgeDoc> {
        pages
            .into_iter()
            .filter_map(|page| self.process_page(page))
            .collect()
    }

    /// 處理單個頁面
    fn process_page(&mut self, page: RawPage) -> Option<KnowledgeDoc> {
        // 1. 提取純文字
        let body = extract_text(&page.html);

        // 2. 過濾太短的內容
        if body.len() < self.min_body_length {
            return None;
        }

        // 3. 去重 (基於 URL)
        if self.deduplicator.contains(&page.url) {
            return None;
        }
        self.deduplicator.add_content(&page.url);

        // 4. 建立知識文檔
        Some(KnowledgeDoc {
            title: page.title,
            body,
            url: page.url,
            source: page.source,
            language: self.language.clone(),
            tags: self.tags.clone(),
        })
    }

    /// 獲取已處理的文檔數量
    pub fn processed_count(&self) -> usize {
        self.deduplicator.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_page(url: &str, title: &str, html: &str) -> RawPage {
        RawPage {
            url: url.to_string(),
            title: title.to_string(),
            html: html.to_string(),
            source: "test".to_string(),
        }
    }

    #[test]
    fn test_pipeline_basic() {
        let mut pipeline = Pipeline::new("rust", &["async"]);
        let pages = vec![make_page(
            "https://example.com/1",
            "Rust Async",
            "<p>Learn async programming in Rust with tokio runtime</p>",
        )];

        let docs = pipeline.process(pages);
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].title, "Rust Async");
        assert_eq!(docs[0].language, "rust");
        assert!(docs[0].tags.contains(&"async".to_string()));
    }

    #[test]
    fn test_pipeline_dedup() {
        let mut pipeline = Pipeline::new("rust", &[]);
        let pages = vec![
            make_page("https://example.com/1", "Page 1", "<p>Content that is definitely long enough to pass the minimum body length filter requirement</p>"),
            make_page("https://example.com/1", "Page 1 Dup", "<p>Different content but same URL should be deduped by the pipeline deduplication logic</p>"),
        ];

        let docs = pipeline.process(pages);
        assert_eq!(docs.len(), 1);
    }

    #[test]
    fn test_pipeline_filters_short() {
        let mut pipeline = Pipeline::new("rust", &[]);
        let pages = vec![make_page(
            "https://example.com/short",
            "Short",
            "<p>Too short</p>",
        )];

        let docs = pipeline.process(pages);
        assert_eq!(docs.len(), 0);
    }

    #[test]
    fn test_pipeline_html_extraction() {
        let mut pipeline = Pipeline::new("rust", &[]);
        let html = "<html><head><script>bad()</script></head>\
                     <body><h1>Title</h1><p>This is a long enough paragraph for testing purposes</p></body></html>";
        let pages = vec![make_page("https://example.com/html", "Test", html)];

        let docs = pipeline.process(pages);
        assert_eq!(docs.len(), 1);
        assert!(!docs[0].body.contains("bad()"));
        assert!(docs[0].body.contains("Title"));
    }

    #[test]
    fn test_pipeline_empty() {
        let mut pipeline = Pipeline::new("rust", &[]);
        let docs = pipeline.process(vec![]);
        assert!(docs.is_empty());
    }

    #[test]
    fn test_pipeline_processed_count() {
        let mut pipeline = Pipeline::new("rust", &[]);
        assert_eq!(pipeline.processed_count(), 0);

        let pages = vec![
            make_page("https://a.com", "A", "<p>Content A is long enough for the minimum body length filter</p>"),
            make_page("https://b.com", "B", "<p>Content B is also long enough for the minimum body length filter</p>"),
        ];
        pipeline.process(pages);
        assert_eq!(pipeline.processed_count(), 2);
    }

    #[test]
    fn test_pipeline_custom_min_length() {
        let mut pipeline = Pipeline::new("rust", &[]).with_min_body_length(10);
        let pages = vec![make_page(
            "https://example.com/1",
            "Short OK",
            "<p>Short but OK now</p>",
        )];

        let docs = pipeline.process(pages);
        assert_eq!(docs.len(), 1);
    }

    #[test]
    fn test_pipeline_multiple_tags() {
        let mut pipeline = Pipeline::new("rust", &["async", "tokio", "runtime"]);
        let pages = vec![make_page(
            "https://example.com/1",
            "Test",
            "<p>Content that is long enough to pass the minimum body length filter</p>",
        )];

        let docs = pipeline.process(pages);
        assert_eq!(docs[0].tags.len(), 3);
    }
}
