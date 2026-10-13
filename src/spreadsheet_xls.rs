//! BIFF8 read-only inventory. Formula expressions are deliberately not guessed.
use crate::spreadsheet::{Cell, Sheet, Value, WorkbookReport};
use anyhow::{Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
    path::Path,
};

fn u16_at(data: &[u8], index: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        data.get(index..index + 2)
            .ok_or_else(|| anyhow::anyhow!("Short BIFF record"))?
            .try_into()?,
    ))
}
fn address(row: u32, column: u32) -> String {
    let mut n = column + 1;
    let mut letters = String::new();
    while n > 0 {
        letters.insert(0, char::from(b'A' + ((n - 1) % 26) as u8));
        n = (n - 1) / 26;
    }
    format!("{letters}{}", row + 1)
}
#[derive(Default, Debug)]
struct Metadata {
    date1904: bool,
    bounds: Vec<(usize, u8)>,
    formulas: BTreeSet<(usize, u32, u32)>,
    xfs: Vec<u16>,
    formats: BTreeMap<u16, String>,
}
fn metadata(data: &[u8]) -> Result<Metadata> {
    let mut meta = Metadata::default();
    let mut position = 0;
    let mut records = 0;
    let mut stack = Vec::new();
    let mut current = None;
    let mut rows = BTreeMap::new();
    let mut slots = 0usize;
    let mut cells = 0usize;
    let mut format_bytes = 0usize;
    let mut date_seen = false;
    while position < data.len() {
        if data[position..].iter().take(4).all(|b| *b == 0) {
            ensure!(
                data[position..].iter().all(|b| *b == 0),
                "Invalid BIFF padding"
            );
            break;
        }
        let kind = u16_at(data, position)?;
        let length = usize::from(u16_at(data, position + 2)?);
        let record = data
            .get(position + 4..position + 4 + length)
            .ok_or_else(|| anyhow::anyhow!("Truncated BIFF record"))?;
        records += 1;
        ensure!(records <= 1_000_000, "BIFF exceeds record limit");
        if records == 1 {
            ensure!(
                kind == 0x0809 && u16_at(record, 0)? == 0x0600 && u16_at(record, 2)? == 0x0005,
                "Only OLE BIFF8 workbooks are supported"
            );
        }
        if kind == 0x002f {
            anyhow::bail!("Encrypted XLS is not supported");
        }
        match kind {
            0x0809 => {
                ensure!(record.len() >= 16, "Short BIFF8 BOF");
                ensure!(
                    u16_at(record, 0)? == 0x0600,
                    "Mixed BIFF versions unsupported"
                );
                if stack.is_empty() {
                    current = meta
                        .bounds
                        .iter()
                        .position(|(offset, _)| *offset == position);
                }
                stack.push(u16_at(record, 2)?);
                ensure!(stack.len() <= 16, "BIFF nesting limit");
            }
            0x000a => {
                ensure!(stack.pop().is_some(), "Unmatched BIFF EOF");
                if stack.is_empty() {
                    current = None;
                }
            }
            0x0085 if stack.as_slice() == [0x0005] => {
                ensure!(record.len() >= 8, "Short BoundSheet8");
                let offset = u32::from_le_bytes(record[..4].try_into()?) as usize;
                ensure!(
                    record[4] <= 2
                        && offset < data.len()
                        && !meta.bounds.iter().any(|(old, _)| *old == offset),
                    "Invalid BoundSheet8"
                );
                meta.bounds.push((offset, record[4]));
                ensure!(meta.bounds.len() <= 100, "XLS exceeds 100 sheets");
            }
            0x0022 if stack.as_slice() == [0x0005] => {
                ensure!(
                    !date_seen && record.len() == 2,
                    "Duplicate or invalid Date1904"
                );
                date_seen = true;
                let mode = u16_at(record, 0)?;
                ensure!(mode <= 1, "Invalid Date1904");
                meta.date1904 = mode == 1;
            }
            0x00e0 if stack.as_slice() == [0x0005] => {
                meta.xfs.push(u16_at(record, 2)?);
            }
            0x041e if stack.as_slice() == [0x0005] => {
                let id = u16_at(record, 0)?;
                let count = usize::from(u16_at(record, 2)?);
                let flag = *record
                    .get(4)
                    .ok_or_else(|| anyhow::anyhow!("Short FORMAT"))?;
                ensure!(flag <= 1, "Extended FORMAT string unsupported");
                let text = record
                    .get(5..5 + count * (usize::from(flag) + 1))
                    .ok_or_else(|| anyhow::anyhow!("Continued FORMAT string unsupported"))?;
                let code = if flag == 1 {
                    String::from_utf16(
                        &text
                            .chunks_exact(2)
                            .map(|p| u16::from_le_bytes([p[0], p[1]]))
                            .collect::<Vec<_>>(),
                    )?
                } else {
                    text.iter().map(|b| char::from(*b)).collect()
                };
                format_bytes += code.len();
                ensure!(format_bytes <= 4 * 1024 * 1024, "XLS formats exceed 4 MiB");
                meta.formats.insert(id, code);
            }
            _ => {}
        }
        if stack.len() == 1
            && stack[0] != 0x0020
            && let Some(sheet) = current
            && matches!(
                kind,
                0x0203
                    | 0x027e
                    | 0x00bd
                    | 0x0006
                    | 0x0205
                    | 0x00fd
                    | 0x0204
                    | 0x00d6
                    | 0x0201
                    | 0x00be
            )
        {
            let minimum = match kind {
                0x0203 => 14,
                0x027e | 0x00fd => 10,
                0x00bd => 12,
                0x0006 => 22,
                0x0205 | 0x00be => 8,
                0x0204 | 0x00d6 => 9,
                _ => 6,
            };
            ensure!(record.len() >= minimum, "Short BIFF cell record");
            let row = u32::from(u16_at(record, 0)?);
            let first = u32::from(u16_at(record, 2)?);
            let last = if matches!(kind, 0x00bd | 0x00be) {
                u32::from(u16_at(
                    record,
                    record
                        .len()
                        .checked_sub(2)
                        .ok_or_else(|| anyhow::anyhow!("Short multi-cell record"))?,
                )?)
            } else {
                first
            };
            ensure!(first <= last && last < 256, "Invalid BIFF8 column");
            if matches!(kind, 0x00bd | 0x00be) {
                let item = if kind == 0x00bd { 6 } else { 2 };
                ensure!(
                    record.len() == 6 + item * (last - first + 1) as usize,
                    "Invalid multi-cell record size"
                );
            }
            cells += (last - first + 1) as usize;
            ensure!(cells <= 100000, "XLS exceeds 100000 cell records");
            let old = rows.entry((sheet, row)).or_insert(0usize);
            let width = (last + 1) as usize;
            if width > *old {
                slots += width - *old;
                *old = width;
            }
            ensure!(slots <= 1_000_000, "XLS jagged grid exceeds 1000000 slots");
            if kind == 0x0006 {
                ensure!(
                    record.len() >= 22 && 22 + usize::from(u16_at(record, 20)?) <= record.len(),
                    "Short FORMULA record"
                );
                ensure!(
                    meta.formulas.insert((sheet, row, first)),
                    "Duplicate FORMULA cell"
                );
            }
        }
        position += 4 + length;
    }
    ensure!(
        records > 0 && stack.is_empty() && !meta.bounds.is_empty(),
        "Incomplete BIFF substreams"
    );
    for (offset, _) in &meta.bounds {
        ensure!(
            u16_at(data, *offset)? == 0x0809,
            "Sheet offset does not point to BOF"
        );
    }
    Ok(meta)
}

pub fn inspect_xls(path: &Path) -> Result<WorkbookReport> {
    inspect_xls_bytes(&crate::spreadsheet::read_xlsx_snapshot(path)?)
}
pub fn inspect_xls_bytes(bytes: &[u8]) -> Result<WorkbookReport> {
    ensure!(bytes.len() <= 64 * 1024 * 1024, "Input exceeds 64 MiB");
    ensure!(
        bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]),
        "File signature does not match OLE XLS"
    );
    let mut container = office_oxide::cfb::CfbReader::new(Cursor::new(bytes))?;
    ensure!(
        container.entries().len() <= 4096 && container.warnings().is_empty(),
        "Invalid or excessive CFB directory"
    );
    ensure!(
        container
            .entries()
            .iter()
            .filter(|entry| entry.name.eq_ignore_ascii_case("Workbook"))
            .count()
            == 1,
        "Missing or ambiguous Workbook stream"
    );
    let stream = container.open_stream_by_path("Workbook")?;
    ensure!(
        container.truncated_streams().is_empty(),
        "Truncated Workbook stream"
    );
    let meta = metadata(&stream)?;
    drop(container);
    drop(stream);
    let book = office_oxide::xls::XlsDocument::from_reader(Cursor::new(bytes))?;
    ensure!(
        !book.truncated() && book.notices().is_empty() && book.warnings().is_empty(),
        "XLS reader reported incomplete recovery"
    );
    ensure!(
        book.sheets.len() == meta.bounds.len(),
        "XLS sheet inventory mismatch"
    );
    let mut report = WorkbookReport {
        format: "xls".into(),
        date1904: meta.date1904,
        sheets: vec![],
        warnings: vec![
            "XLS 只读实验预览；公式只显示源缓存，表达式尚未恢复，禁止据此编辑或重算".into(),
            "格式、图表、对象、宏及工作簿行为未完整呈现；Hidden 与 VeryHidden 按原 BIFF 标记展示"
                .into(),
        ],
        recalculated: false,
    };
    if book.has_macros() {
        report.warnings.push("含宏；未执行".into());
    }
    let mut text_bytes = 0usize;
    let mut count = 0;
    let mut seen_formulas = BTreeSet::new();
    for (index, sheet) in book.sheets.iter().enumerate() {
        text_bytes += sheet.name.len();
        let mut target = Sheet {
            name: sheet.name.clone(),
            visibility: match meta.bounds[index].1 {
                0 => "Visible",
                1 => "Hidden",
                _ => "VeryHidden",
            }
            .into(),
            kind: format!("{:?}", sheet.kind),
            declared_dimension: None,
            merged_ranges: sheet
                .merged_cells
                .iter()
                .map(|&(r1, r2, c1, c2)| {
                    format!(
                        "{}:{}",
                        address(u32::from(r1), u32::from(c1)),
                        address(u32::from(r2), u32::from(c2))
                    )
                })
                .collect(),
            cells: vec![],
        };
        text_bytes += target.merged_ranges.iter().map(String::len).sum::<usize>();
        for (row, values) in sheet.rows.iter().enumerate() {
            for (column, source) in values.iter().enumerate() {
                let formula = meta.formulas.contains(&(index, row as u32, column as u32));
                let xf = sheet
                    .xf
                    .get(row)
                    .and_then(|r| r.get(column))
                    .copied()
                    .unwrap_or(0);
                if matches!(source, office_oxide::xls::CellValue::Empty) && xf == 0 && !formula {
                    continue;
                }
                count += 1;
                ensure!(count <= 100000, "XLS exceeds 100000 materialized cells");
                let format_id = *meta
                    .xfs
                    .get(usize::from(xf))
                    .ok_or_else(|| anyhow::anyhow!("Invalid XF reference"))?;
                let number_format = meta.formats.get(&format_id).cloned().or_else(|| {
                    office_oxide::xlsx::numfmt::builtin_format_code(u32::from(format_id))
                        .map(str::to_owned)
                });
                use office_oxide::xls::CellValue as X;
                let value = match source {
                    X::Empty => Value::Empty,
                    X::Number(n) => {
                        ensure!(n.is_finite(), "Non-finite XLS number");
                        Value::Number(*n)
                    }
                    X::String(t) => Value::Text(t.clone()),
                    X::Bool(b) => Value::Boolean(*b),
                    X::Error(_) => Value::Error(source.as_text()),
                };
                text_bytes += number_format.as_ref().map_or(0, String::len)
                    + match &value {
                        Value::Text(t) | Value::Error(t) => t.len(),
                        _ => 0,
                    };
                ensure!(
                    text_bytes <= 4 * 1024 * 1024,
                    "XLS text/metadata exceeds 4 MiB"
                );
                let formula_cache = if formula {
                    seen_formulas.insert((index, row as u32, column as u32));
                    Some(
                        match &value {
                            Value::Empty => "absent",
                            Value::Text(t) if t.is_empty() => "empty_or_absent",
                            _ => "present",
                        }
                        .into(),
                    )
                } else {
                    None
                };
                target.cells.push(Cell {
                    address: address(row as u32, column as u32),
                    row: row as u32,
                    column: column as u32,
                    value,
                    formula: None,
                    formula_cache,
                    formula_unavailable: formula,
                    number_format,
                });
            }
        }
        report.sheets.push(target);
    }
    ensure!(
        seen_formulas == meta.formulas,
        "XLS reader omitted formula caches"
    );
    ensure!(text_bytes <= 4 * 1024 * 1024, "XLS metadata exceeds 4 MiB");
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(kind: u16, payload: &[u8]) -> Vec<u8> {
        [
            kind.to_le_bytes().as_slice(),
            (payload.len() as u16).to_le_bytes().as_slice(),
            payload,
        ]
        .concat()
    }
    fn stream(body: &[u8]) -> Vec<u8> {
        let bof = |kind: u16| {
            let mut b = vec![0; 16];
            b[..2].copy_from_slice(&0x0600u16.to_le_bytes());
            b[2..4].copy_from_slice(&kind.to_le_bytes());
            record(0x0809, &b)
        };
        let mut global = bof(5);
        let offset = global.len() + 12 + 4;
        let mut bound = vec![0; 8];
        bound[..4].copy_from_slice(&(offset as u32).to_le_bytes());
        global.extend(record(0x0085, &bound));
        global.extend(record(0x000a, &[]));
        global.extend(bof(0x0010));
        global.extend(body);
        global.extend(record(0x000a, &[]));
        global
    }
    #[test]
    fn rejects_bad_record_lengths_encryption_and_unclosed_substreams() {
        assert!(metadata(&stream(&record(0x0203, &[0; 6]))).is_err());
        assert!(metadata(&stream(&record(0x002f, &[0; 4]))).is_err());
        let data = stream(&[]);
        assert!(metadata(&data[..data.len() - 4]).is_err());
        assert!(metadata(&stream(&record(0x00be, &[0; 8]))).is_ok());
        let mut bad = vec![0; 8];
        bad[6..8].copy_from_slice(&2u16.to_le_bytes());
        assert!(metadata(&stream(&record(0x00be, &bad))).is_err());
    }
    #[test]
    fn jagged_allocation_budget_is_checked_before_backend_grid_allocation() {
        let mut body = Vec::new();
        for row in 0..4000u16 {
            let mut cell = vec![0; 14];
            cell[..2].copy_from_slice(&row.to_le_bytes());
            cell[2..4].copy_from_slice(&255u16.to_le_bytes());
            body.extend(record(0x0203, &cell));
        }
        assert!(
            metadata(&stream(&body))
                .unwrap_err()
                .to_string()
                .contains("1000000 slots")
        );
    }
}
