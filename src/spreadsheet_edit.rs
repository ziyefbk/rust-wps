//! Editable sparse workbook model. Formula evaluation is a separate capability.
use anyhow::{Result, ensure};
use office_oxide::xlsx::write::{CellData, CellStyle, XlsxWriter};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::{Cursor, Read, Seek, Write},
    path::Path,
};

// The pinned writer emits cellStyleXfs/cellXfs but omits the named Normal style.
// Complete the generated styles part so independent readers need no fallback.
const MODEL_PART: &str = "rust-wps/workbook-v1.json";

fn write_with_default_style<W: Write + Seek>(
    writer: &XlsxWriter,
    destination: W,
    model: &Workbook,
) -> Result<()> {
    let mut encoded = Cursor::new(Vec::new());
    writer.write_to(&mut encoded)?;
    ensure!(
        encoded.get_ref().len() <= 64 * 1024 * 1024,
        "Generated workbook exceeds 64 MiB"
    );
    let mut source = zip::ZipArchive::new(encoded)?;
    let mut output = zip::ZipWriter::new(destination);
    for index in 0..source.len() {
        let mut entry = source.by_index(index)?;
        if entry.name() != "xl/styles.xml"
            && entry.name() != "[Content_Types].xml"
            && !(model.date1904 && entry.name() == "xl/workbook.xml")
        {
            output.raw_copy_file(entry)?;
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        let name = entry.name().to_owned();
        let mut reader = quick_xml::Reader::from_reader(bytes.as_slice());
        let mut xml = quick_xml::Writer::new(Vec::new());
        loop {
            let event = reader.read_event()?;
            if model.date1904
                && name == "xl/workbook.xml"
                && matches!(&event, quick_xml::events::Event::Start(start) if start.name().as_ref() == "sheets")
            {
                xml.get_mut().write_all(br#"<workbookPr date1904="1"/>"#)?;
            }
            if let quick_xml::events::Event::End(end) = &event
                && end.name().as_ref() == "styleSheet"
            {
                xml.get_mut().write_all(br#"<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>"#)?;
            }
            if let quick_xml::events::Event::End(end) = &event
                && end.name().as_ref() == "Types"
            {
                xml.get_mut().write_all(br#"<Override PartName="/rust-wps/workbook-v1.json" ContentType="application/json"/>"#)?;
            }
            if matches!(event, quick_xml::events::Event::Eof) {
                break;
            }
            xml.write_event(event)?;
        }
        output.start_file(
            name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )?;
        output.write_all(&xml.into_inner())?;
    }
    output.start_file(
        MODEL_PART,
        zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated),
    )?;
    let model_json = serde_json::to_vec(model)?;
    ensure!(
        model_json.len() <= 16 * 1024 * 1024,
        "Embedded workbook model exceeds 16 MiB"
    );
    output.write_all(&model_json)?;
    output.finish()?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Workbook {
    pub version: u32,
    #[serde(default, skip_serializing_if = "is_false")]
    pub date1904: bool,
    pub sheets: Vec<Sheet>,
}
fn is_false(value: &bool) -> bool {
    !value
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Sheet {
    pub name: String,
    #[serde(default)]
    pub hidden: bool,
    pub cells: Vec<Cell>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Cell {
    pub row: u32,
    pub column: u32,
    pub content: Content,
    #[serde(default)]
    pub number_format: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Content {
    Empty,
    Number(f64),
    Text(String),
    Boolean(bool),
    Formula(String),
}

fn valid_xml(text: &str) -> bool {
    text.chars().all(|c| {
        matches!(c, '\t' | '\n' | '\r') || (c >= ' ' && c != '\u{fffe}' && c != '\u{ffff}')
    })
}

/// Workbook drafts use their own slot, separate from the Word document draft.
/// Validate before replacement so an invalid edit cannot destroy recovery data.
pub fn save_draft(model: &Workbook, path: &Path) -> Result<()> {
    validate(model)?;
    let bytes = serde_json::to_vec(model)?;
    ensure!(
        bytes.len() <= 16 * 1024 * 1024,
        "Workbook draft exceeds 16 MiB"
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
        .map_err(|error| anyhow::anyhow!("Cannot save workbook draft: {}", error.error))?;
    Ok(())
}

pub fn load_draft(path: &Path) -> Result<Option<Workbook>> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut bytes = Vec::new();
    file.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 16 * 1024 * 1024,
        "Workbook draft exceeds 16 MiB"
    );
    let model = serde_json::from_slice(&bytes)?;
    validate(&model)?;
    Ok(Some(model))
}

pub fn validate(model: &Workbook) -> Result<()> {
    ensure!(model.version == 1, "Unsupported workbook model version");
    ensure!(
        (1..=100).contains(&model.sheets.len()),
        "Workbook requires 1..100 sheets"
    );
    ensure!(
        model.sheets.iter().any(|sheet| !sheet.hidden),
        "At least one sheet must be visible"
    );
    let mut names = BTreeSet::new();
    let mut count = 0usize;
    let mut text_size = 0usize;
    for sheet in &model.sheets {
        ensure!(
            !sheet.name.trim().is_empty()
                && sheet.name.encode_utf16().count() <= 31
                && !sheet.name.contains(['[', ']', ':', '*', '?', '/', '\\'])
                && !sheet.name.starts_with('\'')
                && !sheet.name.ends_with('\'')
                && valid_xml(&sheet.name),
            "Invalid worksheet name"
        );
        ensure!(
            names.insert(sheet.name.to_lowercase()),
            "Duplicate worksheet name"
        );
        text_size += sheet.name.len();
        let mut positions = BTreeSet::new();
        for cell in &sheet.cells {
            count += 1;
            ensure!(count <= 100_000, "Workbook exceeds 100000 cells");
            ensure!(
                cell.row < 1048576 && cell.column < 16384,
                "Cell outside XLSX grid"
            );
            ensure!(
                positions.insert((cell.row, cell.column)),
                "Duplicate cell address"
            );
            match &cell.content {
                Content::Number(number) => ensure!(number.is_finite(), "Non-finite cell number"),
                Content::Text(text) => {
                    ensure!(
                        text.encode_utf16().count() <= 32767 && valid_xml(text),
                        "Invalid or oversized cell text"
                    );
                    text_size += text.len();
                }
                Content::Formula(formula) => {
                    ensure!(
                        !formula.trim().is_empty()
                            && !formula.starts_with('=')
                            && formula.encode_utf16().count() <= 8192
                            && valid_xml(formula),
                        "Invalid formula; omit leading equals sign"
                    );
                    text_size += formula.len();
                }
                _ => {}
            }
            if let Some(format) = &cell.number_format {
                ensure!(
                    !format.is_empty() && format.len() <= 1024 && valid_xml(format),
                    "Invalid number format"
                );
                text_size += format.len();
            }
            ensure!(
                text_size <= 4 * 1024 * 1024,
                "Workbook model text exceeds 4 MiB"
            );
        }
    }
    Ok(())
}

/// Create a new file, preserving source files and refusing to overwrite destinations.
/// New formulas deliberately have no cached value; no result is invented.
fn encode(model: &Workbook) -> Result<Vec<u8>> {
    validate(model)?;
    let mut writer = XlsxWriter::new();
    for source in &model.sheets {
        let mut sheet = writer.add_sheet(&source.name);
        sheet.set_hidden(source.hidden);
        for cell in &source.cells {
            let value = match &cell.content {
                Content::Empty => CellData::Empty,
                Content::Number(number) => CellData::Number(*number),
                Content::Text(text) => CellData::String(text.clone()),
                Content::Boolean(boolean) => CellData::Boolean(*boolean),
                Content::Formula(formula) => CellData::Formula(formula.clone()),
            };
            let style = cell
                .number_format
                .as_ref()
                .map_or_else(CellStyle::new, |format| {
                    CellStyle::new().number_format_code(format)
                });
            sheet.set_cell_styled(cell.row as usize, cell.column as usize, value, style);
        }
    }
    let mut bytes = Cursor::new(Vec::new());
    write_with_default_style(&writer, &mut bytes, model)?;
    ensure!(
        bytes.get_ref().len() <= 64 * 1024 * 1024,
        "Generated workbook exceeds 64 MiB"
    );
    Ok(bytes.into_inner())
}

fn parts(bytes: &[u8]) -> Result<std::collections::BTreeMap<String, Vec<u8>>> {
    crate::editing::preflight_ooxml(bytes, "XLSX")?;
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut parts = std::collections::BTreeMap::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let mut data = Vec::new();
        entry.read_to_end(&mut data)?;
        parts.insert(entry.name().to_owned(), data);
    }
    Ok(parts)
}

/// A model grants editing only when its regenerated package agrees with every
/// uncompressed part. ZIP timestamps/compression do not affect this comparison.
pub fn load_authored_xlsx(path: &Path) -> Result<Option<Workbook>> {
    load_authored_xlsx_bytes(&crate::spreadsheet::read_xlsx_snapshot(path)?)
}

pub fn load_authored_xlsx_bytes(bytes: &[u8]) -> Result<Option<Workbook>> {
    ensure!(bytes.len() <= 64 * 1024 * 1024, "Input exceeds 64 MiB");
    let actual = parts(bytes)?;
    let Some(model) = actual.get(MODEL_PART) else {
        return Ok(None);
    };
    ensure!(
        model.len() <= 16 * 1024 * 1024,
        "Embedded workbook model exceeds 16 MiB"
    );
    let model: Workbook = serde_json::from_slice(model)?;
    let expected = parts(&encode(&model)?)?;
    ensure!(
        actual.len() == expected.len(),
        "XLSX package parts differ from embedded model"
    );
    for (name, data) in &actual {
        if name != MODEL_PART {
            ensure!(
                expected.get(name) == Some(data),
                "Embedded model disagrees with XLSX part: {name}"
            );
        }
    }
    Ok(Some(model))
}

/// Publish only after reopening both the spreadsheet and its editable model.
pub fn export_xlsx(model: &Workbook, output: &Path) -> Result<crate::spreadsheet::WorkbookReport> {
    ensure!(
        output
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("xlsx")),
        "Output must be XLSX"
    );
    ensure!(
        !output.try_exists()?,
        "Output already exists; choose a new file"
    );
    let bytes = encode(model)?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(&bytes)?;
    temp.flush()?;
    let report = crate::spreadsheet::inspect_xlsx(temp.path())?;
    ensure!(
        load_authored_xlsx(temp.path())?.as_ref() == Some(model),
        "Generated editable workbook model mismatch"
    );
    ensure!(
        report.date1904 == model.date1904 && report.sheets.len() == model.sheets.len(),
        "Generated workbook structure mismatch"
    );
    for (source, actual) in model.sheets.iter().zip(&report.sheets) {
        ensure!(
            source.name == actual.name && (actual.visibility == "Hidden") == source.hidden,
            "Generated sheet metadata mismatch"
        );
        ensure!(
            source.cells.len() == actual.cells.len(),
            "Generated cell count mismatch"
        );
        let actual_cells: std::collections::BTreeMap<_, _> = actual
            .cells
            .iter()
            .map(|cell| ((cell.row, cell.column), cell))
            .collect();
        for expected in &source.cells {
            let actual = actual_cells
                .get(&(expected.row, expected.column))
                .ok_or_else(|| anyhow::anyhow!("Generated cell missing"))?;
            use crate::spreadsheet::Value;
            let equal = match (&expected.content, &actual.value) {
                (Content::Empty, Value::Empty) => true,
                (Content::Number(a), Value::Number(b)) => a == b,
                (Content::Text(a), Value::Text(b)) => a == b,
                (Content::Boolean(a), Value::Boolean(b)) => a == b,
                (Content::Formula(formula), Value::Empty) => {
                    actual.formula.as_ref() == Some(formula)
                        && actual.formula_cache.as_deref() == Some("absent")
                }
                _ => false,
            };
            ensure!(equal, "Generated cell value/formula mismatch");
            if !matches!(expected.content, Content::Formula(_)) {
                ensure!(
                    actual.formula.is_none(),
                    "Unexpected formula in generated cell"
                );
            }
            if let Some(format) = &expected.number_format {
                ensure!(
                    actual.number_format.as_ref() == Some(format),
                    "Generated number format mismatch"
                );
            }
        }
    }
    temp.as_file().sync_all()?;
    temp.persist_noclobber(output)
        .map_err(|err| anyhow::anyhow!("Cannot publish workbook: {}", err.error))?;
    Ok(report)
}
