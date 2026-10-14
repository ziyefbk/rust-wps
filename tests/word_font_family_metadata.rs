use rust_wps::document::{self, Node};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::Path,
};

fn model(metadata: Option<Value>, modern: bool) -> Node {
    let mut value = json!({"type":"doc","content":[{"type":"paragraph","content":[
        {"type":"text","text":"Same literal text","marks":[{"type":"fontFamily","attrs":{"name":"Arial Black"}}]}
    ]}]});
    if modern {
        value["attrs"]["styleProfile"] = json!("standard-v2");
    }
    if let Some(metadata) = metadata {
        value["attrs"]["fontFamilyMetadata"] = metadata;
    }
    serde_json::from_value(value).unwrap()
}
fn metadata() -> Value {
    json!([{"name":"Z Unused","family":"roman"},{"name":"Arial Black","family":"swiss"}])
}
fn parts(path: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    (0..archive.len())
        .map(|index| {
            let mut entry = archive.by_index(index).unwrap();
            let mut data = Vec::new();
            entry.read_to_end(&mut data).unwrap();
            (entry.name().into(), data)
        })
        .collect()
}
fn write(path: &Path, parts: &BTreeMap<String, Vec<u8>>) {
    let mut archive = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, data) in parts {
        archive
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(data).unwrap();
    }
    archive.finish().unwrap();
}
fn refresh_hashes(parts: &mut BTreeMap<String, Vec<u8>>, model_part: &str) {
    let mut embedded: Value = serde_json::from_slice(&parts[model_part]).unwrap();
    embedded["hashes"] = serde_json::to_value(
        parts
            .iter()
            .filter(|(name, _)| name.as_str() != model_part)
            .map(|(name, data)| (name, format!("{:x}", Sha256::digest(data))))
            .collect::<BTreeMap<_, _>>(),
    )
    .unwrap();
    parts.insert(model_part.into(), serde_json::to_vec(&embedded).unwrap());
}

#[test]
fn authored_metadata_is_independent_preserved_state_with_minimal_conditional_opc() {
    let directory = tempfile::tempdir().unwrap();
    for modern in [false, true] {
        let plain = model(None, modern);
        let doc = model(Some(metadata()), modern);
        let old = directory.path().join(format!("old-{modern}.docx"));
        let new = directory.path().join(format!("new-{modern}.docx"));
        document::export_docx(&plain, &old).unwrap();
        document::export_docx(&doc, &new).unwrap();
        let prior = parts(&old);
        let current = parts(&new);
        assert!(!prior.contains_key("word/fontTable.xml"));
        assert!(!current.contains_key("word/settings.xml"));
        assert_eq!(current.len(), prior.len() + 1);
        for name in [
            "word/document.xml",
            "word/styles.xml",
            "word/numbering.xml",
            "_rels/.rels",
        ] {
            assert_eq!(current[name], prior[name], "{name}");
        }
        let table = String::from_utf8(current["word/fontTable.xml"].clone()).unwrap();
        assert_eq!(
            table,
            r#"<w:fonts xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:font w:name="Arial Black"><w:family w:val="swiss"/></w:font><w:font w:name="Z Unused"><w:family w:val="roman"/></w:font></w:fonts>"#
        );
        let rel = String::from_utf8(current["word/_rels/document.xml.rels"].clone()).unwrap();
        assert!(rel.contains(r#"Id="fontTable" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable" Target="fontTable.xml""#));
        let types = String::from_utf8(current["[Content_Types].xml"].clone()).unwrap();
        assert!(types.contains(r#"PartName="/word/fontTable.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml""#));
        assert_eq!(
            serde_json::to_value(document::load_authored_docx(&new).unwrap().unwrap()).unwrap(),
            serde_json::to_value(&doc).unwrap()
        );
        let draft = directory.path().join(format!("draft-{modern}.json"));
        document::save_draft(&doc, &draft).unwrap();
        assert_eq!(
            serde_json::to_value(document::load_draft(&draft).unwrap().unwrap()).unwrap(),
            serde_json::to_value(&doc).unwrap()
        );
        let mut dormant = doc.clone();
        dormant.content[0].content.clear();
        let empty = directory.path().join(format!("unused-{modern}.docx"));
        document::export_docx(&dormant, &empty).unwrap();
        assert_eq!(
            parts(&empty)["word/fontTable.xml"],
            current["word/fontTable.xml"]
        );
        let bytes = fs::read(&new).unwrap();
        assert!(document::export_docx(&doc, &new).is_err());
        assert_eq!(fs::read(new).unwrap(), bytes);
    }
}

#[test]
fn literal_font_metadata_bounds_sorting_and_escaping_are_not_name_inference() {
    let directory = tempfile::tempdir().unwrap();
    let records = json!([
        {"name":"z","family":"swiss"},
        {"name":"A & B <face> \"x\"","family":"roman"},
        {"name":"界".repeat(31),"family":"swiss"},
        {"name":"😀".repeat(15),"family":"roman"}
    ]);
    let doc = model(Some(records.clone()), false);
    let first = directory.path().join("first.docx");
    document::export_docx(&doc, &first).unwrap();
    let mut reversed = records.as_array().unwrap().clone();
    reversed.reverse();
    let second = directory.path().join("second.docx");
    document::export_docx(&model(Some(json!(reversed)), false), &second).unwrap();
    assert_eq!(
        parts(&first)["word/fontTable.xml"],
        parts(&second)["word/fontTable.xml"]
    );
    let table = String::from_utf8(parts(&first)["word/fontTable.xml"].clone()).unwrap();
    assert!(table.contains("A &amp; B &lt;face&gt; &quot;x&quot;"));
    let records: Vec<_> = (0..256)
        .map(|n| json!({"name":format!("Face{n}"),"family":"roman"}))
        .collect();
    document::validate(&model(Some(json!(records)), true)).unwrap();
    // No Unicode case folding/normalization is claimed or performed.
    document::validate(&model(
        Some(json!([{"name":"É","family":"roman"},{"name":"é","family":"swiss"}])),
        false,
    ))
    .unwrap();
}

#[test]
fn metadata_strictly_rejects_invalid_records_duplicates_aliases_and_placement() {
    let mut invalid = vec![
        Value::Null,
        json!([]),
        json!({}),
        json!(true),
        json!("swiss"),
        json!([null]),
        json!([{}]),
        json!([{"name":"Arial"}]),
        json!([{"family":"swiss"}]),
        json!([{"name":"Arial","family":"swiss","extra":0}]),
        json!([{"name":"Arial","family":"swiss"},{"name":"Arial","family":"swiss"}]),
        json!([{"name":"Arial","family":"roman"},{"name":"ARIAL","family":"swiss"}]),
        json!([{"name":"Arial","family":"swiss"},{"name":"arial","family":"swiss"}]),
    ];
    for family in [
        json!("auto"),
        json!("modern"),
        json!("script"),
        json!("decorative"),
        json!("Swiss"),
        json!(""),
        Value::Null,
        json!(2),
    ] {
        invalid.push(json!([{"name":"Arial","family":family}]));
    }
    for name in [
        json!(""),
        json!(" Arial"),
        json!("Arial "),
        json!("A\nB"),
        json!("A\u{feff}"),
        json!("x".repeat(32)),
        json!("😀".repeat(16)),
        Value::Null,
        json!(4),
    ] {
        invalid.push(json!([{"name":name,"family":"swiss"}]));
    }
    invalid.push(json!(
        (0..257)
            .map(|n| json!({"name":format!("Face{n}"),"family":"roman"}))
            .collect::<Vec<_>>()
    ));
    for records in invalid {
        assert!(
            document::validate(&model(Some(records.clone()), false)).is_err(),
            "{records}"
        );
    }
    let mut nested = model(None, false);
    nested.content[0]
        .attrs
        .insert("fontFamilyMetadata".into(), metadata());
    assert!(document::validate(&nested).is_err());
    let mut mark = model(None, false);
    mark.content[0].content[0].marks[0]
        .attrs
        .insert("fontFamilyMetadata".into(), metadata());
    assert!(document::validate(&mark).is_err());
}

#[test]
fn invalid_metadata_cannot_overwrite_draft_or_publish_any_output() {
    let directory = tempfile::tempdir().unwrap();
    let draft = directory.path().join("draft.json");
    let mut doc = model(Some(metadata()), true);
    document::save_draft(&doc, &draft).unwrap();
    let before = fs::read(&draft).unwrap();
    doc.attrs["fontFamilyMetadata"][0]["family"] = json!("auto");
    assert!(document::save_draft(&doc, &draft).is_err());
    assert_eq!(fs::read(draft).unwrap(), before);
    let output = directory.path().join("bad.docx");
    assert!(document::export_docx(&doc, &output).is_err());
    assert!(!output.exists());
}

#[test]
fn trusted_reopen_rejects_model_font_table_relationship_type_and_hash_forgery() {
    let directory = tempfile::tempdir().unwrap();
    for modern in [false, true] {
        let output = directory.path().join(format!("original-{modern}.docx"));
        document::export_docx(&model(Some(metadata()), modern), &output).unwrap();
        let source = parts(&output);
        let mp = if modern {
            "rust-wps/model-v2.json"
        } else {
            "rust-wps/model-v1.json"
        };
        for change in 0..12 {
            let mut archive = source.clone();
            if change < 4 {
                let mut embedded: Value = serde_json::from_slice(&archive[mp]).unwrap();
                match change {
                    0 => {
                        embedded["document"]["attrs"]
                            .as_object_mut()
                            .unwrap()
                            .remove("fontFamilyMetadata");
                    }
                    1 => {
                        embedded["document"]["attrs"]["fontFamilyMetadata"][0]["family"] =
                            json!("swiss")
                    }
                    2 => {
                        embedded["document"]["attrs"]["fontFamilyMetadata"][0]["name"] =
                            json!("Different")
                    }
                    _ => {
                        embedded["document"]["attrs"]["fontFamilyMetadata"][0]["extra"] =
                            json!(false)
                    }
                }
                archive.insert(mp.into(), serde_json::to_vec(&embedded).unwrap());
            } else {
                match change {
                    4 => {
                        archive.remove("word/fontTable.xml");
                    }
                    5 => {
                        archive.insert("word/extra.xml".into(), b"<extra/>".to_vec());
                    }
                    6 | 7 => {
                        let xml = String::from_utf8(archive["word/fontTable.xml"].clone()).unwrap();
                        let xml = if change == 6 {
                            xml.replace("swiss", "roman")
                        } else {
                            xml.replace("Arial Black", "Different")
                        };
                        archive.insert("word/fontTable.xml".into(), xml.into_bytes());
                    }
                    8 | 9 => {
                        let xml =
                            String::from_utf8(archive["word/_rels/document.xml.rels"].clone())
                                .unwrap();
                        let xml = if change == 8 {
                            xml.replace("Target=\"fontTable.xml\"", "Target=\"other.xml\"")
                        } else {
                            xml.replace("relationships/fontTable", "relationships/notFontTable")
                        };
                        archive.insert("word/_rels/document.xml.rels".into(), xml.into_bytes());
                    }
                    10 => {
                        let xml = String::from_utf8(archive["[Content_Types].xml"].clone())
                            .unwrap()
                            .replace(
                                "wordprocessingml.fontTable+xml",
                                "wordprocessingml.styles+xml",
                            );
                        archive.insert("[Content_Types].xml".into(), xml.into_bytes());
                    }
                    _ => {
                        archive.insert("word/fontTable.xml".into(), b"invalid XML".to_vec());
                    }
                }
                refresh_hashes(&mut archive, mp);
            }
            let forged = directory
                .path()
                .join(format!("forged-{modern}-{change}.docx"));
            write(&forged, &archive);
            let before = fs::read(&forged).unwrap();
            assert!(
                document::load_authored_docx(&forged).is_err(),
                "{modern}/{change}"
            );
            assert_eq!(fs::read(forged).unwrap(), before);
        }
    }
}
