use rust_wps::document::{self, Node};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::Path,
};

fn metrics() -> Value {
    json!({"font":{"family":"Calibri","halfPoints":22},"spaceBefore":0,"spaceAfter":200,"lineSpacing":276})
}
fn model(split: bool, metric: bool, modern: bool) -> Node {
    let mut n = json!({"type":"doc","content":[{"type":"pageBreak"}]});
    if modern {
        n["attrs"]["styleProfile"] = json!("standard-v2");
    }
    if split {
        n["attrs"]["splitPageBreakAndParagraphMark"] = json!(true);
    }
    if metric {
        n["content"][0]["attrs"]["breakParagraph"] = metrics();
    }
    serde_json::from_value(n).unwrap()
}
fn parts(path: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut zip = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    (0..zip.len())
        .map(|index| {
            let mut part = zip.by_index(index).unwrap();
            let mut bytes = Vec::new();
            part.read_to_end(&mut bytes).unwrap();
            (part.name().to_owned(), bytes)
        })
        .collect()
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
fn update_hashes(parts: &mut BTreeMap<String, Vec<u8>>, model_part: &str) {
    let mut embedded: Value = serde_json::from_slice(&parts[model_part]).unwrap();
    embedded["hashes"] = serde_json::to_value(
        parts
            .iter()
            .filter(|(name, _)| name.as_str() != model_part)
            .map(|(name, bytes)| (name, format!("{:x}", Sha256::digest(bytes))))
            .collect::<BTreeMap<_, _>>(),
    )
    .unwrap();
    parts.insert(model_part.into(), serde_json::to_vec(&embedded).unwrap());
}
#[test]
fn independent_authored_split_and_metric_states_round_trip_both_profiles() {
    let dir = tempfile::tempdir().unwrap();
    for modern in [false, true] {
        for split in [false, true] {
            for metric in [false, true] {
                let doc = model(split, metric, modern);
                let path = dir.path().join(format!("{modern}-{split}-{metric}.docx"));
                document::export_docx(&doc, &path).unwrap();
                let archive = parts(&path);
                let xml = String::from_utf8(archive["word/document.xml"].clone()).unwrap();
                assert_eq!(xml.matches("<w:p>").count(), 1);
                assert_eq!(xml.matches("<w:br w:type=\"page\"/>").count(), 1);
                assert!(!xml.contains("<w:t"));
                assert!(!xml.contains("pageBreakBefore"));
                if metric {
                    let rp = "<w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\"/><w:sz w:val=\"22\"/></w:rPr>";
                    assert!(xml.contains(&format!("<w:pPr><w:spacing w:before=\"0\" w:after=\"200\" w:line=\"276\" w:lineRule=\"auto\"/>{rp}</w:pPr><w:r>{rp}<w:br w:type=\"page\"/></w:r>")));
                } else {
                    assert!(xml.contains("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>"));
                }
                assert_eq!(archive.contains_key("word/settings.xml"), split || metric);
                if split || metric {
                    let settings = String::from_utf8(archive["word/settings.xml"].clone()).unwrap();
                    assert_eq!(settings.contains("<w:splitPgBreakAndParaMark/>"), split);
                    assert!(!settings.contains("compatibilityMode"));
                    assert!(
                        String::from_utf8(archive["[Content_Types].xml"].clone())
                            .unwrap()
                            .contains("/word/settings.xml")
                    );
                    assert!(
                        String::from_utf8(archive["word/_rels/document.xml.rels"].clone())
                            .unwrap()
                            .contains("Target=\"settings.xml\"")
                    );
                }
                assert_eq!(
                    serde_json::to_value(document::load_authored_docx(&path).unwrap().unwrap())
                        .unwrap(),
                    serde_json::to_value(&doc).unwrap()
                );
                let draft = path.with_extension("json");
                document::save_draft(&doc, &draft).unwrap();
                assert_eq!(
                    serde_json::to_value(document::load_draft(&draft).unwrap().unwrap()).unwrap(),
                    serde_json::to_value(&doc).unwrap()
                );
                let bytes = fs::read(&path).unwrap();
                assert!(document::export_docx(&doc, &path).is_err());
                assert_eq!(fs::read(path).unwrap(), bytes);
            }
        }
    }
}
#[test]
fn authored_mixed_and_dormant_split_state_is_explicit_and_valid() {
    let dir = tempfile::tempdir().unwrap();
    for split in [false, true] {
        let mut doc = model(split, true, true);
        doc.content
            .push(model(false, false, false).content.remove(0));
        doc.content
            .push(serde_json::from_value(json!({"type":"paragraph"})).unwrap());
        let path = dir.path().join(format!("mixed-{split}.docx"));
        document::export_docx(&doc, &path).unwrap();
        assert_eq!(
            serde_json::to_value(document::load_authored_docx(&path).unwrap().unwrap()).unwrap(),
            serde_json::to_value(&doc).unwrap()
        );
        doc.content.drain(..2);
        let path = dir.path().join(format!("dormant-{split}.docx"));
        document::export_docx(&doc, &path).unwrap();
        assert_eq!(parts(&path).contains_key("word/settings.xml"), split);
    }
}
#[test]
fn break_metric_bounds_and_font_xml_are_literal() {
    let dir = tempfile::tempdir().unwrap();
    for (index, (before, after, line, size)) in
        [(0, 31680, 120, 2), (31680, 0, 2400, 3276), (1, 1, 241, 23)]
            .into_iter()
            .enumerate()
    {
        let mut doc = model(true, true, true);
        doc.content[0].attrs["breakParagraph"] = json!({"font":{"family":"A & B <face>","halfPoints":size},"spaceBefore":before,"spaceAfter":after,"lineSpacing":line});
        let path = dir.path().join(format!("boundary-{index}.docx"));
        document::export_docx(&doc, &path).unwrap();
        let xml = String::from_utf8(parts(&path)["word/document.xml"].clone()).unwrap();
        assert!(xml.contains("A &amp; B &lt;face&gt;"));
        assert_eq!(
            serde_json::to_value(document::load_authored_docx(&path).unwrap().unwrap()).unwrap(),
            serde_json::to_value(doc).unwrap()
        );
    }
}
#[test]
fn split_root_and_complete_metric_records_reject_invalid_ingress() {
    for bad in [
        Value::Null,
        json!(false),
        json!(1),
        json!("true"),
        json!({}),
    ] {
        let mut doc = model(false, false, true);
        doc.attrs
            .insert("splitPageBreakAndParagraphMark".into(), bad);
        assert!(document::validate(&doc).is_err());
    }
    let mut invalid = vec![Value::Null, json!({}), json!(true), json!([])];
    for key in ["font", "spaceBefore", "spaceAfter", "lineSpacing"] {
        let mut v = metrics();
        v.as_object_mut().unwrap().remove(key);
        invalid.push(v);
    }
    for (path, values) in [
        (
            "spaceBefore",
            vec![json!(-1), json!(31681), json!(1.5), json!("0"), Value::Null],
        ),
        (
            "spaceAfter",
            vec![
                json!(-1),
                json!(31681),
                json!(1.5),
                json!(true),
                Value::Null,
            ],
        ),
        (
            "lineSpacing",
            vec![json!(0), json!(119), json!(2401), json!(240.5), Value::Null],
        ),
    ] {
        for value in values {
            let mut v = metrics();
            v[path] = value;
            invalid.push(v);
        }
    }
    for (key, values) in [
        (
            "family",
            vec![
                json!(""),
                json!(" Arial"),
                json!("a\n"),
                json!("x".repeat(32)),
                Value::Null,
            ],
        ),
        (
            "halfPoints",
            vec![json!(1), json!(3277), json!(22.5), json!("22"), Value::Null],
        ),
    ] {
        for value in values {
            let mut v = metrics();
            v["font"][key] = value;
            invalid.push(v);
        }
    }
    for key in ["family", "halfPoints"] {
        let mut v = metrics();
        v["font"].as_object_mut().unwrap().remove(key);
        invalid.push(v);
    }
    for nested in [false, true] {
        let mut v = metrics();
        if nested {
            v["font"]["bold"] = json!(false);
        } else {
            v["extra"] = json!(0);
        }
        invalid.push(v);
    }
    for value in invalid {
        let mut doc = model(true, true, true);
        doc.content[0]
            .attrs
            .insert("breakParagraph".into(), value.clone());
        assert!(document::validate(&doc).is_err(), "{value}");
    }
    for kind in ["paragraph", "heading", "hardBreak", "text"] {
        let mut doc = model(true, true, true);
        doc.content[0].kind = kind.into();
        assert!(document::validate(&doc).is_err());
    }
}
#[test]
fn invalid_break_metadata_cannot_replace_draft_or_publish_output() {
    let dir = tempfile::tempdir().unwrap();
    let draft = dir.path().join("draft.json");
    let mut doc = model(true, true, true);
    document::save_draft(&doc, &draft).unwrap();
    let before = fs::read(&draft).unwrap();
    doc.content[0].attrs["breakParagraph"]["lineSpacing"] = json!(0);
    assert!(document::save_draft(&doc, &draft).is_err());
    assert_eq!(fs::read(&draft).unwrap(), before);
    let output = dir.path().join("bad.docx");
    assert!(document::export_docx(&doc, &output).is_err());
    assert!(!output.exists());
}
#[test]
fn complete_trusted_reencoding_rejects_model_settings_metric_and_part_set_forgery() {
    let dir = tempfile::tempdir().unwrap();
    for modern in [false, true] {
        let path = dir.path().join(format!("original-{modern}.docx"));
        document::export_docx(&model(true, true, modern), &path).unwrap();
        let original = parts(&path);
        let mp = if modern {
            "rust-wps/model-v2.json"
        } else {
            "rust-wps/model-v1.json"
        };
        for change in 0..10 {
            let mut archive = original.clone();
            if change < 4 {
                let mut embedded: Value = serde_json::from_slice(&archive[mp]).unwrap();
                match change {
                    0 => {
                        embedded["document"]["attrs"]
                            .as_object_mut()
                            .unwrap()
                            .remove("splitPageBreakAndParagraphMark");
                    }
                    1 => {
                        embedded["document"]["content"][0]["attrs"]["breakParagraph"]["font"]["family"] =
                            json!("Arial")
                    }
                    2 => {
                        embedded["document"]["content"][0]["attrs"]["breakParagraph"]["font"]["halfPoints"] =
                            json!(24)
                    }
                    _ => {
                        embedded["document"]["content"][0]["attrs"]["breakParagraph"]["spaceAfter"] =
                            json!(201)
                    }
                }
                archive.insert(mp.into(), serde_json::to_vec(&embedded).unwrap());
            } else {
                match change {
                    4 => {
                        archive.insert("word/settings.xml".into(),br#"<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#.to_vec());
                    }
                    5 => {
                        archive.remove("word/settings.xml");
                    }
                    6 => {
                        archive.insert("word/extra.xml".into(), b"<extra/>".to_vec());
                    }
                    n => {
                        let xml = String::from_utf8(archive["word/document.xml"].clone()).unwrap();
                        let xml = match n {
                            7 => xml.replace("w:line=\"276\"", "w:line=\"277\""),
                            8 => xml.replace("w:ascii=\"Calibri\"", "w:ascii=\"Arial\""),
                            _ => xml.replacen("w:val=\"22\"", "w:val=\"24\"", 1),
                        };
                        archive.insert("word/document.xml".into(), xml.into_bytes());
                    }
                }
                // Even accurate hashes cannot grant a content/model mismatch trust.
                update_hashes(&mut archive, mp);
            }
            let bad = dir.path().join(format!("forged-{modern}-{change}.docx"));
            write(&bad, &archive);
            let before = fs::read(&bad).unwrap();
            assert!(
                document::load_authored_docx(&bad).is_err(),
                "{modern}/{change}"
            );
            assert_eq!(fs::read(&bad).unwrap(), before);
        }
    }
}
