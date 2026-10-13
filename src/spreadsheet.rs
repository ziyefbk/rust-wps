//! Read-only sparse XLSX inventory. Formula caches are not recalculated.
use anyhow::{Context, Result, ensure};
use office_oxide::xlsx::{CellValue, XlsxDocument};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::{Cursor, Read},
    path::Path,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkbookReport {
    pub format: String,
    pub date1904: bool,
    pub sheets: Vec<Sheet>,
    pub warnings: Vec<String>,
    pub recalculated: bool,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Sheet {
    pub name: String,
    pub visibility: String,
    pub kind: String,
    pub declared_dimension: Option<String>,
    pub merged_ranges: Vec<String>,
    pub cells: Vec<Cell>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Cell {
    pub address: String,
    pub row: u32,
    pub column: u32,
    pub value: Value,
    pub formula: Option<String>,
    pub formula_cache: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub formula_unavailable: bool,
    pub number_format: Option<String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Value {
    Empty,
    Number(f64),
    Text(String),
    Boolean(bool),
    Error(String),
    Date(String),
}

pub fn inspect_xlsx(path: &Path) -> Result<WorkbookReport> {
    inspect_xlsx_bytes(&read_xlsx_snapshot(path)?)
}

pub fn read_xlsx_snapshot(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 64 * 1024 * 1024, "Input exceeds 64 MiB");
    Ok(bytes)
}

pub fn inspect_xlsx_bytes(bytes: &[u8]) -> Result<WorkbookReport> {
    ensure!(bytes.len() <= 64 * 1024 * 1024, "Input exceeds 64 MiB");
    crate::editing::preflight_ooxml(bytes, "XLSX")?;
    let mut workbook = XlsxDocument::from_reader(Cursor::new(bytes))?;
    ensure!(
        workbook.unreadable_sheets.is_empty(),
        "Workbook contains unreadable sheets: {:?}",
        workbook.unreadable_sheets
    );
    ensure!(
        workbook.worksheets.len() <= 100,
        "Workbook exceeds 100 sheets"
    );
    let styles = workbook.ensure_styles().cloned();
    let mut report = WorkbookReport { format: "xlsx".into(), date1904: workbook.workbook.date1904, sheets: vec![], warnings: vec!["Read-only inventory; formulas retain cached values and are not recalculated".into(), "Styles, charts, objects and workbook behavior are not fully represented; this is not an editable or lossless import".into()], recalculated: false };
    if workbook.has_macros {
        report
            .warnings
            .push("Workbook contains macros; they are not executed".into());
    }
    let mut count = 0usize;
    let mut text_bytes = 0usize;
    let mut names = BTreeSet::new();
    for sheet in &workbook.worksheets {
        ensure!(names.insert(&sheet.name), "Duplicate worksheet name");
        text_bytes += sheet.name.len() + sheet.merged_cells.iter().map(String::len).sum::<usize>();
        ensure!(
            text_bytes <= 4 * 1024 * 1024,
            "Workbook metadata exceeds 4 MiB"
        );
        let mut cells = Vec::new();
        let mut positions = BTreeSet::new();
        for cell in sheet.rows.iter().flat_map(|row| &row.cells) {
            count += 1;
            ensure!(
                count <= 100_000,
                "Workbook exceeds 100000 materialized cells"
            );
            ensure!(
                positions.insert((cell.reference.row, cell.reference.col)),
                "Duplicate cell address"
            );
            let value = match &cell.value {
                CellValue::Empty => Value::Empty,
                CellValue::Number(value) => {
                    ensure!(value.is_finite(), "Non-finite cell number");
                    Value::Number(*value)
                }
                CellValue::String(text) => Value::Text(text.clone()),
                CellValue::SharedString(index) => Value::Text(
                    workbook
                        .shared_strings
                        .get(*index)
                        .context("Missing shared string")?
                        .to_owned(),
                ),
                CellValue::Boolean(value) => Value::Boolean(*value),
                CellValue::Error(error) => Value::Error(error.clone()),
                CellValue::Date(date) => Value::Date(date.to_iso_string()),
            };
            if let Value::Text(text) | Value::Error(text) | Value::Date(text) = &value {
                text_bytes += text.len();
            }
            text_bytes += cell.formula.as_ref().map_or(0, String::len);
            let number_format = cell
                .style_index
                .and_then(|index| styles.as_ref()?.number_format_for(index));
            text_bytes += number_format.map_or(0, str::len);
            ensure!(
                text_bytes <= 4 * 1024 * 1024,
                "Workbook text/formulas exceed 4 MiB"
            );
            let formula_cache = cell.formula.as_ref().map(|_| {
                match &value {
                    Value::Empty => "absent",
                    Value::Text(text) if text.is_empty() => "empty_or_absent",
                    _ => "present",
                }
                .to_owned()
            });
            if formula_cache.as_deref() == Some("empty_or_absent")
                && !report
                    .warnings
                    .iter()
                    .any(|warning| warning.starts_with("Empty formula caches"))
            {
                report.warnings.push("Empty formula caches are ambiguous in the backend; an empty string is not proof of a computed result".into());
            }
            cells.push(Cell {
                address: cell.reference.to_string(),
                row: cell.reference.row,
                column: cell.reference.col,
                value,
                formula: cell.formula.clone(),
                formula_cache,
                formula_unavailable: false,
                number_format: number_format.map(str::to_owned),
            });
        }
        report.sheets.push(Sheet {
            name: sheet.name.clone(),
            visibility: format!("{:?}", sheet.state),
            kind: format!("{:?}", sheet.kind),
            declared_dimension: sheet.dimension.clone(),
            merged_ranges: sheet.merged_cells.clone(),
            cells,
        });
    }
    Ok(report)
}
