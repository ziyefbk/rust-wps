use rust_wps::spreadsheet_import::import_xlsx_bytes;
use std::io::{Cursor, Read, Write};
const SOURCE: &[u8] = include_bytes!("fixtures/external-basic.xlsx");
fn changed(part: &str, transform: impl Fn(&str) -> String) -> Vec<u8> {
    changed_bytes(SOURCE, part, transform)
}
fn changed_bytes(source: &[u8], part: &str, transform: impl Fn(&str) -> String) -> Vec<u8> {
    let mut input = zip::ZipArchive::new(Cursor::new(source)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..input.len() {
        let mut entry = input.by_index(index).unwrap();
        let name = entry.name().to_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        if name == part {
            bytes = transform(std::str::from_utf8(&bytes).unwrap()).into_bytes();
        }
        output
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        output.write_all(&bytes).unwrap();
    }
    output.finish().unwrap().into_inner()
}
#[test]
fn basic_external_workbook_converts_with_explicit_risks_and_source_unchanged() {
    let before = SOURCE.to_vec();
    let report = import_xlsx_bytes(SOURCE).unwrap();
    assert!(report.blockers.is_empty(), "{:?}", report.blockers);
    assert!(report.risks.len() >= 4);
    let model = report.model.unwrap();
    assert!(model.sheets[1].hidden);
    assert_eq!(
        model.sheets[0].cells[3].content,
        rust_wps::spreadsheet_edit::Content::Text("001".into())
    );
    assert_eq!(
        model.sheets[0].cells[4].number_format.as_deref(),
        Some("0.00")
    );
    assert_eq!(
        model.sheets[0].cells[5].content,
        rust_wps::spreadsheet_edit::Content::Formula("B2*2".into())
    );
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("copy.xlsx");
    rust_wps::spreadsheet_edit::export_xlsx(&model, &target).unwrap();
    assert_eq!(
        rust_wps::spreadsheet_edit::load_authored_xlsx(&target).unwrap(),
        Some(model)
    );
    assert_eq!(SOURCE, before);
}
#[test]
fn critical_features_never_grant_an_editable_model() {
    for (part, from, to) in [
        (
            "xl/workbook.xml",
            "<workbookProtection/>",
            "<workbookProtection lockStructure=\"1\"/>",
        ),
        (
            "xl/workbook.xml",
            "<definedNames/>",
            "<definedNames><definedName name=\"Budget\">Data!$B$2</definedName></definedNames>",
        ),
        (
            "xl/worksheets/sheet1.xml",
            "<f>",
            "<f t=\"array\" ref=\"C2\">",
        ),
        (
            "xl/worksheets/sheet1.xml",
            "</worksheet>",
            "<mergeCells><mergeCell ref=\"A1:B1\"/></mergeCells></worksheet>",
        ),
        (
            "xl/worksheets/sheet1.xml",
            "</worksheet>",
            "<autoFilter ref=\"A1:C2\"/></worksheet>",
        ),
        (
            "xl/worksheets/sheet1.xml",
            "</worksheet>",
            "<unknownContent/></worksheet>",
        ),
        (
            "xl/_rels/workbook.xml.rels",
            "Target=",
            "TargetMode=\"External\" Target=",
        ),
    ] {
        let bytes = changed(part, |xml| {
            assert!(xml.contains(from), "missing fixture token {from}");
            xml.replace(from, to)
        });
        // Invalid external targets may also be rejected by the reader.
        if let Ok(report) = import_xlsx_bytes(&bytes) {
            assert!(report.model.is_none(), "{from} → {to}");
            assert!(!report.blockers.is_empty());
        }
    }
}
#[test]
fn corrupt_and_oversized_sources_fail_before_model_publication() {
    assert!(import_xlsx_bytes(b"PKbroken").is_err());
    assert!(import_xlsx_bytes(&vec![0; 64 * 1024 * 1024 + 1]).is_err());
    let invalid_number = changed("xl/worksheets/sheet1.xml", |xml| {
        xml.replace("<v>12.5</v>", "<v>not-a-number</v>")
    });
    assert!(import_xlsx_bytes(&invalid_number).is_err());
}

#[test]
fn date1904_survives_import_draft_export_and_verified_reopen() {
    let bytes = changed("xl/workbook.xml", |xml| {
        xml.replace("<workbookPr/>", "<workbookPr date1904=\"1\"/>")
    });
    let report = import_xlsx_bytes(&bytes).unwrap();
    assert!(report.blockers.is_empty(), "{:?}", report.blockers);
    let model = report.model.unwrap();
    assert!(model.date1904);
    let directory = tempfile::tempdir().unwrap();
    let draft = directory.path().join("draft.json");
    rust_wps::spreadsheet_edit::save_draft(&model, &draft).unwrap();
    assert_eq!(
        rust_wps::spreadsheet_edit::load_draft(&draft).unwrap(),
        Some(model.clone())
    );
    let output = directory.path().join("date1904.xlsx");
    let exported = rust_wps::spreadsheet_edit::export_xlsx(&model, &output).unwrap();
    assert!(exported.date1904);
    assert_eq!(
        rust_wps::spreadsheet_edit::load_authored_xlsx(&output).unwrap(),
        Some(model)
    );
    let tampered = changed_bytes(&std::fs::read(&output).unwrap(), "xl/workbook.xml", |xml| {
        xml.replace("date1904=\"1\"", "date1904=\"0\"")
    });
    assert!(rust_wps::spreadsheet_edit::load_authored_xlsx_bytes(&tampered).is_err());
    let mut legacy =
        serde_json::to_value(import_xlsx_bytes(SOURCE).unwrap().model.unwrap()).unwrap();
    assert!(legacy.get("date1904").is_none());
    assert!(
        !serde_json::from_value::<rust_wps::spreadsheet_edit::Workbook>(legacy.clone())
            .unwrap()
            .date1904
    );
    legacy["date1904"] = serde_json::json!("true");
    assert!(serde_json::from_value::<rust_wps::spreadsheet_edit::Workbook>(legacy).is_err());
}
