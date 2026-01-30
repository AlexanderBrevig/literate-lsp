use serde_json::{Value, json};
use crate::position::PositionMapper;

/// Recursively rewrite all Position and Range objects in a JSON value
/// to_virtual=true: rewrite from markdown to virtual coordinates
/// to_virtual=false: rewrite from virtual to markdown coordinates
pub fn rewrite_positions(
    value: &mut Value,
    mapper: &PositionMapper,
    to_virtual: bool,
) {
    match value {
        Value::Object(map) => {
            // Check if this is a Position object (has exactly "line" and "character")
            if is_position_object(map) {
                if let (Some(line), Some(character)) = (
                    map.get("line").and_then(|v| v.as_u64()),
                    map.get("character").and_then(|v| v.as_u64()),
                ) {
                    let (new_line, new_char) = if to_virtual {
                        mapper
                            .markdown_to_virtual(line as u32, character as u32)
                            .unwrap_or((line as u32, character as u32))
                    } else {
                        mapper
                            .virtual_to_markdown(line as u32, character as u32)
                            .unwrap_or((line as u32, character as u32))
                    };
                    map["line"] = json!(new_line);
                    map["character"] = json!(new_char);
                }
            } else {
                // Recursively process nested objects
                for (_key, val) in map.iter_mut() {
                    rewrite_positions(val, mapper, to_virtual);
                }
            }
        }
        Value::Array(arr) => {
            // Recursively process arrays
            for val in arr.iter_mut() {
                rewrite_positions(val, mapper, to_virtual);
            }
        }
        _ => {}
    }
}

/// Check if a JSON object is a Position (has exactly "line" and "character" fields)
fn is_position_object(map: &serde_json::Map<String, Value>) -> bool {
    map.len() == 2 && map.contains_key("line") && map.contains_key("character")
}
