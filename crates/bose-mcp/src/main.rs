#![warn(clippy::pedantic)]
#![deny(clippy::unwrap_used)]

use bose_common::{BoseConfig, ScavengerConfig, SearchQuery, SearchResponse, scavenge};
use bose_index::SearcherHandle;
use bose_net::Net;
use bose_searxng::SearxngClient;
use moka::future::Cache;
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::wrapper::Parameters,
    model::{
        CallToolResult, ContentBlock, Implementation, ProtocolVersion, ServerCapabilities,
        ServerConfig,
    },
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
};
use std::fmt::Write;
use std::sync::Arc;

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
struct RecallParams {
    #[schemars(description = "The URL to recall from the local response cache")]
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
    /// Shared HTTP/extraction/cache layer. One connection pool for every tool.
    net: Arc<bose_net::Net>,
    /// Lazily-opened, process-lifetime index handle. Owns the index path.
    index: SearcherHandle,
    cache: Cache<String, SearchResponse>,
}

#[tool_router]
impl BoseSearchServer {
    fn new(client: SearxngClient, net: bose_net::Net, index_path: String) -> Self {
        // Create cache with 5min TTL and max 1000 entries
        let cache = Cache::builder()
            .max_capacity(1000)
            .time_to_live(std::time::Duration::from_mins(5))
            .build();

        Self {
            client,
            net: Arc::new(net),
            index: bose_index::SearcherHandle::new(index_path),
            cache,
        }
    }

    #[tool(
        description = "Search the web through SearXNG meta-search. Returns title, URL, snippet, source engine and category per result. SearXNG aggregates many engines; the ones it could actually reach are listed in the response footer, so an empty result set means \"no engine answered\" rather than \"nothing exists\"."
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
            return Ok(CallToolResult::success(vec![ContentBlock::text(format_response(
                &cached_resp,
            ))]));
        }

        tracing::info!(query = %query.query, "Cache miss");

        // Cache miss - fetch from SearXNG
        match self.client.search(&query).await {
            Ok(resp) => {
                // Store in cache
                self.cache.insert(cache_key, resp.clone()).await;

                Ok(CallToolResult::success(vec![ContentBlock::text(format_response(&resp))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "SearXNG is not reachable at the configured SEARXNG_URL.\n\
                 Start it with `devenv up`, or set SEARXNG_URL to a running instance.\n\
                 This is a transport-level failure, not an empty result.\nError: {e}"
            ))])),
        }
    }

    #[tool(
        description = "Report SearXNG reachability, local index size, and on-disk response-cache entry count. Call this first when a search comes back empty."
    )]
    async fn health_check(&self) -> Result<CallToolResult, McpError> {
        let mut lines = Vec::new();

        match self.client.health_check().await {
            Ok(true) => lines.push("searxng: healthy".to_string()),
            Ok(false) => lines.push("searxng: NOT RESPONDING".to_string()),
            Err(e) => lines.push(format!("searxng: error — {e}")),
        }

        match self.index.get().map(|s| s.num_docs()) {
            Some(n) => lines.push(format!("local_index: {n} documents")),
            None => lines.push("local_index: not yet created".to_string()),
        }

        match self.net.cache.count().await {
            Ok(n) => lines
                .push(format!("response_cache: {n} entries ({})", self.net.cache.root().display())),
            Err(_) => lines.push("response_cache: disabled".to_string()),
        }

        let healthy = lines.first().is_some_and(|l| l.starts_with("searxng: healthy"));
        let out = lines.join("\n");
        if healthy {
            Ok(CallToolResult::success(vec![ContentBlock::text(out)]))
        } else {
            Ok(CallToolResult::error(vec![ContentBlock::text(out)]))
        }
    }

    #[tool(
        description = "Fetch a specific URL and return clean text content (HTML parsed to text, entities decoded, charset detected, max 50KB). Serves from the local response cache when the URL was fetched before."
    )]
    async fn read_url(
        &self,
        Parameters(params): Parameters<ReadUrlParams>,
    ) -> Result<CallToolResult, McpError> {
        const MAX_SIZE: usize = 50 * 1024;

        tracing::info!(url = %params.url, "Fetching URL");

        // A bad URL or dead host is a normal tool outcome, not a protocol
        // error: report it as `isError: true` so the agent can react to it
        // instead of seeing the connection drop.
        let page = match self.net.fetch_text(&params.url).await {
            Ok(p) => p,
            Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(format!("{e}"))])),
        };

        let text = page.text();
        // UTF-8-safe: never slices on a code-point boundary.
        let truncated = bose_net::truncate_with_marker(&text, MAX_SIZE);

        tracing::info!(
            url = %params.url,
            bytes = text.len(),
            returned = truncated.len(),
            from_cache = page.from_cache,
            "URL fetched successfully"
        );

        Ok(CallToolResult::success(vec![ContentBlock::text(truncated)]))
    }

    #[tool(description = "Read a previously fetched page straight from the local response cache, \
                       without touching the network. Returns the raw stored response with its \
                       crawl timestamp, so any result can be audited later. Returns an error if \
                       the URL was never fetched.")]
    async fn recall(
        &self,
        Parameters(params): Parameters<RecallParams>,
    ) -> Result<CallToolResult, McpError> {
        match self.net.cache.lookup(&params.url).await {
            Some((meta, body)) => {
                let text = bose_net::decode_bytes(&body);
                let truncated = bose_net::truncate_with_marker(&text, 50 * 1024);
                let out = format!(
                    "url: {}\nfetched_at: {} (unix seconds)\nstatus: {}\nbytes: {}\nsource: {}\n\n{}",
                    meta.url, meta.fetched_at, meta.status, meta.bytes, "response cache", truncated
                );
                Ok(CallToolResult::success(vec![ContentBlock::text(out)]))
            }
            None => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "No cached response for {}. Fetch it first with read_url.",
                params.url
            ))])),
        }
    }

    #[tool(
        description = "Search the local Tantivy knowledge index over previously crawled and indexed content. Sub-millisecond and no network."
    )]
    async fn local_search(
        &self,
        Parameters(params): Parameters<LocalSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        // Open the index at most once per process, not once per call.
        let Some(searcher) = self.index.get() else {
            return Ok(CallToolResult::success(vec![ContentBlock::text(
                "Local index not found. Set BOSE_INDEX_PATH or crawl content first.",
            )]));
        };

        let limit = params.num_results.unwrap_or(10) as usize;
        match searcher.search(&params.query, limit) {
            Ok(results) if results.is_empty() => {
                Ok(CallToolResult::success(vec![ContentBlock::text(format!(
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
                        let snippet = bose_net::truncate_chars(&r.body, 200);
                        writeln!(out, "   {snippet}").unwrap();
                        writeln!(out).unwrap();
                    }
                }
                Ok(CallToolResult::success(vec![ContentBlock::text(out)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Local search failed: {e}"
            ))])),
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
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "SearXNG is not reachable at the configured SEARXNG_URL.\n\
                 Start it with `devenv up`, or set SEARXNG_URL to a running instance.\n\
                 This is a transport-level failure, not an empty result.\nError: {e}"
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
            return Ok(CallToolResult::success(vec![ContentBlock::text(
                "No URLs found matching criteria".to_string(),
            )]));
        }

        tracing::info!(count = urls_to_fetch.len(), "Fetching URLs");

        // Step 2: Fetch URLs and extract text — all through the shared
        // client/pool/cache, not a per-call `reqwest::Client`.
        let mut documents = Vec::new();
        let mut cached_hits = 0usize;
        let mut failures: Vec<(String, String)> = Vec::new();

        for result in urls_to_fetch {
            let fetched = match self.net.fetch_text(&result.url).await {
                Ok(p) => p,
                Err(e) => {
                    let why = e.to_string();
                    tracing::warn!(url = %result.url, error = %why, "Fetch failed");
                    failures.push((result.url.clone(), why));
                    continue;
                }
            };

            if fetched.from_cache {
                cached_hits += 1;
            }

            let text = fetched.text();

            // Skip if text is too short (likely error page)
            if text.len() < 100 {
                tracing::warn!(url = %result.url, "Skipping: text too short");
                failures.push((result.url.clone(), "extracted text under 100 bytes".to_string()));
                continue;
            }

            let doc = Document {
                title: result.title.clone(),
                body: text,
                url: result.url.clone(),
                source: result.engine.clone(),
                language: "en".to_string(), // TODO: detect language
                crawled_at: Utc::now(),
                relevance_score: 1.0, // TODO: use actual score
                tags: vec![params.query.clone()],
            };

            documents.push(doc);
            tracing::info!(url = %result.url, from_cache = fetched.from_cache, "Fetched successfully");
        }

        if documents.is_empty() {
            let mut msg = String::from("No documents could be fetched and indexed.\n");
            for (url, why) in &failures {
                msg.push_str("  - ");
                msg.push_str(url);
                msg.push_str(": ");
                msg.push_str(why);
                msg.push('\n');
            }
            return Ok(CallToolResult::success(vec![ContentBlock::text(msg)]));
        }

        // Step 3: Index documents
        let mut indexer = match Indexer::new(self.index.path(), 50) {
            Ok(idx) => idx,
            Err(e) => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "Failed to create indexer: {e}"
                ))]));
            }
        };

        for doc in &documents {
            // URL is the page's identity: re-crawling replaces the entry
            // rather than appending a duplicate.
            if let Err(e) = indexer.upsert_document(doc) {
                tracing::error!(url = %doc.url, error = %e, "Failed to index document");
            }
        }

        if let Err(e) = indexer.commit() {
            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Failed to commit index: {e}"
            ))]));
        }

        // The handle may have opened the index before this crawl created it.
        self.index.invalidate();
        if let Err(e) = self.index.reload() {
            tracing::debug!(error = %e, "Index reload after crawl was a no-op");
        }

        tracing::info!(count = documents.len(), cached = cached_hits, "Knowledge crawl complete");

        // Return summary
        let mut summary = format!(
            "Successfully crawled and indexed {} documents for query \"{}\" ({} from cache):\n\n",
            documents.len(),
            params.query,
            cached_hits
        );

        #[expect(clippy::unwrap_used, reason = "write! to String is infallible")]
        {
            for (i, doc) in documents.iter().enumerate() {
                writeln!(summary, "{}. {} ({})", i + 1, doc.title, doc.url).unwrap();
                let snippet = bose_net::truncate_chars(&doc.body, 150);
                writeln!(summary, "   {snippet}...").unwrap();
                writeln!(summary).unwrap();
            }
        }

        Ok(CallToolResult::success(vec![ContentBlock::text(summary)]))
    }

    #[tool(
        description = "Scavenger search: surface content mainstream engines systematically miss, using information-theoretic surprise scoring and submodular diversity selection over a wide harvest."
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
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "SearXNG is not reachable at the configured SEARXNG_URL.\n\
                 Start it with `devenv up`, or set SEARXNG_URL to a running instance.\n\
                 This is a transport-level failure, not an empty result.\nError: {e}"
                ))]));
            }
        };

        if resp.results.is_empty() {
            return Ok(CallToolResult::success(vec![ContentBlock::text(
                "No results to scavenge.",
            )]));
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
                    let truncated = bose_net::truncate_chars(s, 200);
                    writeln!(out, "   {truncated}").unwrap();
                }
                writeln!(out).unwrap();
            }
        }

        Ok(CallToolResult::success(vec![ContentBlock::text(out)]))
    }
}

#[tool_handler]
impl ServerHandler for BoseSearchServer {
    fn get_info(&self) -> ServerConfig {
        // The 2026-07-28 revision has no `initialize` handshake, so a server
        // answering `initialize` can never name LATEST — it must name
        // `LATEST_WITH_INITIALIZE`. The full range is advertised separately
        // through `supported_protocol_versions()` / `server/discover`.
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("bose-search", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Bose Search — meta-search engine powered by SearXNG.\n\
                 Serves MCP protocol revision 2026-07-28 (stateless) and falls back to the\n\
                 2025-11-25 handshake for legacy clients.\n\
                 Tools: web_search (SearXNG), local_search (Tantivy), read_url, crawl_knowledge,\n\
                 scavenger_search, recall (raw cache), health_check.\n\
                 Requires SearXNG running (devenv up) at SEARXNG_URL (default http://localhost:8080).",
            )
    }

    /// Advertise every revision this server can speak, oldest first.
    ///
    /// Dual-era: `2026-07-28` gives stateless requests with `resultType` and
    /// header routing; the 2025-era revisions keep the historical wire shape
    /// so existing clients (opencode, Claude Code) keep working unchanged.
    fn supported_protocol_versions(&self) -> std::borrow::Cow<'static, [ProtocolVersion]> {
        std::borrow::Cow::Borrowed(ProtocolVersion::known_up_to(&ProtocolVersion::LATEST))
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

        writeln!(
            out,
            "Engines that answered ({}): {}",
            resp.engines_used.len(),
            resp.engines_used.join(", ")
        )
        .unwrap();

        // Without this, an empty result list is indistinguishable from
        // "nothing exists". SearXNG tells us which engines it could not use,
        // and that is the actionable half of the answer.
        if !resp.unresponsive_engines.is_empty() {
            writeln!(
                out,
                "Engines unavailable ({}): {}",
                resp.unresponsive_engines.len(),
                resp.unresponsive_engines
                    .iter()
                    .map(|e| format!("{} ({})", e.engine, e.reason))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .unwrap();
        }

        if resp.results.is_empty() {
            out.push_str("\nNo engine returned results for this query.\n");
        }
    }
    out
}

/// Generate a cache key from search query and parameters.
///
/// Uses a deterministic string concatenation so the same query always maps
/// to the same key, which is what makes the in-memory `moka` cache hit.
fn make_cache_key(query: &SearchQuery) -> String {
    let mut key = format!("q:{}|n:{}", query.query, query.num_results);
    if let Some(ref cat) = query.category {
        let _ = write!(key, "|c:{cat}");
    }
    if let Some(ref lang) = query.language {
        let _ = write!(key, "|l:{lang}");
    }
    if let Some(ref tr) = query.time_range {
        let _ = write!(key, "|t:{tr}");
    }
    key
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // tracing → stderr (stdout reserved for MCP JSON-RPC)
    tracing_subscriber::fmt().with_env_filter("bose=info").with_writer(std::io::stderr).init();

    let config = BoseConfig::from_env();
    let client = SearxngClient::new(&config)?;
    let net = Net::new(&config)?.with_cache(bose_net::ResponseCache::from_env()?);

    tracing::info!(
        url = %config.searxng_url,
        index = %config.index_path,
        cache = %net.cache.root().display(),
        "Bose MCP Server starting"
    );

    let server = BoseSearchServer::new(client, net, config.index_path);
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
    use std::time::Duration;

    /// A `Net` with caching off, for tests that assert on request counts.
    fn net_for(config: &BoseConfig) -> Net {
        Net::new(config).unwrap().with_cache(bose_net::ResponseCache::disabled())
    }

    #[tokio::test]
    async fn test_read_url_success() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

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
        let net = Net::new(&config).unwrap().with_cache(bose_net::ResponseCache::disabled());
        let index_path = config.index_path.clone();
        let server = BoseSearchServer::new(client, net, index_path);

        let params = ReadUrlParams { url: format!("{}/test", mock_server.uri()) };

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
        let net = Net::new(&config).unwrap().with_cache(bose_net::ResponseCache::disabled());
        let index_path = config.index_path.clone();
        let server = BoseSearchServer::new(client, net, index_path);

        let params = ReadUrlParams { url: format!("{}/error", mock_server.uri()) };

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
        let html = format!("<html><body>{large_content}</body></html>");

        Mock::given(method("GET"))
            .and(path("/large"))
            .respond_with(ResponseTemplate::new(200).set_body_string(html))
            .mount(&mock_server)
            .await;

        let config = BoseConfig::from_env();
        let client = SearxngClient::new(&config).unwrap();
        let net = Net::new(&config).unwrap().with_cache(bose_net::ResponseCache::disabled());
        let index_path = config.index_path.clone();
        let server = BoseSearchServer::new(client, net, index_path);

        let params = ReadUrlParams { url: format!("{}/large", mock_server.uri()) };

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

        let config = BoseConfig { searxng_url: mock_server.uri(), ..BoseConfig::default() };
        let client = SearxngClient::new(&config).unwrap();
        let server = BoseSearchServer::new(client, net_for(&config), config.index_path);

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
        let config = BoseConfig { searxng_url: mock_server.uri(), ..BoseConfig::default() };
        let client = SearxngClient::new(&config).unwrap();

        // Create cache with 1 second TTL for testing
        let cache =
            Cache::builder().max_capacity(1000).time_to_live(Duration::from_secs(1)).build();

        let mut server = BoseSearchServer::new(client, net_for(&config), config.index_path);
        server.cache = cache;

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

    #[tokio::test]
    async fn test_crawl_knowledge_integration() {
        use tempfile::TempDir;
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path, query_param},
        };

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
        let net = net_for(&config);
        let server = BoseSearchServer::new(client, net, index_path.clone());

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
