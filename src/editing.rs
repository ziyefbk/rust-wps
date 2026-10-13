//! Surgical DOCX text replacement. Output is always a new file.
use anyhow::{Context, Result, bail, ensure};
use office_oxide::docx::edit::EditableDocx;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Cursor, Read, Write},
    path::Path,
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const INPUT_LIMIT: u64 = 64 * 1024 * 1024;
const EXPANDED_LIMIT: u64 = 128 * 1024 * 1024;

/// Verify ZIP sizes/CRC before handing the same snapshot to a parser. Signed
/// parts are valid for reading; editing restrictions are enforced separately.
pub(crate) fn preflight_docx(bytes: &[u8]) -> Result<()> {
    preflight_ooxml(bytes, "DOCX")
}

pub(crate) fn preflight_ooxml(bytes: &[u8], format: &str) -> Result<()> {
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).with_context(|| format!("Invalid {format} ZIP"))?;
    ensure!(archive.len() <= 4096, "{format} has too many ZIP entries");
    let mut names = std::collections::BTreeSet::new();
    let mut total = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        total = total
            .checked_add(entry.size())
            .context("ZIP size overflow")?;
        ensure!(total <= EXPANDED_LIMIT, "Expanded {format} exceeds 128 MiB");
        ensure!(
            names.insert(entry.name().to_owned()),
            "Duplicate {format} part"
        );
        let size = entry.size();
        let actual = std::io::copy(&mut entry.take(size + 1), &mut std::io::sink())?;
        ensure!(actual == size, "ZIP entry size mismatch");
    }
    Ok(())
}

#[derive(Debug, Serialize, serde::Deserialize)]
pub struct EditReport {
    pub replacements: usize,
    pub changed_parts: Vec<String>,
    pub preserved_parts: usize,
    pub warnings: Vec<String>,
}

pub(crate) fn parts(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).context("Invalid DOCX ZIP")?;
    ensure!(archive.len() <= 4096, "DOCX has too many ZIP entries");
    let mut total = 0u64;
    let mut result = BTreeMap::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        total = total
            .checked_add(entry.size())
            .context("ZIP size overflow")?;
        ensure!(total <= EXPANDED_LIMIT, "Expanded DOCX exceeds 128 MiB");
        let name = entry.name().to_string();
        ensure!(!result.contains_key(&name), "Duplicate DOCX part: {name}");
        ensure!(
            !name.to_ascii_lowercase().starts_with("_xmlsignatures/"),
            "Signed DOCX cannot be edited without invalidating its signature"
        );
        let size = entry.size();
        let mut data = Vec::new();
        entry.take(EXPANDED_LIMIT + 1).read_to_end(&mut data)?;
        ensure!(data.len() as u64 == size, "ZIP entry size mismatch");
        result.insert(name, data);
    }
    ensure!(
        result.contains_key("word/document.xml"),
        "Only the standard word/document.xml main part is supported for editing"
    );
    Ok(result)
}

fn valid_xml_text(text: &str) -> bool {
    text.chars().all(|c| matches!(c, '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}'))
}

// The upstream matcher concatenates w:t nodes, ignoring tabs, line breaks and
// field/object markers. Refuse an edit if that would create a false contiguous
// match across one of those boundaries.
fn check_match_boundaries(xml: &[u8], find: &str) -> Result<()> {
    use quick_xml::events::Event;
    #[derive(Default)]
    struct Paragraph {
        text: String,
        boundaries: Vec<usize>,
    }
    let mut paragraphs: Vec<Paragraph> = Vec::new();
    let mut open = Vec::new();
    let mut in_text = false;
    let mut reader = quick_xml::Reader::from_reader(xml);
    loop {
        let event = reader.read_event()?;
        let empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(tag) | Event::Empty(tag) => {
                let name = tag.name();
                if name.as_ref() == "w:p" {
                    if empty {
                        continue;
                    }
                    if let Some(&parent) = open.last() {
                        let p: &mut Paragraph = &mut paragraphs[parent];
                        p.boundaries.push(p.text.len());
                    }
                    open.push(paragraphs.len());
                    paragraphs.push(Paragraph::default());
                } else if name.as_ref() == "w:t" {
                    in_text = !empty;
                } else if matches!(
                    name.as_ref(),
                    "w:tab"
                        | "w:br"
                        | "w:cr"
                        | "w:fldChar"
                        | "w:instrText"
                        | "w:drawing"
                        | "w:pict"
                        | "w:footnoteReference"
                        | "w:endnoteReference"
                        | "w:delText"
                        | "w:fldSimple"
                ) && let Some(&current) = open.last()
                {
                    let p = &mut paragraphs[current];
                    p.boundaries.push(p.text.len());
                }
            }
            Event::End(tag) => match tag.name().as_ref() {
                "w:p" => {
                    open.pop();
                }
                "w:t" => in_text = false,
                _ => (),
            },
            Event::Text(text) if in_text => {
                if let Some(&current) = open.last() {
                    paragraphs[current].text.push_str(&text);
                }
            }
            Event::GeneralRef(reference) if in_text => {
                if let Some(&current) = open.last() {
                    let escaped = format!("&{};", reference.as_ref());
                    paragraphs[current]
                        .text
                        .push_str(&quick_xml::escape::unescape(&escaped)?);
                }
            }
            Event::Eof => break,
            _ => (),
        }
    }
    for p in paragraphs {
        for (start, matched) in p.text.match_indices(find) {
            ensure!(
                !p.boundaries
                    .iter()
                    .any(|&at| start < at && at < start + matched.len()),
                "Search would cross a tab, break, field or embedded object; choose a smaller search phrase"
            );
        }
    }
    Ok(())
}

/// Replace literal text across runs within each paragraph, including referenced
/// headers/footers/notes/comments. Formatting follows the first matching run.
/// Existing outputs (including source aliases/hardlinks) are never overwritten.
pub fn replace_docx(
    input: &Path,
    output: &Path,
    find: &str,
    replacement: &str,
) -> Result<EditReport> {
    ensure!(!find.is_empty(), "Search text must not be empty");
    ensure!(
        valid_xml_text(find) && valid_xml_text(replacement),
        "Only single-line XML text is supported; control characters and line breaks are rejected"
    );
    ensure!(
        input
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("docx")),
        "Text replacement requires DOCX input"
    );
    ensure!(
        output
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("docx")),
        "Output must have a .docx extension"
    );
    ensure!(
        !output.try_exists()?,
        "Output already exists; choose a new file"
    );
    let mut source = Vec::new();
    File::open(input)?
        .take(INPUT_LIMIT + 1)
        .read_to_end(&mut source)?;
    ensure!(source.len() as u64 <= INPUT_LIMIT, "Input exceeds 64 MiB");
    let original = parts(&source)?;
    let mut editable =
        EditableDocx::from_reader(Cursor::new(&source)).context("Cannot load DOCX editor")?;
    let replacements = editable.replace_text(find, replacement);
    ensure!(replacements > 0, "No matching text; no output was created");
    let mut edited = Cursor::new(Vec::new());
    editable.write_to(&mut edited)?;
    let edited = parts(edited.get_ref())?;
    let mut merged = original.clone();
    let mut changed_parts = Vec::new();
    for (name, before) in &original {
        // Preserve raw relationship XML and content types; the backend regenerates these.
        if name.ends_with(".rels") || name == "[Content_Types].xml" || name.ends_with('/') {
            continue;
        }
        let after = edited
            .get(name)
            .with_context(|| format!("Editor dropped part: {name}"))?;
        if after != before {
            ensure!(name.ends_with(".xml"), "Unexpected non-XML edit: {name}");
            check_match_boundaries(before, find)?;
            let mut reader = quick_xml::Reader::from_reader(after.as_slice());
            loop {
                match reader.read_event()? {
                    quick_xml::events::Event::Eof => break,
                    quick_xml::events::Event::DocType(_) => bail!("DTD is not supported"),
                    _ => (),
                }
            }
            changed_parts.push(name.clone());
            merged.insert(name.clone(), after.clone());
        }
    }
    ensure!(
        !changed_parts.is_empty(),
        "Replacement leaves document unchanged; no output created"
    );
    let mut package = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut package);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in &merged {
            if name.ends_with('/') {
                writer.add_directory(name, options)?;
            } else {
                writer.start_file(name, options)?;
                writer.write_all(data)?;
            }
        }
        writer.finish()?;
    }
    // Verify the exact outgoing package, not a separately regenerated representation.
    let reopened = parts(package.get_ref())?;
    ensure!(reopened == merged, "Saved DOCX part verification failed");
    office_oxide::Document::from_reader(
        Cursor::new(package.get_ref().clone()),
        office_oxide::DocumentFormat::Docx,
    )
    .context("Edited DOCX cannot be reopened")?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(package.get_ref())?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(output)
        .map_err(|e| anyhow::anyhow!("Cannot publish output without overwriting: {}", e.error))?;
    Ok(EditReport {
        replacements,
        preserved_parts: original.len() - changed_parts.len(),
        changed_parts,
        warnings: vec![
            "Replacement covers body and referenced headers, footers, notes and comments; it does not cross paragraphs".into(),
            "Replacement adopts the starting run's formatting; pagination and visual layout still require review".into(),
            "Unmodified ZIP entry payloads are preserved; ZIP compression and metadata are regenerated".into(),
        ],
    })
}
