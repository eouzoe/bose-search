//! Tantivy schema 定義
//!
//! 定義本地知識庫的索引結構。

use tantivy::schema::*;

/// 創建 Tantivy schema
///
/// # Schema 結構
///
/// - `title`: 文檔標題 (全文搜尋 + 儲存)
/// - `body`: 文檔內容 (全文搜尋 + 儲存)
/// - `url`: 文檔 URL (字串 + 儲存)
/// - `source`: 來源 (github/stackoverflow/docs.rs) (字串 + 儲存 + FAST)
/// - `language`: 程式語言 (rust/python/go) (字串 + FAST)
/// - `crawled_at`: 爬取時間 (日期 + 索引 + 儲存 + FAST)
/// - `relevance_score`: 相關性分數 (f64 + 索引 + FAST)
/// - `tags`: 標籤 (字串 + 儲存)
///
/// # 範例
///
/// ```
/// use bose_index::schema::create_schema;
///
/// let schema = create_schema();
/// assert_eq!(schema.fields().count(), 8);
/// ```
pub fn create_schema() -> Schema {
    let mut builder = Schema::builder();

    // 全文搜尋欄位
    builder.add_text_field("title", TEXT | STORED);
    builder.add_text_field("body", TEXT | STORED);
    builder.add_text_field("url", STRING | STORED);

    // 元資料欄位
    builder.add_text_field("source", STRING | STORED | FAST);
    builder.add_text_field("language", STRING | STORED | FAST);
    builder.add_date_field("crawled_at", INDEXED | STORED | FAST);
    builder.add_f64_field("relevance_score", INDEXED | STORED | FAST);

    // 標籤 (多值)
    builder.add_text_field("tags", STRING | STORED);

    builder.build()
}

/// Schema 欄位名稱常數
pub mod fields {
    pub const TITLE: &str = "title";
    pub const BODY: &str = "body";
    pub const URL: &str = "url";
    pub const SOURCE: &str = "source";
    pub const LANGUAGE: &str = "language";
    pub const CRAWLED_AT: &str = "crawled_at";
    pub const RELEVANCE_SCORE: &str = "relevance_score";
    pub const TAGS: &str = "tags";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_schema() {
        let schema = create_schema();
        assert_eq!(schema.fields().count(), 8);
    }

    #[test]
    fn test_schema_has_required_fields() {
        let schema = create_schema();

        // 檢查所有必要欄位都存在
        assert!(schema.get_field(fields::TITLE).is_ok());
        assert!(schema.get_field(fields::BODY).is_ok());
        assert!(schema.get_field(fields::URL).is_ok());
        assert!(schema.get_field(fields::SOURCE).is_ok());
        assert!(schema.get_field(fields::LANGUAGE).is_ok());
        assert!(schema.get_field(fields::CRAWLED_AT).is_ok());
        assert!(schema.get_field(fields::RELEVANCE_SCORE).is_ok());
        assert!(schema.get_field(fields::TAGS).is_ok());
    }

    #[test]
    fn test_field_types() {
        let schema = create_schema();

        let title_field = schema.get_field(fields::TITLE).unwrap();
        let title_entry = schema.get_field_entry(title_field);
        assert!(title_entry.is_indexed());

        let url_field = schema.get_field(fields::URL).unwrap();
        let url_entry = schema.get_field_entry(url_field);
        assert!(url_entry.is_stored());
    }
}
