/// Chunks content into smaller pieces for embedding generation.
///
/// Chunking rules:
/// - Primary split: double newlines (`\n\n`)
/// - Secondary split (for long paragraphs): sentence boundaries (`. `, `! `, `? `)
/// - Minimum chunk size: 50 characters (smaller chunks get merged)
/// - Merging strategy: bidirectional (forward first, then backward)
/// - Maximum chunk: 2000 characters
/// - Preserve original text exactly (no trimming whitespace within chunks)
pub fn chunk_content(content: &str) -> Vec<String> {
    if content.is_empty() {
        return Vec::new();
    }

    // 1. Split by double newlines (paragraphs)
    let paragraphs: Vec<&str> = content.split("\n\n").collect();

    let mut chunks: Vec<String> = Vec::new();

    for paragraph in paragraphs {
        if paragraph.is_empty() {
            continue;
        }

        // 2. For paragraphs > 1500 chars, split by sentences (. ! ?)
        if paragraph.len() > 1500 {
            let sentence_chunks = split_by_sentences(paragraph);
            chunks.extend(sentence_chunks);
        } else {
            chunks.push(paragraph.to_string());
        }
    }

    // 3. Merge small chunks (< 50 chars) with adjacent chunks
    // First pass: merge forward (small + next)
    chunks = merge_small_chunks_forward(chunks, 50);
    // Second pass: merge backward (previous + small) for any remaining small chunks
    chunks = merge_small_chunks_backward(chunks, 50);

    // 4. Cap chunks at 2000 chars max (hard split)
    chunks = hard_split_large_chunks(chunks);

    chunks
}

/// Split a paragraph by sentence boundaries (`. `, `! `, `? `)
fn split_by_sentences(paragraph: &str) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut current_chunk = String::new();

    let chars: Vec<char> = paragraph.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        current_chunk.push(chars[i]);

        // Check for sentence boundary: punctuation followed by space
        if i + 1 < len
            && (chars[i] == '.' || chars[i] == '!' || chars[i] == '?')
            && chars[i + 1] == ' '
        {
            // Include the space in the current chunk
            current_chunk.push(chars[i + 1]);
            i += 2;

            // If current chunk is substantial, save it
            if !current_chunk.is_empty() {
                chunks.push(current_chunk);
                current_chunk = String::new();
            }
        } else {
            i += 1;
        }
    }

    // Don't forget the last chunk
    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }

    chunks
}

/// Merge small chunks (< min_size) with the next chunk
/// These are things like titles, headers, etc. that should be merged with context
/// Supports cascading merges (multiple small chunks merge together)
fn merge_small_chunks_forward(chunks: Vec<String>, min_size: usize) -> Vec<String> {
    if chunks.is_empty() {
        return chunks;
    }

    let mut result: Vec<String> = Vec::new();
    let mut pending: Option<String> = None;

    for chunk in chunks {
        if let Some(prev) = pending.take() {
            // We have a pending small chunk, merge it with current
            let merged = format!("{}\n\n{}", prev, chunk);
            // Check if merged result is still small - if so, keep as pending for cascade
            if merged.len() < min_size {
                pending = Some(merged);
            } else {
                result.push(merged);
            }
        } else if chunk.len() < min_size {
            // Current chunk is small, hold it for merge with next
            pending = Some(chunk);
        } else {
            result.push(chunk);
        }
    }

    // If there's a pending chunk at the end, add it (will be merged backward in next pass)
    if let Some(last) = pending {
        result.push(last);
    }

    result
}

/// Merge small chunks (< min_size) with the previous chunk
/// This handles trailing small chunks that couldn't be merged forward
fn merge_small_chunks_backward(chunks: Vec<String>, min_size: usize) -> Vec<String> {
    if chunks.is_empty() {
        return chunks;
    }

    let mut result: Vec<String> = Vec::new();

    for chunk in chunks {
        if chunk.len() < min_size && !result.is_empty() {
            // Current chunk is small and we have a previous chunk, merge backward
            let prev = result.pop().unwrap();
            result.push(format!("{}\n\n{}", prev, chunk));
        } else {
            result.push(chunk);
        }
    }

    result
}

/// Hard split chunks that exceed 2000 characters
fn hard_split_large_chunks(chunks: Vec<String>) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();

    for chunk in chunks {
        if chunk.len() <= 2000 {
            result.push(chunk);
        } else {
            // Hard split at 2000 char boundaries
            let chars: Vec<char> = chunk.chars().collect();
            let mut start = 0;
            while start < chars.len() {
                let end = std::cmp::min(start + 2000, chars.len());
                let sub_chunk: String = chars[start..end].iter().collect();
                result.push(sub_chunk);
                start = end;
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_paragraphs() {
        let content = "First paragraph here with enough content to stand alone as a chunk.\n\nSecond paragraph here also with enough content to be its own chunk.";
        let chunks = chunk_content(content);
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn test_long_paragraph_splits_on_sentences() {
        // Create a paragraph > 1500 chars
        let long_para = "This is a sentence. ".repeat(100); // ~2000 chars
        let chunks = chunk_content(&long_para);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| c.len() <= 2000));
    }

    #[test]
    fn test_small_chunks_merged() {
        let content = "Short Title\n\nThis is a longer paragraph with enough content to stand alone as a chunk.";
        let chunks = chunk_content(content);
        // "Short Title" is < 50 chars, should be merged with next
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].contains("Short Title"));
        assert!(chunks[0].contains("This is a longer paragraph"));
    }

    #[test]
    fn test_merge_final_small_chunk_backward() {
        let content = "This is a good paragraph with enough content to stand alone.\n\nTrailing fragment";
        let chunks = chunk_content(content);
        // "Trailing fragment" is < 50 chars, should be merged backward
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].contains("good paragraph"));
        assert!(chunks[0].contains("Trailing fragment"));
    }

    #[test]
    fn test_empty_content() {
        let chunks = chunk_content("");
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_max_chunk_size() {
        let long_text = "a".repeat(3000);
        let chunks = chunk_content(&long_text);
        assert!(chunks.iter().all(|c| c.len() <= 2000));
    }

    #[test]
    fn test_preserves_whitespace() {
        let content = "  Leading spaces preserved in this paragraph with enough content.  \n\n  Another paragraph with spaces and enough content to stand alone.  ";
        let chunks = chunk_content(content);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].starts_with("  "));
        assert!(chunks[1].starts_with("  "));
    }

    #[test]
    fn test_sentence_splitting_preserves_punctuation() {
        // Create a long paragraph that will be split by sentences
        let sentence = "This is a test sentence. ";
        let long_para = sentence.repeat(80); // > 1500 chars
        let chunks = chunk_content(&long_para);

        // Each chunk should end with ". " (except possibly the last one)
        for chunk in &chunks[..chunks.len().saturating_sub(1)] {
            assert!(chunk.ends_with(". ") || chunk.ends_with(".\n\n"));
        }
    }

    #[test]
    fn test_multiple_small_paragraphs_merged() {
        let content = "Title\n\nSubtitle\n\nIntro\n\nThis is a longer paragraph that has enough content to stand alone as a proper chunk.";
        let chunks = chunk_content(content);
        // All small paragraphs (< 50 chars) should cascade merge forward into the first substantial chunk
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].contains("Title"));
        assert!(chunks[0].contains("Subtitle"));
        assert!(chunks[0].contains("Intro"));
        assert!(chunks[0].contains("longer paragraph"));
    }

    #[test]
    fn test_single_long_chunk_hard_split() {
        let long_text = "a".repeat(5000);
        let chunks = chunk_content(&long_text);
        assert_eq!(chunks.len(), 3); // 2000 + 2000 + 1000
        assert_eq!(chunks[0].len(), 2000);
        assert_eq!(chunks[1].len(), 2000);
        assert_eq!(chunks[2].len(), 1000);
    }

    #[test]
    fn test_backward_merge_only() {
        let content = "This is a substantial paragraph with enough content to be a chunk.\n\nEnd";
        let chunks = chunk_content(content);
        // "End" (3 chars) should merge backward since there's no next chunk
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].ends_with("End"));
    }

    #[test]
    fn test_wikipedia_title_merged() {
        // Simulate typical Wikipedia article structure
        let content = "Abraham Lincoln\n\nAbraham Lincoln (1809-1865) was the 16th President of the United States.";
        let chunks = chunk_content(content);
        // Title should merge with first paragraph
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].starts_with("Abraham Lincoln\n\n"));
    }
}

