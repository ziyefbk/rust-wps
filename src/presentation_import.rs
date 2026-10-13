//! Audited basic-model conversion. This is never a lossless package import.
use crate::presentation_model::{Picture, Presentation, Slide, TextBox};
use anyhow::{Result, ensure};
use quick_xml::events::Event;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportReport {
    pub model: Option<Presentation>,
    pub risks: Vec<String>,
    pub blockers: Vec<String>,
}
pub fn import_pptx(path: &Path) -> Result<ImportReport> {
    import_pptx_bytes(&crate::spreadsheet::read_xlsx_snapshot(path)?)
}
pub fn import_pptx_bytes(bytes: &[u8]) -> Result<ImportReport> {
    let report = crate::presentation::inspect_pptx_bytes(bytes)?;
    let parts = crate::presentation_edit::read_parts(bytes)?;
    let mut blockers = BTreeSet::new();
    if report.has_macros {
        blockers.insert("宏演示不支持编辑导入".into());
    }
    if !report.unreadable_parts.is_empty() {
        blockers.insert("有部件读取失败".into());
    }
    for (name, data) in &parts {
        if name == "[Content_Types].xml" && String::from_utf8_lossy(data).contains("macroEnabled") {
            blockers.insert("宏类型演示不支持导入".into());
        }
        let metadata = name.starts_with("docProps/") || name.starts_with("ppt/theme/");
        let allowed = metadata
            || name == "[Content_Types].xml"
            || name == "_rels/.rels"
            || name == "ppt/presentation.xml"
            || name == "ppt/presProps.xml"
            || name == "ppt/viewProps.xml"
            || name == "ppt/tableStyles.xml"
            || name == "ppt/_rels/presentation.xml.rels"
            || name.starts_with("ppt/slides/")
            || name.starts_with("ppt/slideMasters/")
            || name.starts_with("ppt/slideLayouts/")
            || name.starts_with("ppt/notesSlides/")
            || name.starts_with("ppt/notesMasters/")
            || name.starts_with("ppt/media/")
            || name == "rust-wps/presentation-v1.json";
        if !allowed {
            blockers.insert(format!(
                "未支持部件：{}",
                name.chars().take(160).collect::<String>()
            ));
        }
        if !(name.ends_with(".xml") || name.ends_with(".rels")) {
            continue;
        }
        let template =
            name.starts_with("ppt/slideMasters/") || name.starts_with("ppt/slideLayouts/");
        if template && template_has_static_text(data)? {
            blockers.insert("母版或布局含文字，当前无法证明逐页继承内容完整".into());
        }
        let mut reader = quick_xml::Reader::from_reader(data.as_slice());
        loop {
            match reader.read_event()? {
                Event::Start(e) | Event::Empty(e) => {
                    let local = e.local_name();
                    let tag = local.as_ref();
                    if name == "ppt/tableStyles.xml" && tag != "tblStyleLst" {
                        blockers.insert("表格样式表含实际样式或扩展，暂不导入".into());
                    }
                    if template && matches!(tag, "pic" | "grpSp" | "graphicFrame") {
                        blockers.insert("母版或布局含对象，当前无法完整恢复继承内容".into());
                    }
                    if !metadata
                        && matches!(
                            tag,
                            "graphicFrame"
                                | "grpSp"
                                | "cxnSp"
                                | "oleObj"
                                | "control"
                                | "timing"
                                | "transition"
                                | "AlternateContent"
                                | "extLst"
                                | "srcRect"
                                | "hlinkClick"
                                | "hlinkMouseOver"
                                | "videoFile"
                                | "audioFile"
                                | "custShowLst"
                                | "sectionLst"
                                | "photoAlbum"
                                | "showPr"
                                | "custGeom"
                        )
                    {
                        blockers.insert(format!("未支持特征：{}", tag));
                    }
                    for attribute in e.attributes() {
                        let attribute = attribute?;
                        let key = attribute.key.local_name();
                        if tag == "prstGeom"
                            && key.as_ref() == "prst"
                            && attribute.value.as_ref() != "rect"
                        {
                            blockers.insert("非矩形几何对象暂不支持转换".into());
                        }
                        let value =
                            attribute.normalized_value(quick_xml::XmlVersion::Implicit1_0)?;
                        let value = value.trim();
                        // OOXML uses XML Schema booleans (false/0, true/1).
                        // Rotation is an integer; equivalent lexical zeroes
                        // must not turn an untransformed object into a blocker.
                        if (key.as_ref() == "rot" && value.parse::<i32>() != Ok(0))
                            || (matches!(key.as_ref(), "flipH" | "flipV")
                                && !matches!(value, "0" | "false"))
                            || (key.as_ref() == "TargetMode" && value == "External")
                        {
                            blockers.insert("旋转、翻转或外部链接需要保留，暂不导入".into());
                        }
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
        ensure!(
            blockers.len() <= 128,
            "Too many unsupported presentation features"
        );
    }
    let mut model = Presentation {
        version: 1,
        size_emu: report.size_emu.unwrap_or([0, 0]),
        slides: Vec::new(),
    };
    let mut inherited_positions = 0usize;
    for page in report.slides {
        let mut slide = Slide {
            hidden: page.hidden,
            notes: page.notes,
            text_boxes: Vec::new(),
            images: Vec::new(),
        };
        for shape in page.shapes {
            let Some(bounds) = shape
                .bounds_emu
                .or(shape.layout_bounds_emu)
                .or(shape.master_bounds_emu)
            else {
                blockers.insert("对象缺少直接位置或可唯一解析的继承位置".into());
                continue;
            };
            if shape.bounds_emu.is_none() {
                inherited_positions += 1;
            }
            if !shape.children.is_empty() {
                blockers.insert("组合对象暂不支持".into());
                continue;
            }
            if shape.kind == "picture" {
                if let Some(data_url) = shape.image {
                    slide.images.push(Picture {
                        bounds_emu: bounds,
                        data_url,
                        alt: shape.name,
                    });
                } else {
                    blockers.insert("图片格式或内容无法导入".into());
                }
            } else if shape.kind == "shape" && !shape.text.is_empty() {
                let style = shape
                    .runs
                    .iter()
                    .find(|r| !r.text.is_empty() && r.text != "\n");
                let key = |r: &crate::presentation::RunReport| {
                    (r.font_size_hundredths_pt, r.bold, r.italic, r.color_rgb)
                };
                if shape
                    .runs
                    .iter()
                    .filter(|r| !r.text.is_empty() && r.text != "\n")
                    .any(|r| Some(key(r)) != style.map(key))
                {
                    blockers.insert("文本框含混合字符样式，当前模型无法保留".into());
                    continue;
                }
                slide.text_boxes.push(TextBox {
                    vertical_alignment: match shape.vertical_alignment.as_deref() {
                        Some("t") => Some(crate::presentation_model::VerticalAlignment::Top),
                        Some("ctr") => Some(crate::presentation_model::VerticalAlignment::Middle),
                        Some("b") => Some(crate::presentation_model::VerticalAlignment::Bottom),
                        _ => None,
                    },
                    alignment: match shape.alignment.as_deref() {
                        Some("l") => Some(crate::presentation_model::TextAlignment::Left),
                        Some("ctr") => Some(crate::presentation_model::TextAlignment::Center),
                        Some("r") => Some(crate::presentation_model::TextAlignment::Right),
                        _ => None,
                    },
                    bounds_emu: bounds,
                    text: shape.text,
                    font_size_hundredths_pt: style
                        .and_then(|r| r.font_size_hundredths_pt)
                        .unwrap_or(2400),
                    bold: style.and_then(|r| r.bold).unwrap_or(false),
                    italic: style.and_then(|r| r.italic).unwrap_or(false),
                    color_rgb: style.and_then(|r| r.color_rgb).unwrap_or([0, 0, 0]),
                });
            } else {
                blockers.insert(format!("对象无法映射为文字或图片：{}", shape.kind));
            }
        }
        model.slides.push(slide);
    }
    if let Err(error) = model.validate() {
        blockers.insert(format!("编辑模型限制：{error:#}"));
    }
    let mut risks=vec!["这是重新生成的基础编辑副本，不保留原始 OOXML 包、对象 ID 或元数据。".into(),
        "统一显式左/中/右对齐可保留；混合、继承或其他对齐规范化为左对齐；显式顶部/居中/底部垂直对齐可保留；缺省、继承和其他垂直对齐规范化为顶部。主题、字体族、其他段落格式、项目符号、填充、边框、背景和效果不保留；段落边界与软换行统一为换行并导出为软换行；未指定字符样式使用 24pt 黑色常规文字。".into(),
        "文本框先于图片绘制，原始叠放顺序可能改变；图片替代文字使用对象名称。视图设置不保留，空表格样式表不带入副本。请逐页检查版面。".into()];
    if inherited_positions > 0 {
        risks.push(format!("{inherited_positions} 个对象的唯一布局/母版位置已固定为页面直接位置，副本不再随模板变化；主题和段落样式仍按上述规则规范化。"));
    }
    Ok(ImportReport {
        model: if blockers.is_empty() {
            Some(model)
        } else {
            None
        },
        risks,
        blockers: blockers.into_iter().collect(),
    })
}

// A placeholder's template text is an authoring prompt, not rendered content.
// Only the exact PresentationML non-visual-property path establishes that
// identity; a similarly named element in another namespace must not hide text.
fn template_has_static_text(xml: &[u8]) -> Result<bool> {
    use quick_xml::name::ResolveResult;
    let mut reader = quick_xml::NsReader::from_reader(xml);
    let mut stack: Vec<(u8, String)> = Vec::new();
    let mut shapes: Vec<(usize, bool)> = Vec::new();
    loop {
        let (namespace, event) = reader.read_resolved_event()?;
        let empty = matches!(event, Event::Empty(_));
        let ns = match namespace {
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
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let tag = e.local_name().as_ref().to_owned();
                if ns == 1
                    && tag == "ph"
                    && stack.len() >= 3
                    && stack[stack.len() - 3..]
                        .iter()
                        .map(|(ns, name)| (*ns, name.as_str()))
                        .eq([(1, "sp"), (1, "nvSpPr"), (1, "nvPr")])
                    && let Some((depth, placeholder)) = shapes.last_mut()
                    && *depth == stack.len() - 2
                {
                    *placeholder = true;
                }
                if !empty {
                    stack.push((ns, tag));
                    if ns == 1 && stack.last().is_some_and(|(_, name)| name == "sp") {
                        shapes.push((stack.len(), false));
                    }
                }
            }
            Event::End(_) => {
                if shapes
                    .last()
                    .is_some_and(|(depth, _)| *depth == stack.len())
                {
                    shapes.pop();
                }
                stack.pop();
            }
            Event::Text(e) if !e.as_ref().is_empty() => {
                if stack.last().is_some_and(|(ns, name)| {
                    name == "t"
                        && (*ns != 2 || !shapes.last().is_some_and(|(_, placeholder)| *placeholder))
                }) {
                    return Ok(true);
                }
            }
            Event::CData(e) if !e.as_ref().is_empty() => {
                if stack.last().is_some_and(|(ns, name)| {
                    name == "t"
                        && (*ns != 2 || !shapes.last().is_some_and(|(_, placeholder)| *placeholder))
                }) {
                    return Ok(true);
                }
            }
            Event::Eof => return Ok(false),
            _ => {}
        }
    }
}

#[cfg(test)]
mod template_tests {
    use super::template_has_static_text;
    fn inspect(shapes: &str) -> bool {
        let xml = format!(
            r#"<p:sldMaster xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:x="urn:unknown"><p:cSld><p:spTree>{shapes}</p:spTree></p:cSld></p:sldMaster>"#
        );
        template_has_static_text(xml.as_bytes()).unwrap()
    }
    #[test]
    fn prompts_require_exact_namespace_and_nonvisual_shape_property_path() {
        for ph in ["<p:ph/>", "<p:ph></p:ph>"] {
            assert!(!inspect(&format!(
                "<p:sp><p:nvSpPr><p:nvPr>{ph}</p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>Click to edit</a:t></a:r></a:p></p:txBody></p:sp>"
            )));
        }
        for ph in ["<x:ph/>", "<a:ph/>"] {
            assert!(inspect(&format!(
                "<p:sp><p:nvSpPr><p:nvPr>{ph}</p:nvPr></p:nvSpPr><a:t>Visible text</a:t></p:sp>"
            )));
        }
        assert!(inspect("<p:sp><p:ph/><a:t>Wrong path</a:t></p:sp>"));
        assert!(inspect(
            "<p:sp><p:nvSpPr><p:nvPr><p:ph/></p:nvPr></p:nvSpPr><x:t>Unknown text semantics</x:t></p:sp>"
        ));
    }
    #[test]
    fn placeholder_identity_does_not_hide_static_sibling_or_cdata_text() {
        let prompt = "<p:sp><p:nvSpPr><p:nvPr><p:ph/></p:nvPr></p:nvSpPr><a:t>Prompt</a:t></p:sp>";
        assert!(inspect(&format!(
            "{prompt}<p:sp><a:t><![CDATA[Company footer]]></a:t></p:sp>"
        )));
        assert!(inspect("<a:t>Outside placeholder</a:t>"));
        assert!(inspect(
            "<p:sp><a:t>Before placeholder declaration</a:t><p:nvSpPr><p:nvPr><p:ph/></p:nvPr></p:nvSpPr></p:sp>"
        ));
    }
}
