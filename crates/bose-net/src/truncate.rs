//! UTF-8-safe truncation.
//!
//! The previous implementation used `&text[..MAX_SIZE]`, which panics on any
//! multi-byte boundary — i.e. on virtually every page containing CJK text,
//! emoji, or accented Latin. The panic happens inside an MCP tool handler,
//! which kills the whole server process.

/// Truncate to at most `max_bytes`, never splitting a UTF-8 code point.
///
/// Returns the longest prefix of `s` whose byte length is `<= max_bytes` and
/// ends on a char boundary. No allocation unless `ellipsis` is appended.
#[must_use]
pub fn truncate_chars(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Walk back to a char boundary. At most 3 steps for UTF-8.
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Truncate to at most `max_bytes` and append `"… [truncated]"` if cut.
///
/// If the budget is too small to hold the marker, a bare truncation is
/// returned instead — the length bound always wins over the marker.
#[must_use]
pub fn truncate_with_marker(s: &str, max_bytes: usize) -> String {
    const MARKER: &str = "… [truncated]";
    if s.len() <= max_bytes {
        return s.to_string();
    }
    match max_bytes.checked_sub(MARKER.len()) {
        Some(budget) => format!("{}{MARKER}", truncate_chars(s, budget)),
        // Budget smaller than the marker itself: honour the byte bound only.
        None => truncate_chars(s, max_bytes).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_short_input_is_verbatim() {
        assert_eq!(truncate_chars("hello", 10), "hello");
    }

    #[test]
    fn ascii_cut_respects_limit() {
        assert_eq!(truncate_chars("hello world", 5), "hello");
    }

    #[test]
    fn never_returns_non_boundary_slice_on_emoji() {
        // "😀" is 4 bytes; cutting at 5 must land on the boundary at 4.
        let s = "😀😀😀";
        let cut = truncate_chars(s, 5);
        assert_eq!(cut, "😀");
        assert!(cut.is_char_boundary(cut.len()));
    }

    #[test]
    fn cjk_is_safe_at_every_offset() {
        let s = "中文測試文章內容很長";
        for i in 0..=s.len() {
            // Must not panic at any byte offset.
            let _ = truncate_chars(s, i);
        }
        // 3 bytes per CJK char: 7 bytes → 2 chars.
        assert_eq!(truncate_chars(s, 7), "中文");
    }

    #[test]
    fn mixed_content_is_safe() {
        let s = "héllo 世界 😀 tail";
        for i in 0..=s.len() {
            let _ = truncate_chars(s, i);
        }
    }

    #[test]
    fn marker_only_added_when_cut() {
        assert_eq!(truncate_with_marker("short", 100), "short");
        // Exactly at the limit: nothing was cut, so no marker.
        assert_eq!(truncate_with_marker("abcdefghij", 10), "abcdefghij");
    }

    #[test]
    fn marker_added_when_cut_and_it_fits() {
        // 30 bytes of input, 20-byte budget: 20 - 15-byte marker = 5 kept.
        let out = truncate_with_marker("abcdefghijklmnopqrstuvwxyz0123", 20);
        assert_eq!(out, "abcde… [truncated]");
        assert_eq!(out.len(), 20);
    }

    #[test]
    fn budget_smaller_than_marker_falls_back_to_bare_cut() {
        // The byte bound always wins over the marker.
        let out = truncate_with_marker("abcdefghijk", 10);
        assert_eq!(out, "abcdefghij");
        assert_eq!(out.len(), 10);
    }

    #[test]
    fn marker_stays_within_budget_on_cjk() {
        let s = "中文測試文章內容很長的中文內容";
        let out = truncate_with_marker(s, 20);
        assert!(out.len() <= 20, "got {} bytes", out.len());
        assert!(out.ends_with("… [truncated]"));
    }
}
