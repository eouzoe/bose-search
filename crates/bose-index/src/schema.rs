//! Tantivy schema definition.
//!
//! 定義本地知識庫的索引結構。

use tantivy::schema::{FAST, INDEXED, STORED, STRING, Schema, TEXT};

/// Create the Tantivy schema.
///
/// # Schema layout
///
/// - `title`: document title (full-text indexed + stored)
/// - `body`: document body (full-text indexed + stored)
/// - `url`: document URL (string + stored)
/// - `source`: origin (github/stackoverflow/docs.rs) (string + stored + FAST)
/// - `language`: programming language (rust/python/go) (string + FAST)
/// - `crawled_at`: crawl timestamp (date + indexed + stored + FAST)
/// - `relevance_score`: relevance score (f64 + indexed + FAST)
/// - `tags`: tags (string + stored)
///
/// # Examples
///
/// ```
/// use bose_index::schema::create_schema;
///
/// let schema = create_schema();
/// assert_eq!(schema.fields().count(), 8);
/// ```
#[must_use]
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

/// Schema field name constants.
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
    #![expect(clippy::unwrap_used, reason = "test code")]

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
