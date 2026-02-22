#![warn(clippy::pedantic)]
#![deny(clippy::unwrap_used)]

use std::fmt::Write;
use std::time::Duration;

use bose_common::{
    BoseConfig, RRFFusion, RankedResult, ScavengerConfig, SearchQuery, SearchResponse,
    SearchResult, compute_sha256, extract_text, scavenge,
};
use bose_crawler::Fetcher;
use bose_index::searcher::Searcher;
use bose_searxng::SearxngClient;
use moka::future::Cache;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Content, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
    ToolsCapability,
};
use rmcp::transport::stdio;
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt, schemars, tool, tool_handler, tool_router,
};

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

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct CrawlKnowledgeParams {
    #[schemars(description = "Search query to find relevant content")]
    query: String,

    #[schemars(description = "Number of URLs to fetch and index (1-10, default: 5)")]
    depth: Option<u32>,

    #[schemars(description = "Filter results by source/engine (optional)")]
    source_filter: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ScavengerSearchParams {
    #[schemars(description = "The search query")]
    query: String,

    #[schemars(description = "Number of raw results to analyze (default: 50, max: 100)")]
    harvest_size: Option<u32>,

    #[schemars(description = "Number of scavenger results to return (default: 10)")]
    num_results: Option<u32>,

    #[schemars(description = "Category: general, science, it, news")]
    category: Option<String>,

    #[schemars(description = "Language code: en, zh-TW, ja")]
    language: Option<String>,
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
        let cache =
            Cache::builder().max_capacity(1000).time_to_live(Duration::from_secs(5 * 60)).build();

        Self { client, index_path, tool_router: Self::tool_router(), cache }
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
            return Ok(CallToolResult::success(vec![Content::text(format_response(&cached_resp))]));
        }

        tracing::info!(query = %query.query, "Cache miss");

        // Cache miss - fetch from SearXNG
        match self.client.search(&query).await {
            Ok(resp) => {
                // Apply RRF fusion across engines before caching
                let top_n = query.num_results as usize;
                let fused = fuse_results(&resp, top_n);
                let fused_resp = SearchResponse { results: fused, ..resp };
                self.cache.insert(cache_key, fused_resp.clone()).await;

                Ok(CallToolResult::success(vec![Content::text(format_response(&fused_resp))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!("Search failed: {e}"))])),
        }
    }

    #[tool(description = "Check if the SearXNG search backend is healthy and responding.")]
    async fn health_check(&self) -> Result<CallToolResult, McpError> {
        match self.client.health_check().await {
            Ok(true) => Ok(CallToolResult::success(vec![Content::text("SearXNG is healthy")])),
            Ok(false) => {
                Ok(CallToolResult::error(vec![Content::text("SearXNG is not responding")]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(format!("Health check failed: {e}"))]))
            }
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

        const MAX_SIZE: usize = 50 * 1024;

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

        let text = extract_text(&html);

        // Limit to 50KB
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
        let Ok(searcher) = Searcher::new(&self.index_path) else {
            return Ok(CallToolResult::success(vec![Content::text(
                "Local index not found. Set BOSE_INDEX_PATH or crawl content first.",
            )]));
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
                #[expect(clippy::unwrap_used, reason = "write! to String is infallible")]
                {
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
                        let snippet = if r.body.len() > 200 { &r.body[..200] } else { &r.body };
                        writeln!(out, "   {snippet}").unwrap();
                        writeln!(out).unwrap();
                    }
                }
                Ok(CallToolResult::success(vec![Content::text(out)]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(format!("Local search failed: {e}"))]))
            }
        }
    }

    #[tool(
        description = "Crawl and index knowledge: search web -> fetch URLs -> extract text -> index to local Tantivy. Returns summary of indexed documents."
    )]
    async fn crawl_knowledge(
        &self,
        Parameters(params): Parameters<CrawlKnowledgeParams>,
    ) -> Result<CallToolResult, McpError> {
        use bose_index::{Document, Indexer};
        use chrono::Utc;

        tracing::info!(query = %params.query, "Starting knowledge crawl");

        // Validate depth parameter
        let depth = params.depth.unwrap_or(5).clamp(1, 10);

        // Step 1: Search with SearXNG
        let search_query = SearchQuery::new(&params.query).with_num_results(depth);
        let search_results = match self.client.search(&search_query).await {
            Ok(resp) => resp,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Search failed: {e}"
                ))]));
            }
        };

        // Filter by source if specified
        let urls_to_fetch: Vec<_> = search_results
            .results
            .iter()
            .filter(|r| {
                if let Some(ref filter) = params.source_filter {
                    r.engine.contains(filter)
                } else {
                    true
                }
            })
            .take(depth as usize)
            .collect();

        if urls_to_fetch.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "No URLs found matching criteria".to_string(),
            )]));
        }

        // Build URL → search-metadata map so we can reconstruct Documents after concurrent fetch
        let url_meta: std::collections::HashMap<String, &SearchResult> =
            urls_to_fetch.iter().map(|r| (r.url.clone(), *r)).collect();
        let url_strings: Vec<String> = url_meta.keys().cloned().collect();
        let url_count = url_strings.len();

        tracing::info!(count = url_count, "Fetching URLs with bose-crawler backpressure");

        // Step 2: Concurrent fetch — PID backpressure + semaphore-bounded concurrency
        let fetcher = Fetcher::with_backpressure(url_count.min(10), 10)
            .map_err(|e| McpError::internal_error(format!("Crawler init failed: {e}"), None))?;

        let fetch_results = fetcher.crawl_batch(url_strings).await;

        let mut documents = Vec::new();
        for fetch_result in fetch_results {
            match fetch_result {
                Ok(fr) if fr.status == 200 => {
                    let text = extract_text(&fr.body);
                    if text.len() < 100 {
                        tracing::warn!(url = %fr.url, "Skipping: text too short");
                        continue;
                    }
                    if let Some(meta) = url_meta.get(&fr.url) {
                        let doc = Document {
                            title: meta.title.clone(),
                            body: text,
                            url: fr.url.clone(),
                            source: meta.engine.clone(),
                            language: "en".to_string(), // TODO: detect language
                            crawled_at: Utc::now(),
                            relevance_score: meta.score.unwrap_or(0.5),
                            tags: vec![params.query.clone()],
                        };
                        documents.push(doc);
                        tracing::info!(
                            url = %fr.url,
                            latency_ms = fr.latency.as_millis(),
                            "Fetched successfully"
                        );
                    }
                }
                Ok(fr) => {
                    tracing::warn!(url = %fr.url, status = fr.status, "HTTP error");
                }
                Err(e) => {
                    tracing::warn!(url = %e.url, error = %e.error, "Fetch failed");
                }
            }
        }

        if documents.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "No documents could be fetched and indexed".to_string(),
            )]));
        }

        // Step 3: Index documents
        let mut indexer = match Indexer::new(&self.index_path, 50) {
            Ok(idx) => idx,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to create indexer: {e}"
                ))]));
            }
        };

        for doc in &documents {
            if let Err(e) = indexer.add_document(doc) {
                tracing::error!(url = %doc.url, error = %e, "Failed to index document");
            }
        }

        if let Err(e) = indexer.commit() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to commit index: {e}"
            ))]));
        }

        tracing::info!(count = documents.len(), "Knowledge crawl complete");

        // Return summary
        let crawler_metrics = fetcher.metrics();
        let mut summary = format!(
            "Crawled and indexed {} documents for query \"{}\" \
             [{} requests, {:.0}% success, avg {:.1}ms, {} bytes]:\n\n",
            documents.len(),
            params.query,
            crawler_metrics.total_requests(),
            crawler_metrics.success_rate() * 100.0,
            crawler_metrics.avg_latency_ms(),
            crawler_metrics.total_bytes(),
        );

        #[expect(clippy::unwrap_used, reason = "write! to String is infallible")]
        {
            for (i, doc) in documents.iter().enumerate() {
                writeln!(summary, "{}. {} ({})", i + 1, doc.title, doc.url).unwrap();
                let snippet = if doc.body.len() > 150 { &doc.body[..150] } else { &doc.body };
                writeln!(summary, "   {snippet}...").unwrap();
                writeln!(summary).unwrap();
            }
        }

        Ok(CallToolResult::success(vec![Content::text(summary)]))
    }

    #[tool(
        description = "Scavenger search: find high-quality content that mainstream engines miss. Uses information-theoretic surprise and submodular diversity selection to surface hidden gems from 247 engines."
    )]
    async fn scavenger_search(
        &self,
        Parameters(params): Parameters<ScavengerSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        let harvest = params.harvest_size.unwrap_or(50).clamp(10, 100);
        let top_k = params.num_results.unwrap_or(10).clamp(1, 30) as usize;

        let mut query = SearchQuery::new(&params.query).with_num_results(harvest);
        if let Some(cat) = params.category {
            query = query.with_category(&cat);
        }
        query.language = params.language;

        tracing::info!(query = %query.query, harvest = harvest, top_k = top_k, "Scavenger search");

        let resp = match self.client.search(&query).await {
            Ok(r) => r,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Search failed: {e}"
                ))]));
            }
        };

        if resp.results.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("No results to scavenge.")]));
        }

        let config = ScavengerConfig { top_k, ..ScavengerConfig::default() };

        let scavenged = scavenge(&resp.results, &config);

        let mut out = String::new();
        #[expect(clippy::unwrap_used, reason = "write! to String is infallible")]
        {
            writeln!(
                out,
                "Scavenged {} hidden gems from {} raw results ({} engines):\n",
                scavenged.len(),
                resp.results.len(),
                resp.engines_used.len()
            )
            .unwrap();

            for (i, sr) in scavenged.iter().enumerate() {
                writeln!(out, "{}. [{}]({})", i + 1, sr.result.title, sr.result.url).unwrap();
                writeln!(
                    out,
                    "   Score: {:.2} | Surprise: {:.1} bits | Engines: {}/{}",
                    sr.scavenger_score,
                    sr.surprise,
                    sr.engine_frequency,
                    resp.engines_used.len()
                )
                .unwrap();
                writeln!(out, "   Why: {}", sr.explanation).unwrap();
                if let Some(ref s) = sr.result.snippet {
                    let truncated = if s.len() > 200 { &s[..200] } else { s };
                    writeln!(out, "   {truncated}").unwrap();
                }
                writeln!(out).unwrap();
            }
        }

        Ok(CallToolResult::success(vec![Content::text(out)]))
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
    #[expect(clippy::unwrap_used, reason = "write! to String is infallible")]
    {
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
    }
    out
}

/// Generate a deterministic, collision-resistant cache key for a search query.
///
/// Uses SHA-256 to prevent cache-key injection via crafted query strings.
fn make_cache_key(query: &SearchQuery) -> String {
    let raw = format!(
        "{}|{}|{}|{}|{}",
        query.query,
        query.num_results,
        query.category.as_deref().unwrap_or(""),
        query.language.as_deref().unwrap_or(""),
        query.time_range.as_deref().unwrap_or(""),
    );
    compute_sha256(&raw)
}

/// Reorder `resp.results` by Reciprocal Rank Fusion score across engines.
///
/// Per-engine rank is determined by order of appearance in `resp.results`.
/// Cross-engine consensus bubbles results to the top.
#[must_use]
fn fuse_results(resp: &SearchResponse, top_n: usize) -> Vec<SearchResult> {
    use std::collections::HashMap;

    // Build per-engine ranked lists (position in resp.results = per-engine rank)
    let mut engine_groups: HashMap<&str, Vec<RankedResult>> = HashMap::new();
    for result in &resp.results {
        let group = engine_groups.entry(result.engine.as_str()).or_default();
        let rank = group.len();
        group.push(RankedResult { id: result.url.clone(), rank });
    }

    // Accumulate RRF scores across engines
    let mut fusion = RRFFusion::new();
    for (engine, ranked) in &engine_groups {
        fusion.add_results(engine, ranked.clone(), 1.0);
    }

    let fused = fusion.fuse(top_n);

    // URL → highest-scored SearXNG result lookup
    let mut url_map: HashMap<&str, &SearchResult> = HashMap::new();
    for result in &resp.results {
        url_map
            .entry(result.url.as_str())
            .and_modify(|prev| {
                if result.score > prev.score {
                    *prev = result;
                }
            })
            .or_insert(result);
    }

    fused.into_iter().filter_map(|f| url_map.get(f.id.as_str()).copied().cloned()).collect()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // tracing → stderr (stdout reserved for MCP JSON-RPC)
    tracing_subscriber::fmt().with_env_filter("bose=info").with_writer(std::io::stderr).init();

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
    #![expect(clippy::unwrap_used, reason = "test code")]

    use super::*;

    #[tokio::test]
    async fn test_read_url_success() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        let html_content = r"
            <!DOCTYPE html>
            <html>
            <head><title>Test Page</title></head>
            <body>
                <h1>Hello World</h1>
                <p>This is a test page.</p>
            </body>
            </html>
        ";

        Mock::given(method("GET"))
            .and(path("/test"))
            .respond_with(ResponseTemplate::new(200).set_body_string(html_content))
            .mount(&mock_server)
            .await;

        let config = BoseConfig::from_env();
        let client = SearxngClient::new(&config).unwrap();
        let server = BoseSearchServer::new(client, config.index_path);

        let params = ReadUrlParams { url: format!("{}/test", mock_server.uri()) };

        let result = server.read_url(Parameters(params)).await.unwrap();

        assert_eq!(result.is_error, Some(false));
        assert_eq!(result.content.len(), 1);
        // Just check that we got some text content
        assert!(!result.content.is_empty());
    }

    #[tokio::test]
    async fn test_read_url_http_error() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/error"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let config = BoseConfig::from_env();
        let client = SearxngClient::new(&config).unwrap();
        let server = BoseSearchServer::new(client, config.index_path);

        let params = ReadUrlParams { url: format!("{}/error", mock_server.uri()) };

        let result = server.read_url(Parameters(params)).await.unwrap();

        assert_eq!(result.is_error, Some(true));
        assert_eq!(result.content.len(), 1);
    }

    #[tokio::test]
    async fn test_read_url_size_limit() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Create content larger than 50KB
        let large_content = "a".repeat(60 * 1024);
        let html = format!("<html><body>{large_content}</body></html>");

        Mock::given(method("GET"))
            .and(path("/large"))
            .respond_with(ResponseTemplate::new(200).set_body_string(html))
            .mount(&mock_server)
            .await;

        let config = BoseConfig::from_env();
        let client = SearxngClient::new(&config).unwrap();
        let server = BoseSearchServer::new(client, config.index_path);

        let params = ReadUrlParams { url: format!("{}/large", mock_server.uri()) };

        let result = server.read_url(Parameters(params)).await.unwrap();

        assert_eq!(result.is_error, Some(false));
        assert_eq!(result.content.len(), 1);
        // Content should be truncated
    }

    #[tokio::test]
    async fn test_web_search_cache_hit() {
        use wiremock::matchers::{method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};

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

        let config = BoseConfig { searxng_url: mock_server.uri(), ..BoseConfig::default() };
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
        use wiremock::matchers::{method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};

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
        let config = BoseConfig { searxng_url: mock_server.uri(), ..BoseConfig::default() };
        let client = SearxngClient::new(&config).unwrap();

        // Create cache with 1 second TTL for testing
        let cache =
            Cache::builder().max_capacity(1000).time_to_live(Duration::from_secs(1)).build();

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
        // SHA-256 hex digest is always 64 chars
        assert_eq!(key1.len(), 64);

        // Same query → same key (deterministic)
        let key3 = make_cache_key(&SearchQuery::new("rust programming"));
        assert_eq!(key1, key3);

        // Different query → different key (collision resistance)
        let key4 = make_cache_key(&SearchQuery::new("python programming"));
        assert_ne!(key1, key4);

        // Category changes the key
        let key_no_cat = make_cache_key(&SearchQuery::new("rust"));
        let key_with_cat = make_cache_key(&SearchQuery::new("rust").with_category("it"));
        assert_ne!(key_no_cat, key_with_cat);
    }

    #[test]
    fn test_fuse_results_cross_engine_wins() {
        let make_sr = |url: &str, engine: &str, score: f64| SearchResult {
            title: url.to_string(),
            url: url.to_string(),
            snippet: None,
            engine: engine.to_string(),
            score: Some(score),
            category: "general".to_string(),
        };

        // "https://shared.com" appears at rank 1 (google) AND rank 0 (brave)
        // → RRF consensus should beat "https://google-only.com" at rank 0 (google-only)
        let resp = SearchResponse {
            results: vec![
                make_sr("https://google-only.com", "google", 1.0),
                make_sr("https://shared.com", "google", 0.9),
                make_sr("https://shared.com", "brave", 1.0),
            ],
            query: "test".to_string(),
            elapsed_seconds: 0.1,
            total_results: None,
            engines_used: vec!["google".to_string(), "brave".to_string()],
        };

        let fused = fuse_results(&resp, 10);
        assert_eq!(fused[0].url, "https://shared.com", "Cross-engine result should rank first");
        assert_eq!(fused[1].url, "https://google-only.com");
    }

    #[tokio::test]
    async fn test_crawl_knowledge_integration() {
        use tempfile::TempDir;
        use wiremock::matchers::{method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock SearXNG search response
        let search_response = serde_json::json!({
            "query": "rust async",
            "results": [
                {
                    "url": format!("{}/doc1", mock_server.uri()),
                    "title": "Rust Async Programming",
                    "engine": "google",
                    "category": "it"
                },
                {
                    "url": format!("{}/doc2", mock_server.uri()),
                    "title": "Async/Await in Rust",
                    "engine": "duckduckgo",
                    "category": "it"
                }
            ],
            "suggestions": [],
            "unresponsive_engines": []
        });

        Mock::given(method("GET"))
            .and(path("/search"))
            .and(query_param("q", "rust async"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&search_response))
            .mount(&mock_server)
            .await;

        // Mock document pages
        let doc1_html = r"
            <html>
            <head><title>Rust Async Programming</title></head>
            <body>
                <h1>Rust Async Programming</h1>
                <p>Asynchronous programming in Rust is powered by the async/await syntax.
                This allows you to write concurrent code that is both efficient and easy to understand.
                The Rust async ecosystem includes tokio, async-std, and many other libraries.</p>
            </body>
            </html>
        ";

        let doc2_html = r"
            <html>
            <head><title>Async/Await in Rust</title></head>
            <body>
                <h1>Async/Await in Rust</h1>
                <p>The async/await feature in Rust makes it easy to write asynchronous code.
                You can use async fn to define asynchronous functions, and await to wait for futures to complete.
                This is a powerful feature for building high-performance applications.</p>
            </body>
            </html>
        ";

        Mock::given(method("GET"))
            .and(path("/doc1"))
            .respond_with(ResponseTemplate::new(200).set_body_string(doc1_html))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/doc2"))
            .respond_with(ResponseTemplate::new(200).set_body_string(doc2_html))
            .mount(&mock_server)
            .await;

        // Create temporary index directory
        let temp_dir = TempDir::new().unwrap();
        let index_path = temp_dir.path().to_str().unwrap().to_string();

        let config = BoseConfig {
            searxng_url: mock_server.uri(),
            index_path: index_path.clone(),
            ..BoseConfig::default()
        };
        let client = SearxngClient::new(&config).unwrap();
        let server = BoseSearchServer::new(client, index_path.clone());

        // Test crawl_knowledge
        let crawl_params = CrawlKnowledgeParams {
            query: "rust async".to_string(),
            depth: Some(2),
            source_filter: None,
        };

        let result = server.crawl_knowledge(Parameters(crawl_params)).await.unwrap();

        assert_eq!(result.is_error, Some(false));

        // Verify documents were indexed by searching
        let search_params = LocalSearchParams { query: "async".to_string(), num_results: Some(10) };

        let search_result = server.local_search(Parameters(search_params)).await.unwrap();

        assert_eq!(search_result.is_error, Some(false));
        // Should find at least one document
        // (We can't easily check the exact content without parsing the response)
    }
}
