//! Author/source tests for the optional row flag. Owned OPC controls are not
//! genuine corpus files or independent consumer acceptance.
use rust_wps::{
    compatibility,
    document::{self, Node},
    importing,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read, Write},
    path::Path,
};
const PRESERVED: &str =
    "Direct table row cantSplit preserved; automatic pagination remains consumer-dependent";
const DECLARATION: &str = "Table row cantSplit not preserved: direct row declarations are invalid or outside the supported subset";
const SOURCE: &str = "Table row cantSplit not preserved: source structure or inherited row pagination is unsupported";
const NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
fn table(flags: &[&str]) -> String {
    format!("<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"2400\"/></w:tblGrid>{}</w:tbl>", flags.iter().map(|flag| format!(r#"<w:tr><w:trPr>{flag}</w:trPr><w:tc><w:tcPr><w:tcW w:type="dxa" w:w="2400"/></w:tcPr><w:p><w:r><w:t>cell</w:t></w:r></w:p><w:p/></w:tc></w:tr>"#)).collect::<String>())
}
fn parts(body: &str, styles: &str) -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
        ("[Content_Types].xml".into(), br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/meta/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#.to_vec()),
        ("_rels/.rels".into(), br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="office" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec()),
        ("word/_rels/document.xml.rels".into(), br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="meta/styles.xml"/></Relationships>"#.to_vec()),
        ("word/document.xml".into(), format!(r#"<w:document xmlns:w="{NS}" xmlns:q="{NS}"><w:body>{body}</w:body></w:document>"#).into_bytes()),
        ("word/meta/styles.xml".into(), format!(r#"<w:styles xmlns:w="{NS}">{styles}</w:styles>"#).into_bytes()),
    ])
}
fn write(path: &Path, parts: &BTreeMap<String, Vec<u8>>) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, bytes) in parts {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}
fn read_parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|i| {
            let mut entry = archive.by_index(i).unwrap();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            (entry.name().to_owned(), bytes)
        })
        .collect()
}
fn source(body: &str, styles: &str) -> importing::ImportReport {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.docx");
    write(&input, &parts(body, styles));
    let before = fs::read(&input).unwrap();
    let result = importing::import_docx(&input).unwrap();
    assert_eq!(before, fs::read(&input).unwrap());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    result
}
fn model(value: Option<Value>) -> Node {
    let mut node: Node = serde_json::from_value(json!({"type":"doc","attrs":{"styleProfile":"standard-v2"},"content":[{"type":"table","content":[{"type":"tableRow","content":[{"type":"tableCell","content":[{"type":"paragraph","content":[{"type":"text","text":"cell"}]}]}]}]}]})).unwrap();
    if let Some(value) = value {
        node.content[0].content[0]
            .attrs
            .insert("rowCantSplit".into(), value);
    }
    node
}
#[test]
fn direct_source_tri_state_and_existing_header_are_preserved_separately() {
    let report = source(
        &table(&[
            "<w:tblHeader/>",
            "<w:cantSplit w:val=\"0\"/>",
            "<w:cantSplit/>",
        ]),
        "",
    );
    assert!(report.compatibility.risks.contains(PRESERVED));
    let rows = &report.document.content[0].content;
    assert!(!rows[0].attrs.contains_key("rowCantSplit"));
    assert_eq!(rows[0].content[0].kind, "tableHeader");
    assert_eq!(rows[1].attrs["rowCantSplit"], false);
    assert_eq!(rows[2].attrs["rowCantSplit"], true);
    for (raw, value) in [
        ("1", true),
        ("true", true),
        ("on", true),
        ("0", false),
        ("false", false),
        ("off", false),
    ] {
        assert_eq!(
            source(&table(&[&format!(r#"<w:cantSplit w:val="{raw}"/>"#)]), "")
                .document
                .content[0]
                .content[0]
                .attrs["rowCantSplit"],
            value
        );
    }
}
#[test]
fn unsupported_direct_source_retains_complete_old_model_and_single_warning() {
    let baseline = source(&table(&["", ""]), "");
    for bad in [
        r#"<w:cantSplit w:val="TRUE"/>"#,
        "<w:cantSplit/><w:cantSplit/>",
        "<q:cantSplit/>",
        r#"<w:cantSplit w:val="1" q:val="0"/>"#,
    ] {
        let report = source(&table(&["<w:cantSplit/>", bad]), "");
        assert_eq!(json!(report.document), json!(baseline.document));
        let mut expected = baseline.compatibility.risks.clone();
        expected.insert(DECLARATION.into());
        assert_eq!(report.compatibility.risks, expected);
    }
}
#[test]
fn one_unproved_candidate_falls_back_every_candidate_table_atomically() {
    let body = table(&["<w:cantSplit/>"])
        + &table(&["<w:cantSplit w:val=\"0\"/>"]).replace(
            "<w:tblPr/>",
            r#"<w:tblPr><w:tblStyle w:val="Active"/></w:tblPr>"#,
        );
    let styles =
        r#"<w:style w:type="table" w:styleId="Active"><w:trPr><w:cantSplit/></w:trPr></w:style>"#;
    let report = source(&body, styles);
    assert!(report.compatibility.risks.contains(SOURCE));
    assert!(!report.compatibility.risks.contains(PRESERVED));
    for table in &report.document.content {
        for row in &table.content {
            assert!(!row.attrs.contains_key("rowCantSplit"));
        }
    }
}
#[test]
fn source_geometry_refusals_keep_old_risk_and_do_not_gain_new_diagnostics() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("blocked.docx");
    for flag in ["<w:cantSplit/>", "<w:cantSplit w:val=\"0\"/>"] {
        write(
            &input,
            &parts(&table(&[&format!("<w:trHeight w:val=\"1\"/>{flag}")]), ""),
        );
        let report = compatibility::audit_docx(&input).unwrap();
        assert!(
            report
                .risks
                .contains("Unsupported table geometry cannot be preserved in an editable copy")
        );
        assert!(!report.risks.iter().any(|r| r.contains("cantSplit")));
        assert!(
            importing::import_docx(&input)
                .unwrap_err()
                .to_string()
                .contains("Unsupported table geometry")
        );
    }
}
#[test]
fn raw_author_boolean_location_and_nested_ownership_are_strict() {
    for value in [
        Value::Null,
        json!(0),
        json!(1),
        json!("false"),
        json!([]),
        json!({}),
    ] {
        assert!(document::validate(&model(Some(value))).is_err());
    }
    for location in 0..4 {
        let mut doc = model(None);
        let node = match location {
            0 => &mut doc,
            1 => &mut doc.content[0],
            2 => &mut doc.content[0].content[0].content[0],
            _ => &mut doc.content[0].content[0].content[0].content[0],
        };
        node.attrs.insert("rowCantSplit".into(), false.into());
        assert!(document::validate(&doc).is_err());
    }
    let mut nested = model(Some(json!(false)));
    nested.content[0].content[0].content[0]
        .content
        .push(model(None).content.remove(0));
    assert!(document::validate(&nested).is_err());
    let mut nested = model(None);
    nested.content[0].content[0].content[0]
        .content
        .push(model(Some(json!(true))).content.remove(0));
    assert!(document::validate(&nested).is_err());
}
#[test]
fn author_flag_height_and_header_are_independent_through_draft_export_and_trusted_reopen() {
    let dir = tempfile::tempdir().unwrap();
    for (i, value) in [None, Some(false), Some(true)].into_iter().enumerate() {
        for header in [false, true] {
            for minimum in [None, Some(0), Some(720)] {
                let mut doc = model(value.map(Value::Bool));
                let row = &mut doc.content[0].content[0];
                if header {
                    row.content[0].kind = "tableHeader".into();
                }
                if let Some(height) = minimum {
                    row.attrs.insert("rowMinHeightTwips".into(), height.into());
                }
                document::validate(&doc).unwrap();
                let draft = dir.path().join("draft.json");
                document::save_draft(&doc, &draft).unwrap();
                assert_eq!(
                    json!(document::load_draft(&draft).unwrap()),
                    json!(Some(doc.clone()))
                );
                let output = dir
                    .path()
                    .join(format!("row-{i}-{header}-{minimum:?}.docx"));
                document::export_docx(&doc, &output).unwrap();
                assert_eq!(
                    json!(document::load_authored_docx(&output).unwrap()),
                    json!(Some(doc.clone()))
                );
                let bytes = fs::read(&output).unwrap();
                let parts = read_parts(&bytes);
                let xml = String::from_utf8(parts["word/document.xml"].clone()).unwrap();
                assert_eq!(xml.contains("<w:cantSplit"), value.is_some());
                if let Some(value) = value {
                    assert!(xml.contains(if value {
                        r#"<w:cantSplit w:val="1"/>"#
                    } else {
                        r#"<w:cantSplit w:val="0"/>"#
                    }));
                }
                assert_eq!(xml.contains("<w:tblHeader/>"), header);
                assert_eq!(xml.contains("<w:trHeight"), minimum.is_some());
                assert!(document::export_docx(&doc, &output).is_err());
                assert_eq!(fs::read(&output).unwrap(), bytes);
            }
        }
    }
}
#[test]
fn modified_flag_cannot_be_laundered_by_repairing_part_hashes() {
    use sha2::{Digest, Sha256};
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("original.docx");
    document::export_docx(&model(Some(json!(true))), &output).unwrap();
    let mut parts = read_parts(&fs::read(&output).unwrap());
    parts.insert(
        "word/document.xml".into(),
        String::from_utf8(parts["word/document.xml"].clone())
            .unwrap()
            .replace(r#"<w:cantSplit w:val="1"/>"#, r#"<w:cantSplit w:val="0"/>"#)
            .into_bytes(),
    );
    let tampered = dir.path().join("tampered.docx");
    write(&tampered, &parts);
    assert!(document::load_authored_docx(&tampered).is_err());
    let model_name = parts
        .keys()
        .find(|name| name.ends_with(".json"))
        .unwrap()
        .clone();
    let mut metadata: Value = serde_json::from_slice(&parts[&model_name]).unwrap();
    metadata["hashes"]["word/document.xml"] =
        json!(format!("{:x}", Sha256::digest(&parts["word/document.xml"])));
    parts.insert(model_name, serde_json::to_vec(&metadata).unwrap());
    write(&tampered, &parts);
    assert!(document::load_authored_docx(&tampered).is_err());
}
