use serde::{Deserialize, Serialize};

/// 統一的搜尋結果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: Option<String>,
    pub engine: String,
    pub score: Option<f64>,
    pub category: String,
}

/// 搜尋請求參數
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    pub num_results: u32,
    pub category: Option<String>,
    pub language: Option<String>,
    pub time_range: Option<String>,
}

impl SearchQuery {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            num_results: 10,
            category: None,
            language: None,
            time_range: None,
        }
    }

    #[must_use]
    pub fn with_num_results(mut self, n: u32) -> Self {
        self.num_results = n;
        self
    }

    #[must_use]
    pub fn with_category(mut self, cat: impl Into<String>) -> Self {
        self.category = Some(cat.into());
        self
    }
}

/// An engine `SearXNG` could not use for a query, and why.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnresponsiveEngine {
    pub engine: String,
    pub reason: String,
}

/// 搜尋回應
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub query: String,
    pub elapsed_seconds: f64,
    pub total_results: Option<u64>,
    pub engines_used: Vec<String>,
    #[serde(default)]
    pub unresponsive_engines: Vec<UnresponsiveEngine>,
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "test code")]

    use super::*;

    #[test]
    fn test_search_query_new() {
        let q = SearchQuery::new("rust");
        assert_eq!(q.query, "rust");
        assert_eq!(q.num_results, 10);
        assert!(q.category.is_none());
        assert!(q.language.is_none());
        assert!(q.time_range.is_none());
    }

    #[test]
    fn test_search_query_builder() {
        let q = SearchQuery::new("rust").with_num_results(20).with_category("it");
        assert_eq!(q.num_results, 20);
        assert_eq!(q.category.as_deref(), Some("it"));
    }

    #[test]
    fn test_search_result_serialize() {
        let r = SearchResult {
            title: "Rust Lang".into(),
            url: "https://rust-lang.org".into(),
            snippet: Some("Systems programming".into()),
            engine: "google".into(),
            score: Some(0.95),
            category: "general".into(),
        };
        insta::assert_json_snapshot!(r);
    }

    #[test]
    fn test_search_result_partial_fields() {
        let r = SearchResult {
            title: "Test".into(),
            url: "https://example.com".into(),
            snippet: None,
            engine: "bing".into(),
            score: None,
            category: "it".into(),
        };
        insta::assert_json_snapshot!(r);
    }

    #[test]
    fn test_search_response_serialize() {
        let resp = SearchResponse {
            unresponsive_engines: vec![],
            results: vec![SearchResult {
                title: "Rust".into(),
                url: "https://rust-lang.org".into(),
                snippet: Some("Fast".into()),
                engine: "google".into(),
                score: Some(1.0),
                category: "general".into(),
            }],
            query: "rust".into(),
            elapsed_seconds: 0.5,
            total_results: Some(100),
            engines_used: vec!["google".into()],
        };
        // Round-trip rather than a struct snapshot: the type is what SearXNG
        // hands us, so deserialise -> serialise -> compare must be lossless.
        let json = serde_json::to_value(&resp).unwrap();
        let back: SearchResponse = serde_json::from_value(json).unwrap();
        assert_eq!(back.engines_used, vec!["google".to_string()]);
        assert!(back.unresponsive_engines.is_empty());
        assert_eq!(back.results.len(), 1);
    }

    #[test]
    fn test_search_result_roundtrip() {
        let r = SearchResult {
            title: "Test".into(),
            url: "https://example.com".into(),
            snippet: Some("Hello".into()),
            engine: "bing".into(),
            score: Some(0.5),
            category: "general".into(),
        };
        let json = serde_json::to_string(&r).unwrap();
        let r2: SearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(r, r2);
    }
}
