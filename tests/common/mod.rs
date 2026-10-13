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

// External re-import resolves the exported standard-v2 style defaults. This
// helper materializes them in expected models without deleting any evidence.
pub fn materialize_standard_spacing(node: &mut Node) {
    if matches!(node.kind.as_str(), "paragraph" | "heading") {
        let before = if node.kind == "heading" { 240 } else { 0 };
        for (key, value) in [
            ("spaceBefore", before),
            ("spaceAfter", 120),
            ("lineSpacing", 360),
        ] {
            node.attrs.entry(key).or_insert(json!(value));
        }
    }
    for child in &mut node.content {
        materialize_standard_spacing(child);
    }
}

// Re-importing an authored DOCX now proves its explicit historical A4 section.
// This does not alter trusted reopens or erase newly preserved evidence.
pub fn materialize_exported_page(node: &mut Node) {
    assert_eq!(node.kind, "doc");
    node.attrs.entry("pageGeometry").or_insert(json!({
        "widthTwips":11906,"heightTwips":16838,
        "marginTopTwips":1440,"marginRightTwips":1440,
        "marginBottomTwips":1440,"marginLeftTwips":1440
    }));
}
