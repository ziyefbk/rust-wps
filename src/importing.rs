//! Explicit conversion to an editable copy; never a lossless package editor.
use crate::{
    compatibility::{CompatibilityReport, audit_docx_bytes_with_geometry},
    document::{Mark, Node},
};
use anyhow::{Context, Result, bail, ensure};
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
    let mut report = convert_ir(ir, compatibility, false)?;
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
    let audit = audit_docx_bytes_with_geometry(&bytes)?;
    let compatibility = audit.compatibility;
    let blocking: Vec<_> = compatibility
        .risks
        .iter()
        .filter(|risk| {
            matches!(
                risk.as_str(),
                "Tracked changes"
                    | crate::compatibility::HIDDEN_TEXT_RISK
                    | crate::compatibility::PARAGRAPH_SPACING_RISK
                    | crate::paragraph_indentation::PARAGRAPH_INDENT_RISK
                    | crate::page_geometry::PAGE_GEOMETRY_RISK
                    | crate::table_widths::TABLE_GEOMETRY_RISK
                    | crate::image_dimensions::IMAGE_GEOMETRY_RISK
                    | "Custom theme color mapping is unsupported"
                    | "Unsupported theme color scheme content"
                    | "Unsupported or unresolved hyperlink target"
                    | "Fields and generated text"
                    | "Merged table cells"
                    | "Mathematical equations"
                    | "Content controls or embedded content"
                    | "Floating objects, legacy drawings or text boxes"
            ) || risk.starts_with("Additional document content:")
                || risk.starts_with("Embedded object or executable content:")
        })
        .map(String::as_str)
        .collect();
    ensure!(
        blocking.is_empty(),
        "Import cannot preserve {}; source remains read-only",
        blocking.join("; ")
    );
    let parsed = office_oxide::Document::from_reader(
        std::io::Cursor::new(bytes),
        office_oxide::DocumentFormat::Docx,
    )?;
    if let Some(page) = &audit.page {
        crate::page_geometry::verify_parsed(
            page,
            parsed
                .as_docx()
                .ok_or_else(|| anyhow::anyhow!("Expected DOCX parser"))?,
        )?;
    }
    let mut report = convert_ir(parsed.to_ir(), compatibility, true)?;
    if let Some(page) = audit.page {
        report
            .document
            .attrs
            .insert("pageGeometry".into(), serde_json::to_value(page)?);
    }
    // Attach only geometry proven from the exact audited package snapshot.
    // A count or shape mismatch means the converter discarded table structure.
    fn attach_widths(
        node: &mut Node,
        widths: &mut std::slice::Iter<'_, crate::table_widths::TableGeometry>,
    ) -> Result<()> {
        if node.kind == "table" {
            let width = widths
                .next()
                .ok_or_else(|| anyhow::anyhow!("Table metadata does not match imported tables"))?;
            ensure!(
                node.content
                    .iter()
                    .map(|row| row.content.len())
                    .eq(width.row_cells.iter().copied()),
                "Table metadata does not match imported rows and cells"
            );
            if let Some(alignment) = &width.alignment {
                node.attrs
                    .insert("tableAlign".into(), serde_json::json!(alignment));
            }
            if let Some(margins) = width.margins {
                node.attrs.insert(
                    "tableCellMarginsTwips".into(),
                    serde_json::to_value(margins)?,
                );
            }
            if let Some(indent) = width.indent {
                node.attrs
                    .insert("tableIndentTwips".into(), serde_json::json!(indent));
            }
            if let Some(width) = &width.widths {
                node.attrs
                    .insert("columnWidthsTwips".into(), serde_json::json!(width));
            }
        }
        for child in &mut node.content {
            attach_widths(child, widths)?;
        }
        Ok(())
    }
    let mut widths = audit.tables.iter();
    attach_widths(&mut report.document, &mut widths)?;
    ensure!(
        widths.next().is_none(),
        "Source table was discarded during import"
    );
    if let Some(page) = crate::document::page_geometry(&report.document)? {
        for table in report
            .document
            .content
            .iter()
            .filter(|node| node.kind == "table")
        {
            if !table.attrs.contains_key("columnWidthsTwips")
                && !table.attrs.contains_key("tableAlign")
                && !table.attrs.contains_key("tableIndentTwips")
                && crate::document::table_layout_width(table, Some(&page))? < 9000
            {
                report
                    .compatibility
                    .risks
                    .insert(crate::page_geometry::PAGE_AUTO_WIDTH_RISK.into());
            }
        }
    }
    // Keep the existing warned image-only normalization separate from new
    // text indentation. Remove only exact, independently proved placeholders
    // before matching the remaining body/cell paragraph indentation targets.
    if verify_image_conversion(&mut report.document, &audit.images)? {
        report.compatibility.risks.insert("Image-only paragraph indentation normalized; text paragraph indentation preservation does not include images".into());
    }
    crate::paragraph_indentation::verify_converted(&audit.paragraph_indents, &report.document)?;
    if !audit.images.is_empty() {
        report
            .compatibility
            .risks
            .insert(crate::image_dimensions::IMAGE_FORMATTING_RISK.into());
    }
    if audit
        .images
        .iter()
        .any(|image| image.paragraph.paragraph_font.is_none())
    {
        report
            .compatibility
            .risks
            .insert(crate::image_dimensions::IMAGE_BASELINE_RISK.into());
    }
    // Missing effective DOCX spacing means OOXML's zero paragraph gaps and
    // single line spacing, not the editable profile's 6 pt / 1.5 defaults.
    // Trusted authored models bypass external conversion and retain omission.
    fn materialize(node: &mut Node) {
        if matches!(node.kind.as_str(), "paragraph" | "heading") {
            for (key, value) in [("spaceBefore", 0), ("spaceAfter", 0), ("lineSpacing", 240)] {
                node.attrs.entry(key).or_insert(value.into());
            }
        }
        for child in &mut node.content {
            materialize(child);
        }
    }
    materialize(&mut report.document);
    crate::paragraph_contextual_spacing::apply(&audit.contextual_spacing, &mut report.document)
        .with_context(|| {
            format!(
                "{}; source remains read-only",
                crate::compatibility::PARAGRAPH_SPACING_RISK
            )
        })?;
    crate::document::validate_paragraph_indents(&report.document).with_context(|| {
        format!(
            "{}; source remains read-only",
            crate::paragraph_indentation::PARAGRAPH_INDENT_RISK
        )
    })?;
    crate::document::validate(&report.document)?;
    Ok(report)
}

// Source-order count alone is insufficient: verify the exact decoded media and
// declared format and dimensions against geometry proven by the package audit.
fn verify_image_conversion(
    document: &mut Node,
    images: &[crate::image_dimensions::ImageGeometry],
) -> Result<bool> {
    use sha2::{Digest, Sha256};
    fn visit(
        node: &mut Node,
        images: &mut std::slice::Iter<'_, crate::image_dimensions::ImageGeometry>,
    ) -> Result<()> {
        if node.kind == "image" {
            let source = images
                .next()
                .ok_or_else(|| anyhow::anyhow!("Image metadata does not match imported images"))?;
            let src = node
                .attrs
                .get("src")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("Imported image has no source"))?;
            let prefix = format!("data:{};base64,", source.mime);
            let encoded = src.strip_prefix(&prefix).ok_or_else(|| {
                anyhow::anyhow!("Imported image MIME differs from audited source")
            })?;
            let bytes = base64::engine::general_purpose::STANDARD.decode(encoded)?;
            ensure!(
                <[u8; 32]>::from(Sha256::digest(&bytes)) == source.sha256,
                "Imported image bytes differ from audited source"
            );
            ensure!(
                crate::document::image_display_dimensions(node)?
                    == Some((source.width_emu, source.height_emu)),
                "Imported image dimensions differ from audited source"
            );
            node.attrs.insert(
                "imageParagraph".into(),
                serde_json::to_value(&source.paragraph)?,
            );
        }
        for child in &mut node.content {
            visit(child, images)?;
        }
        Ok(())
    }
    let mut images = images.iter();
    visit(document, &mut images)?;
    ensure!(
        images.next().is_none(),
        "Source image was discarded during import"
    );
    // The pinned converter hoists each inline picture AFTER its surrounding
    // paragraph. The audit admits only one picture and no text in that body
    // paragraph, so the preceding empty node is its placeholder, not a source
    // blank paragraph. Remove exactly that one; never collapse neighboring
    // genuine empty paragraphs. Refuse if this parser invariant changes.
    let mut normalized_indentation = false;
    let mut content: Vec<Node> = Vec::with_capacity(document.content.len());
    for child in std::mem::take(&mut document.content) {
        if child.kind == "image" {
            ensure!(
                content.last().is_some_and(|previous| matches!(
                    previous.kind.as_str(),
                    "paragraph" | "heading"
                ) && previous.content.is_empty()
                    && previous.text.is_none()),
                "Image paragraph does not match the audited converter structure"
            );
            let previous = content.last().expect("checked placeholder");
            let paragraph = crate::document::image_paragraph(&child)?
                .context("Missing audited image paragraph")?;
            for (key, expected, default) in [
                ("spaceBefore", paragraph.space_before, 0),
                ("spaceAfter", paragraph.space_after, 0),
                ("lineSpacing", paragraph.line_spacing, 240),
            ] {
                ensure!(
                    previous
                        .attrs
                        .get(key)
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(default)
                        == expected,
                    "Image paragraph {key} differs from audited source"
                );
            }
            ensure!(
                previous
                    .attrs
                    .get("textAlign")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("left")
                    == paragraph.text_align,
                "Image paragraph alignment differs from audited source"
            );
            normalized_indentation |= crate::document::PARAGRAPH_INDENT_FIELDS
                .iter()
                .any(|key| previous.attrs.contains_key(*key));
            content.pop();
        }
        content.push(child);
    }
    document.content = content;
    Ok(normalized_indentation)
}

fn convert_ir(
    ir: office_oxide::ir::DocumentIR,
    mut compatibility: CompatibilityReport,
    spacing: bool,
) -> Result<ImportReport> {
    for warning in &ir.metadata.warnings {
        compatibility
            .risks
            .insert(format!("Parser warning: {warning}"));
    }
    let mut content = Vec::new();
    for section in &ir.sections {
        // Defense in depth: a parser change must not let side-story content
        // disappear just because the package preflight missed a declaration.
        ensure!(
            section.header.is_none()
                && section.footer.is_none()
                && section.first_page_header.is_none()
                && section.first_page_footer.is_none()
                && section.even_page_header.is_none()
                && section.even_page_footer.is_none(),
            "Import cannot preserve headers or footers; source remains read-only"
        );
        content.extend(blocks(&section.elements, 0, &mut compatibility, spacing)?);
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
    // DOCX layout is validated after audited page/table geometry has been
    // attached. Validating new indentation against a temporary default width
    // would falsely reject wider pages or cells before their context exists.
    if !spacing {
        crate::document::validate(&document)?;
    }
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

fn list(
    value: &List,
    depth: usize,
    report: &mut CompatibilityReport,
    spacing: bool,
) -> Result<Node> {
    ensure!(depth <= 24, "Import nesting exceeds limits");
    let mut items = Vec::new();
    for item in &value.items {
        let mut children = blocks(&item.content, depth + 1, report, spacing)?;
        if children.is_empty() {
            children.push(node("paragraph", vec![]));
        }
        // Numbering has its own indentation model. The source audit admits only
        // the existing normalized list route; never attach body-indent metadata
        // to list paragraphs or quietly reinterpret numbering offsets.
        fn omit_list_indents(node: &mut Node) {
            for key in crate::document::PARAGRAPH_INDENT_FIELDS {
                node.attrs.remove(key);
            }
            for child in &mut node.content {
                omit_list_indents(child);
            }
        }
        for child in &mut children {
            omit_list_indents(child);
        }
        if let Some(nested) = &item.nested {
            children.push(list(nested, depth + 1, report, spacing)?);
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
    if spacing {
        report
            .risks
            .insert("List numbering indentation normalized to editor defaults".into());
    }
    Ok(result)
}

fn blocks(
    elements: &[Element],
    depth: usize,
    report: &mut CompatibilityReport,
    spacing: bool,
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
                if spacing {
                    preserve_indents(
                        &mut value,
                        p.indent_left_twips,
                        p.indent_right_twips,
                        p.first_line_indent_twips,
                    );
                    preserve_spacing(
                        &mut value,
                        p.space_before_twips,
                        p.space_after_twips,
                        p.line_spacing.as_ref(),
                    )?;
                }
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
                if spacing {
                    preserve_indents(
                        &mut value,
                        h.indent_left_twips,
                        h.indent_right_twips,
                        h.first_line_indent_twips,
                    );
                    preserve_spacing(
                        &mut value,
                        h.space_before_twips,
                        h.space_after_twips,
                        h.line_spacing.as_ref(),
                    )?;
                }
                value
            }
            Element::List(l) => list(l, depth + 1, report, spacing)?,
            Element::Table(t) => {
                let mut rows = Vec::new();
                for row in &t.rows {
                    let mut cells = Vec::new();
                    for cell in &row.cells {
                        ensure!(
                            cell.col_span <= 1 && cell.row_span <= 1,
                            "Merged table cells require a richer editor model"
                        );
                        let mut content = blocks(&cell.content, depth + 1, report, spacing)?;
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
                report.risks.insert(if spacing {
                    crate::table_widths::TABLE_FORMATTING_RISK.into()
                } else {
                    "Table widths, borders and cell formatting normalized".into()
                });
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
                if spacing {
                    let width = image
                        .display_width_emu
                        .ok_or_else(|| anyhow::anyhow!("Imported image width is missing"))?;
                    let height = image
                        .display_height_emu
                        .ok_or_else(|| anyhow::anyhow!("Imported image height is missing"))?;
                    value.attrs.insert("displayWidthEmu".into(), width.into());
                    value.attrs.insert("displayHeightEmu".into(), height.into());
                }
                if !spacing {
                    report
                        .risks
                        .insert("Image position and display dimensions normalized".into());
                }
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

fn preserve_indents(node: &mut Node, left: Option<i32>, right: Option<i32>, first: Option<i32>) {
    for (key, value) in [
        ("indentLeftTwips", left),
        ("indentRightTwips", right),
        ("firstLineIndentTwips", first),
    ] {
        if let Some(value) = value {
            node.attrs.insert(key.into(), value.into());
        }
    }
}

fn preserve_spacing(
    node: &mut Node,
    before: Option<u32>,
    after: Option<u32>,
    line: Option<&office_oxide::ir::LineSpacing>,
) -> Result<()> {
    for (key, value) in [("spaceBefore", before), ("spaceAfter", after)] {
        if let Some(value) = value {
            ensure!(
                value <= 31680,
                "Import cannot preserve {key} outside 0–31680 twips; source remains read-only"
            );
            node.attrs.insert(key.into(), value.into());
        }
    }
    if let Some(line) = line {
        use office_oxide::ir::LineSpacing;
        let (LineSpacing::Auto(value) | LineSpacing::Multiple(value)) = line else {
            bail!(
                "{}; source remains read-only",
                crate::compatibility::PARAGRAPH_SPACING_RISK
            );
        };
        ensure!(
            (120..=2400).contains(value),
            "Import cannot preserve lineSpacing outside 120–2400 units; source remains read-only"
        );
        node.attrs.insert("lineSpacing".into(), (*value).into());
    }
    Ok(())
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

#[cfg(test)]
mod side_story_tests {
    use super::*;
    use office_oxide::ir::{DocumentIR, HeaderFooter, Section};

    #[test]
    fn conversion_refuses_all_six_ir_story_slots_even_if_preflight_misses_them() {
        for slot in 0..6 {
            let mut section = Section::default();
            let story = Some(HeaderFooter::default());
            match slot {
                0 => section.header = story,
                1 => section.footer = story,
                2 => section.first_page_header = story,
                3 => section.first_page_footer = story,
                4 => section.even_page_header = story,
                _ => section.even_page_footer = story,
            }
            let ir = DocumentIR {
                sections: vec![section],
                ..Default::default()
            };
            let report = CompatibilityReport {
                paragraphs: 0,
                tables: 0,
                drawings: 0,
                risks: BTreeSet::new(),
                fidelity_verified: false,
            };
            assert!(
                convert_ir(ir, report, true)
                    .unwrap_err()
                    .to_string()
                    .contains("headers or footers")
            );
        }
    }
}

#[cfg(test)]
mod image_conversion_tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn audited_image_identity_counts_dimensions_and_placeholder_structure_must_match() {
        let legacy: Node =
            serde_json::from_str(include_str!("../tests/fixtures/word-image-legacy.json")).unwrap();
        let mut picture = legacy.content[0].clone();
        picture.attrs.insert("displayWidthEmu".into(), 95250.into());
        picture
            .attrs
            .insert("displayHeightEmu".into(), 95250.into());
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(
                picture.attrs["src"]
                    .as_str()
                    .unwrap()
                    .split_once(',')
                    .unwrap()
                    .1,
            )
            .unwrap();
        let geometry = crate::image_dimensions::ImageGeometry {
            width_emu: 95250,
            height_emu: 95250,
            mime: "image/png".into(),
            sha256: Sha256::digest(&bytes).into(),
            paragraph: serde_json::from_value(serde_json::json!({"spaceBefore":0,"spaceAfter":0,"lineSpacing":240,"textAlign":"left"})).unwrap(),
        };
        let original = node("doc", vec![node("paragraph", vec![]), picture.clone()]);
        let mut supported = original.clone();
        verify_image_conversion(&mut supported, std::slice::from_ref(&geometry)).unwrap();
        assert_eq!(supported.content.len(), 1);
        assert_eq!(supported.content[0].kind, "image");
        for mutation in [
            "bytes",
            "mime",
            "width",
            "height",
            "missing",
            "extra",
            "placeholder",
            "text",
        ] {
            let mut candidate = original.clone();
            let mut proofs = vec![geometry.clone()];
            match mutation {
                "bytes" => proofs[0].sha256[0] ^= 1,
                "mime" => proofs[0].mime = "image/jpeg".into(),
                "width" => proofs[0].width_emu += 1,
                "height" => proofs[0].height_emu += 1,
                "missing" => proofs.clear(),
                "extra" => proofs.push(geometry.clone()),
                "placeholder" => {
                    candidate.content.remove(0);
                }
                "text" => candidate.content[0].content.push(Node {
                    kind: "text".into(),
                    text: Some("Keep me".into()),
                    attrs: Default::default(),
                    content: vec![],
                    marks: vec![],
                }),
                _ => unreachable!(),
            }
            assert!(
                verify_image_conversion(&mut candidate, &proofs).is_err(),
                "{mutation}"
            );
        }
    }
}
