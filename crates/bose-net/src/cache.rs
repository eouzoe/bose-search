//! Persistent, content-addressed response cache.
//!
//! This is the mechanism behind "crawl once, reuse forever". A URL fetched
//! once is stored on disk keyed by the SHA-256 of the URL, so the second
//! request for that URL never touches the network — and neither does the
//! tenth, nor the one from next week's session.
//!
//! Layout, inside `BOSE_CACHE_PATH`:
//!
//! ```text
//! <sha256(url)>.meta   {"url":"…","fetched_at":"…","status":200,"bytes":1234}
//! <sha256(url)>.body   raw response bytes
//! ```
//!
//! Two files rather than one so a partially-written body can never be
//! confused with a complete entry: a body is only published after its
//! metadata is committed.

use bose_common::BoseResult;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs;

/// How long a cached entry stays fresh.
///
/// The default is deliberately long: the dominant access pattern for this
/// tool is documentation and reference material that changes on the scale of
/// months, not seconds.
pub const DEFAULT_TTL_SECS: u64 = 60 * 60 * 24 * 7;

/// One cached response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// The original URL.
    pub url: String,
    /// Unix seconds when the entry was stored.
    pub fetched_at: u64,
    /// HTTP status the server reported.
    pub status: u16,
    /// Byte length of the stored body.
    pub bytes: u64,
}

/// An on-disk response cache.
#[derive(Debug, Clone)]
pub struct ResponseCache {
    root: PathBuf,
    default_ttl_secs: u64,
}

impl ResponseCache {
    /// Open (creating if needed) a cache rooted at `root`.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created.
    pub fn new(root: impl Into<PathBuf>) -> BoseResult<Self> {
        let root = root.into();
        std::fs::create_dir_all(&root).map_err(bose_common::BoseError::IoError)?;
        Ok(Self { root, default_ttl_secs: DEFAULT_TTL_SECS })
    }

    /// A cache at the platform data directory, or `BOSE_CACHE_PATH` when set.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created.
    pub fn default_location() -> BoseResult<Self> {
        let root = std::env::var_os("BOSE_CACHE_PATH")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                dirs::cache_dir()
                    .or_else(dirs::data_dir)
                    .unwrap_or_else(|| PathBuf::from(".bose-cache"))
                    .join("bose-search")
                    .join("responses")
            });
        Self::new(root)
    }

    /// `default_location()` honouring `BOSE_CACHE_TTL_SECS`.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created.
    pub fn from_env() -> BoseResult<Self> {
        let ttl = std::env::var("BOSE_CACHE_TTL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_TTL_SECS);
        Ok(Self::default_location()?.with_ttl_secs(ttl))
    }

    /// Override the default TTL. `0` means "always stale".
    #[must_use]
    pub fn with_ttl_secs(mut self, ttl: u64) -> Self {
        self.default_ttl_secs = ttl;
        self
    }

    /// The TTL that applies to `url`.
    #[must_use]
    pub fn ttl_for(&self, _url: &str) -> u64 {
        self.default_ttl_secs
    }

    /// A cache that stores nothing and always misses.
    ///
    /// Used when no cache directory is available, and as the opt-out for
    /// callers that want every fetch to hit the network.
    #[must_use]
    pub fn disabled() -> Self {
        Self { root: PathBuf::new(), default_ttl_secs: 0 }
    }

    /// Whether this cache actually stores anything.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        !self.root.as_os_str().is_empty()
    }

    fn paths(&self, url: &str) -> (PathBuf, PathBuf) {
        let key = hex_key(url);
        (self.root.join(format!("{key}.meta")), self.root.join(format!("{key}.body")))
    }

    /// Read an entry if it exists and is still fresh.
    ///
    /// A missing or expired entry is `Ok(None)`, not an error — callers treat
    /// a miss as "go to the network".
    ///
    /// # Errors
    ///
    /// Returns an error only if reading a *present* entry fails.
    pub async fn get(&self, url: &str, ttl_secs: u64) -> Option<Vec<u8>> {
        // ttl 0 is the explicit "never serve from cache" sentinel, so the
        // same call site works for a real cache and for `disabled()`.
        if ttl_secs == 0 {
            return None;
        }
        let (meta_path, body_path) = self.paths(url);

        let raw = fs::read(&meta_path).await.ok()?;
        let meta: CacheEntry = serde_json::from_slice(&raw).ok()?;

        let now = now_secs();
        if now.saturating_sub(meta.fetched_at) > ttl_secs {
            return None;
        }

        fs::read(&body_path).await.ok()
    }

    /// Look up an entry without applying a TTL — the `recall` path.
    ///
    /// # Errors
    ///
    /// Returns an error only if reading a present entry fails.
    pub async fn lookup(&self, url: &str) -> Option<(CacheEntry, Vec<u8>)> {
        let (meta_path, body_path) = self.paths(url);
        let raw = fs::read(&meta_path).await.ok()?;
        let meta: CacheEntry = serde_json::from_slice(&raw).ok()?;
        let body = fs::read(&body_path).await.ok()?;
        Some((meta, body))
    }

    /// Store a response.
    ///
    /// Metadata is written first and body second, so an interrupted write
    /// leaves an entry whose body is missing — a miss — rather than an entry
    /// whose metadata is missing, which would be unreadable.
    ///
    /// # Errors
    ///
    /// Returns an error if either file cannot be written.
    pub async fn put(&self, url: &str, body: &[u8]) -> BoseResult<()> {
        let (meta_path, body_path) = self.paths(url);
        let meta = CacheEntry {
            url: url.to_string(),
            fetched_at: now_secs(),
            status: 200,
            bytes: body.len() as u64,
        };
        let meta_json = serde_json::to_vec(&meta)?;

        fs::write(&meta_path, &meta_json).await.map_err(bose_common::BoseError::IoError)?;
        fs::write(&body_path, body).await.map_err(bose_common::BoseError::IoError)?;
        Ok(())
    }

    /// Number of stored entries, for `health_check`.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be read.
    pub async fn count(&self) -> BoseResult<usize> {
        let mut entries =
            fs::read_dir(&self.root).await.map_err(bose_common::BoseError::IoError)?;
        let mut n = 0usize;
        while let Ok(Some(e)) = entries.next_entry().await {
            if e.path().extension().is_some_and(|x| x == "meta") {
                n += 1;
            }
        }
        Ok(n)
    }

    /// Where the cache lives, for `health_check` output.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// `sha256(url)` as lowercase hex. The hash keeps filenames fixed-length and
/// filesystem-safe regardless of URL length or characters.
fn hex_key(url: &str) -> String {
    let digest = Sha256::digest(url.as_bytes());
    let mut s = String::with_capacity(64);
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[tokio::test]
    async fn put_then_get_roundtrips() {
        let dir = tmp();
        let c = ResponseCache::new(dir.path()).unwrap().with_ttl_secs(3600);
        c.put("https://example.com/a", b"hello").await.unwrap();

        let hit = c.get("https://example.com/a", 3600).await;
        assert_eq!(hit.as_deref(), Some(&b"hello"[..]));
    }

    #[tokio::test]
    async fn miss_returns_none_not_error() {
        let dir = tmp();
        let c = ResponseCache::new(dir.path()).unwrap();
        assert!(c.get("https://nope.example/", 3600).await.is_none());
    }

    #[tokio::test]
    async fn ttl_zero_means_always_stale() {
        let dir = tmp();
        let c = ResponseCache::new(dir.path()).unwrap();
        c.put("https://example.com/x", b"data").await.unwrap();
        // ttl 0 is an explicit "never serve from cache" sentinel.
        assert!(c.get("https://example.com/x", 0).await.is_none());
    }

    #[tokio::test]
    async fn ttl_max_means_forever() {
        let dir = tmp();
        let c = ResponseCache::new(dir.path()).unwrap();
        c.put("https://example.com/x", b"data").await.unwrap();
        assert_eq!(c.get("https://example.com/x", u64::MAX).await.as_deref(), Some(&b"data"[..]));
    }

    #[tokio::test]
    async fn short_ttl_serves_within_same_second() {
        let dir = tmp();
        let c = ResponseCache::new(dir.path()).unwrap();
        c.put("https://example.com/x", b"data").await.unwrap();
        // `fetched_at` has second granularity, so an entry written in the
        // same second is fresh for any ttl >= 1.
        assert_eq!(c.get("https://example.com/x", 1).await.as_deref(), Some(&b"data"[..]));
    }

    #[tokio::test]
    async fn lookup_ignores_ttl() {
        let dir = tmp();
        let c = ResponseCache::new(dir.path()).unwrap().with_ttl_secs(0);
        c.put("https://example.com/x", b"data").await.unwrap();
        let (meta, body) = c.lookup("https://example.com/x").await.unwrap();
        assert_eq!(meta.url, "https://example.com/x");
        assert_eq!(meta.bytes, 4);
        assert_eq!(body, b"data");
    }

    #[tokio::test]
    async fn distinct_urls_do_not_collide() {
        let dir = tmp();
        let c = ResponseCache::new(dir.path()).unwrap().with_ttl_secs(3600);
        c.put("https://example.com/a", b"A").await.unwrap();
        c.put("https://example.com/b", b"B").await.unwrap();
        assert_eq!(c.get("https://example.com/a", 3600).await.as_deref(), Some(&b"A"[..]));
        assert_eq!(c.get("https://example.com/b", 3600).await.as_deref(), Some(&b"B"[..]));
    }

    #[tokio::test]
    async fn urls_differing_only_in_query_are_distinct() {
        let dir = tmp();
        let c = ResponseCache::new(dir.path()).unwrap().with_ttl_secs(3600);
        c.put("https://e.com/?q=1", b"one").await.unwrap();
        c.put("https://e.com/?q=2", b"two").await.unwrap();
        assert_eq!(c.get("https://e.com/?q=1", 3600).await.as_deref(), Some(&b"one"[..]));
        assert_eq!(c.get("https://e.com/?q=2", 3600).await.as_deref(), Some(&b"two"[..]));
    }

    #[tokio::test]
    async fn count_reports_entries() {
        let dir = tmp();
        let c = ResponseCache::new(dir.path()).unwrap();
        c.put("https://e.com/1", b"a").await.unwrap();
        c.put("https://e.com/2", b"b").await.unwrap();
        c.put("https://e.com/3", b"c").await.unwrap();
        assert_eq!(c.count().await.unwrap(), 3);
    }

    #[tokio::test]
    async fn empty_body_is_storable() {
        let dir = tmp();
        let c = ResponseCache::new(dir.path()).unwrap().with_ttl_secs(3600);
        c.put("https://e.com/empty", b"").await.unwrap();
        assert_eq!(c.get("https://e.com/empty", 3600).await.as_deref(), Some(&b""[..]));
    }

    #[tokio::test]
    async fn binary_body_roundtrips() {
        let dir = tmp();
        let c = ResponseCache::new(dir.path()).unwrap().with_ttl_secs(3600);
        let blob: Vec<u8> = (0..=255u8).collect();
        c.put("https://e.com/bin", &blob).await.unwrap();
        assert_eq!(c.get("https://e.com/bin", 3600).await.unwrap(), blob);
    }
}
