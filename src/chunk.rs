//! Paragraph-preferring chunker.
//!
//! Ported from Satchel's `src/ingest/mod.rs::chunk_text` (MIT,
//! virgilvox/satchel; see THIRD-PARTY.md), including its property tests.
//! The token count is a cheap `len/4` approximation, not a real tokenizer
//! pass -- deliberately: chunking must not require the embedding model to
//! be loaded, and running the real tokenizer here would mean tokenizing
//! every text twice. architecture.md §11 names this "Satchel's
//! property-tested algorithm"; parameters are the product's own
//! (D014): 1024-token chunks, 64-token overlap.

pub const CHUNK_SIZE_TOKENS: usize = 1024;
pub const CHUNK_OVERLAP_TOKENS: usize = 64;

pub struct Chunk {
    pub text: String,
    // Offsets into the source text; not yet consumed by `save` (S2 stores
    // chunk text only) but exercised by the property tests below and
    // useful for a later sprint's neighbor-chunk expansion (architecture.md
    // §23).
    #[allow(dead_code)]
    pub char_start: usize,
    #[allow(dead_code)]
    pub char_end: usize,
}

pub fn chunk_text(text: &str, chunk_size: usize, overlap: usize) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let paragraphs: Vec<&str> = text.split("\n\n").collect();

    let mut current_text = String::new();
    let mut current_start = 0;
    let mut char_offset = 0;

    for para in paragraphs {
        let para_tokens = approximate_tokens(para);

        if approximate_tokens(&current_text) + para_tokens > chunk_size && !current_text.is_empty()
        {
            let char_end = char_offset;
            chunks.push(Chunk {
                text: current_text.trim().to_string(),
                char_start: current_start,
                char_end,
            });

            let overlap_text = get_tail_tokens(&current_text, overlap);
            current_text = overlap_text;
            current_start = char_end.saturating_sub(current_text.len());
        }

        if !current_text.is_empty() {
            current_text.push_str("\n\n");
        }
        current_text.push_str(para);
        char_offset += para.len() + 2;
    }

    if !current_text.trim().is_empty() {
        chunks.push(Chunk {
            text: current_text.trim().to_string(),
            char_start: current_start,
            char_end: text.len(),
        });
    }

    chunks
}

/// Chunks `text` using the product's fixed cap/overlap (§11).
pub fn chunk(text: &str) -> Vec<Chunk> {
    chunk_text(text, CHUNK_SIZE_TOKENS, CHUNK_OVERLAP_TOKENS)
}

fn approximate_tokens(text: &str) -> usize {
    text.len() / 4
}

/// Returns the last `token_count` approximate tokens of `text`, respecting
/// UTF-8 char boundaries.
fn get_tail_tokens(text: &str, token_count: usize) -> String {
    let char_count = token_count * 4;
    if text.len() <= char_count {
        return text.to_string();
    }
    let target = text.len() - char_count;
    // `str::ceil_char_boundary` is nightly-only; walk forward by hand to the
    // next valid UTF-8 boundary at or after `target` instead.
    let mut start = target;
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_short_text_is_one_chunk() {
        let text = "Hello world.";
        let chunks = chunk_text(text, 1024, 64);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "Hello world.");
    }

    #[test]
    fn respects_paragraphs_when_they_fit() {
        let para1 = "A".repeat(300);
        let para2 = "B".repeat(300);
        let text = format!("{}\n\n{}", para1, para2);
        let chunks = chunk_text(&text, 1024, 64);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn splits_when_larger_than_chunk_size() {
        let para1 = "A".repeat(4096);
        let para2 = "B".repeat(4096);
        let text = format!("{}\n\n{}", para1, para2);
        let chunks = chunk_text(&text, 1024, 64);
        assert!(chunks.len() >= 2);
    }

    #[test]
    fn empty_text_is_no_chunks() {
        assert!(chunk_text("", 1024, 64).is_empty());
    }

    #[test]
    fn single_long_paragraph_with_no_break_is_one_chunk() {
        let text = "word ".repeat(2000);
        let chunks = chunk_text(&text, 1024, 64);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn overlap_is_nonempty_between_split_chunks() {
        let para1 = "A".repeat(4096);
        let para2 = "B".repeat(4096);
        let para3 = "C".repeat(4096);
        let text = format!("{}\n\n{}\n\n{}", para1, para2, para3);
        let chunks = chunk_text(&text, 1024, 64);
        assert!(chunks.len() >= 2);
        assert!(!chunks[0].text.is_empty());
        assert!(!chunks[1].text.is_empty());
    }

    #[test]
    fn default_chunk_uses_product_parameters() {
        let text = "one short memory".to_string();
        let chunks = chunk(&text);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn get_tail_tokens_is_unicode_safe() {
        let text = "prefix_text_here_\u{1F600}\u{1F600}\u{1F600}";
        let tail = get_tail_tokens(text, 2);
        assert!(!tail.is_empty());
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn chunking_produces_nonempty_chunks(text in "[a-z]{10,500}") {
            let chunks = chunk_text(&text, 1024, 64);
            for chunk in &chunks {
                prop_assert!(!chunk.text.trim().is_empty());
            }
        }

        #[test]
        fn chunking_never_loses_words(text in "[a-z ]{20,1000}") {
            let chunks = chunk_text(&text, 200, 20);
            let all_chunk_text: String = chunks.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join(" ");
            for word in text.split_whitespace() {
                prop_assert!(
                    all_chunk_text.contains(word),
                    "Word '{}' lost during chunking",
                    word
                );
            }
        }

        #[test]
        fn chunking_never_panics_on_arbitrary_utf8(text in "\\PC{0,2000}") {
            let _ = chunk_text(&text, 1024, 64);
        }

        #[test]
        fn offsets_are_within_bounds(text in "[a-z ]{20,1000}") {
            let chunks = chunk_text(&text, 200, 20);
            for c in &chunks {
                prop_assert!(c.char_start <= c.char_end);
                prop_assert!(c.char_end <= text.len());
            }
        }
    }
}
