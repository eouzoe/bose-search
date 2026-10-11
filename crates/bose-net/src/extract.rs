//! DOM-based HTML → text extraction.
//!
//! Replaces the previous hand-rolled character scanner, which had four real
//! defects: it did not decode HTML entities, it did not turn `<br>` into
//! whitespace, its state machine desynchronised when `>` appeared inside an
//! attribute value, and it had no notion of block boundaries at all.

use scraper::{ElementRef, Html, Node};

/// Tags whose entire subtree is dropped.
const DROPPED: &[&str] = &[
    "script", "style", "noscript", "template", "svg", "math", "iframe", "object", "embed",
    "canvas", "audio", "video", "source", "track", "map", "area", "form", "input", "button",
    "select", "textarea", "option", "datalist", "canvas",
];

/// Tags that imply a line break before their content.
const BLOCK: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "body",
    "br",
    "caption",
    "dd",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "html",
    "li",
    "main",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
    "ul",
];

/// Extract readable text from an HTML document.
///
/// Walks the parsed DOM, skipping non-visible subtrees, and emits whitespace
/// at block boundaries so words from adjacent elements do not run together.
#[must_use]
pub fn extract_text(html: &str) -> String {
    let doc = Html::parse_document(html);
    let mut out = String::with_capacity(html.len() / 4);
    let mut visited = 0usize;
    // `root_element()` is the `<html>` ElementRef; `tree.root()` is the
    // Document node, which `ElementRef::wrap` refuses to wrap.
    walk(doc.root_element(), &mut out, &mut visited);
    collapse_whitespace(&out)
}

/// Extract the document title.
#[must_use]
pub fn extract_title(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = scraper::Selector::parse("title").ok()?;
    let el = doc.select(&sel).next()?;
    let text: String = el.text().collect();
    let text = text.trim();
    if text.is_empty() { None } else { Some(collapse_whitespace(text)) }
}

/// Recursive pre-order walk over `element`'s subtree, appending visible text.
///
/// Uses `ElementRef`/`NodeRef` — scraper's public API — rather than reaching
/// into the internal `ego_tree`.
fn walk(element: ElementRef, out: &mut String, visited: &mut usize) {
    // Guard against pathologically nested documents.
    *visited += 1;
    if *visited > 200_000 {
        return;
    }

    for child in element.children() {
        match child.value() {
            Node::Text(text) => {
                let t = text.text.as_ref();
                if !t.trim().is_empty() {
                    out.push_str(t);
                    out.push(' ');
                }
            }
            Node::Element(el) => {
                let lowered = el.name.local.as_ref().to_ascii_lowercase();
                if DROPPED.iter().any(|d| *d == lowered) {
                    continue;
                }
                if BLOCK.iter().any(|b| *b == lowered) {
                    out.push('\n');
                }
                if let Some(inner) = ElementRef::wrap(child) {
                    walk(inner, out, visited);
                }
            }
            Node::Document
            | Node::Comment(_)
            | Node::ProcessingInstruction(_)
            | Node::Doctype(_)
            | Node::Fragment => {}
        }
    }
}

/// Collapse runs of whitespace to a single space and trim.
fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            pending_space = true;
        } else {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tags_and_entities() {
        // The old scanner produced "Tom &amp; Jerry" literally.
        let out = extract_text("<p>Tom &amp; Jerry</p>");
        assert_eq!(out, "Tom & Jerry");
    }

    #[test]
    fn decodes_numeric_and_named_entities() {
        assert_eq!(extract_text("<p>&lt;b&gt; &quot;q&quot; &#39;s&#39;</p>"), "<b> \"q\" 's'");
        assert_eq!(extract_text("<p>&copy; 2026 &mdash; libre</p>"), "© 2026 — libre");
    }

    #[test]
    fn br_becomes_a_separator_not_a_glue() {
        // Old scanner produced "onetwo"; now "one two".
        assert_eq!(extract_text("<p>one<br>two</p>"), "one two");
    }

    #[test]
    fn blocks_do_not_run_together() {
        let out = extract_text("<div>alpha</div><div>beta</div>");
        assert_eq!(out, "alpha beta");
    }

    #[test]
    fn script_and_style_are_dropped() {
        let out =
            extract_text("<p>keep</p><script>var x = 'drop';</script><style>p{color:red}</style>");
        assert_eq!(out, "keep");
    }

    #[test]
    fn gt_inside_attribute_does_not_desync() {
        // The old state machine treated the `>` inside the attribute as the
        // end of the tag and then leaked the rest of the page as text.
        let out = extract_text(r#"<a title="a > b">link text</a><p>after</p>"#);
        assert_eq!(out, "link text after");
    }

    #[test]
    fn unclosed_tags_do_not_loop_forever() {
        let out = extract_text("<div><div><div><p>deep</p>");
        assert_eq!(out, "deep");
    }

    #[test]
    fn empty_and_whitespace_documents() {
        assert_eq!(extract_text(""), "");
        assert_eq!(extract_text("   \n\t  "), "");
    }

    #[test]
    fn title_extraction() {
        assert_eq!(extract_title("<title>Hello &amp; bye</title>").as_deref(), Some("Hello & bye"));
        assert_eq!(extract_title("<title>  </title>"), None);
        assert_eq!(extract_title("<p>no title</p>"), None);
    }

    #[test]
    fn cjk_content_is_preserved() {
        assert_eq!(extract_text("<p>中文內容測試</p>"), "中文內容測試");
    }
}
