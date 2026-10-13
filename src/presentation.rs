//! Bounded, read-only PPTX inventory; not a slide renderer or editable model.
use anyhow::{Result, ensure};
use office_oxide::pptx::{PptxDocument, Shape, ShapePosition};
use serde::{Deserialize, Serialize};
use std::{
    io::{Cursor, Read},
    path::Path,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct PresentationReport {
    pub format: String,
    pub size_emu: Option<[i64; 2]>,
    pub slides: Vec<SlideReport>,
    pub unreadable_parts: Vec<(String, String)>,
    pub has_macros: bool,
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub master_text: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct SlideReport {
    pub id: u32,
    pub name: String,
    pub hidden: bool,
    pub text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    pub parse_error: Option<String>,
    pub shapes: Vec<ShapeReport>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ShapeReport {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical_alignment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<String>,
    pub id: u32,
    pub name: String,
    pub kind: String,
    pub bounds_emu: Option<[i64; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_bounds_emu: Option<[i64; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub master_bounds_emu: Option<[i64; 4]>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub runs: Vec<RunReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_notice: Option<String>,
    pub children: Vec<ShapeReport>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RunReport {
    pub text: String,
    pub font_size_hundredths_pt: Option<u32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub color_rgb: Option<[u8; 3]>,
}
impl RunReport {
    fn plain(text: &str) -> Self {
        Self {
            text: text.into(),
            font_size_hundredths_pt: None,
            bold: None,
            italic: None,
            color_rgb: None,
        }
    }
}

pub fn inspect_pptx(path: &Path) -> Result<PresentationReport> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    inspect_pptx_bytes(&bytes)
}

pub fn inspect_pptx_bytes(bytes: &[u8]) -> Result<PresentationReport> {
    ensure!(bytes.len() <= 64 * 1024 * 1024, "PPTX input exceeds 64 MiB");
    crate::editing::preflight_ooxml(bytes, "PPTX")?;
    // Bound recursive XML before the backend builds its object graph.
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut elements = 0usize;
    let mut vertical_parts = std::collections::BTreeMap::new();
    let mut slide_targets = std::collections::BTreeMap::new();
    for i in 0..archive.len() {
        let mut part = archive.by_index(i)?;
        if !(part.name().ends_with(".xml") || part.name().ends_with(".rels")) {
            continue;
        }
        let mut xml = Vec::new();
        part.read_to_end(&mut xml)?;
        let mut reader = quick_xml::Reader::from_reader(xml.as_slice());
        let mut depth = 0usize;
        loop {
            use quick_xml::events::Event;
            match reader.read_event()? {
                Event::Start(_) => {
                    depth += 1;
                    elements += 1;
                }
                Event::Empty(_) => elements += 1,
                Event::End(_) => {
                    ensure!(depth > 0, "Unbalanced PPTX XML");
                    depth -= 1;
                }
                Event::DocType(_) => anyhow::bail!("PPTX DTD is unsupported"),
                Event::Eof => {
                    ensure!(depth == 0, "Truncated PPTX XML");
                    break;
                }
                _ => {}
            }
            ensure!(
                depth <= 64 && elements <= 1_000_000,
                "PPTX XML complexity limit"
            );
        }
        if part.name() == "ppt/_rels/presentation.xml.rels" {
            slide_targets = crate::presentation_vertical::slide_targets(&xml)?;
        }
        if part.name().starts_with("ppt/slides/")
            && part.name().ends_with(".xml")
            && !part.name()[11..].contains('/')
        {
            vertical_parts.insert(
                part.name().to_owned(),
                crate::presentation_vertical::anchors(&xml)?,
            );
        }
    }
    let mut doc = PptxDocument::from_reader(Cursor::new(bytes))?;
    ensure!(doc.slides.len() <= 1000, "PPTX exceeds 1000 slides");
    ensure!(
        doc.slides.len() == doc.presentation.slides.len(),
        "PPTX slide inventory mismatch"
    );
    let mut count = 0usize;
    let mut text_bytes = 0usize;
    let mut image_bytes = 0usize;
    let mut slides = Vec::new();
    let mut layout_shape_count = 0usize;
    let mut master_positions = Vec::new();
    for master in &doc.masters {
        let mut positions = std::collections::BTreeMap::new();
        for shape in &master.shapes {
            layout_shape_count += 1;
            ensure!(
                layout_shape_count <= 100_000,
                "PPTX template exceeds 100000 shapes"
            );
            if let Shape::AutoShape(shape) = shape
                && let Some(placeholder) = &shape.placeholder
            {
                let bounds = shape.position.as_ref().map(|p| [p.x, p.y, p.cx, p.cy]);
                positions
                    .entry(placeholder.ph_type.clone().unwrap_or_else(|| "obj".into()))
                    .and_modify(|value| *value = None)
                    .or_insert(bounds);
            }
        }
        master_positions.push(positions);
    }
    let mut layout_positions = Vec::new();
    for layout in &doc.layouts {
        let mut positions = std::collections::BTreeMap::new();
        for shape in &layout.shapes {
            layout_shape_count += 1;
            ensure!(
                layout_shape_count <= 100_000,
                "PPTX layout exceeds 100000 shapes"
            );
            if let Shape::AutoShape(shape) = shape
                && let Some(placeholder) = &shape.placeholder
            {
                let bounds = shape.position.as_ref().map(|p| [p.x, p.y, p.cx, p.cy]);
                let inherited = if bounds.is_none() {
                    layout
                        .master_index
                        .and_then(|i| master_positions.get(i))
                        .and_then(|positions| {
                            master_role(placeholder.ph_type.as_deref())
                                .and_then(|role| positions.get(role))
                        })
                        .copied()
                        .flatten()
                } else {
                    None
                };
                // Ambiguous indices must never choose an arbitrary position.
                positions
                    .entry(placeholder.idx.unwrap_or(0))
                    .and_modify(|value| *value = None)
                    .or_insert(Some((bounds, inherited)));
            }
        }
        layout_positions.push(positions);
    }
    for i in 0..doc.slides.len() {
        // The dependency appends notes to slide_plain_text. Remove the
        // object temporarily rather than trimming ambiguous literal text.
        let notes_body = doc.slides[i].notes.take();
        let text = doc.slide_plain_text(i).unwrap_or_default();
        doc.slides[i].notes = notes_body;
        let slide = &doc.slides[i];
        text_bytes += text.len();
        let mut notes = String::new();
        if let Some(body) = &slide.notes {
            for (index, paragraph) in body.paragraphs.iter().enumerate() {
                if index > 0 {
                    notes.push('\n');
                    text_bytes += 1;
                }
                for item in &paragraph.content {
                    use office_oxide::pptx::TextContent;
                    let value = match item {
                        TextContent::Run(run) => run.text.as_str(),
                        TextContent::Field(field) => field.text.as_str(),
                        TextContent::LineBreak => "\n",
                    };
                    text_bytes += value.len();
                    ensure!(text_bytes <= 4 * 1024 * 1024, "PPTX text exceeds 4 MiB");
                    notes.push_str(value);
                }
                ensure!(text_bytes <= 4 * 1024 * 1024, "PPTX text exceeds 4 MiB");
            }
        }
        ensure!(text_bytes <= 4 * 1024 * 1024, "PPTX text exceeds 4 MiB");
        let mut shapes = inventory(&slide.shapes, &mut count, &mut text_bytes, &mut image_bytes)?;
        if let Some(anchors) = slide_targets
            .get(&doc.presentation.slides[i].rel_id)
            .and_then(Option::as_ref)
            .and_then(|path| vertical_parts.get(path))
        {
            fn apply(
                shapes: &mut [ShapeReport],
                anchors: &std::collections::BTreeMap<u32, Option<String>>,
            ) {
                for shape in shapes {
                    if shape.kind == "shape" {
                        shape.vertical_alignment = anchors.get(&shape.id).cloned().flatten();
                    }
                    apply(&mut shape.children, anchors);
                }
            }
            apply(&mut shapes, anchors);
        }
        if let Some(positions) = slide
            .layout_index
            .and_then(|index| layout_positions.get(index))
        {
            for (source, report) in slide.shapes.iter().zip(&mut shapes) {
                if report.bounds_emu.is_none()
                    && let Shape::AutoShape(shape) = source
                    && let Some(placeholder) = &shape.placeholder
                    && let Some((layout_bounds, master_bounds)) = positions
                        .get(&placeholder.idx.unwrap_or(0))
                        .copied()
                        .flatten()
                {
                    report.layout_bounds_emu = layout_bounds;
                    report.master_bounds_emu = master_bounds;
                }
            }
        }
        slides.push(SlideReport {
            id: doc.presentation.slides[i].id,
            name: slide.name.clone(),
            hidden: slide.hidden,
            text,
            notes,
            parse_error: slide.parse_error.clone(),
            shapes,
        });
    }
    let mut master_text = String::new();
    for text in doc.master_static_text() {
        text_bytes += text.len() + 1;
        ensure!(text_bytes <= 4 * 1024 * 1024, "PPTX text exceeds 4 MiB");
        master_text.push_str(&text);
        master_text.push('\n');
    }
    Ok(PresentationReport {
        format: "pptx".into(), size_emu: doc.presentation.slide_size.map(|s| [s.cx, s.cy]),
        slides, unreadable_parts: doc.unreadable_parts, has_macros: doc.has_macros, master_text,
        warnings: vec![
            "Read-only inventory; not a rendered slide or editable presentation".into(),
            "Bounds are source-local EMU; inherited bounds require unique top-level layout indices and unique master placeholder roles; group transforms, partial geometry and inherited styles remain unresolved".into(),
            "Only bounded embedded PNG/JPEG previews are included; image crop/effects, theme, charts and animations are not restored; macros and links are not executed".into(),
        ],
    })
}

fn master_role(role: Option<&str>) -> Option<&'static str> {
    match role.unwrap_or("obj") {
        "title" | "ctrTitle" => Some("title"),
        "body" | "obj" | "subTitle" | "chart" | "tbl" | "clipArt" | "dgm" | "media" | "pic" => {
            Some("body")
        }
        "dt" => Some("dt"),
        "ftr" => Some("ftr"),
        "sldNum" => Some("sldNum"),
        "hdr" => Some("hdr"),
        _ => None,
    }
}

fn inventory(
    shapes: &[Shape],
    count: &mut usize,
    text_bytes: &mut usize,
    image_bytes: &mut usize,
) -> Result<Vec<ShapeReport>> {
    let mut output = Vec::new();
    for shape in shapes {
        *count += 1;
        ensure!(*count <= 100_000, "PPTX exceeds 100000 shapes");
        let (id, name, kind, pos, children): (_, _, _, &Option<ShapePosition>, _) = match shape {
            Shape::AutoShape(s) => (s.id, &s.name, "shape", &s.position, Vec::new()),
            Shape::Picture(s) => (s.id, &s.name, "picture", &s.position, Vec::new()),
            Shape::Connector(s) => (s.id, &s.name, "connector", &s.position, Vec::new()),
            Shape::GraphicFrame(s) => (s.id, &s.name, "graphic", &s.position, Vec::new()),
            Shape::Group(s) => (
                s.id,
                &s.name,
                "group",
                &s.position,
                inventory(&s.children, count, text_bytes, image_bytes)?,
            ),
        };
        let mut text = String::new();
        let mut runs = Vec::new();
        let mut alignment = None;
        if let Shape::AutoShape(s) = shape
            && let Some(body) = &s.text_body
        {
            let key = |p: &office_oxide::pptx::TextParagraph| {
                p.alignment.as_ref().map(|a| match a {
                    office_oxide::ir::ParagraphAlignment::Left => "l",
                    office_oxide::ir::ParagraphAlignment::Center => "ctr",
                    office_oxide::ir::ParagraphAlignment::Right => "r",
                    office_oxide::ir::ParagraphAlignment::Justify => "just",
                    office_oxide::ir::ParagraphAlignment::Distribute => "dist",
                })
            };
            if let Some(first) = body.paragraphs.first() {
                let first = key(first);
                if body.paragraphs.iter().all(|p| key(p) == first) {
                    alignment = first.map(str::to_owned);
                }
            }
            for (index, paragraph) in body.paragraphs.iter().enumerate() {
                if index > 0 {
                    text.push('\n');
                    runs.push(RunReport::plain("\n"));
                }
                for item in &paragraph.content {
                    use office_oxide::pptx::TextContent;
                    match item {
                        TextContent::Run(run) => {
                            text.push_str(&run.text);
                            runs.push(RunReport {
                                text: run.text.clone(),
                                font_size_hundredths_pt: run.font_size_hundredths_pt,
                                bold: run.bold,
                                italic: run.italic,
                                color_rgb: run.color_rgb,
                            });
                        }
                        TextContent::Field(field) => {
                            text.push_str(&field.text);
                            runs.push(RunReport::plain(&field.text));
                        }
                        TextContent::LineBreak => {
                            text.push('\n');
                            runs.push(RunReport::plain("\n"));
                        }
                    }
                    ensure!(runs.len() <= 10000, "PPTX shape exceeds 10000 text runs");
                }
                ensure!(runs.len() <= 10000, "PPTX shape exceeds 10000 text runs");
            }
        }
        *text_bytes += text.len() * 2; // Both plain text and structured runs are serialized.
        ensure!(*text_bytes <= 4 * 1024 * 1024, "PPTX text exceeds 4 MiB");
        let (image, image_notice) = if let Shape::Picture(picture) = shape {
            match picture.data.as_deref() {
                Some(bytes) => match preview_image(bytes, image_bytes) {
                    Ok(image) => (Some(image), Some("Embedded PNG/JPEG preview; crop, effects and rotation are not restored".into())),
                    Err(error) => (None, Some(format!("Image preview unavailable: {error}"))),
                },
                None => (None, Some("No embedded image bytes; linked images are not fetched".into())),
            }
        } else {
            (None, None)
        };
        output.push(ShapeReport {
            vertical_alignment: None,
            alignment,
            id,
            name: name.clone(),
            kind: kind.into(),
            bounds_emu: pos.as_ref().map(|p| [p.x, p.y, p.cx, p.cy]),
            layout_bounds_emu: None,
            master_bounds_emu: None,
            text,
            runs,
            image,
            image_notice,
            children,
        });
    }
    Ok(output)
}

pub(crate) fn preview_image(bytes: &[u8], total: &mut usize) -> Result<String> {
    use base64::Engine;
    ensure!(bytes.len() <= 4 * 1024 * 1024, "image exceeds 4 MiB");
    ensure!(
        *total + bytes.len() <= 8 * 1024 * 1024,
        "deck image preview budget exceeds 8 MiB"
    );
    let format = image::guess_format(bytes)?;
    ensure!(
        matches!(format, image::ImageFormat::Png | image::ImageFormat::Jpeg),
        "only PNG/JPEG are supported"
    );
    let (width, height) =
        image::ImageReader::with_format(Cursor::new(bytes), format).into_dimensions()?;
    ensure!(
        width > 0
            && height > 0
            && width <= 10000
            && height <= 10000
            && u64::from(width) * u64::from(height) <= 16_000_000,
        "image pixel limit exceeded"
    );
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(80 * 1024 * 1024);
    reader.limits(limits);
    reader.decode()?;
    *total += bytes.len();
    let mime = if format == image::ImageFormat::Png {
        "image/png"
    } else {
        "image/jpeg"
    };
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_preview_rejects_invalid_input_and_preserves_budget_on_failure() {
        let mut total = 0;
        assert!(preview_image(b"<svg xmlns='http://www.w3.org/2000/svg'/>", &mut total).is_err());
        assert!(preview_image(&vec![0; 4 * 1024 * 1024 + 1], &mut total).is_err());
        assert_eq!(total, 0);
        let mut zip = zip::ZipArchive::new(Cursor::new(include_bytes!(
            "../tests/fixtures/basic-slides.pptx"
        )))
        .unwrap();
        let mut bytes = Vec::new();
        zip.by_name("ppt/media/image1.png")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert!(
            preview_image(&bytes, &mut total)
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        assert_eq!(total, bytes.len());
        let mut jpeg = Cursor::new(Vec::new());
        image::RgbImage::new(2, 2)
            .write_to(&mut jpeg, image::ImageFormat::Jpeg)
            .unwrap();
        let mut jpeg_total = 0;
        assert!(
            preview_image(jpeg.get_ref(), &mut jpeg_total)
                .unwrap()
                .starts_with("data:image/jpeg;base64,")
        );
        let mut exhausted = 8 * 1024 * 1024;
        assert!(preview_image(&bytes, &mut exhausted).is_err());
        assert_eq!(exhausted, 8 * 1024 * 1024);
        assert!(preview_image(&bytes[..bytes.len() / 2], &mut total).is_err());
        assert_eq!(total, bytes.len());
    }
}
