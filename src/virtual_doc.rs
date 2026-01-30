use tracing::debug;

#[derive(Debug, Clone)]
pub struct CodeBlock {
    pub lang: String,
    pub markdown_start: usize,    // Line with opening ```
    pub markdown_end: usize,      // Line with closing ```
    pub content_start: usize,     // First line of actual content
    pub content_end: usize,       // Last line of actual content
    pub virtual_start: usize,
    pub virtual_end: usize,
    pub content: String,
}

#[derive(Debug)]
pub struct VirtualDocument {
    pub content: String,
    pub blocks: Vec<CodeBlock>,
}

pub fn build_virtual_document(markdown: &str, target_lang: &str) -> VirtualDocument {
    debug!("[VirtualDoc] Building virtual document for language: '{}'", target_lang);
    let lines: Vec<&str> = markdown.lines().collect();
    let mut blocks = Vec::new();
    let mut virtual_content = String::new();
    let mut virtual_line = 0;
    let mut in_code_block = false;
    let mut current_block_lang = String::new();
    let mut block_start = 0;
    let mut block_content = String::new();

    for (idx, line) in lines.iter().enumerate() {
        if !in_code_block {
            if let Some(pos) = line.find("```") {
                let lang_start = pos + 3;
                if lang_start <= line.len() {
                    let lang_part = &line[lang_start..];
                    let lang = lang_part.split_whitespace().next().unwrap_or("").to_string();
                    debug!("[VirtualDoc] Found code block with language: '{}'", lang);
                    in_code_block = true;
                    current_block_lang = lang;
                    block_start = idx;
                    block_content.clear();
                }
            }
        } else if line.contains("```") {
            debug!("[VirtualDoc] End of block '{}', checking if matches target '{}'", current_block_lang, target_lang);
            if current_block_lang == target_lang {
                // Add blank line separator before this block (except for first block)
                if !blocks.is_empty() {
                    virtual_content.push('\n');
                    virtual_line += 1;
                }

                let virtual_start = virtual_line;
                let block_lines: Vec<&str> = block_content.trim_end().lines().collect();
                for (i, content_line) in block_lines.iter().enumerate() {
                    if i > 0 {
                        virtual_content.push('\n');
                        virtual_line += 1;
                    }
                    virtual_content.push_str(content_line);
                }
                // Always push newline at end of block
                if !block_content.is_empty() {
                    virtual_content.push('\n');
                    virtual_line += 1;
                }

                blocks.push(CodeBlock {
                    lang: current_block_lang.clone(),
                    markdown_start: block_start,
                    markdown_end: idx,
                    content_start: block_start + 1,  // First line after opening ```
                    content_end: idx - 1,             // Last line before closing ```
                    virtual_start,
                    virtual_end: virtual_line,
                    content: block_content.clone(),
                });
            }
            in_code_block = false;
        } else if in_code_block {
            block_content.push_str(line);
            block_content.push('\n');
        }
    }

    debug!("[VirtualDoc] Collected {} blocks, content length: {}", blocks.len(), virtual_content.len());
    if virtual_content.is_empty() {
        debug!("[VirtualDoc] WARNING: Virtual document is empty!");
    }

    VirtualDocument {
        content: virtual_content,
        blocks,
    }
}

pub fn find_code_block_at_line(
    markdown: &str,
    line: usize,
) -> Option<(String, usize, usize)> {
    let lines: Vec<&str> = markdown.lines().collect();
    let mut in_code_block = false;
    let mut block_lang = String::new();
    let mut block_start = 0;
    let mut fence_backtick_count = 0;

    for (idx, current_line) in lines.iter().enumerate() {
        // Count leading backticks
        let backtick_count = current_line.chars().take_while(|&c| c == '`').count();

        if backtick_count < 3 {
            continue; // Not a fence
        }

        if !in_code_block {
            // Starting a code block
            let lang_start = backtick_count;
            let lang = if lang_start < current_line.len() {
                let lang_part = &current_line[lang_start..];
                lang_part.split_whitespace().next().unwrap_or("").to_string()
            } else {
                String::new()
            };

            in_code_block = true;
            block_lang = lang;
            block_start = idx;
            fence_backtick_count = backtick_count;
        } else if backtick_count >= fence_backtick_count {
            // Closing a code block (must have at least as many backticks)
            if line >= block_start && line <= idx {
                return Some((block_lang.clone(), block_start, idx));
            }
            in_code_block = false;
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_virtual_document() {
        let markdown = "# Forth\n\n```forth\n: square ( n -- n ) dup * ;\n```\n\n```forth\n5 square .\n```\n";
        let vdoc = build_virtual_document(markdown, "forth");
        assert_eq!(vdoc.blocks.len(), 2);
        assert!(vdoc.content.contains("square"));
    }

    #[test]
    fn test_find_code_block_at_line() {
        let markdown = "# Forth\n\n```forth\n: square ( n -- n ) dup * ;\n```\n\nText\n\n```forth\n5 square .\n```\n";
        let result = find_code_block_at_line(markdown, 3);
        assert!(result.is_some());
        let (lang, _start, _end) = result.unwrap();
        assert_eq!(lang, "forth");
    }
}
