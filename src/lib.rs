mod empty_paragraph_fonts;
mod image_dimensions;
mod legacy_typography;
mod list_geometry;
mod page_geometry;
mod paragraph_contextual_spacing;
mod paragraph_indentation;
mod paragraph_pagination;
mod pdf_snapshot;
mod presentation_vertical;
mod table_cell_colors;
mod table_vertical;
mod table_widths;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Cursor, Read, Seek},
    path::Path,
    time::Instant,
};

pub mod calculation;
pub mod compatibility;
pub mod document;
pub mod editing;
pub mod importing;
pub mod pdf_rotation;
pub mod presentation;
pub mod presentation_edit;
pub mod presentation_import;
pub mod presentation_model;
pub mod presentation_ppt;
pub mod spreadsheet;
pub mod spreadsheet_edit;
pub mod spreadsheet_import;
pub mod spreadsheet_xls;

/// Extraction report, deliberately separate from an editable document model.
#[derive(Debug, Serialize, Deserialize)]
pub struct Inspection {
    pub format: String,
    pub backend: String,
    pub bytes: u64,
    pub elapsed_ms: u64,
    pub text: String,
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_error: Option<String>,
}

/// Constant-size identity of the exact validated bytes inspected for PDF preview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdfSourceIdentity {
    pub bytes: u64,
    pub sha256: [u8; 32],
}

/// Desktop preview metadata, kept outside the existing Inspection wire shape.
#[derive(Debug, Serialize, Deserialize)]
pub struct PreviewInspection {
    pub inspection: Inspection,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdf_identity: Option<PdfSourceIdentity>,
}

pub fn inspect_for_preview_with_identity(path: &Path) -> Result<PreviewInspection> {
    let is_pdf = path
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("pdf"));
    if is_pdf {
        let started = Instant::now();
        let file = File::open(path).with_context(|| format!("Cannot open {}", path.display()))?;
        let (inspection, identity) = pdf_snapshot::inspect_with_identity(file, true, started)?;
        Ok(PreviewInspection {
            inspection,
            pdf_identity: Some(identity),
        })
    } else {
        Ok(PreviewInspection {
            inspection: inspect_for_preview(path)?,
            pdf_identity: None,
        })
    }
}

/// Reopen once, then verify and return that same bounded buffer. This proves
/// byte equality, not filesystem atomicity or a history of intervening changes.
pub fn read_verified_pdf(path: &Path, identity: &PdfSourceIdentity) -> Result<Vec<u8>> {
    let file = File::open(path)
        .with_context(|| format!("Cannot open {}", path.display()))
        .context("PDF 源文件无法验证，请重新打开文档后再预览")?;
    pdf_snapshot::read_verified(file, identity)
}

pub fn inspect(path: &Path) -> Result<Inspection> {
    inspect_with_policy(path, false)
}

/// A PDF renderer may support a file that the text extractor cannot read.
/// Signature, IO and size failures still reject the input. A successful result
/// with text_error is a preview candidate, not proof that the PDF is valid.
pub fn inspect_for_preview(path: &Path) -> Result<Inspection> {
    inspect_with_policy(path, true)
}

fn inspect_with_policy(path: &Path, allow_pdf_text_failure: bool) -> Result<Inspection> {
    let started = Instant::now();
    let mut file = File::open(path).with_context(|| format!("Cannot open {}", path.display()))?;
    let format = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if format == "pdf" {
        return pdf_snapshot::inspect(file, allow_pdf_text_failure, started);
    }
    let bytes = file.metadata()?.len();
    // A coarse input guard, not a decompression or parser resource sandbox.
    if bytes > 64 * 1024 * 1024 {
        bail!("Exploration limit: input must be at most 64 MiB");
    }
    let mut signature = [0u8; 8];
    let n = file.read(&mut signature)?;
    let valid = match format.as_str() {
        "doc" => n == 8 && signature == [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1],
        "docx" => signature[..n].starts_with(b"PK\x03\x04"),
        _ => bail!("Unsupported format: {format}. Phase 1 accepts DOC, DOCX and PDF"),
    };
    if !valid {
        bail!(
            "File signature does not match .{format}; RTF/HTML disguised as DOC are not supported yet"
        );
    }
    let (backend, text, mut warnings) = match format.as_str() {
        "doc" | "docx" => {
            file.rewind()?;
            let mut source = Vec::new();
            file.take(64 * 1024 * 1024 + 1).read_to_end(&mut source)?;
            anyhow::ensure!(source.len() <= 64 * 1024 * 1024, "Input exceeds 64 MiB");
            let input_format = if format == "docx" {
                editing::preflight_docx(&source)?;
                office_oxide::DocumentFormat::Docx
            } else {
                office_oxide::DocumentFormat::Doc
            };
            let doc = office_oxide::Document::from_reader(Cursor::new(source), input_format)
                .context("Office parser failed")?;
            ("office_oxide 0.1.13", doc.plain_text(), vec![
                "Text extraction only; formatting, pagination and lossless editing are not verified".into(),
            ])
        }
        _ => unreachable!(),
    };
    if text.trim().is_empty() {
        warnings.push("No text extracted; this does not prove the document is empty".into());
    }
    Ok(Inspection {
        format,
        backend: backend.into(),
        bytes,
        elapsed_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        text,
        warnings,
        text_error: None,
    })
}
