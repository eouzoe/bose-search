use crate::response::SearxngResponse;
use bose_common::{BoseConfig, BoseError, BoseResult, SearchQuery, SearchResponse};
use std::fmt::Write as _;
use std::time::Instant;

/// `SearXNG` HTTP client.
#[derive(Clone)]
pub struct SearxngClient {
    http: reqwest::Client,
    base_url: String,
}

impl SearxngClient {
    /// Create a new client from a [`BoseConfig`].
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be built.
    pub fn new(config: &BoseConfig) -> BoseResult<Self> {
        let http = reqwest::Client::builder()
            .tcp_keepalive(std::time::Duration::from_mins(1))
            .timeout(std::time::Duration::from_secs(config.request_timeout_secs))
            .user_agent("bose-search/0.1")
            .build()
            .map_err(BoseError::HttpError)?;

        Ok(Self { http, base_url: config.searxng_url.clone() })
    }

    /// Create a client from a raw URL string.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be built.
    pub fn from_url(url: &str) -> BoseResult<Self> {
        let config = BoseConfig { searxng_url: url.to_string(), ..BoseConfig::default() };
        Self::new(&config)
    }

    /// Execute a search query against `SearXNG`.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response cannot be parsed.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "elapsed_ms fits in u64 for any practical request duration"
    )]
    pub async fn search(&self, query: &SearchQuery) -> BoseResult<SearchResponse> {
        let start = Instant::now();

        let mut url = format!(
            "{}/search?q={}&format=json&number_of_results={}",
            self.base_url,
            urlencoding::encode(&query.query),
            query.num_results,
        );

        if let Some(ref cat) = query.category {
            write!(url, "&categories={}", urlencoding::encode(cat))
                .unwrap_or_else(|_| unreachable!("write! to String is infallible"));
        }
        if let Some(ref lang) = query.language {
            write!(url, "&language={}", urlencoding::encode(lang))
                .unwrap_or_else(|_| unreachable!("write! to String is infallible"));
        }
        if let Some(ref tr) = query.time_range {
            write!(url, "&time_range={}", urlencoding::encode(tr))
                .unwrap_or_else(|_| unreachable!("write! to String is infallible"));
        }

        tracing::info!(query = %query.query, "SearXNG search");

        let resp = self.http.get(&url).send().await?;

        if !resp.status().is_success() {
            return Err(BoseError::SearxngError(format!("HTTP {}", resp.status())));
        }

        let searxng_resp: SearxngResponse = resp.json().await?;
        let elapsed = start.elapsed().as_secs_f64();

        if !searxng_resp.unresponsive_engines.is_empty() {
            tracing::warn!(
                engines = ?searxng_resp.unresponsive_engines,
                "Unresponsive engines"
            );
        }

        let result_count = searxng_resp.results.len();
        let response = searxng_resp.into_search_response(elapsed);

        #[expect(clippy::cast_sign_loss, reason = "elapsed is always positive after abs()")]
        let elapsed_ms = (elapsed * 1000.0).abs() as u64;

        tracing::info!(
            query = %query.query,
            results = result_count,
            elapsed_ms = %elapsed_ms,
            "Search complete"
        );

        Ok(response)
    }

    /// Check if the `SearXNG` instance is reachable and responding.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response cannot be parsed.
    pub async fn health_check(&self) -> BoseResult<bool> {
        let url = format!("{}/search?q=test&format=json&number_of_results=1", self.base_url);
        match self.http.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "test code")]

    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_search_success() {
        let mock_server = MockServer::start().await;

        let mock_response = serde_json::json!({
            "query": "rust programming",
            "number_of_results": 2,
            "results": [
                {
                    "url": "https://rust-lang.org",
                    "title": "Rust Programming Language",
                    "content": "A systems programming language",
                    "engine": "google",
                    "score": 1.5,
                    "category": "general"
                },
                {
                    "url": "https://doc.rust-lang.org",
                    "title": "Rust Documentation",
                    "content": "Official docs",
                    "engine": "duckduckgo",
                    "score": 1.2,
                    "category": "it"
                }
            ],
            "suggestions": [],
            "unresponsive_engines": []
        });

        Mock::given(method("GET"))
            .and(path("/search"))
            .and(query_param("q", "rust programming"))
            .and(query_param("format", "json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
            .mount(&mock_server)
            .await;

        let client = SearxngClient::from_url(&mock_server.uri()).unwrap();
        let query = SearchQuery::new("rust programming");
        let resp = client.search(&query).await.unwrap();

        assert_eq!(resp.query, "rust programming");
        assert_eq!(resp.results.len(), 2);
        assert_eq!(resp.results[0].title, "Rust Programming Language");
        assert_eq!(resp.results[0].engine, "google");
        assert_eq!(resp.results[1].engine, "duckduckgo");
        assert_eq!(resp.engines_used.len(), 2);
        assert!(resp.engines_used.contains(&"google".to_string()));
        assert!(resp.engines_used.contains(&"duckduckgo".to_string()));
    }

    #[tokio::test]
    async fn test_search_with_category() {
        let mock_server = MockServer::start().await;

        let mock_response = serde_json::json!({
            "query": "linux kernel",
            "results": [{
                "url": "https://kernel.org",
                "title": "Linux Kernel",
                "engine": "brave",
                "category": "it"
            }],
            "suggestions": [],
            "unresponsive_engines": []
        });

        Mock::given(method("GET"))
            .and(path("/search"))
            .and(query_param("categories", "it"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
            .mount(&mock_server)
            .await;

        let client = SearxngClient::from_url(&mock_server.uri()).unwrap();
        let query = SearchQuery::new("linux kernel").with_category("it");
        let resp = client.search(&query).await.unwrap();

        assert_eq!(resp.results.len(), 1);
        assert_eq!(resp.results[0].category, "it");
    }

    #[tokio::test]
    async fn test_search_http_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/search"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let client = SearxngClient::from_url(&mock_server.uri()).unwrap();
        let query = SearchQuery::new("test");
        let result = client.search(&query).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            BoseError::SearxngError(msg) => assert!(msg.contains("500")),
            _ => panic!("Expected SearxngError"),
        }
    }

    #[tokio::test]
    async fn test_health_check_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/search"))
            .and(query_param("q", "test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "query": "test",
                "results": []
            })))
            .mount(&mock_server)
            .await;

        let client = SearxngClient::from_url(&mock_server.uri()).unwrap();
        let healthy = client.health_check().await.unwrap();
        assert!(healthy);
    }

    #[tokio::test]
    async fn test_health_check_failure() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&mock_server)
            .await;

        let client = SearxngClient::from_url(&mock_server.uri()).unwrap();
        let healthy = client.health_check().await.unwrap();
        assert!(!healthy);
    }
}
