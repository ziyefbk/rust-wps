//! Explicit body properties only; never infer layout/master inheritance.
use anyhow::{Result, ensure};
use quick_xml::{
    NsReader,
    events::{BytesStart, Event},
    name::ResolveResult,
};
use std::collections::BTreeMap;
fn attribute(e: &BytesStart<'_>, name: &str) -> Result<Option<String>> {
    for a in e.attributes() {
        let a = a?;
        if a.key.as_ref() == name {
            ensure!(
                a.value.len() <= 4096,
                "PPTX vertical property attribute exceeds limit"
            );
            return Ok(Some(
                a.normalized_value(quick_xml::XmlVersion::Implicit1_0)?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}
pub(crate) fn slide_targets(xml: &[u8]) -> Result<BTreeMap<String, Option<String>>> {
    let mut reader = NsReader::from_reader(xml);
    let mut result = BTreeMap::new();
    let mut depth: usize = 0;
    let mut root = false;
    loop {
        let (ns, event) = reader.read_resolved_event()?;
        let package = matches!(ns,ResolveResult::Bound(ns) if ns.as_ref()=="http://schemas.openxmlformats.org/package/2006/relationships");
        let empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                if depth == 0 {
                    root = package && e.local_name().as_ref() == "Relationships";
                }
                if depth == 1 && root && package && e.local_name().as_ref() == "Relationship" {
                    let id = attribute(&e, "Id")?;
                    let target = attribute(&e, "Target")?;
                    let kind = attribute(&e, "Type")?;
                    let mode = attribute(&e, "TargetMode")?;
                    if let Some(id) = id {
                        let path = if kind.as_deref()
                            == Some(
                                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide",
                            )
                            && matches!(mode.as_deref(), None | Some("Internal"))
                        {
                            target.as_deref().and_then(normalize_target)
                        } else {
                            None
                        };
                        result.entry(id).and_modify(|v| *v = None).or_insert(path);
                    }
                }
                if !empty {
                    depth += 1;
                }
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(result)
}

fn normalize_target(target: &str) -> Option<String> {
    if target.contains(['\\', ':', '?', '#', '%']) {
        return None;
    }
    let mut parts = if target.starts_with('/') {
        vec![]
    } else {
        vec!["ppt"]
    };
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

struct PendingShape {
    depth: usize,
    id: Option<u32>,
    anchor: Option<String>,
    ids: usize,
    bodies: usize,
}
pub(crate) fn anchors(xml: &[u8]) -> Result<BTreeMap<u32, Option<String>>> {
    let mut reader = NsReader::from_reader(xml);
    let mut stack: Vec<(u8, String)> = vec![];
    let mut shape: Option<PendingShape> = None;
    let mut result = BTreeMap::new();
    loop {
        let (ns, event) = reader.read_resolved_event()?;
        let ns = match ns {
            ResolveResult::Bound(ns)
                if ns.as_ref() == "http://schemas.openxmlformats.org/presentationml/2006/main" =>
            {
                1
            }
            ResolveResult::Bound(ns)
                if ns.as_ref() == "http://schemas.openxmlformats.org/drawingml/2006/main" =>
            {
                2
            }
            _ => 0,
        };
        let empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let tag = e.local_name().as_ref().to_owned();
                if ns == 1
                    && tag == "sp"
                    && stack.len() >= 3
                    && stack[..3].iter().map(|(n, t)| (*n, t.as_str())).eq([
                        (1, "sld"),
                        (1, "cSld"),
                        (1, "spTree"),
                    ])
                    && stack[3..].iter().all(|(n, t)| *n == 1 && t == "grpSp")
                    && !empty
                {
                    shape = Some(PendingShape {
                        depth: stack.len(),
                        id: None,
                        anchor: None,
                        ids: 0,
                        bodies: 0,
                    });
                }
                if let Some(state) = shape.as_mut() {
                    if stack.len() == state.depth + 2
                        && ns == 1
                        && tag == "cNvPr"
                        && stack.last().is_some_and(|(n, t)| *n == 1 && t == "nvSpPr")
                    {
                        state.ids += 1;
                        state.id = if state.ids == 1 {
                            attribute(&e, "id")?.and_then(|v| v.parse().ok())
                        } else {
                            None
                        };
                    }
                    if stack.len() == state.depth + 2
                        && ns == 2
                        && tag == "bodyPr"
                        && stack.last().is_some_and(|(n, t)| *n == 1 && t == "txBody")
                    {
                        state.bodies += 1;
                        state.anchor = if state.bodies == 1 {
                            attribute(&e, "anchor")?.map(|a| {
                                if ["t", "ctr", "b", "just", "dist"].contains(&a.as_str()) {
                                    a
                                } else {
                                    "unsupported".into()
                                }
                            })
                        } else {
                            None
                        };
                    }
                }
                if !empty {
                    stack.push((ns, tag));
                }
            }
            Event::End(_) => {
                stack.pop();
                if shape
                    .as_ref()
                    .is_some_and(|state| state.depth == stack.len())
                    && let Some(PendingShape {
                        id: Some(id),
                        anchor,
                        ..
                    }) = shape.take()
                {
                    result.entry(id).and_modify(|v| *v = None).or_insert(anchor);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn slide(shapes: &str) -> String {
        format!(
            r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:x="urn:wrong"><p:cSld><p:spTree>{shapes}</p:spTree></p:cSld></p:sld>"#
        )
    }
    fn shape(id: u32, body: &str) -> String {
        format!(
            r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}"/></p:nvSpPr><p:txBody>{body}</p:txBody></p:sp>"#
        )
    }
    #[test]
    fn exact_paths_namespaces_entities_and_ambiguities() {
        let xml = slide(&format!(
            "{}{}{}{}",
            shape(1, r#"<a:bodyPr anchor="c&#116;r"/>"#),
            shape(2, r#"<x:bodyPr anchor="b"/>"#),
            shape(3, r#"<a:bodyPr anchor="t"/><a:bodyPr anchor="b"/>"#),
            shape(4, r#"<a:bodyPr anchor="unknown"/>"#)
        ));
        let values = anchors(xml.as_bytes()).unwrap();
        assert_eq!(values[&1].as_deref(), Some("ctr"));
        assert_eq!(values[&2], None);
        assert_eq!(values[&3], None);
        assert_eq!(values[&4].as_deref(), Some("unsupported"));
        let repeated = slide(&format!(
            "{}{}",
            shape(1, r#"<a:bodyPr anchor="t"/>"#),
            shape(1, r#"<a:bodyPr anchor="b"/>"#)
        ));
        assert_eq!(anchors(repeated.as_bytes()).unwrap()[&1], None);
        let wrong = slide(
            r#"<x:sp><p:nvSpPr><p:cNvPr id="1"/></p:nvSpPr><p:txBody><a:bodyPr anchor="b"/></p:txBody></x:sp>"#,
        );
        assert!(anchors(wrong.as_bytes()).unwrap().is_empty());
    }
    #[test]
    fn relationship_identity_and_paths_are_unambiguous() {
        let kind = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";
        let xml = format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="r2" Type="{kind}" Target="slides/slide2.xml"/><Relationship Id="r1" Type="{kind}" Target="/ppt/slides/slide1.xml"/><Relationship Id="r3" Type="{kind}" Target="slides/slide3.xml"/><Relationship Id="r3" Type="other" Target="other"/><Relationship Id="r4" Type="{kind}" TargetMode="External" Target="https://example.com/a"/></Relationships>"#
        );
        let values = slide_targets(xml.as_bytes()).unwrap();
        assert_eq!(values["r1"].as_deref(), Some("ppt/slides/slide1.xml"));
        assert_eq!(values["r2"].as_deref(), Some("ppt/slides/slide2.xml"));
        assert_eq!(values["r3"], None);
        assert_eq!(values["r4"], None);
        assert_eq!(normalize_target("../../outside"), None);
        assert_eq!(normalize_target("slides/%73lide1.xml"), None);
        assert_eq!(
            normalize_target("../ppt/slides/./slide1.xml").as_deref(),
            Some("ppt/slides/slide1.xml")
        );
    }
}
