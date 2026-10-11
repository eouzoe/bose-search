use thiserror::Error;

#[derive(Error, Debug)]
pub enum BoseError {
    #[error("SearXNG 請求失敗: {0}")]
    SearxngError(String),

    #[error("HTTP 請求失敗: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("JSON 解析失敗: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("配置錯誤: {0}")]
    ConfigError(String),

    #[error("查詢無效: {0}")]
    InvalidQuery(String),

    #[error("IO 失敗: {0}")]
    IoError(#[from] std::io::Error),
}

pub type BoseResult<T> = Result<T, BoseError>;
