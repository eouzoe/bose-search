use bose_common::*;
use bose_index::searcher::Searcher;
use bose_searxng::SearxngClient;
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt, handler::server::tool::ToolRouter,
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_handler, tool_router,
    transport::stdio,
};
use std::fmt::Write;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
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

#[derive(Clone)]
struct BoseSearchServer {
    client: SearxngClient,
    index_path: String,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl BoseSearchServer {
    fn new(client: SearxngClient, index_path: String) -> Self {
        Self {
            client,
            index_path,
            tool_router: Self::tool_router(),
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

        match self.client.search(&query).await {
            Ok(resp) => Ok(CallToolResult::success(vec![Content::text(
                format_response(&resp),
            )])),
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
