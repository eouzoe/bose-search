use bose_common::{SearchResponse, SearchResult, UnresponsiveEngine};
use serde::Deserialize;

/// Top-level `SearXNG` JSON response.
#[derive(Debug, Deserialize)]
pub struct SearxngResponse {
    pub query: String,
    pub number_of_results: Option<u64>,
    #[serde(default)]
    pub results: Vec<SearxngResult>,
    #[serde(default)]
    pub suggestions: Vec<String>,
    #[serde(default)]
    pub unresponsive_engines: Vec<(String, String)>,

    /// Present on some `SearXNG` revisions as structured direct answers.
    /// Ignored for now, but modelled so its presence cannot fail parsing.
    #[serde(default)]
    #[allow(dead_code, reason = "reserved for a future direct-answer tool")]
    pub answers: Vec<String>,
}

/// A single search result from `SearXNG`.
#[derive(Debug, Deserialize)]
pub struct SearxngResult {
    pub url: String,
    pub title: String,
    pub content: Option<String>,
    pub engine: Option<String>,
    pub score: Option<f64>,
    pub category: Option<String>,
}

impl From<SearxngResult> for SearchResult {
    fn from(r: SearxngResult) -> Self {
        Self {
            title: r.title,
            url: r.url,
            snippet: r.content,
            engine: r.engine.unwrap_or_else(|| "unknown".to_string()),
            score: r.score,
            category: r.category.unwrap_or_else(|| "general".to_string()),
        }
    }
}

impl SearxngResponse {
    pub fn into_search_response(self, elapsed: f64) -> SearchResponse {
        // A HashSet makes the order nondeterministic, so the tool output
        // would differ between two identical queries. Sort it.
        let engines_used: Vec<String> = {
            let mut v: Vec<String> = self
                .results
                .iter()
                .filter_map(|r| r.engine.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            v.sort();
            v
        };

        let mut unresponsive_engines: Vec<UnresponsiveEngine> = self
            .unresponsive_engines
            .into_iter()
            .map(|(engine, reason)| UnresponsiveEngine { engine, reason })
            .collect();
        unresponsive_engines.sort_by(|a, b| a.engine.cmp(&b.engine));

        SearchResponse {
            query: self.query,
            results: self.results.into_iter().map(Into::into).collect(),
            elapsed_seconds: elapsed,
            total_results: self.number_of_results,
            engines_used,
            unresponsive_engines,
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "test code")]
    #![expect(clippy::float_cmp, reason = "test: comparing exact literal value")]

    use super::*;

    fn sample_searxng_json() -> serde_json::Value {
        serde_json::json!({
            "query": "rust",
            "number_of_results": 100,
            "results": [{
                "url": "https://rust-lang.org",
                "title": "Rust",
                "content": "Systems language",
                "engine": "google",
                "score": 1.5,
                "category": "general"
            }],
            "suggestions": ["rust lang"],
            "unresponsive_engines": []
        })
    }

    #[test]
    fn test_parse_searxng_response() {
        let resp: SearxngResponse = serde_json::from_value(sample_searxng_json()).unwrap();
        assert_eq!(resp.query, "rust");
        assert_eq!(resp.results.len(), 1);
        assert_eq!(resp.results[0].title, "Rust");
    }

    #[test]
    fn test_parse_empty_results() {
        let json = serde_json::json!({
            "query": "nothing",
            "results": [],
            "suggestions": [],
            "unresponsive_engines": []
        });
        let resp: SearxngResponse = serde_json::from_value(json).unwrap();
        assert!(resp.results.is_empty());
        assert!(resp.number_of_results.is_none());
    }

    #[test]
    fn test_parse_missing_fields() {
        let json = serde_json::json!({
            "query": "test",
            "results": [{
                "url": "https://example.com",
                "title": "Example"
            }],
            "suggestions": [],
            "unresponsive_engines": []
        });
        let resp: SearxngResponse = serde_json::from_value(json).unwrap();
        assert!(resp.results[0].content.is_none());
        assert!(resp.results[0].engine.is_none());
        assert!(resp.results[0].score.is_none());
    }

    #[test]
    fn test_convert_to_search_result() {
        let sr = SearxngResult {
            url: "https://example.com".into(),
            title: "Example".into(),
            content: Some("Hello".into()),
            engine: Some("google".into()),
            score: Some(1.0),
            category: Some("it".into()),
        };
        let result: SearchResult = sr.into();
        assert_eq!(result.engine, "google");
        assert_eq!(result.category, "it");
    }

    #[test]
    fn test_convert_missing_engine_defaults() {
        let sr = SearxngResult {
            url: "https://example.com".into(),
            title: "Example".into(),
            content: None,
            engine: None,
            score: None,
            category: None,
        };
        let result: SearchResult = sr.into();
        assert_eq!(result.engine, "unknown");
        assert_eq!(result.category, "general");
    }

    #[test]
    fn test_into_search_response() {
        let resp: SearxngResponse = serde_json::from_value(sample_searxng_json()).unwrap();
        let search_resp = resp.into_search_response(0.5);
        assert_eq!(search_resp.query, "rust");
        assert_eq!(search_resp.results.len(), 1);
        assert_eq!(search_resp.elapsed_seconds, 0.5);
        assert_eq!(search_resp.total_results, Some(100));
    }
}

#[cfg(test)]
mod health_tests {
    #![expect(clippy::unwrap_used, reason = "test code")]

    use super::*;
    use bose_common::UnresponsiveEngine;

    fn json_with_engines() -> serde_json::Value {
        serde_json::json!({
            "query": "rust",
            "results": [
                { "url": "https://b.rs", "title": "B", "engine": "mojeek" },
                { "url": "https://a.rs", "title": "A", "engine": "bing" },
                { "url": "https://c.rs", "title": "C", "engine": "bing" }
            ],
            "unresponsive_engines": [
                ["brave", "too many requests"],
                ["yacy", "timeout"]
            ]
        })
    }

    #[test]
    fn unresponsive_engines_survive_the_mapping() {
        let resp: SearxngResponse = serde_json::from_value(json_with_engines()).unwrap();
        let out = resp.into_search_response(0.1);
        assert_eq!(
            out.unresponsive_engines,
            vec![
                UnresponsiveEngine { engine: "brave".into(), reason: "too many requests".into() },
                UnresponsiveEngine { engine: "yacy".into(), reason: "timeout".into() },
            ]
        );
    }

    #[test]
    fn engines_used_is_sorted_for_stable_output() {
        // Two identical queries must produce identical text.
        let resp: SearxngResponse = serde_json::from_value(json_with_engines()).unwrap();
        let out = resp.into_search_response(0.1);
        assert_eq!(out.engines_used, vec!["bing".to_string(), "mojeek".to_string()]);
    }

    #[test]
    fn unknown_searxng_fields_do_not_break_parsing() {
        let json = serde_json::json!({
            "query": "q",
            "results": [],
            "answers": ["42"],
            "infoboxes": [],
            "corrections": [],
            "unresponsive_engines": []
        });
        let resp: SearxngResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.answers, vec!["42".to_string()]);
    }
}
