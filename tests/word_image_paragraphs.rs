use rust_wps::{
    document::{self, Node},
    importing,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::Path,
};

fn paragraph(fonts: bool) -> Value {
    let mut value =
        json!({"spaceBefore":0,"spaceAfter":240,"lineSpacing":276,"textAlign":"center"});
    if fonts {
        value["paragraphFont"] =
            json!({"family":"DejaVu Sans","halfPoints":24,"bold":false,"italic":false});
        value["runFont"] =
            json!({"family":"Liberation Serif","halfPoints":32,"bold":true,"italic":true});
    }
    value
}
fn model(modern: bool, dimensions: bool, fonts: bool) -> Node {
    let mut model: Node =
        serde_json::from_str(include_str!("fixtures/word-image-legacy.json")).unwrap();
    if modern {
        model
            .attrs
            .insert("styleProfile".into(), json!("standard-v2"));
    }
    for image in &mut model.content {
        image
            .attrs
            .insert("imageParagraph".into(), paragraph(fonts));
        if dimensions {
            image.attrs.insert("displayWidthEmu".into(), json!(95251));
            image.attrs.insert("displayHeightEmu".into(), json!(38107));
        }
    }
    model
}
fn value(model: &Node) -> Value {
    serde_json::to_value(model).unwrap()
}
fn parts(path: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut zip = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    (0..zip.len())
        .map(|index| {
            let mut item = zip.by_index(index).unwrap();
            let mut bytes = Vec::new();
            item.read_to_end(&mut bytes).unwrap();
            (item.name().to_owned(), bytes)
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
#[test]
fn image_paragraph_complete_metadata_is_valid_with_independent_size_and_font_options() {
    for modern in [false, true] {
        for dimensions in [false, true] {
            for fonts in [false, true] {
                document::validate(&model(modern, dimensions, fonts)).unwrap();
            }
        }
    }
    for align in ["left", "center", "right", "justify"] {
        for (before, after, line) in [(0, 0, 120), (31680, 31680, 2400), (1, 3, 241)] {
            let mut m = model(true, true, true);
            m.content[0].attrs["imageParagraph"] = json!({"spaceBefore":before,"spaceAfter":after,"lineSpacing":line,"textAlign":align});
            document::validate(&m).unwrap();
        }
    }
}
#[test]
fn image_paragraph_raw_null_partial_unknown_and_wrong_types_are_never_defaults() {
    let mut bad = vec![Value::Null, json!([]), json!("paragraph"), json!({})];
    for key in ["spaceBefore", "spaceAfter", "lineSpacing", "textAlign"] {
        let mut p = paragraph(false);
        p.as_object_mut().unwrap().remove(key);
        bad.push(p);
        let mut p = paragraph(false);
        p[key] = Value::Null;
        bad.push(p);
    }
    for (key, values) in [
        (
            "spaceBefore",
            vec![
                json!(-1),
                json!(31681),
                json!(0.5),
                json!("0"),
                json!(false),
            ],
        ),
        (
            "spaceAfter",
            vec![
                json!(-1),
                json!(31681),
                json!(0.5),
                json!("0"),
                json!(false),
            ],
        ),
        (
            "lineSpacing",
            vec![json!(0), json!(119), json!(2401), json!(1.5), json!("240")],
        ),
        (
            "textAlign",
            vec![
                json!("both"),
                json!("start"),
                json!("distributed"),
                json!(0),
            ],
        ),
    ] {
        for invalid in values {
            let mut p = paragraph(false);
            p[key] = invalid;
            bad.push(p);
        }
    }
    let mut p = paragraph(false);
    p["other"] = json!(0);
    bad.push(p);
    for p in bad {
        let mut m = model(true, true, false);
        m.content[0]
            .attrs
            .insert("imageParagraph".into(), p.clone());
        assert!(document::validate(&m).is_err(), "{p}");
    }
}
#[test]
fn image_paragraph_font_pair_is_complete_literal_bounded_and_typed() {
    let mut bad = Vec::new();
    for key in ["paragraphFont", "runFont"] {
        let mut p = paragraph(true);
        p.as_object_mut().unwrap().remove(key);
        bad.push(p);
        for invalid in [Value::Null, json!({}), json!([]), json!("Arial")] {
            let mut p = paragraph(true);
            p[key] = invalid;
            bad.push(p);
        }
        for field in ["family", "halfPoints", "bold", "italic"] {
            let mut p = paragraph(true);
            p[key].as_object_mut().unwrap().remove(field);
            bad.push(p);
        }
        for (field, values) in [
            (
                "family",
                vec![
                    json!(""),
                    json!(" Arial"),
                    json!("Arial\n"),
                    json!("a".repeat(32)),
                    Value::Null,
                ],
            ),
            (
                "halfPoints",
                vec![
                    json!(0),
                    json!(1),
                    json!(3277),
                    json!(2.5),
                    json!("24"),
                    Value::Null,
                ],
            ),
            ("bold", vec![json!(0), json!("false"), Value::Null]),
            ("italic", vec![json!(1), json!("true"), Value::Null]),
        ] {
            for invalid in values {
                let mut p = paragraph(true);
                p[key][field] = invalid;
                bad.push(p);
            }
        }
        let mut p = paragraph(true);
        p[key]["color"] = json!("000000");
        bad.push(p);
    }
    for p in bad {
        let mut m = model(false, false, true);
        m.content[0]
            .attrs
            .insert("imageParagraph".into(), p.clone());
        assert!(document::validate(&m).is_err(), "{p}");
    }
}
#[test]
fn image_paragraph_is_root_only_and_never_accepted_on_other_nodes() {
    let mut m = model(true, true, true);
    let image = m.content.remove(0);
    for parent in [
        json!({"type":"bulletList","content":[{"type":"listItem","content":[{"type":"paragraph"},value(&image)]}]}),
        json!({"type":"table","content":[{"type":"tableRow","content":[{"type":"tableCell","content":[value(&image)]}]}]}),
    ] {
        let m: Node = serde_json::from_value(json!({"type":"doc","content":[parent]})).unwrap();
        assert!(document::validate(&m).is_err());
    }
    for kind in ["doc", "paragraph", "heading", "text", "table"] {
        let m: Node = serde_json::from_value(
            json!({"type":kind,"attrs":{"imageParagraph":paragraph(false)}}),
        )
        .unwrap();
        assert!(document::validate(&m).is_err());
    }
}
#[test]
fn image_paragraph_export_uses_dedicated_style_explicit_spacing_and_distinct_run_metrics() {
    let dir = tempfile::tempdir().unwrap();
    for modern in [false, true] {
        let m = model(modern, true, true);
        let path = dir.path().join(format!("styled-{modern}.docx"));
        document::export_docx(&m, &path).unwrap();
        let p = parts(&path);
        let xml = String::from_utf8(p["word/document.xml"].clone()).unwrap();
        let styles = String::from_utf8(p["word/styles.xml"].clone()).unwrap();
        for id in 1..=m.content.len() {
            assert!(xml.contains(&format!("<w:pStyle w:val=\"ImageParagraph{id}\"/>")));
            assert!(styles.contains(&format!("w:styleId=\"ImageParagraph{id}\"")));
        }
        assert!(xml.contains("<w:spacing w:before=\"0\" w:after=\"240\" w:line=\"276\" w:lineRule=\"auto\"/><w:jc w:val=\"center\"/>"));
        assert!(xml.contains("w:ascii=\"Liberation Serif\""));
        assert!(xml.contains("<w:sz w:val=\"32\"/>"));
        assert!(styles.contains("w:ascii=\"DejaVu Sans\""));
        assert!(styles.contains("<w:sz w:val=\"24\"/>"));
        assert!(p.contains_key("word/settings.xml"));
        assert_eq!(
            value(&document::load_authored_docx(&path).unwrap().unwrap()),
            value(&m)
        );
    }
}
#[test]
fn image_paragraph_reset_alone_restores_previous_dimension_only_encoded_parts() {
    let dir = tempfile::tempdir().unwrap();
    for modern in [false, true] {
        for dimensions in [false, true] {
            let mut m = model(modern, dimensions, true);
            let changed = dir
                .path()
                .join(format!("changed-{modern}-{dimensions}.docx"));
            document::export_docx(&m, &changed).unwrap();
            for node in &mut m.content {
                node.attrs.remove("imageParagraph");
            }
            let clean = dir.path().join(format!("clean-{modern}-{dimensions}.docx"));
            document::export_docx(&m, &clean).unwrap();
            let mut old: Node =
                serde_json::from_str(include_str!("fixtures/word-image-legacy.json")).unwrap();
            if modern {
                old.attrs
                    .insert("styleProfile".into(), json!("standard-v2"));
            }
            if dimensions {
                for image in &mut old.content {
                    image.attrs.insert("displayWidthEmu".into(), json!(95251));
                    image.attrs.insert("displayHeightEmu".into(), json!(38107));
                }
            }
            let previous = dir
                .path()
                .join(format!("previous-{modern}-{dimensions}.docx"));
            document::export_docx(&old, &previous).unwrap();
            assert_eq!(parts(&clean), parts(&previous));
        }
    }
}
#[test]
fn image_paragraph_draft_roundtrip_and_failed_mutation_protect_previous_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.json");
    for modern in [false, true] {
        let mut m = model(modern, true, true);
        document::save_draft(&m, &path).unwrap();
        let before = fs::read(&path).unwrap();
        assert_eq!(
            value(&document::load_draft(&path).unwrap().unwrap()),
            value(&m)
        );
        m.content[0].attrs["imageParagraph"]["runFont"] = Value::Null;
        assert!(document::save_draft(&m, &path).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}
#[test]
fn image_paragraph_failed_export_never_clobbers_or_publishes_invalid_model() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.docx");
    let mut m = model(true, true, true);
    document::export_docx(&m, &path).unwrap();
    let original = fs::read(&path).unwrap();
    assert!(document::export_docx(&m, &path).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    m.content[0].attrs["imageParagraph"]["spaceBefore"] = json!(-1);
    let bad = dir.path().join("invalid.docx");
    assert!(document::export_docx(&m, &bad).is_err());
    assert!(!bad.exists());
}
#[test]
fn image_paragraph_style_tampering_fails_trusted_reopen_even_with_forged_hash() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.docx");
    let m = model(true, true, true);
    document::export_docx(&m, &path).unwrap();
    let mut p = parts(&path);
    let original = p["word/styles.xml"].clone();
    let changed = String::from_utf8(original)
        .unwrap()
        .replace("DejaVu Sans", "Liberation Sans")
        .into_bytes();
    p.insert("word/styles.xml".into(), changed.clone());
    for forged in [false, true] {
        if forged {
            let mut embedded: Value = serde_json::from_slice(&p["rust-wps/model-v2.json"]).unwrap();
            embedded["hashes"]["word/styles.xml"] =
                json!(format!("{:x}", Sha256::digest(&changed)));
            p.insert(
                "rust-wps/model-v2.json".into(),
                serde_json::to_vec(&embedded).unwrap(),
            );
        }
        let output = dir.path().join(format!("tampered-{forged}.docx"));
        write(&output, &p);
        assert!(document::load_authored_docx(&output).is_err());
    }
}
#[test]
fn image_paragraph_export_external_reimport_keeps_layout_and_metrics() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.docx");
    let m = model(true, true, true);
    document::export_docx(&m, &path).unwrap();
    let report = importing::import_docx(&path).unwrap();
    let images: Vec<_> = report
        .document
        .content
        .iter()
        .filter(|n| n.kind == "image")
        .collect();
    assert_eq!(images.len(), m.content.len());
    for (image, original) in images.iter().zip(&m.content) {
        assert_eq!(
            image.attrs["imageParagraph"],
            original.attrs["imageParagraph"]
        );
        assert_eq!(image.attrs["src"], original.attrs["src"]);
    }
}

#[test]
fn resetting_dimensions_keeps_layout_and_media_through_draft_and_trusted_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = model(true, true, true);
    for image in &mut m.content {
        image.attrs.remove("displayWidthEmu");
        image.attrs.remove("displayHeightEmu");
    }
    let path = dir.path().join("layout-only.docx");
    document::export_docx(&m, &path).unwrap();
    assert_eq!(
        value(&document::load_authored_docx(&path).unwrap().unwrap()),
        value(&m)
    );
    let p = parts(&path);
    let xml = String::from_utf8(p["word/document.xml"].clone()).unwrap();
    assert!(xml.contains("<w:spacing w:before=\"0\" w:after=\"240\""));
    assert!(xml.contains("<w:pStyle w:val=\"ImageParagraph1\"/>"));
    assert!(xml.contains("distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\""));
    let draft = dir.path().join("draft.json");
    document::save_draft(&m, &draft).unwrap();
    assert_eq!(
        value(&document::load_draft(&draft).unwrap().unwrap()),
        value(&m)
    );
}

#[test]
fn metadata_without_dimensions_enforces_media_validation_and_total_count_budget() {
    use base64::Engine;
    let m = model(true, false, false);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.json");
    document::save_draft(&m, &path).unwrap();
    let previous = fs::read(&path).unwrap();
    let mut large = m.clone();
    large.content = vec![large.content[0].clone(); 101];
    assert!(document::save_draft(&large, &path).is_err());
    assert_eq!(fs::read(&path).unwrap(), previous);
    let mut invalid = m.clone();
    let source = invalid.content[0].attrs["src"].as_str().unwrap().to_owned();
    let (header, encoded) = source.split_once(',').unwrap();
    let mut bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .unwrap();
    bytes.extend_from_slice(b"extraneous metadata");
    invalid.content[0].attrs.insert(
        "src".into(),
        json!(format!(
            "{header},{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )),
    );
    assert!(document::validate(&invalid).is_err());
}

#[test]
fn unspecified_baseline_is_explicitly_warned_without_losing_spacing() {
    let dir = tempfile::tempdir().unwrap();
    let m = model(true, true, false);
    let path = dir.path().join("source.docx");
    document::export_docx(&m, &path).unwrap();
    let mut p = parts(&path);
    p.insert(
        "word/styles.xml".into(),
        br#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#
            .to_vec(),
    );
    p.remove("rust-wps/model-v2.json");
    let external = dir.path().join("external.docx");
    write(&external, &p);
    let report = importing::import_docx(&external).unwrap();
    assert!(report.compatibility.risks.contains(
        "Image paragraph baseline font is unspecified; editor defaults may change its line box"
    ));
    for image in report.document.content.iter().filter(|n| n.kind == "image") {
        assert_eq!(image.attrs["imageParagraph"], paragraph(false));
    }
}
