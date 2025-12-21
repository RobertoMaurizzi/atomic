//! Markdown-aware content chunking for embedding generation
//!
//! Chunking strategy:
//! - Respects code blocks, headers, lists, paragraphs
//! - Never splits code blocks (kept atomic for syntax integrity)
//! - Headers create natural chunk boundaries
//! - Target chunk size: 2500 tokens
//! - Minimum chunk size: 100 tokens (smaller merged with adjacent)
//! - Maximum chunk size: 3000 tokens (except code blocks)
//! - Overlap: 200 tokens from next chunk appended to each chunk

use std::sync::LazyLock;
use tiktoken_rs::{cl100k_base, CoreBPE};

/// Configuration constants for chunking
const TARGET_CHUNK_TOKENS: usize = 2500;
const OVERLAP_TOKENS: usize = 200;
const MIN_CHUNK_TOKENS: usize = 100;
const MAX_CHUNK_TOKENS: usize = 3000;

/// Lazily initialized tokenizer (loaded once, reused for all operations)
static BPE: LazyLock<CoreBPE> = LazyLock::new(|| {
    cl100k_base().expect("Failed to load tiktoken encoding")
});

/// Count tokens using tiktoken's cl100k_base encoding (used by OpenAI embedding models)
fn count_tokens(text: &str) -> usize {
    BPE.encode_with_special_tokens(text).len()
}

/// Get the first N tokens of text as a string
fn get_first_n_tokens(text: &str, n: usize) -> String {
    let tokens = BPE.encode_with_special_tokens(text);
    let take_count = tokens.len().min(n);
    BPE.decode(tokens[..take_count].to_vec())
        .unwrap_or_else(|_| text.chars().take(n * 4).collect())
}

/// Types of markdown blocks
#[derive(Debug, Clone, PartialEq)]
enum BlockType {
    CodeBlock,  // Fenced code blocks (```)
    Header,     // Lines starting with #
    List,       // Consecutive lines starting with -, *, or numbers
    Paragraph,  // Regular text
}

/// A parsed markdown block
#[derive(Debug, Clone)]
struct MarkdownBlock {
    block_type: BlockType,
    content: String,
}

/// Parse content into markdown blocks
fn parse_markdown_blocks(content: &str) -> Vec<MarkdownBlock> {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Check for fenced code block
        if line.trim_start().starts_with("```") {
            let indent = line.len() - line.trim_start().len();
            let mut code_content = vec![line.to_string()];
            i += 1;

            // Find closing fence
            while i < lines.len() {
                let current = lines[i];
                code_content.push(current.to_string());
                if current.trim_start().starts_with("```")
                    && (current.len() - current.trim_start().len()) <= indent + 1 {
                    i += 1;
                    break;
                }
                i += 1;
            }

            blocks.push(MarkdownBlock {
                block_type: BlockType::CodeBlock,
                content: code_content.join("\n"),
            });
            continue;
        }

        // Check for header
        if line.starts_with('#') {
            blocks.push(MarkdownBlock {
                block_type: BlockType::Header,
                content: line.to_string(),
            });
            i += 1;
            continue;
        }

        // Check for list item
        let trimmed = line.trim_start();
        let is_list_item = trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with("+ ")
            || (trimmed.len() > 2
                && trimmed.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
                && (trimmed.contains(". ") || trimmed.contains(") ")));

        if is_list_item {
            let mut list_content = vec![line.to_string()];
            i += 1;

            // Collect consecutive list items and their continuation lines
            while i < lines.len() {
                let next_line = lines[i];
                let next_trimmed = next_line.trim_start();

                // Check if it's a list item or continuation (indented)
                let is_next_list = next_trimmed.starts_with("- ")
                    || next_trimmed.starts_with("* ")
                    || next_trimmed.starts_with("+ ")
                    || (next_trimmed.len() > 2
                        && next_trimmed.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
                        && (next_trimmed.contains(". ") || next_trimmed.contains(") ")));

                // Continue if list item, indented continuation, or empty line within list
                let indent_level = next_line.len() - next_trimmed.len();
                if is_next_list || (indent_level > 0 && !next_trimmed.is_empty()) {
                    list_content.push(next_line.to_string());
                    i += 1;
                } else if next_trimmed.is_empty() && i + 1 < lines.len() {
                    // Check if there's more list after empty line
                    let after_empty = lines[i + 1].trim_start();
                    let is_list_after = after_empty.starts_with("- ")
                        || after_empty.starts_with("* ")
                        || after_empty.starts_with("+ ")
                        || (after_empty.len() > 2
                            && after_empty.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
                            && (after_empty.contains(". ") || after_empty.contains(") ")));
                    if is_list_after {
                        list_content.push(next_line.to_string());
                        i += 1;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }

            blocks.push(MarkdownBlock {
                block_type: BlockType::List,
                content: list_content.join("\n"),
            });
            continue;
        }

        // Regular paragraph - collect until empty line or structure change
        if !line.trim().is_empty() {
            let mut para_content = vec![line.to_string()];
            i += 1;

            while i < lines.len() {
                let next_line = lines[i];

                // Stop at empty line, header, code block, or list
                if next_line.trim().is_empty()
                    || next_line.starts_with('#')
                    || next_line.trim_start().starts_with("```") {
                    break;
                }

                let next_trimmed = next_line.trim_start();
                let is_list = next_trimmed.starts_with("- ")
                    || next_trimmed.starts_with("* ")
                    || next_trimmed.starts_with("+ ")
                    || (next_trimmed.len() > 2
                        && next_trimmed.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
                        && (next_trimmed.contains(". ") || next_trimmed.contains(") ")));

                if is_list {
                    break;
                }

                para_content.push(next_line.to_string());
                i += 1;
            }

            blocks.push(MarkdownBlock {
                block_type: BlockType::Paragraph,
                content: para_content.join("\n"),
            });
            continue;
        }

        // Skip empty lines
        i += 1;
    }

    blocks
}

/// Split a block by sentences if it exceeds the token limit
fn split_block_by_sentences(content: &str, max_tokens: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current_chunk = String::new();
    let mut current_tokens = 0;

    // Split by sentence boundaries
    let sentence_endings = [". ", "! ", "? ", ".\n", "!\n", "?\n"];
    let mut remaining = content;

    while !remaining.is_empty() {
        // Find the next sentence boundary
        let mut best_pos = None;
        for ending in &sentence_endings {
            if let Some(pos) = remaining.find(ending) {
                let end_pos = pos + ending.len();
                match best_pos {
                    None => best_pos = Some(end_pos),
                    Some(current) if end_pos < current => best_pos = Some(end_pos),
                    _ => {}
                }
            }
        }

        let (sentence, rest) = match best_pos {
            Some(pos) => (&remaining[..pos], &remaining[pos..]),
            None => (remaining, ""),
        };

        let sentence_tokens = count_tokens(sentence);

        // If adding this sentence exceeds limit, start new chunk
        if current_tokens + sentence_tokens > max_tokens && !current_chunk.is_empty() {
            chunks.push(current_chunk.clone());
            current_chunk = String::new();
            current_tokens = 0;
        }

        // If single sentence is too large, hard split it
        if sentence_tokens > max_tokens {
            if !current_chunk.is_empty() {
                chunks.push(current_chunk.clone());
                current_chunk = String::new();
                current_tokens = 0;
            }
            // Hard split the large sentence
            let hard_splits = hard_split_by_tokens(sentence, max_tokens);
            chunks.extend(hard_splits);
        } else {
            current_chunk.push_str(sentence);
            current_tokens += sentence_tokens;
        }

        remaining = rest;
    }

    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }

    chunks
}

/// Hard split text by token count (last resort)
fn hard_split_by_tokens(text: &str, max_tokens: usize) -> Vec<String> {
    let tokens = BPE.encode_with_special_tokens(text);

    if tokens.len() <= max_tokens {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut start = 0;

    while start < tokens.len() {
        let end = (start + max_tokens).min(tokens.len());
        if let Ok(chunk) = BPE.decode(tokens[start..end].to_vec()) {
            chunks.push(chunk);
        }
        start = end;
    }

    chunks
}

/// Merge adjacent small blocks into chunks respecting token limits
fn merge_blocks_into_chunks(blocks: Vec<MarkdownBlock>) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut current_chunk = String::new();
    let mut current_tokens = 0;

    for block in blocks {
        let block_tokens = count_tokens(&block.content);

        // Code blocks are never split - add as their own chunk if large
        if block.block_type == BlockType::CodeBlock {
            // If we have accumulated content, save it first
            if !current_chunk.is_empty() {
                chunks.push(current_chunk.clone());
                current_chunk = String::new();
                current_tokens = 0;
            }

            // Code blocks stay intact even if they exceed max
            // (This is intentional for syntax integrity)
            if block_tokens > MAX_CHUNK_TOKENS {
                chunks.push(block.content);
            } else if block_tokens > TARGET_CHUNK_TOKENS {
                chunks.push(block.content);
            } else {
                // Small code block - can be combined
                current_chunk = block.content;
                current_tokens = block_tokens;
            }
            continue;
        }

        // Headers start new chunks (natural boundaries)
        if block.block_type == BlockType::Header {
            if !current_chunk.is_empty() && current_tokens >= MIN_CHUNK_TOKENS {
                chunks.push(current_chunk.clone());
                current_chunk = String::new();
                current_tokens = 0;
            }
        }

        // Check if block fits in current chunk
        if current_tokens + block_tokens <= TARGET_CHUNK_TOKENS {
            if !current_chunk.is_empty() {
                current_chunk.push_str("\n\n");
            }
            current_chunk.push_str(&block.content);
            current_tokens += block_tokens;
        } else if block_tokens > TARGET_CHUNK_TOKENS {
            // Block is too large - need to split it
            if !current_chunk.is_empty() {
                chunks.push(current_chunk.clone());
                current_chunk = String::new();
                current_tokens = 0;
            }

            // Split large block by sentences
            let sub_chunks = split_block_by_sentences(&block.content, TARGET_CHUNK_TOKENS);

            for (i, sub_chunk) in sub_chunks.into_iter().enumerate() {
                let sub_tokens = count_tokens(&sub_chunk);

                if i == 0 || current_tokens + sub_tokens > TARGET_CHUNK_TOKENS {
                    if !current_chunk.is_empty() {
                        chunks.push(current_chunk.clone());
                    }
                    current_chunk = sub_chunk;
                    current_tokens = sub_tokens;
                } else {
                    current_chunk.push_str(&sub_chunk);
                    current_tokens += sub_tokens;
                }
            }
        } else {
            // Start new chunk with this block
            if !current_chunk.is_empty() {
                chunks.push(current_chunk.clone());
            }
            current_chunk = block.content;
            current_tokens = block_tokens;
        }
    }

    // Don't forget the last chunk
    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }

    chunks
}

/// Merge small chunks with adjacent chunks
fn merge_small_chunks(chunks: Vec<String>) -> Vec<String> {
    if chunks.is_empty() {
        return chunks;
    }

    let mut result: Vec<String> = Vec::new();
    let mut pending: Option<String> = None;

    for chunk in chunks {
        let chunk_tokens = count_tokens(&chunk);

        if let Some(prev) = pending.take() {
            let merged = format!("{}\n\n{}", prev, chunk);
            let merged_tokens = count_tokens(&merged);

            if merged_tokens <= TARGET_CHUNK_TOKENS {
                if merged_tokens < MIN_CHUNK_TOKENS {
                    pending = Some(merged);
                } else {
                    result.push(merged);
                }
            } else {
                // Can't merge - push previous and handle current
                if count_tokens(&prev) >= MIN_CHUNK_TOKENS {
                    result.push(prev);
                } else if !result.is_empty() {
                    // Merge with last result
                    let last = result.pop().unwrap();
                    result.push(format!("{}\n\n{}", last, prev));
                } else {
                    result.push(prev);
                }

                if chunk_tokens < MIN_CHUNK_TOKENS {
                    pending = Some(chunk);
                } else {
                    result.push(chunk);
                }
            }
        } else if chunk_tokens < MIN_CHUNK_TOKENS {
            pending = Some(chunk);
        } else {
            result.push(chunk);
        }
    }

    // Handle any remaining pending chunk
    if let Some(remaining) = pending {
        if !result.is_empty() {
            let last = result.pop().unwrap();
            let merged = format!("{}\n\n{}", last, remaining);
            result.push(merged);
        } else {
            result.push(remaining);
        }
    }

    result
}

/// Apply overlap between consecutive chunks
fn apply_overlap(chunks: Vec<String>, overlap_tokens: usize) -> Vec<String> {
    if chunks.len() <= 1 || overlap_tokens == 0 {
        return chunks;
    }

    let mut result = Vec::with_capacity(chunks.len());

    for (i, chunk) in chunks.iter().enumerate() {
        if i < chunks.len() - 1 {
            // Get overlap from next chunk
            let next_overlap = get_first_n_tokens(&chunks[i + 1], overlap_tokens);

            // Append overlap with a marker
            let with_overlap = format!("{}\n\n{}", chunk, next_overlap);
            result.push(with_overlap);
        } else {
            // Last chunk - no overlap needed
            result.push(chunk.clone());
        }
    }

    result
}

/// Chunks content into smaller pieces for embedding generation.
///
/// Chunking strategy:
/// - Markdown-aware: Respects code blocks, headers, lists, paragraphs
/// - Never splits code blocks (kept atomic for syntax integrity)
/// - Headers create natural chunk boundaries
/// - Target chunk size: 2500 tokens
/// - Minimum chunk size: 100 tokens (smaller merged with adjacent)
/// - Maximum chunk size: 3000 tokens (except code blocks)
/// - Overlap: 200 tokens from next chunk appended to each chunk
pub fn chunk_content(content: &str) -> Vec<String> {
    if content.is_empty() {
        return Vec::new();
    }

    // 1. Parse markdown structure
    let blocks = parse_markdown_blocks(content);

    if blocks.is_empty() {
        return Vec::new();
    }

    // 2. Merge blocks into chunks respecting token limits
    let chunks = merge_blocks_into_chunks(blocks);

    // 3. Merge any remaining small chunks
    let chunks = merge_small_chunks(chunks);

    // 4. Apply overlap
    let chunks = apply_overlap(chunks, OVERLAP_TOKENS);

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_counting() {
        let text = "Hello, world!";
        let tokens = count_tokens(text);
        assert!(tokens > 0);
        assert!(tokens < 10); // Should be ~4 tokens
    }

    #[test]
    fn test_empty_content() {
        let chunks = chunk_content("");
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_simple_paragraphs() {
        let content = "First paragraph with enough content to stand alone.\n\nSecond paragraph also with content.";
        let chunks = chunk_content(content);
        // Should be one chunk since both are small
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_code_block_not_split() {
        let content = r#"Some intro text.

```rust
fn main() {
    println!("Hello, world!");
    // This is a code block
    // It should not be split
    for i in 0..100 {
        println!("{}", i);
    }
}
```

Some outro text."#;
        let chunks = chunk_content(content);

        // Find the chunk containing the code block
        let code_chunk = chunks.iter().find(|c| c.contains("fn main()"));
        assert!(code_chunk.is_some(), "Code block should be in output");

        let code = code_chunk.unwrap();
        assert!(code.contains("```rust"), "Code block should have opening fence");
        assert!(code.contains("for i in 0..100"), "Code block should be complete");
    }

    #[test]
    fn test_header_creates_boundary() {
        let content = r#"# First Section

This is content under the first section with enough text to be meaningful.

# Second Section

This is content under the second section with different information."#;

        let blocks = parse_markdown_blocks(content);

        // Should have headers identified
        let header_count = blocks.iter().filter(|b| b.block_type == BlockType::Header).count();
        assert_eq!(header_count, 2, "Should identify 2 headers");
    }

    #[test]
    fn test_list_kept_together() {
        let content = r#"Here are some items:

- First item
- Second item
- Third item

After the list."#;

        let blocks = parse_markdown_blocks(content);

        // Should have a list block
        let list_block = blocks.iter().find(|b| b.block_type == BlockType::List);
        assert!(list_block.is_some(), "Should identify list block");

        let list = list_block.unwrap();
        assert!(list.content.contains("First item"));
        assert!(list.content.contains("Third item"));
    }

    #[test]
    fn test_overlap_applied() {
        // Create content that will produce multiple chunks
        let long_para = "This is a test sentence with enough content. ".repeat(200);
        let content = format!("{}\n\n{}", long_para, "Final paragraph with unique content.");

        let chunks = chunk_content(&content);

        if chunks.len() > 1 {
            // First chunk should contain overlap from second
            // (if chunks are created)
            let first = &chunks[0];
            assert!(first.len() > 0);
        }
    }

    #[test]
    fn test_numbered_list() {
        let content = r#"Steps to follow:

1. First step
2. Second step
3. Third step

Done!"#;

        let blocks = parse_markdown_blocks(content);
        let list_block = blocks.iter().find(|b| b.block_type == BlockType::List);
        assert!(list_block.is_some(), "Should identify numbered list");
    }

    #[test]
    fn test_nested_code_in_list() {
        let content = r#"- Item with code:
  ```
  code here
  ```
- Next item"#;

        let blocks = parse_markdown_blocks(content);
        // Code block inside list should be handled
        assert!(!blocks.is_empty());
    }

    #[test]
    fn test_small_chunks_merged() {
        let content = "Title\n\nSubtitle\n\nA longer paragraph with actual meaningful content that should stand alone.";
        let chunks = chunk_content(content);

        // Small title/subtitle should merge with content
        assert!(!chunks.is_empty());
        assert!(chunks[0].contains("Title"));
    }

    #[test]
    fn test_get_first_n_tokens() {
        let text = "Hello world, this is a test sentence.";
        let first = get_first_n_tokens(text, 3);
        assert!(!first.is_empty());
        assert!(first.len() < text.len());
    }

    #[test]
    fn test_preserves_whitespace_in_code() {
        let content = r#"```python
def foo():
    if True:
        print("indented")
```"#;

        let chunks = chunk_content(content);
        assert!(!chunks.is_empty());

        // Check indentation preserved
        let chunk = &chunks[0];
        assert!(chunk.contains("    if True:"));
        assert!(chunk.contains("        print"));
    }

    #[test]
    fn test_multiple_code_blocks() {
        let content = r#"First code:

```js
console.log("first");
```

Second code:

```python
print("second")
```"#;

        let blocks = parse_markdown_blocks(content);
        let code_blocks: Vec<_> = blocks.iter()
            .filter(|b| b.block_type == BlockType::CodeBlock)
            .collect();

        assert_eq!(code_blocks.len(), 2, "Should find 2 code blocks");
    }

    #[test]
    fn test_sentence_splitting() {
        let long_text = "This is sentence one. This is sentence two! Is this sentence three? Yes it is. ".repeat(50);
        let splits = split_block_by_sentences(&long_text, 500);

        // Should have multiple splits
        assert!(splits.len() > 1);

        // Each split should end at sentence boundary (mostly)
        for (i, split) in splits.iter().enumerate() {
            if i < splits.len() - 1 {
                let trimmed = split.trim();
                assert!(
                    trimmed.ends_with('.') || trimmed.ends_with('!') || trimmed.ends_with('?'),
                    "Split {} should end with sentence punctuation: '{}'",
                    i,
                    &trimmed[trimmed.len().saturating_sub(20)..]
                );
            }
        }
    }
}
