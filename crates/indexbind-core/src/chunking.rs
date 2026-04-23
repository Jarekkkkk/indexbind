use crate::lexical::estimate_token_count;
use crate::types::StoredChunk;
use pulldown_cmark::{Event, Parser, Tag};

#[derive(Debug, Clone)]
pub struct ChunkingOptions {
    pub target_tokens: usize,
    pub overlap_tokens: usize,
}

impl Default for ChunkingOptions {
    fn default() -> Self {
        Self {
            target_tokens: 512,
            overlap_tokens: 64,
        }
    }
}

/// Check if a chunk contains meaningful content for search.
/// Filters out chunks that are predominantly links, navigation, or too short.
fn is_meaningful_chunk(text: &str) -> bool {
    let parser = Parser::new(text);
    let mut text_len = 0;
    let mut url_len = 0;
    let mut word_count = 0;
    let mut nav_pattern_count = 0;

    let nav_patterns_en = ["home", "about", "contact", "privacy", "terms", "sitemap"];
    let nav_patterns_cn = [
        "首页",
        "关于",
        "联系",
        "隐私",
        "条款",
        "网站地图",
        "关于我们",
        "联系我们",
        "更多文章",
        "返回首页",
        "订阅",
        "取消订阅",
        "查看更多",
        "点击这里",
    ];

    for event in parser {
        match event {
            Event::Text(t) => {
                text_len += t.len();
                word_count += t.split_whitespace().count();
                let lower = t.to_lowercase();

                // Check English navigation patterns
                nav_pattern_count += nav_patterns_en
                    .iter()
                    .filter(|p| lower.contains(*p))
                    .count();

                // Check Chinese navigation patterns
                nav_pattern_count += nav_patterns_cn.iter().filter(|p| t.contains(*p)).count();
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                url_len += dest_url.len();
            }
            Event::Code(_) => {
                // Code is less meaningful for search
            }
            Event::Html(html) => {
                // HTML blocks are typically navigation/ads
                let lower = html.to_lowercase();
                if lower.contains("<nav") || lower.contains("<footer") || lower.contains("<header")
                {
                    return false;
                }
            }
            _ => {}
        }
    }

    // Filter: too many navigation patterns
    if nav_pattern_count >= 3 {
        return false;
    }

    // Filter: too short (minimum 25 words)
    if word_count < 25 {
        return false;
    }

    // Filter: predominantly URLs (>50% is URLs)
    if text_len > 0 && (url_len as f32 / text_len as f32) > 0.5 {
        return false;
    }

    true
}

#[derive(Debug)]
struct Block {
    text: String,
    heading_path: Vec<String>,
}

pub fn chunk_document(doc_id: &str, content: &str, options: &ChunkingOptions) -> Vec<StoredChunk> {
    let blocks = split_blocks(content);
    if blocks.is_empty() {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut ordinal = 0usize;
    let mut next_chunk_id = 1_i64;
    let mut cursor = 0usize;

    while cursor < blocks.len() {
        let heading_path = blocks[cursor].heading_path.clone();
        let mut text = String::new();
        let mut block_index = cursor;
        let mut token_count = 0usize;
        let char_start = content
            .find(blocks[cursor].text.as_str())
            .unwrap_or_default();

        while block_index < blocks.len() {
            let block = &blocks[block_index];
            let block_tokens = estimate_tokens(&block.text);
            let fits = text.is_empty() || token_count + block_tokens <= options.target_tokens;
            if !fits {
                break;
            }
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            text.push_str(&block.text);
            token_count += block_tokens;
            block_index += 1;
            if block_index < blocks.len()
                && !blocks[block_index].heading_path.is_empty()
                && blocks[block_index].heading_path != heading_path
                && token_count >= options.target_tokens / 2
            {
                break;
            }
        }

        let char_end = char_start.saturating_add(text.len());
        let excerpt = text.chars().take(280).collect::<String>();

        // Only add chunk if it contains meaningful content
        if is_meaningful_chunk(&text) {
            chunks.push(StoredChunk {
                chunk_id: next_chunk_id,
                doc_id: doc_id.to_string(),
                ordinal,
                heading_path,
                char_start,
                char_end,
                token_count,
                chunk_text: text.clone(),
                excerpt,
            });
            ordinal += 1;
            next_chunk_id += 1;
        }

        if block_index == cursor {
            cursor += 1;
            continue;
        }

        let mut overlap = 0usize;
        let mut rewind = block_index;
        while rewind > cursor {
            let candidate = &blocks[rewind - 1];
            overlap += estimate_tokens(&candidate.text);
            if overlap >= options.overlap_tokens {
                break;
            }
            rewind -= 1;
        }
        cursor = rewind.max(cursor + 1);
    }

    chunks
}

fn split_blocks(content: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut heading_path: Vec<String> = Vec::new();
    let mut current = Vec::new();

    let flush = |blocks: &mut Vec<Block>, current: &mut Vec<String>, heading_path: &[String]| {
        let text = current.join("\n").trim().to_string();
        if !text.is_empty() {
            blocks.push(Block {
                text,
                heading_path: heading_path.to_vec(),
            });
        }
        current.clear();
    };

    for line in content.lines() {
        if let Some((level, heading)) = parse_heading(line) {
            flush(&mut blocks, &mut current, &heading_path);
            heading_path.truncate(level.saturating_sub(1));
            heading_path.push(heading.to_string());
            continue;
        }
        if line.trim().is_empty() && !current.is_empty() {
            current.push(String::new());
            flush(&mut blocks, &mut current, &heading_path);
            continue;
        }
        current.push(line.to_string());
    }

    flush(&mut blocks, &mut current, &heading_path);
    blocks
}

fn parse_heading(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim();
    let prefix_len = trimmed.chars().take_while(|ch| *ch == '#').count();
    if prefix_len == 0 || prefix_len > 6 {
        return None;
    }
    let heading = trimmed[prefix_len..].trim();
    if heading.is_empty() {
        return None;
    }
    Some((prefix_len, heading))
}

fn estimate_tokens(text: &str) -> usize {
    estimate_token_count(text)
}

#[cfg(test)]
mod tests {
    use super::{chunk_document, ChunkingOptions};

    #[test]
    fn chunker_keeps_heading_context() {
        let input = "# Intro\nhello world\n\n## Details\nmore words here";
        let chunks = chunk_document("doc-1", input, &ChunkingOptions::default());
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].heading_path, vec!["Intro"]);
        assert_eq!(chunks[1].heading_path, vec!["Intro", "Details"]);
    }

    #[test]
    fn chunker_estimates_cjk_tokens_without_whitespace() {
        let input = "# 中文\n模块化区块链\n\n## 细节\n调用层";
        let chunks = chunk_document(
            "doc-1",
            input,
            &ChunkingOptions {
                target_tokens: 5,
                overlap_tokens: 0,
            },
        );
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].token_count, 5);
        assert_eq!(chunks[1].token_count, 2);
    }
}
