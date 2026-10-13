use base64::Engine;
use rust_wps::document::{self, Node, STANDARD_STYLE_PROFILE};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read, Write},
    path::Path,
};

const V1_MODEL: &str = "rust-wps/model-v1.json";
const V2_MODEL: &str = "rust-wps/model-v2.json";
type Parts = BTreeMap<String, Vec<u8>>;
type StyleFields = BTreeMap<String, BTreeMap<String, String>>;

fn model(standard: bool) -> Node {
    let mut doc: Node = serde_json::from_str(include_str!("fixtures/authoring.json")).unwrap();
    if standard {
        doc.attrs
            .insert("styleProfile".into(), json!(STANDARD_STYLE_PROFILE));
    }
    doc
}

fn with_image(mut doc: Node) -> Node {
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(12, 8)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let encoded = base64::engine::general_purpose::STANDARD.encode(png.get_ref());
    doc.content.push(serde_json::from_value(json!({
        "type":"image","attrs":{"src":format!("data:image/png;base64,{encoded}"),"alt":"profile image"}
    })).unwrap());
    doc
}

fn parts(bytes: &[u8]) -> Parts {
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

fn write_parts(path: &Path, parts: &Parts) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, data) in parts {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
}

fn change_model(parts: &mut Parts, name: &str, change: impl FnOnce(&mut Value)) {
    let mut value: Value = serde_json::from_slice(&parts[name]).unwrap();
    change(&mut value);
    parts.insert(name.into(), serde_json::to_vec(&value).unwrap());
}

fn refresh_hashes(parts: &mut Parts, model_part: &str) {
    let hashes = parts
        .iter()
        .filter(|(name, _)| name.as_str() != model_part)
        .map(|(name, data)| (name.clone(), format!("{:x}", Sha256::digest(data))))
        .collect::<BTreeMap<_, _>>();
    change_model(parts, model_part, |metadata| {
        metadata["hashes"] = json!(hashes)
    });
}

fn assert_rejected(path: &Path, parts: &Parts, expected: &str) {
    write_parts(path, parts);
    let before = fs::read(path).unwrap();
    let error = document::load_authored_docx(path).unwrap_err().to_string();
    assert!(error.contains(expected), "expected {expected:?}: {error}");
    assert_eq!(fs::read(path).unwrap(), before);
}

fn style_fields(xml: &[u8]) -> BTreeMap<String, StyleFields> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut styles = BTreeMap::<String, StyleFields>::new();
    let mut current = None;
    loop {
        match reader.read_event().unwrap() {
            Event::Start(element) | Event::Empty(element) => {
                let name = element.local_name().as_ref().to_owned();
                let attrs = element
                    .attributes()
                    .map(|attribute| {
                        let attribute = attribute.unwrap();
                        (
                            attribute.key.local_name().as_ref().to_owned(),
                            attribute
                                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .unwrap()
                                .into_owned(),
                        )
                    })
                    .collect::<BTreeMap<_, _>>();
                if name == "docDefaults" {
                    current = Some("defaults".to_string());
                } else if name == "style" {
                    current = Some(attrs["styleId"].clone());
                } else if let Some(style) = &current
                    && matches!(
                        name.as_str(),
                        "rFonts"
                            | "color"
                            | "b"
                            | "bCs"
                            | "sz"
                            | "szCs"
                            | "spacing"
                            | "keepNext"
                            | "outlineLvl"
                            | "basedOn"
                    )
                {
                    assert!(
                        styles
                            .entry(style.clone())
                            .or_default()
                            .insert(name, attrs)
                            .is_none()
                    );
                }
            }
            Event::End(element)
                if matches!(element.local_name().as_ref(), "style" | "docDefaults") =>
            {
                current = None
            }
            Event::Eof => break,
            _ => {}
        }
    }
    styles
}

#[test]
fn legacy_profile_keeps_every_preexisting_package_part_byte_identical() {
    let baseline = include_bytes!("fixtures/authoring-before-font-size.docx");
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old.docx");
    fs::write(&old, baseline).unwrap();
    let doc = model(false);
    assert_eq!(
        serde_json::to_value(document::load_authored_docx(&old).unwrap().unwrap()).unwrap(),
        serde_json::to_value(&doc).unwrap()
    );
    let output = dir.path().join("still-v1.docx");
    document::export_docx(&doc, &output).unwrap();
    assert_eq!(parts(&fs::read(&output).unwrap()), parts(baseline));
    assert_eq!(fs::read(&old).unwrap(), baseline);
}

#[test]
fn draft_version_and_profile_roundtrip_in_the_same_slot_without_implicit_upgrade() {
    let dir = tempfile::tempdir().unwrap();
    let slot = dir.path().join("drafts/current.json");
    for standard in [false, true, false] {
        let doc = model(standard);
        document::save_draft(&doc, &slot).unwrap();
        let stored: Value = serde_json::from_slice(&fs::read(&slot).unwrap()).unwrap();
        assert_eq!(stored["version"], if standard { 2 } else { 1 });
        assert_eq!(stored["document"], serde_json::to_value(&doc).unwrap());
        assert_eq!(
            serde_json::to_value(document::load_draft(&slot).unwrap().unwrap()).unwrap(),
            stored["document"]
        );
        if !standard {
            assert!(stored["document"].get("attrs").is_none());
            assert_eq!(
                fs::read(&slot).unwrap(),
                format!(
                    "{{\"version\":1,\"document\":{}}}",
                    serde_json::to_string(&doc).unwrap()
                )
                .as_bytes()
            );
        }
    }
    for standard in [false, true] {
        for version in [0, 1, 2, 3, 99] {
            if version == if standard { 2 } else { 1 } {
                continue;
            }
            let bytes =
                serde_json::to_vec(&json!({"version":version,"document":model(standard)})).unwrap();
            fs::write(&slot, &bytes).unwrap();
            assert!(
                document::load_draft(&slot).is_err(),
                "version {version}, standard {standard}"
            );
            assert_eq!(fs::read(&slot).unwrap(), bytes);
        }
    }
}

#[test]
fn standard_profile_exports_fixed_defaults_and_reopens_the_complete_model() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("standard.docx");
    let mut doc = with_image(model(true));
    for level in [2, 3] {
        doc.content.push(serde_json::from_value(json!({"type":"heading","attrs":{"level":level},"content":[{"type":"text","text":"Heading 中文 سلام"}]})).unwrap());
    }
    doc.content.push(serde_json::from_value(json!({"type":"paragraph","content":[{
        "type":"text","text":"Explicit 中文 سلام","marks":[{"type":"fontSize","attrs":{"halfPoints":27}},{"type":"textColor","attrs":{"rgb":"C03030"}},{"type":"link","attrs":{"href":"https://example.com"}}]
    }]})).unwrap());
    document::export_docx(&doc, &output).unwrap();
    let before = fs::read(&output).unwrap();
    let archive = parts(&before);
    assert!(archive.contains_key(V2_MODEL));
    assert!(!archive.contains_key(V1_MODEL));
    let embedded: Value = serde_json::from_slice(&archive[V2_MODEL]).unwrap();
    assert_eq!(embedded["version"], 2);
    assert_eq!(embedded["document"], serde_json::to_value(&doc).unwrap());
    assert_eq!(
        embedded["hashes"].as_object().unwrap().len() + 1,
        archive.len()
    );
    assert!(embedded["hashes"].get(V2_MODEL).is_none());
    assert!(
        std::str::from_utf8(&archive["_rels/.rels"])
            .unwrap()
            .contains("Target=\"rust-wps/model-v2.json\"")
    );
    let styles = style_fields(&archive["word/styles.xml"]);
    let defaults = &styles["defaults"];
    for (script, font) in [
        ("ascii", "Arial"),
        ("hAnsi", "Arial"),
        ("cs", "Arial"),
        ("eastAsia", "Microsoft YaHei"),
    ] {
        assert_eq!(defaults["rFonts"][script], font);
    }
    for (tag, value) in [
        ("color", "000000"),
        ("sz", "22"),
        ("szCs", "22"),
        ("b", "0"),
        ("bCs", "0"),
    ] {
        assert_eq!(defaults[tag]["val"], value);
    }
    for (attribute, value) in [
        ("before", "0"),
        ("after", "120"),
        ("line", "360"),
        ("lineRule", "auto"),
    ] {
        assert_eq!(defaults["spacing"][attribute], value);
    }
    for (level, half_points) in [(1, "36"), (2, "30"), (3, "26")] {
        let heading = &styles[&format!("Heading{level}")];
        assert_eq!(heading["basedOn"]["val"], "Normal");
        assert_eq!(heading["outlineLvl"]["val"], (level - 1).to_string());
        assert_eq!(heading["sz"]["val"], half_points);
        assert_eq!(heading["szCs"]["val"], half_points);
        for flag in ["b", "bCs", "keepNext"] {
            assert!(heading[flag].is_empty());
        }
        for (attribute, value) in [
            ("before", "240"),
            ("after", "120"),
            ("line", "360"),
            ("lineRule", "auto"),
        ] {
            assert_eq!(heading["spacing"][attribute], value);
        }
    }
    let visible = std::str::from_utf8(&archive["word/document.xml"]).unwrap();
    assert!(visible.contains("<w:sz w:val=\"27\"/><w:szCs w:val=\"27\"/>"));
    assert!(visible.contains("<w:color w:val=\"C03030\"/>"));
    assert!(visible.contains("<w:pgSz w:w=\"11906\" w:h=\"16838\"/>"));
    assert!(
        visible
            .contains("<w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\"")
    );
    assert_eq!(
        serde_json::to_value(document::load_authored_docx(&output).unwrap().unwrap()).unwrap(),
        serde_json::to_value(&doc).unwrap()
    );
    assert_eq!(fs::read(&output).unwrap(), before);
}

#[test]
fn standard_explicit_emphasis_covers_complex_script_in_body_heading_and_header() {
    let dir = tempfile::tempdir().unwrap();
    for standard in [false, true] {
        let text = json!({"type":"text","text":"Latin 中文 سلام","marks":[{"type":"bold"},{"type":"italic"}]});
        let mut doc: Node = serde_json::from_value(json!({"type":"doc","content":[
            {"type":"paragraph","content":[text.clone()]},
            {"type":"heading","attrs":{"level":2},"content":[text.clone()]},
            {"type":"table","content":[{"type":"tableRow","content":[
                {"type":"tableHeader","content":[{"type":"paragraph","content":[text]}]},
                {"type":"tableHeader","content":[{"type":"paragraph","content":[{"type":"text","text":"Unmarked header"}]}]}
            ]}]}
        ]})).unwrap();
        if standard {
            doc.attrs
                .insert("styleProfile".into(), json!(STANDARD_STYLE_PROFILE));
        }
        let output = dir.path().join(format!("emphasis-{standard}.docx"));
        document::export_docx(&doc, &output).unwrap();
        let archive = parts(&fs::read(&output).unwrap());
        let visible = std::str::from_utf8(&archive["word/document.xml"]).unwrap();
        assert_eq!(
            visible
                .matches(if standard {
                    "<w:b/><w:bCs/><w:i/><w:iCs/>"
                } else {
                    "<w:b/><w:i/>"
                })
                .count(),
            3
        );
        if !standard {
            assert!(!visible.contains("<w:bCs"));
            assert!(!visible.contains("<w:iCs"));
        }
        // The exporter does not infer bold from tableHeader. Existing explicit
        // marks are preserved without silently adding extra formatting.
        assert!(
            visible.contains("<w:rPr></w:rPr><w:t xml:space=\"preserve\">Unmarked header</w:t>")
        );
        assert_eq!(
            serde_json::to_value(document::load_authored_docx(&output).unwrap().unwrap()).unwrap(),
            serde_json::to_value(&doc).unwrap()
        );
    }
}

#[test]
fn invalid_profiles_and_nonroot_profile_attributes_never_write() {
    let dir = tempfile::tempdir().unwrap();
    let draft = dir.path().join("current.json");
    document::save_draft(&model(true), &draft).unwrap();
    let before = fs::read(&draft).unwrap();
    let mut invalid = [
        json!(null),
        json!(true),
        json!(2),
        json!([]),
        json!({}),
        json!(""),
        json!("legacy-v1"),
        json!("standard-v1"),
        json!("standard-v3"),
        json!("STANDARD-V2"),
        json!("standard-v2 "),
    ]
    .into_iter()
    .map(|profile| {
        let mut doc = model(false);
        doc.attrs.insert("styleProfile".into(), profile);
        doc
    })
    .collect::<Vec<_>>();
    let mut extra = model(true);
    extra.attrs.insert("styleVersion".into(), json!(2));
    invalid.push(extra);
    for location in [0, 1, 2] {
        let mut doc = model(true);
        let node = match location {
            0 => &mut doc.content[0],
            1 => &mut doc.content[1],
            _ => &mut doc.content[1].content[0],
        };
        node.attrs
            .insert("styleProfile".into(), json!(STANDARD_STYLE_PROFILE));
        invalid.push(doc);
    }
    for (index, doc) in invalid.into_iter().enumerate() {
        assert!(document::validate(&doc).is_err(), "case {index}");
        assert!(document::save_draft(&doc, &draft).is_err(), "case {index}");
        assert_eq!(fs::read(&draft).unwrap(), before);
        let output = dir.path().join(format!("invalid-{index}.docx"));
        assert!(
            document::export_docx(&doc, &output).is_err(),
            "case {index}"
        );
        assert!(!output.exists());
        fs::write(&output, b"existing").unwrap();
        assert!(document::export_docx(&doc, &output).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"existing");
    }
}

#[test]
fn authored_model_part_version_and_profile_must_agree_without_silent_upgrade() {
    let dir = tempfile::tempdir().unwrap();
    for standard in [false, true] {
        let source = dir.path().join(format!("source-{standard}.docx"));
        document::export_docx(&model(standard), &source).unwrap();
        let archive = parts(&fs::read(&source).unwrap());
        let (name, other, opposite_version) = if standard {
            (V2_MODEL, V1_MODEL, 1)
        } else {
            (V1_MODEL, V2_MODEL, 2)
        };
        for (case, value, expected) in [
            (
                "opposite-version",
                json!(opposite_version),
                "version, profile and part name disagree",
            ),
            (
                "unknown-version",
                json!(99),
                "Unsupported embedded model version",
            ),
        ] {
            let mut forged = archive.clone();
            change_model(&mut forged, name, |metadata| metadata["version"] = value);
            assert_rejected(
                &dir.path().join(format!("{standard}-{case}.docx")),
                &forged,
                expected,
            );
        }
        let mut forged = archive.clone();
        change_model(&mut forged, name, |metadata| {
            if standard {
                metadata["document"]
                    .as_object_mut()
                    .unwrap()
                    .remove("attrs");
            } else {
                metadata["document"]["attrs"] = json!({"styleProfile":STANDARD_STYLE_PROFILE});
            }
        });
        assert_rejected(
            &dir.path().join(format!("{standard}-profile-only.docx")),
            &forged,
            "version, profile and part name disagree",
        );
        for profile in [json!("standard-v3"), json!(null)] {
            let mut forged = archive.clone();
            change_model(&mut forged, name, |metadata| {
                metadata["document"]["attrs"] = json!({"styleProfile":profile})
            });
            assert_rejected(
                &dir.path().join(format!("{standard}-unknown-profile.docx")),
                &forged,
                "Style profile",
            );
        }
        let mut forged = archive.clone();
        let metadata = forged.remove(name).unwrap();
        forged.insert(other.into(), metadata);
        assert_rejected(
            &dir.path().join(format!("{standard}-wrong-part.docx")),
            &forged,
            "version, profile and part name disagree",
        );
        // Changing all version declarations still must not relabel the old
        // visible styles as a different profile, even with fresh package hashes.
        change_model(&mut forged, other, |metadata| {
            metadata["version"] = json!(opposite_version);
            if standard {
                metadata["document"]
                    .as_object_mut()
                    .unwrap()
                    .remove("attrs");
            } else {
                metadata["document"]["attrs"] = json!({"styleProfile":STANDARD_STYLE_PROFILE});
            }
        });
        let relationships = String::from_utf8(forged["_rels/.rels"].clone())
            .unwrap()
            .replace(name, other);
        forged.insert("_rels/.rels".into(), relationships.into_bytes());
        refresh_hashes(&mut forged, other);
        assert_rejected(
            &dir.path().join(format!("{standard}-relabeled.docx")),
            &forged,
            "disagrees with DOCX content:",
        );
    }
}

#[test]
fn multiple_or_unknown_authoring_model_parts_never_fall_back_to_external_document() {
    let dir = tempfile::tempdir().unwrap();
    for standard in [false, true] {
        let source = dir.path().join(format!("source-{standard}.docx"));
        document::export_docx(&model(standard), &source).unwrap();
        let archive = parts(&fs::read(&source).unwrap());
        let (name, other) = if standard {
            (V2_MODEL, V1_MODEL)
        } else {
            (V1_MODEL, V2_MODEL)
        };
        for additional in [other, "rust-wps/model-v3.json"] {
            let mut forged = archive.clone();
            forged.insert(additional.into(), archive[name].clone());
            assert_rejected(
                &dir.path().join(format!("{standard}-multiple.docx")),
                &forged,
                "multiple authoring models",
            );
        }
        for unknown in [
            "rust-wps/model-v3.json",
            "rust-wps/model-v99.json",
            "rust-wps/model-vnext.json",
        ] {
            let mut forged = archive.clone();
            let metadata = forged.remove(name).unwrap();
            forged.insert(unknown.into(), metadata);
            assert_rejected(
                &dir.path().join(format!("{standard}-unknown.docx")),
                &forged,
                "Unsupported authoring model part",
            );
        }
    }
}

#[test]
fn both_versions_bind_hash_coverage_and_the_complete_visible_part_set() {
    let dir = tempfile::tempdir().unwrap();
    for standard in [false, true] {
        let source = dir.path().join(format!("complete-{standard}.docx"));
        document::export_docx(&with_image(model(standard)), &source).unwrap();
        let archive = parts(&fs::read(&source).unwrap());
        let name = if standard { V2_MODEL } else { V1_MODEL };
        for removed in ["word/styles.xml", "word/media/image1.png"] {
            let mut forged = archive.clone();
            forged.remove(removed).unwrap();
            refresh_hashes(&mut forged, name);
            assert_rejected(
                &dir.path().join(format!("{standard}-missing.docx")),
                &forged,
                "part set",
            );
        }
        let mut forged = archive.clone();
        forged.insert("word/extra.xml".into(), b"<extra/>".to_vec());
        refresh_hashes(&mut forged, name);
        assert_rejected(
            &dir.path().join(format!("{standard}-extra.docx")),
            &forged,
            "part set",
        );
        let mut forged = archive.clone();
        change_model(&mut forged, name, |metadata| {
            metadata["hashes"]
                .as_object_mut()
                .unwrap()
                .remove("word/styles.xml");
        });
        assert_rejected(
            &dir.path().join(format!("{standard}-hash-coverage.docx")),
            &forged,
            "parts changed outside the authoring model",
        );
        let mut forged = archive.clone();
        let wrong_target = if standard { V1_MODEL } else { V2_MODEL };
        let relationships = String::from_utf8(forged["_rels/.rels"].clone())
            .unwrap()
            .replace(name, wrong_target);
        forged.insert("_rels/.rels".into(), relationships.into_bytes());
        refresh_hashes(&mut forged, name);
        assert_rejected(
            &dir.path().join(format!("{standard}-relationship.docx")),
            &forged,
            "disagrees with DOCX content: _rels/.rels",
        );
    }
}
