use tower_lsp::lsp_types::{Position, Range, Location, Url};
use crate::virtual_doc::CodeBlock;

pub struct PositionMapper {
    blocks: Vec<CodeBlock>,
}

impl PositionMapper {
    pub fn new(blocks: Vec<CodeBlock>) -> Self {
        PositionMapper { blocks }
    }

    pub fn markdown_to_virtual(&self, markdown_line: u32, col: u32) -> Option<(u32, u32)> {
        for block in &self.blocks {
            // Check if position is within the content range (not in fence lines)
            if markdown_line >= block.content_start as u32
                && markdown_line <= block.content_end as u32
            {
                let offset = markdown_line as usize - block.content_start;
                let virtual_line = block.virtual_start as u32 + offset as u32;
                return Some((virtual_line, col));
            }
        }
        None
    }

    pub fn virtual_to_markdown(&self, virtual_line: u32, col: u32) -> Option<(u32, u32)> {
        for block in &self.blocks {
            if virtual_line >= block.virtual_start as u32
                && virtual_line < block.virtual_end as u32
            {
                let offset = virtual_line as usize - block.virtual_start;
                let markdown_line = block.content_start as u32 + offset as u32;
                return Some((markdown_line, col));
            }
        }
        None
    }

    pub fn map_location(
        &self,
        virtual_location: Location,
        markdown_uri: Url,
    ) -> Option<Location> {
        let (markdown_line_start, col_start) =
            self.virtual_to_markdown(virtual_location.range.start.line, virtual_location.range.start.character)?;
        let (markdown_line_end, col_end) =
            self.virtual_to_markdown(virtual_location.range.end.line, virtual_location.range.end.character)?;
        Some(Location {
            uri: markdown_uri,
            range: Range {
                start: Position {
                    line: markdown_line_start,
                    character: col_start,
                },
                end: Position {
                    line: markdown_line_end,
                    character: col_end,
                },
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_to_virtual() {
        let blocks = vec![
            CodeBlock {
                lang: "forth".to_string(),
                markdown_start: 2,
                markdown_end: 4,
                content_start: 3,
                content_end: 3,
                virtual_start: 0,
                virtual_end: 2,
                content: ": square ( n -- n ) dup * ;\n".to_string(),
            },
            CodeBlock {
                lang: "forth".to_string(),
                markdown_start: 7,
                markdown_end: 9,
                content_start: 8,
                content_end: 8,
                virtual_start: 2,
                virtual_end: 4,
                content: "5 square .\n".to_string(),
            },
        ];

        let mapper = PositionMapper::new(blocks);
        let (vline, col) = mapper.markdown_to_virtual(3, 5).unwrap();
        assert_eq!(vline, 0);
        assert_eq!(col, 5);

        let (vline, col) = mapper.markdown_to_virtual(8, 2).unwrap();
        assert_eq!(vline, 2);
        assert_eq!(col, 2);
    }

    #[test]
    fn test_virtual_to_markdown() {
        let blocks = vec![
            CodeBlock {
                lang: "forth".to_string(),
                markdown_start: 2,
                markdown_end: 4,
                content_start: 3,
                content_end: 3,
                virtual_start: 0,
                virtual_end: 2,
                content: ": square ( n -- n ) dup * ;\n".to_string(),
            },
            CodeBlock {
                lang: "forth".to_string(),
                markdown_start: 7,
                markdown_end: 9,
                content_start: 8,
                content_end: 8,
                virtual_start: 2,
                virtual_end: 4,
                content: "5 square .\n".to_string(),
            },
        ];

        let mapper = PositionMapper::new(blocks);
        let (mline, col) = mapper.virtual_to_markdown(0, 5).unwrap();
        assert_eq!(mline, 3);
        assert_eq!(col, 5);

        let (mline, col) = mapper.virtual_to_markdown(2, 2).unwrap();
        assert_eq!(mline, 8);
        assert_eq!(col, 2);
    }
}
