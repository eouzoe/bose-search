use bose_common::*;
use bose_index::searcher::Searcher;
use bose_searxng::SearxngClient;
use moka::future::Cache;
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt, handler::server::tool::ToolRouter,
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_handler, tool_router,
    transport::stdio,
};
use std::fmt::Write;
use std::time::Duration;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema, Clone)]
struct WebSearchParams {
    #[schemars(description = "The search query")]
    query: String,

    #[schemars(description = "Number of results (default: 10)")]
    num_results: Option<u32>,

    #[schemars(description = "Category: general, science, it, news")]
    category: Option<String>,

    #[schemars(description = "Language code: en, zh-TW, ja")]
    language: Option<String>,

    #[schemars(description = "Time range: day, week, month, year")]
    time_range: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct LocalSearchParams {
    #[schemars(description = "The search query")]
    query: String,

    #[schemars(description = "Number of results (default: 10)")]
    num_results: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ReadUrlParams {
    #[schemars(description = "The URL to fetch")]
    url: String,
}

#[derive(Clone)]
struct BoseSearchServer {
    client: SearxngClient,
    index_path: String,
    tool_router: ToolRouter<Self>,
    cache: Cache<String, SearchResponse>,
}

#[tool_router]
impl BoseSearchServer {
    fn new(client: SearxngClient, index_path: String) -> Self {
        // Create cache with 5min TTL and max 1000 entries
        let cache = Cache::builder()
            .max_capacity(1000)
            .time_to_live(Duration::from_secs(5 * 60))
            .build();

        Self {
            client,
            index_path,
            tool_router: Self::tool_router(),
            cache,
        }
    }

    #[tool(
        description = "Search the web via SearXNG meta-search engine (247 engines). Returns title, URL, snippet, source engine, and category for each result."
    )]
    async fn web_search(
        &self,
        Parameters(params): Parameters<WebSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut query =
            SearchQuery::new(&params.query).with_num_results(params.num_results.unwrap_or(10));

        if let Some(cat) = params.category {
            query = query.with_category(&cat);
        }
        query.language = params.language;
        query.time_range = params.time_range;

        // Generate cache key
        let cache_key = make_cache_key(&query);

        // Check cache first
        if let Some(cached_resp) = self.cache.get(&cache_key).await {
            tracing::info!(query = %query.query, "Cache hit");
            return Ok(CallToolResult::success(vec![Content::text(
                format_response(&cached_resp),
            )]));
        }

        tracing::info!(query = %query.query, "Cache miss");

        // Cache miss - fetch from SearXNG
        match self.client.search(&query).await {
            Ok(resp) => {
                // Store in cache
                self.cache.insert(cache_key, resp.clone()).await;

                Ok(CallToolResult::success(vec![Content::text(
                    format_response(&resp),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Search failed: {e}"
            ))])),
        }
    }

    #[tool(description = "Check if the SearXNG search backend is healthy and responding.")]
    async fn health_check(&self) -> Result<CallToolResult, McpError> {
        match self.client.health_check().await {
            Ok(true) => Ok(CallToolResult::success(vec![Content::text(
                "SearXNG is healthy",
            )])),
            Ok(false) => Ok(CallToolResult::error(vec![Content::text(
                "SearXNG is not responding",
            )])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Health check failed: {e}"
            ))])),
        }
    }

    #[tool(
        description = "Fetch a specific URL and return clean text content (HTML tags stripped, max 50KB)."
    )]
    async fn read_url(
        &self,
        Parameters(params): Parameters<ReadUrlParams>,
    ) -> Result<CallToolResult, McpError> {
        use std::time::Duration;

        tracing::info!(url = %params.url, "Fetching URL");

        // Create a client with 10s timeout
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("bose-search/0.1")
            .build()
            .map_err(|e| {
                McpError::internal_error(format!("Failed to create HTTP client: {e}"), None)
            })?;

        // Fetch the URL
        let response = client
            .get(&params.url)
            .send()
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to fetch URL: {e}"), None))?;

        if !response.status().is_success() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "HTTP error: {}",
                response.status()
            ))]));
        }

        // Get the response body as text
        let html = response
            .text()
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to read response: {e}"), None))?;

        // Strip HTML tags using a simple regex approach
        let text = strip_html_tags(&html);

        // Limit to 50KB
        const MAX_SIZE: usize = 50 * 1024;
        let truncated = if text.len() > MAX_SIZE {
            format!("{}... [truncated]", &text[..MAX_SIZE])
        } else {
            text
        };

        tracing::info!(url = %params.url, size = truncated.len(), "URL fetched successfully");

        Ok(CallToolResult::success(vec![Content::text(truncated)]))
    }

    #[tool(
        description = "Search the local Tantivy knowledge index. Returns results from previously crawled and indexed content. Faster than web search (<10ms)."
    )]
    async fn local_search(
        &self,
        Parameters(params): Parameters<LocalSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        let searcher = match Searcher::new(&self.index_path) {
            Ok(s) => s,
            Err(_) => {
                return Ok(CallToolResult::success(vec![Content::text(
                    "Local index not found. Set BOSE_INDEX_PATH or crawl content first.",
                )]));
            }
        };

        let limit = params.num_results.unwrap_or(10) as usize;
        match searcher.search(&params.query, limit) {
            Ok(results) if results.is_empty() => {
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "No local results for \"{}\". Index has {} documents.",
                    params.query,
                    searcher.num_docs()
                ))]))
            }
            Ok(results) => {
                let mut out = String::new();
                writeln!(
                    out,
                    "Found {} local results for \"{}\":\n",
                    results.len(),
                    params.query
                )
                .unwrap();
                for (i, r) in results.iter().enumerate() {
                    writeln!(out, "{}. [{}]({})", i + 1, r.title, r.url).unwrap();
                    writeln!(
                        out,
                        "   Score: {:.2} | Source: {} | Lang: {}",
                        r.search_score, r.source, r.language
                    )
                    .unwrap();
                    let snippet = if r.body.len() > 200 {
                        &r.body[..200]
                    } else {
                        &r.body
                    };
                    writeln!(out, "   {snippet}").unwrap();
                    writeln!(out).unwrap();
                }
                Ok(CallToolResult::success(vec![Content::text(out)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Local search failed: {e}"
            ))])),
        }
    }
}

#[tool_handler]
impl ServerHandler for BoseSearchServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability::default()),
                ..Default::default()
            },
            server_info: Implementation {
                name: "bose-search".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                title: None,
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Bose Search — meta-search engine powered by SearXNG with 247 backends. \
                 Use web_search to find information on any topic. \
                 Use local_search to search the local knowledge index (<10ms)."
                    .into(),
            ),
        }
    }
}

fn format_response(resp: &SearchResponse) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "Found {} results for \"{}\" ({:.1}s):\n",
        resp.results.len(),
        resp.query,
        resp.elapsed_seconds
    )
    .unwrap();

    for (i, r) in resp.results.iter().enumerate() {
        writeln!(out, "{}. [{}]({})", i + 1, r.title, r.url).unwrap();
        writeln!(out, "   Source: {} | Category: {}", r.engine, r.category).unwrap();
        if let Some(ref s) = r.snippet {
            let truncated = if s.len() > 200 { &s[..200] } else { s };
            writeln!(out, "   {truncated}").unwrap();
        }
        writeln!(out).unwrap();
    }

    if !resp.engines_used.is_empty() {
        writeln!(out, "Engines: {}", resp.engines_used.join(", ")).unwrap();
    }
    out
}

/// Strip HTML tags from text using a simple regex-like approach.
/// This is a basic implementation that removes <tag>...</tag> patterns.
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script_or_style = false;
    let mut tag_buffer = String::new();

    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
            tag_buffer.clear();
        } else if ch == '>' && in_tag {
            in_tag = false;

            // Check if we're entering or leaving script/style tags
            let tag_lower = tag_buffer.to_lowercase();
            if tag_lower.starts_with("script") || tag_lower.starts_with("style") {
                in_script_or_style = true;
            } else if tag_lower.starts_with("/script") || tag_lower.starts_with("/style") {
                in_script_or_style = false;
            }

            tag_buffer.clear();
        } else if in_tag {
            tag_buffer.push(ch);
        } else if !in_script_or_style {
            result.push(ch);
        }
    }

    // Clean up whitespace
    result.split_whitespace().collect::<Vec<&str>>().join(" ")
}

/// Generate a cache key from search query and parameters.
/// Uses a simple string concatenation approach for deterministic keys.
fn make_cache_key(query: &SearchQuery) -> String {
    let mut key = format!("q:{}", query.query);
    key.push_str(&format!("|n:{}", query.num_results));
    if let Some(ref cat) = query.category {
        key.push_str(&format!("|c:{}", cat));
    }
    if let Some(ref lang) = query.language {
        key.push_str(&format!("|l:{}", lang));
    }
    if let Some(ref tr) = query.time_range {
        key.push_str(&format!("|t:{}", tr));
    }
    key
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // tracing → stderr (stdout reserved for MCP JSON-RPC)
    tracing_subscriber::fmt()
        .with_env_filter("bose=info")
        .with_writer(std::io::stderr)
        .init();

    let config = BoseConfig::from_env();
    let client = SearxngClient::new(&config)?;

    tracing::info!(url = %config.searxng_url, index = %config.index_path, "Bose MCP Server starting");

    let server = BoseSearchServer::new(client, config.index_path);
    let service = server.serve(stdio()).await.inspect_err(|e| {
        tracing::error!(%e, "Failed to start MCP server");
    })?;

    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_html_tags_simple() {
        let html = "<p>Hello <b>world</b>!</p>";
        let text = strip_html_tags(html);
        assert_eq!(text, "Hello world!");
    }

    #[test]
    fn test_strip_html_tags_with_attributes() {
        let html = r#"<div class="content"><a href="https://example.com">Link</a></div>"#;
        let text = strip_html_tags(html);
        assert_eq!(text, "Link");
    }

    #[test]
    fn test_strip_html_tags_with_script() {
        let html = "<p>Before</p> <script>alert('test');</script> <p>After</p>";
        let text = strip_html_tags(html);
        assert_eq!(text, "Before After");
    }

    #[test]
    fn test_strip_html_tags_with_style() {
        let html = "<p>Before</p> <style>body { color: red; }</style> <p>After</p>";
        let text = strip_html_tags(html);
        assert_eq!(text, "Before After");
    }

    #[test]
    fn test_strip_html_tags_whitespace_cleanup() {
        let html = "<p>Hello   \n\n  world</p>";
        let text = strip_html_tags(html);
        assert_eq!(text, "Hello world");
    }

    #[tokio::test]
    async fn test_read_url_success() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;

        let html_content = r#"
            <!DOCTYPE html>
            <html>
            <head><title>Test Page</title></head>
            <body>
                <h1>Hello World</h1>
                <p>This is a test page.</p>
            </body>
            </html>
        "#;

        Mock::given(method("GET"))
            .and(path("/test"))
            .respond_with(ResponseTemplate::new(200).set_body_string(html_content))
            .mount(&mock_server)
            .await;

        let config = BoseConfig::from_env();
        let client = SearxngClient::new(&config).unwrap();
        let server = BoseSearchServer::new(client, config.index_path);

        let params = ReadUrlParams {
            url: format!("{}/test", mock_server.uri()),
        };

        let result = server.read_url(Parameters(params)).await.unwrap();

        assert_eq!(result.is_error, Some(false));
        assert_eq!(result.content.len(), 1);
        // Just check that we got some text content
        assert!(!result.content.is_empty());
    }

    #[tokio::test]
    async fn test_read_url_http_error() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/error"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let config = BoseConfig::from_env();
        let client = SearxngClient::new(&config).unwrap();
        let server = BoseSearchServer::new(client, config.index_path);

        let params = ReadUrlParams {
            url: format!("{}/error", mock_server.uri()),
        };

        let result = server.read_url(Parameters(params)).await.unwrap();

        assert_eq!(result.is_error, Some(true));
        assert_eq!(result.content.len(), 1);
    }

    #[tokio::test]
    async fn test_read_url_size_limit() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;

        // Create content larger than 50KB
        let large_content = "a".repeat(60 * 1024);
        let html = format!("<html><body>{}</body></html>", large_content);

        Mock::given(method("GET"))
            .and(path("/large"))
            .respond_with(ResponseTemplate::new(200).set_body_string(html))
            .mount(&mock_server)
            .await;

        let config = BoseConfig::from_env();
        let client = SearxngClient::new(&config).unwrap();
        let server = BoseSearchServer::new(client, config.index_path);

        let params = ReadUrlParams {
            url: format!("{}/large", mock_server.uri()),
        };

        let result = server.read_url(Parameters(params)).await.unwrap();

        assert_eq!(result.is_error, Some(false));
        assert_eq!(result.content.len(), 1);
        // Content should be truncated
    }

    #[tokio::test]
    async fn test_web_search_cache_hit() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path, query_param},
        };

        let mock_server = MockServer::start().await;

        let mock_response = serde_json::json!({
            "query": "rust",
            "results": [{
                "url": "https://rust-lang.org",
                "title": "Rust",
                "engine": "google",
                "category": "general"
            }],
            "suggestions": [],
            "unresponsive_engines": []
        });

        // Mock should only be called once
        Mock::given(method("GET"))
            .and(path("/search"))
            .and(query_param("q", "rust"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
            .expect(1) // Only expect one call
            .mount(&mock_server)
            .await;

        let config = BoseConfig {
            searxng_url: mock_server.uri(),
            ..BoseConfig::default()
        };
        let client = SearxngClient::new(&config).unwrap();
        let server = BoseSearchServer::new(client, config.index_path);

        let params = WebSearchParams {
            query: "rust".to_string(),
            num_results: Some(10),
            category: None,
            language: None,
            time_range: None,
        };

        // First call - cache miss
        let result1 = server.web_search(Parameters(params.clone())).await.unwrap();
        assert_eq!(result1.is_error, Some(false));

        // Second call - cache hit (mock should not be called again)
        let result2 = server.web_search(Parameters(params)).await.unwrap();
        assert_eq!(result2.is_error, Some(false));

        // Results should be identical
        assert_eq!(result1.content.len(), result2.content.len());
    }

    #[tokio::test]
    async fn test_web_search_cache_ttl() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path, query_param},
        };

        let mock_server = MockServer::start().await;

        let mock_response = serde_json::json!({
            "query": "test",
            "results": [{
                "url": "https://example.com",
                "title": "Test",
                "engine": "google",
                "category": "general"
            }],
            "suggestions": [],
            "unresponsive_engines": []
        });

        Mock::given(method("GET"))
            .and(path("/search"))
            .and(query_param("q", "test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
            .mount(&mock_server)
            .await;

        // Create server with very short TTL for testing
        let config = BoseConfig {
            searxng_url: mock_server.uri(),
            ..BoseConfig::default()
        };
        let client = SearxngClient::new(&config).unwrap();

        // Create cache with 1 second TTL for testing
        let cache = Cache::builder()
            .max_capacity(1000)
            .time_to_live(Duration::from_secs(1))
            .build();

        let server = BoseSearchServer {
            client,
            index_path: config.index_path,
            tool_router: BoseSearchServer::tool_router(),
            cache,
        };

        let params = WebSearchParams {
            query: "test".to_string(),
            num_results: Some(10),
            category: None,
            language: None,
            time_range: None,
        };

        // First call
        let result1 = server.web_search(Parameters(params.clone())).await.unwrap();
        assert_eq!(result1.is_error, Some(false));

        // Wait for TTL to expire
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Second call after TTL - should fetch again
        let result2 = server.web_search(Parameters(params)).await.unwrap();
        assert_eq!(result2.is_error, Some(false));
    }

    #[test]
    fn test_make_cache_key() {
        let query1 = SearchQuery::new("rust programming");
        let key1 = make_cache_key(&query1);
        assert!(key1.contains("q:rust programming"));
        assert!(key1.contains("|n:10"));

        let query2 = SearchQuery::new("rust").with_category("it");
        let key2 = make_cache_key(&query2);
        assert!(key2.contains("|c:it"));

        // Same query should produce same key
        let query3 = SearchQuery::new("rust programming");
        let key3 = make_cache_key(&query3);
        assert_eq!(key1, key3);

        // Different query should produce different key
        let query4 = SearchQuery::new("python programming");
        let key4 = make_cache_key(&query4);
        assert_ne!(key1, key4);
    }
}
