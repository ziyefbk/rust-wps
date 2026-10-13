//! Explicit conversion to an editable copy; never a lossless package editor.
use crate::{
    compatibility::{CompatibilityReport, audit_docx_bytes},
    document::{Mark, Node},
};
use anyhow::{Result, bail, ensure};
use base64::Engine;
use office_oxide::ir::{Element, ImageFormat, InlineContent, List, UnderlineStyle, VerticalAlign};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, io::Read, path::Path};

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportReport {
    pub document: Node,
    pub compatibility: CompatibilityReport,
}

pub fn import_doc(path: &Path) -> Result<ImportReport> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 64 * 1024 * 1024, "Input exceeds 64 MiB");
    ensure!(
        bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]),
        "Input is not a binary DOC compound file"
    );
    let parsed = office_oxide::Document::from_reader(
        std::io::Cursor::new(bytes),
        office_oxide::DocumentFormat::Doc,
    )?;
    let legacy = parsed
        .as_doc()
        .ok_or_else(|| anyhow::anyhow!("Expected DOC parser"))?;
    ensure!(
        legacy.text_complete(),
        "DOC text recovery is incomplete; import refused"
    );
    ensure!(
        legacy.warnings().is_empty(),
        "DOC parser reported uncertain recovery: {:?}",
        legacy.warnings()
    );
    ensure!(
        !legacy.has_macros(),
        "Macro-bearing DOC import is not supported"
    );
    ensure!(
        legacy
            .subdocuments()
            .iter()
            .all(|s| s.text.trim().is_empty()),
        "DOC contains notes, headers, comments or text boxes outside the editor model"
    );
    let mut compatibility = CompatibilityReport {
        paragraphs: 0, tables: 0, drawings: legacy.images().len(),
        risks: BTreeSet::from([
            "Legacy DOC conversion is experimental; unsupported binary features may not be detected".into(),
            "Page layout, fonts and pagination require visual verification".into(),
            "Typography, spacing or alignment outside the basic editor model".into(),
        ]), fidelity_verified: false,
    };
    let has_structured_paragraphs = legacy.has_structured_paragraphs();
    compatibility.risks.insert(if has_structured_paragraphs {
        "Legacy DOC body manual page breaks are preserved; section layout is not preserved".into()
    } else {
        "Legacy DOC page and section breaks may become line or paragraph boundaries; explicit pagination is not preserved".into()
    });
    let source_text = legacy.plain_text();
    let ir = parsed.to_ir();
    ensure!(
        ir.metadata.warnings.is_empty(),
        "DOC structural conversion reported uncertainty: {:?}",
        ir.metadata.warnings
    );
    let mut report = convert_ir(ir, compatibility)?;
    // Only the unstructured fallback needs blank-line recovery. Structured
    // paragraphs now preserve real blank paragraphs and manual page breaks;
    // never reconstruct their boundaries from flattened plain text.
    let body = &mut report.document.content;
    if !has_structured_paragraphs
        && body
            .iter()
            .all(|n| matches!(n.kind.as_str(), "paragraph" | "heading"))
    {
        let lines: Vec<_> = source_text.trim_end_matches('\n').split('\n').collect();
        let actual: Vec<String> = body
            .iter()
            .map(|n| {
                n.content
                    .iter()
                    .map(|s| s.text.as_deref().unwrap_or(""))
                    .collect()
            })
            .collect();
        let nonempty: Vec<_> = lines.iter().copied().filter(|s| !s.is_empty()).collect();
        if actual.iter().map(String::as_str).eq(nonempty) {
            let mut original = std::mem::take(body).into_iter();
            for line in lines {
                body.push(if line.is_empty() {
                    node("paragraph", vec![])
                } else {
                    original.next().expect("matched paragraph sequence")
                });
            }
        } else {
            report
                .compatibility
                .risks
                .insert("Legacy whitespace and paragraph boundaries may be normalized".into());
        }
    }
    fn count(n: &Node, paragraphs: &mut usize, tables: &mut usize) {
        if matches!(n.kind.as_str(), "paragraph" | "heading") {
            *paragraphs += 1;
        }
        if n.kind == "table" {
            *tables += 1;
        }
        for child in &n.content {
            count(child, paragraphs, tables);
        }
    }
    count(
        &report.document,
        &mut report.compatibility.paragraphs,
        &mut report.compatibility.tables,
    );
    Ok(report)
}

pub fn import_docx(path: &Path) -> Result<ImportReport> {
    // Audit and convert the same bounded snapshot: replacing the source path
    // after the audit must not bypass the unsupported-content checks.
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 64 * 1024 * 1024, "Input exceeds 64 MiB");
    let compatibility = audit_docx_bytes(&bytes)?;
    for risk in &compatibility.risks {
        ensure!(
            !matches!(
                risk.as_str(),
                "Tracked changes"
                    | crate::compatibility::HIDDEN_TEXT_RISK
                    | "Custom theme color mapping is unsupported"
                    | "Unsupported theme color scheme content"
                    | "Unsupported or unresolved hyperlink target"
                    | "Fields and generated text"
                    | "Merged table cells"
                    | "Mathematical equations"
                    | "Content controls or embedded content"
                    | "Floating objects, legacy drawings or text boxes"
            ) && !risk.starts_with("Additional document content:")
                && !risk.starts_with("Embedded object or executable content:"),
            "Import cannot preserve {risk}; source remains read-only"
        );
    }
    let parsed = office_oxide::Document::from_reader(
        std::io::Cursor::new(bytes),
        office_oxide::DocumentFormat::Docx,
    )?;
    convert_ir(parsed.to_ir(), compatibility)
}

fn convert_ir(
    ir: office_oxide::ir::DocumentIR,
    mut compatibility: CompatibilityReport,
) -> Result<ImportReport> {
    for warning in &ir.metadata.warnings {
        compatibility
            .risks
            .insert(format!("Parser warning: {warning}"));
    }
    let mut content = Vec::new();
    for section in &ir.sections {
        content.extend(blocks(&section.elements, 0, &mut compatibility)?);
    }
    if content.is_empty() {
        content.push(node("paragraph", vec![]));
    }
    let mut document = node("doc", content);
    document.attrs.insert(
        "styleProfile".into(),
        crate::document::STANDARD_STYLE_PROFILE.into(),
    );
    compatibility
        .risks
        .insert("Default document style normalized in editable copy".into());
    crate::document::validate(&document)?;
    Ok(ImportReport {
        document,
        compatibility,
    })
}

fn node(kind: &str, content: Vec<Node>) -> Node {
    Node {
        kind: kind.into(),
        attrs: Default::default(),
        content,
        text: None,
        marks: vec![],
    }
}

fn inline(items: &[InlineContent], report: &mut CompatibilityReport) -> Result<Vec<Node>> {
    let mut out: Vec<Node> = Vec::new();
    for item in items {
        match item {
            InlineContent::Text(span) => {
                if span.text.is_empty() {
                    continue;
                }
                let mut text = node("text", vec![]);
                text.text = Some(span.text.clone());
                for (enabled, kind) in [
                    (span.bold, "bold"),
                    (span.italic, "italic"),
                    (span.strikethrough, "strike"),
                    (
                        matches!(span.vertical_align, Some(VerticalAlign::Superscript)),
                        "superscript",
                    ),
                    (
                        matches!(span.vertical_align, Some(VerticalAlign::Subscript)),
                        "subscript",
                    ),
                    (
                        span.underline
                            .as_ref()
                            .is_some_and(|u| *u != UnderlineStyle::None),
                        "underline",
                    ),
                ] {
                    if enabled {
                        text.marks.push(Mark {
                            kind: kind.into(),
                            attrs: Default::default(),
                        });
                    }
                }
                if span
                    .underline
                    .as_ref()
                    .is_some_and(|u| !matches!(u, UnderlineStyle::Single | UnderlineStyle::None))
                {
                    report
                        .risks
                        .insert("Underline pattern converted to single underline".into());
                }
                if let Some(half_points) = span.font_size_half_pt {
                    ensure!(
                        (2..=3276).contains(&half_points),
                        "Import cannot preserve font size outside 1–1638 pt; source remains read-only"
                    );
                    text.marks.push(Mark {
                        kind: "fontSize".into(),
                        attrs: serde_json::Map::from_iter([(
                            "halfPoints".into(),
                            half_points.into(),
                        )]),
                    });
                }
                if let Some([red, green, blue]) = span.color {
                    text.marks.push(Mark {
                        kind: "textColor".into(),
                        attrs: serde_json::Map::from_iter([(
                            "rgb".into(),
                            format!("{red:02X}{green:02X}{blue:02X}").into(),
                        )]),
                    });
                }
                if let Some(name) = &span.font_name {
                    ensure!(
                        crate::document::supported_font_family(name),
                        "Import cannot preserve an invalid or overlong font family name; source remains read-only"
                    );
                    text.marks.push(Mark {
                        kind: "fontFamily".into(),
                        attrs: serde_json::Map::from_iter([("name".into(), name.clone().into())]),
                    });
                    report.risks.insert(
                        "Font names are preserved; unavailable fonts may be substituted locally"
                            .into(),
                    );
                }
                if let Some(href) = &span.hyperlink {
                    ensure!(
                        crate::document::supported_link(href),
                        "Import cannot preserve unsupported link target; source remains read-only"
                    );
                    text.marks.push(Mark {
                        kind: "link".into(),
                        attrs: serde_json::Map::from_iter([("href".into(), href.clone().into())]),
                    });
                }
                // Coalesce only identical formatting; different script fonts,
                // effective sizes, colors and link targets keep their ranges.
                if let Some(previous) = out
                    .last_mut()
                    .filter(|previous| previous.kind == "text" && previous.marks == text.marks)
                {
                    previous
                        .text
                        .as_mut()
                        .expect("text node")
                        .push_str(text.text.as_deref().unwrap_or(""));
                } else {
                    out.push(text);
                }
            }
            InlineContent::LineBreak => out.push(node("hardBreak", vec![])),
            _ => bail!(
                "Import requires unsupported note or comment references; source remains read-only"
            ),
        }
    }
    Ok(out)
}

fn list(value: &List, depth: usize, report: &mut CompatibilityReport) -> Result<Node> {
    ensure!(depth <= 24, "Import nesting exceeds limits");
    let mut items = Vec::new();
    for item in &value.items {
        let mut children = blocks(&item.content, depth + 1, report)?;
        if children.is_empty() {
            children.push(node("paragraph", vec![]));
        }
        if let Some(nested) = &item.nested {
            children.push(list(nested, depth + 1, report)?);
        }
        items.push(node("listItem", children));
    }
    let mut result = node(
        if value.ordered {
            "orderedList"
        } else {
            "bulletList"
        },
        items,
    );
    if value.ordered {
        result
            .attrs
            .insert("start".into(), value.start_number.unwrap_or(1).into());
    }
    report
        .risks
        .insert("List marker appearance normalized to editor defaults".into());
    Ok(result)
}

fn blocks(
    elements: &[Element],
    depth: usize,
    report: &mut CompatibilityReport,
) -> Result<Vec<Node>> {
    ensure!(
        depth <= 24 && elements.len() <= 10000,
        "Import structure exceeds limits"
    );
    let mut result = Vec::new();
    for element in elements {
        let converted = match element {
            Element::Paragraph(p) => {
                let mut value = node("paragraph", inline(&p.content, report)?);
                preserve_alignment(&mut value, p.alignment.as_ref())?;
                value
            }
            Element::Heading(h) => {
                let mut value = node("heading", inline(&h.content, report)?);
                ensure!(
                    (1..=3).contains(&h.level),
                    "Heading levels beyond 1–3 are not supported yet"
                );
                value.attrs.insert("level".into(), h.level.into());
                preserve_alignment(&mut value, h.alignment.as_ref())?;
                value
            }
            Element::List(l) => list(l, depth + 1, report)?,
            Element::Table(t) => {
                let mut rows = Vec::new();
                for row in &t.rows {
                    let mut cells = Vec::new();
                    for cell in &row.cells {
                        ensure!(
                            cell.col_span <= 1 && cell.row_span <= 1,
                            "Merged table cells require a richer editor model"
                        );
                        let mut content = blocks(&cell.content, depth + 1, report)?;
                        if content.is_empty() {
                            content.push(node("paragraph", vec![]));
                        }
                        cells.push(node(
                            if row.is_header {
                                "tableHeader"
                            } else {
                                "tableCell"
                            },
                            content,
                        ));
                    }
                    rows.push(node("tableRow", cells));
                }
                report
                    .risks
                    .insert("Table widths, borders and cell formatting normalized".into());
                node("table", rows)
            }
            Element::Image(image) => {
                let mime = match image.format {
                    Some(ImageFormat::Png) => "png",
                    Some(ImageFormat::Jpeg) => "jpeg",
                    _ => bail!("Only PNG/JPEG images can be imported"),
                };
                let data = image
                    .data
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Linked or missing image cannot be imported"))?;
                ensure!(
                    data.len() <= 8 * 1024 * 1024,
                    "Imported image exceeds 8 MiB"
                );
                let mut value = node("image", vec![]);
                value.attrs.insert(
                    "src".into(),
                    format!(
                        "data:image/{mime};base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(data)
                    )
                    .into(),
                );
                if let Some(alt) = &image.alt_text {
                    value.attrs.insert("alt".into(), alt.clone().into());
                }
                report
                    .risks
                    .insert("Image position and display dimensions normalized".into());
                value
            }
            Element::PageBreak => {
                ensure!(
                    depth == 0,
                    "Page breaks inside lists or tables require a richer editor model"
                );
                node("pageBreak", vec![])
            }
            Element::ColumnBreak => {
                report
                    .risks
                    .insert("Column breaks converted to paragraph boundaries".into());
                node("paragraph", vec![])
            }
            _ => bail!("Unsupported block in DOCX import; source remains read-only"),
        };
        result.push(converted);
    }
    Ok(result)
}

fn preserve_alignment(
    node: &mut Node,
    alignment: Option<&office_oxide::ir::ParagraphAlignment>,
) -> Result<()> {
    use office_oxide::ir::ParagraphAlignment;
    let Some(alignment) = alignment else {
        return Ok(());
    };
    let value = match alignment {
        ParagraphAlignment::Left => "left",
        ParagraphAlignment::Center => "center",
        ParagraphAlignment::Right => "right",
        ParagraphAlignment::Justify => "justify",
        ParagraphAlignment::Distribute => {
            bail!("Distributed paragraph alignment is not supported; source remains read-only")
        }
    };
    node.attrs.insert("textAlign".into(), value.into());
    Ok(())
}
