use rust_wps::spreadsheet::{Value, inspect_xlsx};
use std::io::Write;

fn workbook(path: &std::path::Path, cells: &str) {
    let sheet = format!(
        r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="A1:XFD1048576"/><sheetData>{cells}</sheetData><mergeCells><mergeCell ref="A1:B1"/></mergeCells></worksheet>"#
    );
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, xml) in [
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="office" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
        ),
        (
            "xl/workbook.xml",
            r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><workbookPr date1904="1"/><sheets><sheet name="数据" sheetId="1" r:id="s1"/><sheet name="隐藏" sheetId="2" state="hidden" r:id="s2"/></sheets></workbook>"#,
        ),
        (
            "xl/_rels/workbook.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="s1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="s2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/><Relationship Id="strings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#,
        ),
        (
            "xl/sharedStrings.xml",
            r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><si><t>中文共享文本</t></si></sst>"#,
        ),
        (
            "xl/styles.xml",
            r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><cellXfs count="2"><xf numFmtId="0"/><xf numFmtId="14" applyNumberFormat="1"/></cellXfs></styleSheet>"#,
        ),
        ("xl/worksheets/sheet1.xml", sheet.as_str()),
        (
            "xl/worksheets/sheet2.xml",
            r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>隐藏内容</t></is></c></row></sheetData></worksheet>"#,
        ),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn sparse_xlsx_keeps_types_formula_cache_hidden_sheets_and_date_system() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("book.xlsx");
    workbook(
        &source,
        r#"<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1"><v>12.5</v></c><c r="C1" t="b"><v>1</v></c><c r="D1" t="e"><v>#DIV/0!</v></c><c r="E1"><f>B1*2</f><v>25</v></c><c r="F1"><f>SUM(B1:E1)</f></c><c r="G1" s="1"><v>1</v></c></row><row r="1048576"><c r="XFD1048576" t="inlineStr"><is><t>远端</t></is></c></row>"#,
    );
    let before = std::fs::read(&source).unwrap();
    let report = inspect_xlsx(&source).unwrap();
    assert!(report.date1904);
    assert!(!report.recalculated);
    assert_eq!(report.sheets.len(), 2);
    assert_eq!(report.sheets[0].name, "数据");
    assert_eq!(report.sheets[1].visibility, "Hidden");
    assert_eq!(report.sheets[0].merged_ranges, ["A1:B1"]);
    let cells = &report.sheets[0].cells;
    assert_eq!(cells.len(), 8);
    assert!(matches!(&cells[0].value, Value::Text(s) if s == "中文共享文本"));
    assert!(matches!(cells[1].value, Value::Number(12.5)));
    assert!(matches!(cells[2].value, Value::Boolean(true)));
    assert!(matches!(&cells[3].value, Value::Error(s) if s == "#DIV/0!"));
    assert_eq!(cells[4].formula.as_deref(), Some("B1*2"));
    assert_eq!(cells[4].formula_cache.as_deref(), Some("present"));
    assert!(matches!(cells[4].value, Value::Number(25.0)));
    assert_eq!(cells[5].formula.as_deref(), Some("SUM(B1:E1)"));
    assert!(matches!(cells[5].value, Value::Empty));
    assert_eq!(cells[5].formula_cache.as_deref(), Some("absent"));
    assert!(cells[6].number_format.is_some());
    assert_eq!((cells[7].row, cells[7].column), (1048575, 16383));
    assert_eq!(cells[7].address, "XFD1048576");
    assert_eq!(std::fs::read(source).unwrap(), before);
    let json = serde_json::to_string(&report).unwrap();
    let _: rust_wps::spreadsheet::WorkbookReport = serde_json::from_str(&json).unwrap();
}

#[test]
fn rejects_duplicate_cells_bad_archives_and_oversized_input() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("bad.xlsx");
    workbook(
        &source,
        r#"<row r="1"><c r="A1"><v>1</v></c><c r="A1"><v>2</v></c></row>"#,
    );
    assert!(
        inspect_xlsx(&source)
            .unwrap_err()
            .to_string()
            .contains("Duplicate cell")
    );
    std::fs::write(&source, b"PK\x03\x04broken").unwrap();
    assert!(inspect_xlsx(&source).is_err());
    std::fs::File::create(&source)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    assert!(
        inspect_xlsx(&source)
            .unwrap_err()
            .to_string()
            .contains("64 MiB")
    );
}
