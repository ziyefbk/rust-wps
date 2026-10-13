//! Versioned authored presentation; deliberately separate from external inventory.
use anyhow::{Result, ensure};
use office_oxide::pptx::write::{PptxWriter, Run};
use serde::{Deserialize, Serialize};
use std::{
    io::{Cursor, Read, Write},
    path::Path,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Presentation {
    pub version: u32,
    pub size_emu: [i64; 2],
    pub slides: Vec<Slide>,
}

pub fn save_draft(model: &Presentation, path: &Path) -> Result<()> {
    model.validate()?;
    let bytes = serde_json::to_vec(model)?;
    ensure!(
        bytes.len() <= 4 * 1024 * 1024,
        "Presentation draft exceeds 4 MiB"
    );
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(&bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path)
        .map_err(|e| anyhow::anyhow!("Cannot save presentation draft: {}", e.error))?;
    Ok(())
}
pub fn load_draft(path: &Path) -> Result<Option<Presentation>> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    file.take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 4 * 1024 * 1024,
        "Presentation draft exceeds 4 MiB"
    );
    let model: Presentation = serde_json::from_slice(&bytes)?;
    model.validate()?;
    Ok(Some(model))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slide {
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    pub text_boxes: Vec<TextBox>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<Picture>,
}
fn is_false(value: &bool) -> bool {
    !value
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Picture {
    pub bounds_emu: [i64; 4],
    pub data_url: String,
    pub alt: String,
}
fn picture_bytes(picture: &Picture) -> Result<Vec<u8>> {
    picture_bytes_with_budget(picture, &mut 0)
}
#[test]
fn picture_pixel_budget_is_checked_before_full_decode() {
    use base64::Engine;
    let picture = Picture {
        bounds_emu: [0; 4],
        alt: String::new(),
        data_url: format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD
                .encode(include_bytes!("../tests/fixtures/slide-image.png"))
        ),
    };
    let mut pixels = 64_000_000;
    assert!(
        picture_bytes_with_budget(&picture, &mut pixels)
            .unwrap_err()
            .to_string()
            .contains("64 million pixels")
    );
    assert_eq!(pixels, 64_000_000);
}
fn picture_bytes_with_budget(picture: &Picture, pixels: &mut u64) -> Result<Vec<u8>> {
    use base64::Engine;
    ensure!(picture.data_url.len() <= 1_398_128, "Picture exceeds 1 MiB");
    let (_, data) = picture
        .data_url
        .split_once(',')
        .ok_or_else(|| anyhow::anyhow!("Expected PNG/JPEG data URL"))?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(data)?;
    ensure!(bytes.len() <= 1024 * 1024, "Picture exceeds 1 MiB");
    let (w, h) = image::ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()?
        .into_dimensions()?;
    let next = *pixels + u64::from(w) * u64::from(h);
    ensure!(
        next <= 64_000_000,
        "Presentation pictures exceed 64 million pixels"
    );
    ensure!(
        crate::presentation::preview_image(&bytes, &mut 0)? == picture.data_url,
        "Expected canonical PNG/JPEG data URL"
    );
    *pixels = next;
    Ok(bytes)
}
pub fn inspect_picture(data_url: &str) -> Result<[u32; 2]> {
    ensure!(data_url.len() <= 1_398_128, "Picture exceeds 1 MiB");
    let bytes = picture_bytes(&Picture {
        bounds_emu: [0; 4],
        data_url: data_url.into(),
        alt: String::new(),
    })?;
    let (w, h) = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()?
        .into_dimensions()?;
    Ok([w, h])
}
fn valid_bounds(bounds: [i64; 4], size: [i64; 2]) -> bool {
    let [x, y, w, h] = bounds;
    x >= 0
        && y >= 0
        && w > 0
        && h > 0
        && x.checked_add(w).is_some_and(|v| v <= size[0])
        && y.checked_add(h).is_some_and(|v| v <= size[1])
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextBox {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical_alignment: Option<VerticalAlignment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<TextAlignment>,
    pub bounds_emu: [i64; 4],
    pub text: String,
    pub font_size_hundredths_pt: u32,
    pub bold: bool,
    pub italic: bool,
    pub color_rgb: [u8; 3],
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlignment {
    Left,
    Center,
    Right,
}
impl TextAlignment {
    pub fn ooxml(self) -> &'static str {
        match self {
            Self::Left => "l",
            Self::Center => "ctr",
            Self::Right => "r",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerticalAlignment {
    Top,
    Middle,
    Bottom,
}
impl VerticalAlignment {
    pub fn ooxml(self) -> &'static str {
        match self {
            Self::Top => "t",
            Self::Middle => "ctr",
            Self::Bottom => "b",
        }
    }
}

fn align_generated_slide(xml: String, slide: &Slide) -> Result<String> {
    if slide
        .text_boxes
        .iter()
        .all(|b| b.alignment.is_none() && b.vertical_alignment.is_none())
    {
        return Ok(xml);
    }
    let mut chunks = xml.split("<p:sp>");
    let mut result = chunks.next().unwrap_or_default().to_owned();
    let chunks: Vec<_> = chunks.collect();
    ensure!(
        chunks.len() == slide.text_boxes.len(),
        "Unexpected generated text shapes"
    );
    for (chunk, text) in chunks.into_iter().zip(&slide.text_boxes) {
        result.push_str("<p:sp>");
        let mut chunk = chunk.to_owned();
        if let Some(vertical) = text.vertical_alignment {
            ensure!(
                chunk.matches("<a:bodyPr ").count() == 1 && !chunk.contains(" anchor="),
                "Unexpected generated text body properties"
            );
            chunk = chunk.replacen(
                "<a:bodyPr ",
                &format!("<a:bodyPr anchor=\"{}\" ", vertical.ooxml()),
                1,
            );
        }
        if let Some(alignment) = text.alignment {
            ensure!(
                chunk.matches("<a:p>").count() == 1 && !chunk.contains("<a:pPr"),
                "Unexpected generated paragraph properties"
            );
            result.push_str(&chunk.replacen(
                "<a:p>",
                &format!("<a:p><a:pPr algn=\"{}\"/>", alignment.ooxml()),
                1,
            ));
        } else {
            result.push_str(&chunk);
        }
    }
    Ok(result)
}
impl Presentation {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "Unsupported presentation model version");
        ensure!(
            self.size_emu.iter().all(|n| *n >= 914400 && *n <= 91440000),
            "Slide size must be 1–100 inches"
        );
        ensure!(
            !self.slides.is_empty() && self.slides.len() <= 100,
            "Presentation requires 1–100 slides"
        );
        let mut total = 0usize;
        let mut count = 0usize;
        let mut image_bytes = 0usize;
        let mut pixels = 0;
        ensure!(
            self.slides.iter().map(|s| s.images.len()).sum::<usize>() <= 100,
            "Too many pictures; maximum 100"
        );
        for slide in &self.slides {
            ensure!(slide.notes.chars().all(|c| c=='\n' || matches!(c,'\u{20}'..='\u{d7ff}'|'\u{e000}'..='\u{fffd}'|'\u{10000}'..='\u{10ffff}')), "Unsupported notes control character");
            ensure!(
                slide.notes.split('\n').count() <= 1000,
                "Notes exceed 1000 lines"
            );
            total += slide.notes.len();
            ensure!(total <= 1024 * 1024, "Presentation text exceeds 1 MiB");
            for picture in &slide.images {
                count += 1;
                ensure!(count <= 10000, "Too many slide objects");
                ensure!(
                    valid_bounds(picture.bounds_emu, self.size_emu),
                    "Picture must fit inside the slide"
                );
                ensure!(
                    picture.alt.len() <= 1024 && !picture.alt.chars().any(char::is_control),
                    "Invalid picture alternative text"
                );
                image_bytes += picture_bytes_with_budget(picture, &mut pixels)?.len();
                ensure!(
                    image_bytes <= 2 * 1024 * 1024,
                    "Presentation pictures exceed 2 MiB"
                );
            }
            for text in &slide.text_boxes {
                count += 1;
                ensure!(count <= 10000, "Too many text boxes");
                let [x, y, w, h] = text.bounds_emu;
                ensure!(
                    x >= 0
                        && y >= 0
                        && w > 0
                        && h > 0
                        && x.checked_add(w).is_some_and(|v| v <= self.size_emu[0])
                        && y.checked_add(h).is_some_and(|v| v <= self.size_emu[1]),
                    "Text box must fit inside the slide"
                );
                ensure!(
                    (100..=40000).contains(&text.font_size_hundredths_pt),
                    "Font size must be 1–400 pt"
                );
                ensure!(text.text.chars().all(|c| c=='\n' || matches!(c,'\u{20}'..='\u{d7ff}'|'\u{e000}'..='\u{fffd}'|'\u{10000}'..='\u{10ffff}')),"Unsupported text control character");
                total += text.text.len();
                ensure!(total <= 1024 * 1024, "Presentation text exceeds 1 MiB");
                ensure!(
                    text.text.split('\n').count() <= 1000,
                    "Text box exceeds 1000 lines"
                );
            }
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= 4 * 1024 * 1024,
            "Presentation model exceeds 4 MiB"
        );
        Ok(())
    }
}

const MODEL_PART: &str = "rust-wps/presentation-v1.json";

fn encode_model(model: &Presentation) -> Result<Vec<u8>> {
    model.validate()?;
    let mut writer = PptxWriter::new();
    writer.set_presentation_size(model.size_emu[0] as u64, model.size_emu[1] as u64);
    for slide in &model.slides {
        let target = writer.add_slide();
        if !slide.notes.is_empty() {
            // set_notes uses str::lines, which consumes a terminal LF.
            // One extra LF preserves the authored final empty paragraph.
            target.set_notes(&format!("{}\n", slide.notes));
        }
        for text in &slide.text_boxes {
            let mut runs = Vec::new();
            for (index, line) in text.text.split('\n').enumerate() {
                if index > 0 {
                    runs.push(Run::line_break());
                }
                let mut run = Run::new(line)
                    .font_size(f64::from(text.font_size_hundredths_pt) / 100.0)
                    .color(format!(
                        "{:02X}{:02X}{:02X}",
                        text.color_rgb[0], text.color_rgb[1], text.color_rgb[2]
                    ));
                run.bold = text.bold;
                run.italic = text.italic;
                runs.push(run);
            }
            let [x, y, w, h] = text.bounds_emu;
            target.add_rich_text_box(&runs, x, y, w, h);
        }
        for picture in &slide.images {
            let bytes = picture_bytes(picture)?;
            let format = if picture.data_url.starts_with("data:image/png;") {
                office_oxide::ir::ImageFormat::Png
            } else {
                office_oxide::ir::ImageFormat::Jpeg
            };
            let [x, y, w, h] = picture.bounds_emu;
            target.add_image_with_alt(
                bytes,
                format,
                x,
                y,
                w as u64,
                h as u64,
                Some(picture.alt.clone()),
            );
        }
    }
    let mut encoded = Cursor::new(Vec::new());
    writer.write_to(&mut encoded)?;
    ensure!(
        encoded.get_ref().len() <= 64 * 1024 * 1024,
        "Generated PPTX exceeds 64 MiB"
    );
    let mut source = zip::ZipArchive::new(encoded)?;
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..source.len() {
        let mut entry = source.by_index(index)?;
        if let Some((_, slide)) = model.slides.iter().enumerate().find(|(i, slide)| {
            (slide.hidden
                || slide
                    .text_boxes
                    .iter()
                    .any(|b| b.alignment.is_some() || b.vertical_alignment.is_some()))
                && entry.name() == format!("ppt/slides/slide{}.xml", i + 1)
        }) {
            let name = entry.name().to_string();
            let mut xml = String::new();
            entry.read_to_string(&mut xml)?;
            ensure!(
                xml.matches("<p:sld ").count() == 1,
                "Unexpected generated slide root"
            );
            let xml = if slide.hidden {
                xml.replacen("<p:sld ", "<p:sld show=\"0\" ", 1)
            } else {
                xml
            };
            let xml = align_generated_slide(xml, slide)?;
            output.start_file(name, zip::write::SimpleFileOptions::default())?;
            output.write_all(xml.as_bytes())?;
            continue;
        }
        if entry.name() != "[Content_Types].xml" {
            output.raw_copy_file(entry)?;
            continue;
        }
        let mut data = String::new();
        std::io::Read::read_to_string(&mut entry, &mut data)?;
        ensure!(data.contains("</Types>"), "Unexpected content types XML");
        let data=data.replace("</Types>", "<Override PartName=\"/rust-wps/presentation-v1.json\" ContentType=\"application/json\"/></Types>");
        output.start_file(
            "[Content_Types].xml",
            zip::write::SimpleFileOptions::default(),
        )?;
        output.write_all(data.as_bytes())?;
    }
    let json = serde_json::to_vec(model)?;
    ensure!(
        json.len() <= 4 * 1024 * 1024,
        "Presentation model exceeds 4 MiB"
    );
    output.start_file(MODEL_PART, zip::write::SimpleFileOptions::default())?;
    output.write_all(&json)?;
    let bytes = output.finish()?.into_inner();
    ensure!(
        bytes.len() <= 64 * 1024 * 1024,
        "Generated PPTX exceeds 64 MiB"
    );
    Ok(bytes)
}

pub fn load_authored_pptx_bytes(bytes: &[u8]) -> Result<Option<Presentation>> {
    ensure!(bytes.len() <= 64 * 1024 * 1024, "PPTX exceeds 64 MiB");
    let parts = crate::presentation_edit::read_parts(bytes)?;
    let Some(json) = parts.get(MODEL_PART) else {
        return Ok(None);
    };
    ensure!(
        json.len() <= 4 * 1024 * 1024,
        "Presentation model exceeds 4 MiB"
    );
    let model: Presentation = serde_json::from_slice(json)?;
    let generated = encode_model(&model)?;
    ensure!(
        parts == crate::presentation_edit::read_parts(&generated)?,
        "Presentation contents differ from editable model; use read-only preview"
    );
    Ok(Some(model))
}

pub fn load_authored_pptx(path: &Path) -> Result<Option<Presentation>> {
    load_authored_pptx_bytes(&crate::spreadsheet::read_xlsx_snapshot(path)?)
}

pub fn export_pptx(
    model: &Presentation,
    output: &Path,
) -> Result<crate::presentation::PresentationReport> {
    ensure!(!output.try_exists()?, "Output exists; choose a new file");
    let encoded = Cursor::new(encode_model(model)?);
    let report = crate::presentation::inspect_pptx_bytes(encoded.get_ref())?;
    ensure!(
        report.unreadable_parts.is_empty()
            && report.size_emu == Some(model.size_emu)
            && report.slides.len() == model.slides.len(),
        "Generated presentation failed verification"
    );
    for (expected, actual) in model.slides.iter().zip(&report.slides) {
        ensure!(
            expected.hidden == actual.hidden,
            "Generated slide visibility failed verification"
        );
        ensure!(
            expected.notes == actual.notes,
            "Generated notes failed verification"
        );
        ensure!(
            expected.text_boxes.len() + expected.images.len() == actual.shapes.len(),
            "Generated shape count mismatch"
        );
        for (expected, actual) in expected.text_boxes.iter().zip(&actual.shapes) {
            ensure!(
                actual.vertical_alignment.as_deref()
                    == expected.vertical_alignment.map(VerticalAlignment::ooxml),
                "Generated vertical alignment mismatch"
            );
            ensure!(
                actual.alignment.as_deref() == expected.alignment.map(TextAlignment::ooxml),
                "Generated text alignment mismatch"
            );
            ensure!(
                actual.bounds_emu == Some(expected.bounds_emu) && actual.text == expected.text,
                "Generated text or bounds mismatch"
            );
            for run in actual.runs.iter().filter(|r| r.text != "\n") {
                ensure!(
                    run.font_size_hundredths_pt == Some(expected.font_size_hundredths_pt)
                        && run.bold.unwrap_or(false) == expected.bold
                        && run.italic.unwrap_or(false) == expected.italic
                        && run.color_rgb == Some(expected.color_rgb),
                    "Generated text style mismatch"
                );
            }
        }
        for (picture, shape) in expected
            .images
            .iter()
            .zip(actual.shapes.iter().skip(expected.text_boxes.len()))
        {
            ensure!(
                shape.bounds_emu == Some(picture.bounds_emu)
                    && shape.image.as_deref() == Some(picture.data_url.as_str()),
                "Generated picture mismatch"
            );
        }
    }
    ensure!(
        load_authored_pptx_bytes(encoded.get_ref())?.is_some(),
        "Generated model cannot be verified"
    );
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(encoded.get_ref())?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(output)
        .map_err(|e| anyhow::anyhow!("Cannot publish without overwriting: {}", e.error))?;
    Ok(report)
}
