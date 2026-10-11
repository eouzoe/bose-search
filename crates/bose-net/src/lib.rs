//! Shared HTTP client, HTML extraction, UTF-8-safe truncation, and a
//! persistent on-disk response cache.
//!
//! `bose-net` exists so that every call site in the server shares one
//! connection pool, one builder, one extractor, and one cache. Before this
//! crate, `read_url`, `crawl_knowledge`, and `SearxngClient` each constructed
//! their own `reqwest::Client`, which threw away TLS sessions and connection
//! pool state on every single tool call.

use bose_common::{BoseConfig, BoseResult};
use std::sync::Arc;
use std::time::Duration;

mod cache;
mod extract;
mod truncate;

pub use cache::{CacheEntry, ResponseCache};
pub use extract::{extract_text, extract_title};
pub use truncate::{truncate_chars, truncate_with_marker};

/// A shared, pre-configured HTTP client.
///
/// Wrap it in an [`Arc`] and clone freely — `reqwest::Client` is itself an
/// `Arc`-based handle, so cloning is cheap and keeps the underlying pool.
#[derive(Clone)]
pub struct Net {
    client: reqwest::Client,
    pub cache: Arc<ResponseCache>,
}

impl Net {
    /// Build the shared client from configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying `reqwest::Client` cannot be built.
    pub fn new(config: &BoseConfig) -> BoseResult<Self> {
        let client = reqwest::Client::builder()
            // Keep-alive matters more than it looks: without it every tool
            // call pays a fresh TCP + TLS handshake to SearXNG.
            .tcp_keepalive(Duration::from_secs(60))
            .pool_idle_timeout(Duration::from_secs(90))
            .timeout(Duration::from_secs(config.request_timeout_secs))
            .user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(bose_common::BoseError::HttpError)?;

        Ok(Self { client, cache: Arc::new(ResponseCache::disabled()) })
    }

    /// The inner `reqwest::Client`, for call sites that need full control.
    #[must_use]
    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// Attach a persistent on-disk cache, so a URL fetched once is never
    /// fetched again until its TTL expires.
    #[must_use]
    pub fn with_cache(mut self, cache: ResponseCache) -> Self {
        self.cache = Arc::new(cache);
        self
    }

    /// Fetch a URL as text, consulting (and populating) the response cache.
    ///
    /// This is the single entry point for every outbound page fetch in the
    /// server. De-duplication and TTL live here rather than at each call site.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the status is not successful.
    pub async fn fetch_text(&self, url: &str) -> BoseResult<FetchedPage> {
        let ttl = self.cache.ttl_for(url);
        if ttl == 0 {
            // TTL 0 is the "bypass the cache" sentinel, matching
            // `ResponseCache::disabled()`.
            return self.fetch_uncached(url).await;
        }
        if let Some(bytes) = self.cache.get(url, ttl).await {
            return Ok(FetchedPage { status: 200, bytes, from_cache: true });
        }
        self.fetch_uncached(url).await
    }

    /// Fetch without consulting or populating the cache.
    async fn fetch_uncached(&self, url: &str) -> BoseResult<FetchedPage> {
        let response = self.client.get(url).send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(bose_common::BoseError::SearxngError(format!("HTTP {status}")));
        }
        let bytes = response.bytes().await?.to_vec();
        if self.cache.is_enabled() {
            let _ =
                self.cache.put(url, &bytes).await.inspect_err(
                    |e| tracing::warn!(error = %e, "Failed to store response in cache"),
                );
        }

        Ok(FetchedPage { status: status.as_u16(), bytes, from_cache: false })
    }
}

/// A page fetched from the network, or served from the local cache.
#[derive(Debug, Clone)]
pub struct FetchedPage {
    /// HTTP status code (200 for cache hits, which only ever store 200s).
    pub status: u16,
    /// Raw response bytes, before decoding or extraction.
    pub bytes: Vec<u8>,
    /// Whether this came from the on-disk cache rather than the network.
    pub from_cache: bool,
}

impl FetchedPage {
    /// Decode the bytes to a `String` using the declared charset, falling
    /// back to `chardetng` detection, then UTF-8.
    ///
    /// A page declaring no charset is the common case on the small-web end
    /// of the long tail, so detection is not optional.
    #[must_use]
    pub fn decode(&self) -> String {
        decode_bytes(&self.bytes)
    }

    /// Extract readable text from the decoded page.
    #[must_use]
    pub fn text(&self) -> String {
        extract_text(&self.decode())
    }
}

/// Decode raw bytes into a `String`.
///
/// Resolution order: BOM → declared `<meta charset>` / `Content-Type` →
/// `chardetng` statistical detection → UTF-8 lossy. Never panics.
#[must_use]
pub fn decode_bytes(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&bytes[3..]).into_owned();
    }
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return String::from_utf8_lossy(bytes).into_owned();
    }

    let (declared, sniff_len) = sniff_charset(bytes);
    let enc = declared
        .as_deref()
        .and_then(|name| encoding_rs::Encoding::for_label(name.as_bytes()))
        .unwrap_or_else(|| {
            let mut det = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
            det.feed(&bytes[..sniff_len.min(bytes.len())], true);
            det.guess(None, chardetng::Utf8Detection::Allow)
        });

    let (cow, _, _) = enc.decode(bytes);
    cow.into_owned()
}

/// Find a charset declared in the first `sniff_len` bytes of the document.
///
/// Only the head is scanned, which is the only place a declaration is legal,
/// and keeps this cheap on multi-megabyte pages.
fn sniff_charset(bytes: &[u8]) -> (Option<String>, usize) {
    const SNIFF_LEN: usize = 4096;
    let end = SNIFF_LEN.min(bytes.len());

    // Look for the lowercase form on an ASCII-folded copy so we do not need
    // to allocate a real lowercase String.
    let head = &bytes[..end];
    let lowered: Vec<u8> = head.iter().map(u8::to_ascii_lowercase).collect();
    let text = String::from_utf8_lossy(&lowered);

    for marker in ["charset=", "charset=\""] {
        if let Some(pos) = text.find(marker) {
            let rest = &text[pos + marker.len()..];
            let stop = rest.find(['"', '\'', ';', '/', '>']).unwrap_or(rest.len());
            let name = rest[..stop].trim();
            if !name.is_empty() {
                return (Some(name.to_string()), end);
            }
        }
    }
    (None, end)
}
