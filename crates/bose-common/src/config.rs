/// 全域配置
#[derive(Debug, Clone)]
pub struct BoseConfig {
    pub searxng_url: String,
    pub default_num_results: u32,
    pub request_timeout_secs: u64,
    pub index_path: String,
}

impl Default for BoseConfig {
    fn default() -> Self {
        Self {
            searxng_url: "http://localhost:8080".to_string(),
            default_num_results: 10,
            request_timeout_secs: 30,
            index_path: dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("bose-search")
                .join("index")
                .to_string_lossy()
                .into_owned(),
        }
    }
}

impl BoseConfig {
    #[must_use]
    pub fn from_env() -> Self {
        let default = Self::default();
        Self {
            searxng_url: std::env::var("SEARXNG_URL")
                .unwrap_or_else(|_| "http://localhost:8080".to_string()),
            default_num_results: std::env::var("DEFAULT_NUM_RESULTS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            request_timeout_secs: std::env::var("REQUEST_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
            index_path: std::env::var("BOSE_INDEX_PATH").unwrap_or(default.index_path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let c = BoseConfig::default();
        assert_eq!(c.searxng_url, "http://localhost:8080");
        assert_eq!(c.default_num_results, 10);
        assert_eq!(c.request_timeout_secs, 30);
    }

    #[test]
    fn test_config_from_env() {
        let c = BoseConfig::from_env();
        assert_eq!(c.default_num_results, 10);
    }
}
