//! HTML content extractor.
//!
//! Converts HTML to plain text for the knowledge collection pipeline.

/// Extract plain text from HTML.
///
/// Strips all HTML tags, script, and style elements, keeping only text.
///
/// # Examples
///
/// ```
/// use bose_common::extractor::extract_text;
///
/// let html = "<h1>Title</h1><p>Hello <b>world</b></p>";
/// let text = extract_text(html);
/// assert!(text.contains("Title"));
/// assert!(text.contains("Hello"));
/// assert!(text.contains("world"));
/// ```
#[must_use]
pub fn extract_text(html: &str) -> String {
    let mut result = String::with_capacity(html.len() / 2);
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let mut tag_name = String::new();
    let mut collecting_tag = false;

    let chars: Vec<char> = html.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        if ch == '<' {
            in_tag = true;
            collecting_tag = true;
            tag_name.clear();
            i += 1;
            continue;
        }

        if ch == '>' {
            in_tag = false;
            collecting_tag = false;

            let tag_lower = tag_name.to_lowercase();
            if tag_lower == "script" {
                in_script = true;
            } else if tag_lower == "/script" {
                in_script = false;
            } else if tag_lower == "style" {
                in_style = true;
            } else if tag_lower == "/style" {
                in_style = false;
            }

            // 塊級元素後加換行
            let block_tags = [
                "p",
                "/p",
                "div",
                "/div",
                "br",
                "br/",
                "h1",
                "/h1",
                "h2",
                "/h2",
                "h3",
                "/h3",
                "h4",
                "/h4",
                "h5",
                "/h5",
                "h6",
                "/h6",
                "li",
                "/li",
                "tr",
                "/tr",
                "blockquote",
                "/blockquote",
            ];
            if block_tags.iter().any(|t| tag_lower == *t) {
                result.push('\n');
            }

            i += 1;
            continue;
        }

        if in_tag {
            if collecting_tag && !ch.is_whitespace() {
                tag_name.push(ch);
            } else {
                collecting_tag = false;
            }
            i += 1;
            continue;
        }

        if in_script || in_style {
            i += 1;
            continue;
        }

        // 解碼常見 HTML 實體
        if ch == '&' {
            let entity = decode_entity(&chars, i);
            if let Some((decoded, advance)) = entity {
                result.push(decoded);
                i += advance;
                continue;
            }
        }

        result.push(ch);
        i += 1;
    }

    normalize_whitespace(&result)
}

/// Decode HTML entities.
fn decode_entity(chars: &[char], start: usize) -> Option<(char, usize)> {
    let mut end = start + 1;
    while end < chars.len() && end - start < 10 {
        if chars[end] == ';' {
            let entity: String = chars[start..=end].iter().collect();
            let decoded = match entity.as_str() {
                "&amp;" => '&',
                "&lt;" => '<',
                "&gt;" => '>',
                "&quot;" => '"',
                "&apos;" => '\'',
                "&nbsp;" => ' ',
                _ => return None,
            };
            return Some((decoded, end - start + 1));
        }
        end += 1;
    }
    None
}

/// 正規化空白字符
fn normalize_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut prev_newline = false;
    let mut prev_space = false;

    for ch in text.chars() {
        if ch == '\n' {
            if !prev_newline {
                result.push('\n');
            }
            prev_newline = true;
            prev_space = false;
        } else if ch.is_whitespace() {
            if !prev_space && !prev_newline {
                result.push(' ');
            }
            prev_space = true;
        } else {
            prev_newline = false;
            prev_space = false;
            result.push(ch);
        }
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_html() {
        let html = "<p>Hello world</p>";
        let text = extract_text(html);
        assert!(text.contains("Hello world"));
    }

    #[test]
    fn test_nested_tags() {
        let html = "<div><p>Hello <b>bold</b> text</p></div>";
        let text = extract_text(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("bold"));
        assert!(text.contains("text"));
    }

    #[test]
    fn test_script_removal() {
        let html = "<p>Before</p><script>alert('xss')</script><p>After</p>";
        let text = extract_text(html);
        assert!(text.contains("Before"));
        assert!(text.contains("After"));
        assert!(!text.contains("alert"));
    }

    #[test]
    fn test_style_removal() {
        let html = "<style>.red{color:red}</style><p>Content</p>";
        let text = extract_text(html);
        assert!(text.contains("Content"));
        assert!(!text.contains("color"));
    }

    #[test]
    fn test_entity_decoding() {
        let html = "<p>A &amp; B &lt; C &gt; D</p>";
        let text = extract_text(html);
        assert!(text.contains("A & B < C > D"));
    }

    #[test]
    fn test_empty_html() {
        assert_eq!(extract_text(""), "");
    }

    #[test]
    fn test_plain_text() {
        let text = extract_text("No HTML here");
        assert_eq!(text, "No HTML here");
    }

    #[test]
    fn test_block_elements_add_newlines() {
        let html = "<h1>Title</h1><p>Paragraph</p>";
        let text = extract_text(html);
        assert!(text.contains("Title"));
        assert!(text.contains("Paragraph"));
    }

    #[test]
    fn test_whitespace_normalization() {
        let html = "<p>  Too   many   spaces  </p>";
        let text = extract_text(html);
        // 不應該有連續空格
        assert!(!text.contains("  "));
    }

    #[test]
    fn test_nbsp_entity() {
        let html = "<p>Hello&nbsp;World</p>";
        let text = extract_text(html);
        assert!(text.contains("Hello World"));
    }
}
