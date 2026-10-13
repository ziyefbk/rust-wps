use std::io::Write;

#[test]
fn inventories_risky_features_without_changing_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("external.docx");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    for (name, xml) in [
        (
            "word/document.xml",
            r#"<x:document xmlns:x="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><x:body><x:p><x:ins/><x:fldSimple/><x:drawing/></x:p><x:tbl><x:gridSpan/></x:tbl></x:body></x:document>"#,
        ),
        ("word/header1.xml", "<header/>"),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    let before = std::fs::read(&path).unwrap();
    let report = rust_wps::compatibility::audit_docx(&path).unwrap();
    assert_eq!(
        (report.paragraphs, report.tables, report.drawings),
        (1, 1, 1)
    );
    assert!(report.risks.contains("Tracked changes"));
    assert!(report.risks.contains("Merged table cells"));
    assert!(report.risks.iter().any(|r| r.contains("header1.xml")));
    assert!(!report.fidelity_verified);
    assert_eq!(before, std::fs::read(path).unwrap());
}

#[test]
fn malformed_xml_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.docx");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    zip.start_file(
        "word/document.xml",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.write_all(b"<document><p></document>").unwrap();
    zip.finish().unwrap();
    assert!(rust_wps::compatibility::audit_docx(&path).is_err());
}
