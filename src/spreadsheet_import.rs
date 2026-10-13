//! Audited conversion into the basic editor model, never a lossless package import.
use crate::{
    spreadsheet::{self, Value},
    spreadsheet_edit::{self, Cell, Content, Sheet, Workbook},
};
use anyhow::{Result, ensure};
use quick_xml::events::Event;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::{Cursor, Read},
    path::Path,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportReport {
    pub model: Option<Workbook>,
    pub risks: Vec<String>,
    pub blockers: Vec<String>,
}

pub fn import_xlsx(path: &Path) -> Result<ImportReport> {
    import_xlsx_bytes(&spreadsheet::read_xlsx_snapshot(path)?)
}

pub fn import_xlsx_bytes(bytes: &[u8]) -> Result<ImportReport> {
    ensure!(bytes.len() <= 64 * 1024 * 1024, "Input exceeds 64 MiB");
    crate::editing::preflight_ooxml(bytes, "XLSX")?;
    let mut blockers = BTreeSet::new();
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut raw_cells = 0usize;
    let mut raw_formulas = 0usize;
    let mut raw_sheets = 0usize;
    for index in 0..zip.len() {
        let mut part = zip.by_index(index)?;
        let name = part.name().to_owned();
        if part.is_dir() {
            continue;
        }
        let worksheet = name.starts_with("xl/worksheets/")
            && name.ends_with(".xml")
            && !name.contains("/_rels/");
        let styles = name == "xl/styles.xml";
        let strings = name == "xl/sharedStrings.xml";
        let workbook = name == "xl/workbook.xml";
        let relationships = name == "_rels/.rels" || name == "xl/_rels/workbook.xml.rels";
        let metadata = name.starts_with("docProps/") && name.ends_with(".xml")
            || name.starts_with("xl/theme/") && name.ends_with(".xml");
        if !(worksheet
            || styles
            || strings
            || workbook
            || relationships
            || metadata
            || name == "[Content_Types].xml")
        {
            blockers.insert(format!(
                "不支持的工作簿部件：{}",
                name.chars().take(200).collect::<String>()
            ));
            ensure!(
                blockers.len() <= 128,
                "Too many unsupported workbook features"
            );
            continue;
        }
        if worksheet {
            raw_sheets += 1;
        }
        let mut xml = Vec::new();
        part.read_to_end(&mut xml)?;
        let mut reader = quick_xml::Reader::from_reader(xml.as_slice());
        let mut cell_type = String::from("n");
        loop {
            let event = reader.read_event()?;
            let empty = matches!(event, Event::Empty(_));
            match event {
                Event::DocType(_) => anyhow::bail!("DOCTYPE is not supported"),
                Event::Start(element) | Event::Empty(element) => {
                    let qualified = element.name();
                    let tag = qualified.as_ref();
                    let allowed = if worksheet {
                        matches!(
                            tag,
                            "worksheet"
                                | "sheetPr"
                                | "outlinePr"
                                | "pageSetUpPr"
                                | "dimension"
                                | "sheetViews"
                                | "sheetView"
                                | "pane"
                                | "selection"
                                | "sheetFormatPr"
                                | "cols"
                                | "col"
                                | "sheetData"
                                | "row"
                                | "c"
                                | "v"
                                | "f"
                                | "is"
                                | "t"
                                | "r"
                                | "rPr"
                                | "rFont"
                                | "b"
                                | "i"
                                | "u"
                                | "strike"
                                | "sz"
                                | "color"
                                | "family"
                                | "scheme"
                                | "vertAlign"
                                | "pageMargins"
                                | "printOptions"
                                | "pageSetup"
                                | "headerFooter"
                                | "oddHeader"
                                | "oddFooter"
                                | "evenHeader"
                                | "evenFooter"
                                | "firstHeader"
                                | "firstFooter"
                        )
                    } else if workbook {
                        matches!(
                            tag,
                            "workbook"
                                | "fileVersion"
                                | "workbookPr"
                                | "bookViews"
                                | "workbookView"
                                | "definedNames"
                                | "workbookProtection"
                                | "sheets"
                                | "sheet"
                                | "calcPr"
                        )
                    } else if strings {
                        matches!(
                            tag,
                            "sst"
                                | "si"
                                | "t"
                                | "r"
                                | "rPr"
                                | "rFont"
                                | "b"
                                | "i"
                                | "u"
                                | "strike"
                                | "sz"
                                | "color"
                                | "family"
                                | "scheme"
                                | "vertAlign"
                        )
                    } else if styles {
                        matches!(
                            tag,
                            "styleSheet"
                                | "numFmts"
                                | "numFmt"
                                | "fonts"
                                | "font"
                                | "name"
                                | "sz"
                                | "color"
                                | "family"
                                | "scheme"
                                | "b"
                                | "i"
                                | "u"
                                | "strike"
                                | "vertAlign"
                                | "fills"
                                | "fill"
                                | "patternFill"
                                | "fgColor"
                                | "bgColor"
                                | "gradientFill"
                                | "stop"
                                | "borders"
                                | "border"
                                | "left"
                                | "right"
                                | "top"
                                | "bottom"
                                | "diagonal"
                                | "vertical"
                                | "horizontal"
                                | "start"
                                | "end"
                                | "cellStyleXfs"
                                | "cellXfs"
                                | "xf"
                                | "alignment"
                                | "cellStyles"
                                | "cellStyle"
                                | "dxfs"
                                | "tableStyles"
                                | "colors"
                                | "indexedColors"
                                | "rgbColor"
                                | "mruColors"
                        )
                    } else {
                        true
                    };
                    if !allowed {
                        blockers.insert(format!(
                            "未恢复的元素：{} / {}",
                            name.chars().take(100).collect::<String>(),
                            tag.chars().take(100).collect::<String>()
                        ));
                    }
                    if worksheet && tag == "c" {
                        raw_cells += 1;
                        cell_type = "n".into();
                    }
                    if worksheet && tag == "f" {
                        raw_formulas += 1;
                    }
                    for attribute in element.attributes() {
                        let attribute = attribute?;
                        let key = attribute.key.as_ref();
                        let value =
                            attribute.normalized_value(quick_xml::XmlVersion::Implicit1_0)?;
                        if worksheet && tag == "c" && key == "t" {
                            cell_type = value.to_string();
                            if !matches!(
                                cell_type.as_str(),
                                "n" | "s" | "str" | "inlineStr" | "b" | "e"
                            ) {
                                blockers.insert("尚不支持此单元格类型（含 ISO 日期）".into());
                            }
                        }
                        if workbook && tag == "workbookProtection" {
                            blockers.insert("工作簿保护属性不能恢复".into());
                        }
                        if (worksheet || workbook || strings || styles)
                            && key.starts_with("xmlns")
                            && !matches!(
                                value.as_ref(),
                                "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
                                    | "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
                            )
                        {
                            blockers.insert("尚不支持此 SpreadsheetML 命名空间或扩展".into());
                        }
                        if relationships && key == "TargetMode" && value == "External" {
                            blockers.insert("外部链接不能转为编辑副本".into());
                        }
                        if relationships && key == "Type" && ![
                            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument",
                            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet",
                            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles",
                            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings",
                            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme",
                            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties",
                            "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties",
                        ].contains(&value.as_ref()) { blockers.insert("尚不支持此工作簿关系类型".into()); }
                        if worksheet
                            && tag == "f"
                            && key == "t"
                            && !matches!(value.as_ref(), "normal" | "shared")
                        {
                            blockers.insert("数组或数据表公式不能转换为独立普通公式".into());
                        }
                        if worksheet && tag == "c" && !matches!(key, "r" | "s" | "t") {
                            blockers.insert("尚不支持单元格元数据属性".into());
                        }
                    }
                    ensure!(
                        raw_cells <= 100000 && raw_formulas <= 100000 && blockers.len() <= 128,
                        "Workbook audit exceeds limits"
                    );
                    if worksheet && tag == "v" && !empty {
                        let text = reader.read_text(element.name())?;
                        let valid = match cell_type.as_str() {
                            "b" => matches!(text.as_ref(), "0" | "1" | "true" | "false"),
                            "s" => text.parse::<u32>().is_ok(),
                            "n" => text.is_empty() || text.parse::<f64>().is_ok_and(f64::is_finite),
                            _ => true,
                        };
                        ensure!(
                            valid,
                            "Invalid typed cell value; refusing silent parser coercion"
                        );
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
    }
    let report = spreadsheet::inspect_xlsx_bytes(bytes)?;
    ensure!(
        raw_sheets == report.sheets.len(),
        "Unrepresented worksheet parts"
    );
    ensure!(
        raw_cells == report.sheets.iter().map(|s| s.cells.len()).sum::<usize>(),
        "Parser omitted materialized cells"
    );
    ensure!(
        raw_formulas
            == report
                .sheets
                .iter()
                .flat_map(|s| &s.cells)
                .filter(|c| c.formula.is_some())
                .count(),
        "Parser omitted formulas"
    );
    let mut model = Workbook {
        version: 1,
        date1904: report.date1904,
        sheets: vec![],
    };
    for sheet in report.sheets {
        if sheet.kind != "Worksheet" {
            blockers.insert("仅支持普通工作表编辑副本".into());
        }
        if sheet.visibility == "VeryHidden" {
            blockers.insert("不能恢复 VeryHidden 工作表语义".into());
        }
        let mut target = Sheet {
            name: sheet.name,
            hidden: sheet.visibility == "Hidden",
            cells: vec![],
        };
        for cell in sheet.cells {
            let content = if let Some(formula) = cell.formula {
                Content::Formula(formula)
            } else {
                match cell.value {
                    Value::Empty => Content::Empty,
                    Value::Number(n) => Content::Number(n),
                    Value::Text(t) => Content::Text(t),
                    Value::Boolean(b) => Content::Boolean(b),
                    Value::Error(_) | Value::Date(_) => {
                        blockers.insert("独立错误值或 ISO 日期尚无可保存的编辑类型".into());
                        Content::Empty
                    }
                }
            };
            target.cells.push(Cell {
                row: cell.row,
                column: cell.column,
                content,
                number_format: cell.number_format,
            });
        }
        model.sheets.push(target);
    }
    let risks = vec![
        "仅转换基本单元格内容、普通公式、数字格式及隐藏表；不是无损导入".into(),
        "字体、颜色、边框、富文本、行列尺寸/隐藏、视图、页眉页脚和打印设置会规范化或丢失".into(),
        "主题、文档属性及计算设置不保留；源公式缓存丢弃，需重新计算".into(),
        "原文件保持不变，编辑副本只能另存新文件；请与原件核对".into(),
    ];
    if blockers.is_empty() {
        spreadsheet_edit::validate(&model)?;
    }
    Ok(ImportReport {
        model: blockers.is_empty().then_some(model),
        risks,
        blockers: blockers.into_iter().collect(),
    })
}
