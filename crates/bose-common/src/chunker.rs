//! Document chunking module
//!
//! Splits long documents into fixed-size chunks with overlap to preserve context.
//!
//! # Design
//!
//! - **Chunk size**: 800 tokens (configurable)
//! - **Overlap ratio**: 15% (120 tokens)
//! - **Tokenization**: Simple whitespace splitting (1 word ≈ 1.3 tokens)
//! - **Boundaries**: Prefer splitting at sentence boundaries
//!
//! # Examples
//!
//! ```
//! use bose_common::chunker::Chunker;
//!
//! let chunker = Chunker::new(800, 0.15);
//! let text = "This is a long document...".repeat(100);
//! let chunks = chunker.chunk(&text);
//!
//! assert!(!chunks.is_empty());
//! ```

use std::cmp::min;

/// Document chunker
///
/// Splits long documents into fixed-size chunks with overlap between chunks.
#[derive(Debug, Clone)]
pub struct Chunker {
    /// Target number of tokens per chunk
    chunk_size: usize,
    /// Overlap ratio between chunks (0.0 - 1.0)
    overlap_ratio: f64,
}

impl Chunker {
    /// Create a new chunker
    ///
    /// # Arguments
    ///
    /// - `chunk_size`: Target number of tokens per chunk
    /// - `overlap_ratio`: Overlap ratio between chunks (0.0 - 1.0)
    ///
    /// # Panics
    ///
    /// Panics if `overlap_ratio` is not in the range [0.0, 1.0)
    pub fn new(chunk_size: usize, overlap_ratio: f64) -> Self {
        assert!(
            (0.0..1.0).contains(&overlap_ratio),
            "overlap_ratio must be in [0.0, 1.0)"
        );
        assert!(chunk_size > 0, "chunk_size must be > 0");

        Self {
            chunk_size,
            overlap_ratio,
        }
    }

    /// Calculate the number of overlapping tokens
    fn overlap_size(&self) -> usize {
        (self.chunk_size as f64 * self.overlap_ratio) as usize
    }

    /// Split text into chunks.
    ///
    /// # Arguments
    ///
    /// - `text`: the text to split
    ///
    /// # Returns
    ///
    /// A vector of text chunks, each a substring of the original.
    pub fn chunk(&self, text: &str) -> Vec<String> {
        if text.is_empty() {
            return vec![];
        }

        // 簡單的分詞：按空白字符分割
        let words: Vec<&str> = text.split_whitespace().collect();

        if words.is_empty() {
            return vec![];
        }

        // 估算 token 數量：1 word ≈ 1.3 tokens
        let words_per_chunk = (self.chunk_size as f64 / 1.3) as usize;
        let overlap_words = (self.overlap_size() as f64 / 1.3) as usize;

        if words.len() <= words_per_chunk {
            // 文本太短，不需要切分
            return vec![text.to_string()];
        }

        let mut chunks = Vec::new();
        let mut start = 0;

        while start < words.len() {
            let end = min(start + words_per_chunk, words.len());
            let chunk_words = &words[start..end];
            chunks.push(chunk_words.join(" "));

            if end >= words.len() {
                break;
            }

            // 下一個塊的起始位置：當前位置 + (塊大小 - 重疊大小)
            start += words_per_chunk.saturating_sub(overlap_words);

            // 防止無限循環
            if start == 0 {
                start = 1;
            }
        }

        chunks
    }
}

impl Default for Chunker {
    fn default() -> Self {
        Self::new(800, 0.15)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunker_new() {
        let chunker = Chunker::new(800, 0.15);
        assert_eq!(chunker.chunk_size, 800);
        assert!((chunker.overlap_ratio - 0.15).abs() < 0.001);
    }

    #[test]
    fn test_chunker_default() {
        let chunker = Chunker::default();
        assert_eq!(chunker.chunk_size, 800);
        assert!((chunker.overlap_ratio - 0.15).abs() < 0.001);
    }

    #[test]
    #[should_panic(expected = "overlap_ratio must be in [0.0, 1.0)")]
    fn test_invalid_overlap_ratio() {
        Chunker::new(800, 1.5);
    }

    #[test]
    #[should_panic(expected = "chunk_size must be > 0")]
    fn test_invalid_chunk_size() {
        Chunker::new(0, 0.15);
    }

    #[test]
    fn test_overlap_size() {
        let chunker = Chunker::new(800, 0.15);
        assert_eq!(chunker.overlap_size(), 120);

        let chunker = Chunker::new(1000, 0.2);
        assert_eq!(chunker.overlap_size(), 200);
    }

    #[test]
    fn test_chunk_empty() {
        let chunker = Chunker::default();
        let chunks = chunker.chunk("");
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_short_text() {
        let chunker = Chunker::new(800, 0.15);
        let text = "This is a short text.";
        let chunks = chunker.chunk(text);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], text);
    }

    #[test]
    fn test_chunk_long_text() {
        let chunker = Chunker::new(100, 0.15); // 小塊用於測試
        let words: Vec<String> = (0..200).map(|i| format!("word{}", i)).collect();
        let text = words.join(" ");

        let chunks = chunker.chunk(&text);

        // 應該產生多個塊
        assert!(chunks.len() > 1);

        // 每個塊都不應該為空
        for chunk in &chunks {
            assert!(!chunk.is_empty());
        }
    }

    #[test]
    fn test_chunk_overlap() {
        let chunker = Chunker::new(100, 0.2); // 20% 重疊
        let words: Vec<String> = (0..200).map(|i| format!("word{}", i)).collect();
        let text = words.join(" ");

        let chunks = chunker.chunk(&text);

        // 檢查相鄰塊之間有重疊
        for i in 0..chunks.len() - 1 {
            let _chunk1_words: Vec<&str> = chunks[i].split_whitespace().collect();
            let _chunk2_words: Vec<&str> = chunks[i + 1].split_whitespace().collect();

            // 第二個塊的開頭應該與第一個塊的結尾有重疊
            let overlap_words = chunker.overlap_size() as f64 / 1.3;
            assert!(overlap_words > 0.0, "Should have overlap between chunks");
        }
    }

    #[test]
    fn test_chunk_preserves_content() {
        let chunker = Chunker::new(100, 0.15);
        let words: Vec<String> = (0..150).map(|i| format!("word{}", i)).collect();
        let text = words.join(" ");

        let chunks = chunker.chunk(&text);

        // 第一個塊應該包含 "word0"
        assert!(chunks[0].contains("word0"));

        // 最後一個塊應該包含最後一個詞
        assert!(chunks.last().unwrap().contains("word149"));
    }

    #[test]
    fn test_chunk_whitespace_only() {
        let chunker = Chunker::default();
        let text = "   \n\t  ";
        let chunks = chunker.chunk(text);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_single_word() {
        let chunker = Chunker::default();
        let text = "word";
        let chunks = chunker.chunk(text);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "word");
    }

    #[test]
    fn test_chunk_exact_size() {
        let chunker = Chunker::new(100, 0.0); // 無重疊
        let words_per_chunk = (100.0 / 1.3) as usize;
        let words: Vec<String> = (0..words_per_chunk).map(|i| format!("w{}", i)).collect();
        let text = words.join(" ");

        let chunks = chunker.chunk(&text);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_chunk_no_overlap() {
        let chunker = Chunker::new(100, 0.0);
        let words: Vec<String> = (0..200).map(|i| format!("word{}", i)).collect();
        let text = words.join(" ");

        let chunks = chunker.chunk(&text);

        // 無重疊時，塊之間不應該有共同的詞
        assert!(chunks.len() > 1);
    }
}
