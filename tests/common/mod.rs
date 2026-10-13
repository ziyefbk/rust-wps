use rust_wps::document::Node;
use serde_json::{Value, json};

// Size/color regressions compare every character's other marks and the entire
// block structure. Newly preserved script fonts may split the same old range.
pub fn without_font_segmentation(node: &Node) -> Value {
    let mut value = serde_json::to_value(node).unwrap();
    if !node.content.is_empty() {
        value["content"] = Value::Array(
            node.content
                .iter()
                .flat_map(|child| {
                    if child.kind != "text" {
                        return vec![without_font_segmentation(child)];
                    }
                    let mut character = child.clone();
                    character.marks.retain(|mark| mark.kind != "fontFamily");
                    child
                        .text
                        .as_deref()
                        .unwrap_or("")
                        .chars()
                        .map(|text| {
                            character.text = Some(text.to_string());
                            json!(character)
                        })
                        .collect()
                })
                .collect(),
        );
    }
    value
}
