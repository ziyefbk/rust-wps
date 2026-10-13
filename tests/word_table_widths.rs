use rust_wps::document::{self, Node};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read, Write},
};

fn model(widths: Option<Value>, modern: bool) -> Node {
    let cell = |text| json!({"type":"tableCell","content":[{"type":"paragraph","content":[{"type":"text","text":text}]}]});
    let mut value = json!({"type":"doc","content":[{"type":"table","content":[
        {"type":"tableRow","content":[cell("A"),cell("B"),cell("C")]},
        {"type":"tableRow","content":[cell("D"),cell("E"),cell("F")]}
    ]}]});
    if let Some(widths) = widths {
        value["content"][0]["attrs"] = json!({"columnWidthsTwips": widths});
    }
    if modern {
        value["attrs"] = json!({"styleProfile":"standard-v2"});
    }
    serde_json::from_value(value).unwrap()
}
fn parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..zip.len())
        .map(|index| {
            let mut entry = zip.by_index(index).unwrap();
            let mut data = Vec::new();
            entry.read_to_end(&mut data).unwrap();
            (entry.name().to_owned(), data)
        })
        .collect()
}
fn write_parts(path: &std::path::Path, parts: &BTreeMap<String, Vec<u8>>) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, data) in parts {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn strict_table_width_units_cardinality_and_total_limits() {
    for widths in [
        json!([1, 1, 31678]),
        json!([1, 1, 1]),
        json!([1800, 3600, 2700]),
    ] {
        document::validate(&model(Some(widths), true)).unwrap();
    }
    for widths in [
        Value::Null,
        json!("1800"),
        json!([]),
        json!([1800]),
        json!([1, 2, 3, 4]),
        json!([0, 1, 1]),
        json!([-1, 1, 1]),
        json!([1.5, 1, 1]),
        json!([1.0, 1, 1]),
        json!(["1", 1, 1]),
        json!([true, 1, 1]),
        json!([null, 1, 1]),
        json!([31681, 1, 1]),
        json!([31680, 1, 1]),
        json!([u64::MAX, 1, 1]),
    ] {
        assert!(
            document::validate(&model(Some(widths.clone()), true)).is_err(),
            "{widths}"
        );
    }
    let mut wrong_scope = model(None, true);
    wrong_scope.content[0].content[0]
        .attrs
        .insert("columnWidthsTwips".into(), json!([1, 1, 1]));
    assert!(document::validate(&wrong_scope).is_err());
}

#[test]
fn fixed_widths_refuse_merges_nested_tables_cell_widths_and_ragged_rows() {
    let original = model(Some(json!([1800, 3600, 2700])), true);
    for (key, value) in [
        ("colspan", json!(2)),
        ("rowspan", json!(2)),
        ("colwidth", json!([120])),
        ("colwidth", json!([1800])),
    ] {
        let mut doc = original.clone();
        doc.content[0].content[0].content[0]
            .attrs
            .insert(key.into(), value);
        assert!(document::validate(&doc).is_err());
    }
    let mut ragged = original.clone();
    ragged.content[0].content[1].content.pop();
    assert!(document::validate(&ragged).is_err());
    for (outer_fixed, inner_fixed) in [(true, false), (false, true), (true, true)] {
        let mut outer = model(outer_fixed.then(|| json!([1800, 3600, 2700])), true);
        let inner = model(inner_fixed.then(|| json!([100, 100, 100])), true)
            .content
            .remove(0);
        outer.content[0].content[0].content[0].content.push(inner);
        assert!(document::validate(&outer).is_err());
    }
    // Existing unconfigured nested-table models retain their historical scope.
    let mut legacy = model(None, false);
    legacy.content[0].content[0].content[0]
        .content
        .push(model(None, false).content.remove(0));
    document::validate(&legacy).unwrap();
}

#[test]
fn fixed_width_xml_draft_and_trusted_reopen_match_exact_model() {
    let dir = tempfile::tempdir().unwrap();
    for modern in [false, true] {
        let doc = model(Some(json!([1801, 3599, 2700])), modern);
        let draft = dir.path().join(format!("{modern}.json"));
        document::save_draft(&doc, &draft).unwrap();
        let expected = serde_json::to_value(&doc).unwrap();
        assert_eq!(
            serde_json::to_value(document::load_draft(&draft).unwrap().unwrap()).unwrap(),
            expected
        );
        let output = dir.path().join(format!("{modern}.docx"));
        document::export_docx(&doc, &output).unwrap();
        assert_eq!(
            serde_json::to_value(document::load_authored_docx(&output).unwrap().unwrap()).unwrap(),
            expected
        );
        let bytes = fs::read(&output).unwrap();
        let archive = parts(&bytes);
        let xml = String::from_utf8(archive["word/document.xml"].clone()).unwrap();
        assert!(xml.contains(r#"<w:tblW w:w="8100" w:type="dxa"/>"#));
        assert!(xml.contains(r#"<w:tblLayout w:type="fixed"/>"#));
        assert!(xml.contains(r#"<w:tblGrid><w:gridCol w:w="1801"/><w:gridCol w:w="3599"/><w:gridCol w:w="2700"/></w:tblGrid>"#));
        for width in [1801, 3599, 2700] {
            assert_eq!(
                xml.matches(&format!(r#"<w:tcW w:w="{width}" w:type="dxa"/>"#))
                    .count(),
                2
            );
        }
        assert!(document::export_docx(&doc, &output).is_err());
        assert_eq!(fs::read(&output).unwrap(), bytes);
    }
}

#[test]
fn malformed_width_mutations_never_replace_drafts_or_publish_files() {
    let dir = tempfile::tempdir().unwrap();
    let good = model(Some(json!([1800, 3600, 2700])), true);
    let draft = dir.path().join("draft.json");
    document::save_draft(&good, &draft).unwrap();
    let before = fs::read(&draft).unwrap();
    for (index, widths) in [
        json!([1, 2]),
        json!([0, 1, 2]),
        json!([31680, 1, 1]),
        Value::Null,
    ]
    .into_iter()
    .enumerate()
    {
        let bad = model(Some(widths), true);
        let output = dir.path().join(format!("bad-{index}.docx"));
        assert!(document::save_draft(&bad, &draft).is_err());
        assert_eq!(fs::read(&draft).unwrap(), before);
        assert!(document::export_docx(&bad, &output).is_err());
        assert!(!output.exists());
    }
}

#[test]
fn tampered_model_or_geometry_cannot_be_trusted_reopened() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("original.docx");
    document::export_docx(&model(Some(json!([1800, 3600, 2700])), true), &path).unwrap();
    let original = fs::read(&path).unwrap();
    let archive = parts(&original);
    for geometry in [false, true] {
        let mut modified = archive.clone();
        if geometry {
            modified.insert(
                "word/document.xml".into(),
                String::from_utf8(modified["word/document.xml"].clone())
                    .unwrap()
                    .replacen("w:type=\"fixed\"", "w:type=\"autofit\"", 1)
                    .into_bytes(),
            );
            // Even synchronizing the advertised hash cannot bypass re-encoding.
            use sha2::{Digest, Sha256};
            let mut embedded: Value =
                serde_json::from_slice(&modified["rust-wps/model-v2.json"]).unwrap();
            embedded["hashes"]["word/document.xml"] = json!(format!(
                "{:x}",
                Sha256::digest(&modified["word/document.xml"])
            ));
            modified.insert(
                "rust-wps/model-v2.json".into(),
                serde_json::to_vec(&embedded).unwrap(),
            );
        } else {
            let mut embedded: Value =
                serde_json::from_slice(&modified["rust-wps/model-v2.json"]).unwrap();
            embedded["document"]["content"][0]["attrs"]["columnWidthsTwips"] =
                json!([2700, 3600, 1800]);
            modified.insert(
                "rust-wps/model-v2.json".into(),
                serde_json::to_vec(&embedded).unwrap(),
            );
        }
        let corrupt = dir.path().join(format!("corrupt-{geometry}.docx"));
        write_parts(&corrupt, &modified);
        assert!(document::load_authored_docx(&corrupt).is_err());
    }
    assert_eq!(fs::read(path).unwrap(), original);
}

#[test]
fn width_absence_retains_historical_automatic_table_encoding() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("automatic.docx");
    document::export_docx(&model(None, false), &path).unwrap();
    let archive = parts(&fs::read(path).unwrap());
    let xml = String::from_utf8(archive["word/document.xml"].clone()).unwrap();
    assert!(!xml.contains("tblLayout"));
    assert!(xml.contains(r#"<w:tblW w:w="9000" w:type="dxa"/>"#));
    assert_eq!(xml.matches(r#"<w:gridCol w:w="3000"/>"#).count(), 3);
}
