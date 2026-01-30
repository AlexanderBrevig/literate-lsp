use literatemd_lsp::virtual_doc::{build_virtual_document, find_code_block_at_line};
use literatemd_lsp::position::PositionMapper;

#[test]
fn test_virtual_document_extraction() {
    let markdown = r#"# Forth

In forth, we can add a new word to the dictionary by compiling it.
We start by entering compilation mode with the `:` word.

Let's define our `square` to square a number.

```forth
: square ( n -- n ) dup * ;    \ ok
```

Now, `square` can be used to consume one off of the stack, and add the answer back.

```forth
5 square .                     \ 25 ok
```
"#;

    // Test building virtual document for forth
    let vdoc = build_virtual_document(markdown, "forth");

    // Should have extracted 2 forth blocks
    assert_eq!(vdoc.blocks.len(), 2, "Should find 2 forth code blocks");

    // Check first block
    assert_eq!(vdoc.blocks[0].markdown_start, 7, "First block should start at line 7");
    assert_eq!(vdoc.blocks[0].markdown_end, 9, "First block should end at line 9");
    assert_eq!(vdoc.blocks[0].virtual_start, 0, "First block virtual should start at 0");

    // Check second block (starts at 2 due to newline separator between blocks)
    assert_eq!(vdoc.blocks[1].markdown_start, 13, "Second block should start at line 13");
    assert_eq!(vdoc.blocks[1].markdown_end, 15, "Second block should end at line 15");
    assert_eq!(vdoc.blocks[1].virtual_start, 2, "Second block virtual should start at 2 (after newline separator)");

    // Virtual document should contain both blocks
    assert!(vdoc.content.contains("square"), "Virtual document should contain 'square'");
    assert!(vdoc.content.contains("5 square"), "Virtual document should contain '5 square'");
}

#[test]
fn test_find_code_block_at_line() {
    let markdown = r#"# Forth

```forth
: square ( n -- n ) dup * ;
```

Text between blocks

```forth
5 square .
```
"#;

    // Line 2 is inside first block (starting at line 2, ending at line 4)
    let result = find_code_block_at_line(markdown, 2);
    assert!(result.is_some(), "Should find code block at line 2");
    let (lang, _start, _end) = result.unwrap();
    assert_eq!(lang, "forth");

    // Line 8 is inside second block (starting at line 8, ending at line 10)
    let result = find_code_block_at_line(markdown, 8);
    assert!(result.is_some(), "Should find code block at line 8");
    let (lang, _start, _end) = result.unwrap();
    assert_eq!(lang, "forth");

    // Line 6 is not inside any code block
    let result = find_code_block_at_line(markdown, 6);
    assert!(result.is_none(), "Should not find code block at line 6");
}

#[test]
fn test_position_mapping() {
    // Based on example.md line numbers:
    // Line 7: ```forth (markdown_start)
    // Line 8: : square ( n -- n ) dup * ;    \ ok (content_start/content_end)
    // Line 9: ``` (markdown_end)
    // Line 13: ```forth (markdown_start)
    // Line 14: 5 square .                     \ 25 ok (content_start/content_end)
    // Line 15: ``` (markdown_end)
    // When building virtual document, we get:
    // - Block 0: md [7..9], content [8..8], maps to virtual line 0
    // - Block 1: md [13..15], content [14..14], maps to virtual line 2 (with newline between blocks)
    let blocks = vec![
        literatemd_lsp::virtual_doc::CodeBlock {
            lang: "forth".to_string(),
            markdown_start: 7,
            markdown_end: 9,
            content_start: 8,
            content_end: 8,
            virtual_start: 0,
            virtual_end: 1,
            content: ": square ( n -- n ) dup * ;    \\ ok\n".to_string(),
        },
        literatemd_lsp::virtual_doc::CodeBlock {
            lang: "forth".to_string(),
            markdown_start: 13,
            markdown_end: 15,
            content_start: 14,
            content_end: 14,
            virtual_start: 2,
            virtual_end: 3,
            content: "5 square .                     \\ 25 ok\n".to_string(),
        },
    ];

    let mapper = PositionMapper::new(blocks);

    // Test mapping markdown to virtual
    // Line 8 (first block content) column 2 should map to virtual line 0, column 2
    let (vline, col) = mapper.markdown_to_virtual(8, 2).unwrap();
    assert_eq!(vline, 0, "Line 8 should map to virtual line 0");
    assert_eq!(col, 2);

    // Line 14 (second block content) column 2 should map to virtual line 2, column 2
    // (Line 1 is the newline separator between blocks)
    let (vline, col) = mapper.markdown_to_virtual(14, 2).unwrap();
    assert_eq!(vline, 2, "Line 14 should map to virtual line 2");
    assert_eq!(col, 2);

    // Test mapping virtual back to markdown
    // Virtual line 0 should map back to markdown line 8
    let (mline, col) = mapper.virtual_to_markdown(0, 2).unwrap();
    assert_eq!(mline, 8, "Virtual line 0 should map to markdown line 8");
    assert_eq!(col, 2);

    // Virtual line 2 should map back to markdown line 14
    // (Virtual line 1 is the newline separator, which doesn't map back)
    let (mline, col) = mapper.virtual_to_markdown(2, 2).unwrap();
    assert_eq!(mline, 14, "Virtual line 2 should map to markdown line 14");
    assert_eq!(col, 2);
}

#[test]
fn test_example_md_parsing() {
    let markdown = std::fs::read_to_string("example.md").expect("Failed to read example.md");

    // Find the forth code block at line 14 (where `5 square .` is)
    let result = find_code_block_at_line(&markdown, 14);
    assert!(result.is_some(), "Should find forth block at line 14");
    let (lang, _start, _end) = result.unwrap();
    assert_eq!(lang, "forth", "Block at line 14 should be forth");

    // Build virtual document
    let vdoc = build_virtual_document(&markdown, "forth");
    assert!(!vdoc.blocks.is_empty(), "Should extract forth blocks");

    // Virtual document should contain the definition and usage
    assert!(vdoc.content.contains("square"), "Virtual doc should contain 'square' definition");
    assert!(vdoc.content.contains("dup"), "Virtual doc should contain 'dup'");
}
