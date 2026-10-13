use base64::Engine;
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

fn fixture() -> Node {
    serde_json::from_str(include_str!("fixtures/word-image-legacy.json")).unwrap()
}
fn model(width: u64, height: u64, modern: bool) -> Node {
    let mut model = fixture();
    if modern {
        model
            .attrs
            .insert("styleProfile".into(), json!("standard-v2"));
    }
    for image in &mut model.content {
        image.attrs.insert("displayWidthEmu".into(), json!(width));
        image.attrs.insert("displayHeightEmu".into(), json!(height));
    }
    model
}
fn parts(path: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut zip = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    (0..zip.len())
        .map(|index| {
            let mut part = zip.by_index(index).unwrap();
            let mut bytes = Vec::new();
            part.read_to_end(&mut bytes).unwrap();
            (part.name().into(), bytes)
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
fn value(node: &Node) -> Value {
    serde_json::to_value(node).unwrap()
}

#[test]
fn dimension_pairs_are_bounded_integer_emu_without_null_coercion() {
    for (width, height) in [(1, 1), (95251, 38107), (5_715_000, 7_620_000)] {
        for modern in [false, true] {
            document::validate(&model(width, height, modern)).unwrap();
        }
    }
    for (field, values) in [
        (
            "displayWidthEmu",
            vec![
                Value::Null,
                json!(0),
                json!(-1),
                json!(1.5),
                json!("12700"),
                json!(true),
                json!([]),
                json!({}),
                json!(5_715_001),
                json!(u64::MAX),
            ],
        ),
        (
            "displayHeightEmu",
            vec![
                Value::Null,
                json!(0),
                json!(-1),
                json!(1.5),
                json!("12700"),
                json!(false),
                json!([]),
                json!({}),
                json!(7_620_001),
                json!(u64::MAX),
            ],
        ),
    ] {
        for invalid in values {
            let mut m = model(95250, 95250, true);
            m.content[0].attrs.insert(field.into(), invalid.clone());
            assert!(document::validate(&m).is_err(), "{field}: {invalid}");
        }
        let mut partial = model(95250, 95250, true);
        partial.content[0].attrs.remove(field);
        assert!(document::validate(&partial).is_err());
        let mut wrong = fixture();
        wrong.attrs.insert(field.into(), json!(95250));
        assert!(document::validate(&wrong).is_err());
    }
    let mut pixel = model(12700, 12700, true);
    pixel.content[0].attrs.insert("width".into(), json!(1));
    assert!(document::validate(&pixel).is_err());
    pixel.content[0].attrs.insert("width".into(), Value::Null);
    pixel.content[0].attrs.insert("height".into(), json!(1));
    assert!(document::validate(&pixel).is_err());
}

#[test]
fn sized_images_require_top_level_body_while_legacy_nested_images_stay_valid() {
    for dimensions in [false, true] {
        let image = if dimensions {
            model(95250, 95250, true)
        } else {
            fixture()
        }
        .content
        .remove(0);
        let cell = json!({"type":"tableCell","content":[value(&image)]});
        let table: Node=serde_json::from_value(json!({"type":"doc","content":[{"type":"table","content":[{"type":"tableRow","content":[cell]}]}]})).unwrap();
        assert_eq!(document::validate(&table).is_ok(), !dimensions);
        let list:Node=serde_json::from_value(json!({"type":"doc","content":[{"type":"bulletList","content":[{"type":"listItem","content":[{"type":"paragraph"},value(&image)]}]}]})).unwrap();
        assert_eq!(document::validate(&list).is_ok(), !dimensions);
    }
}

#[test]
fn png_jpeg_extent_pairs_and_blobs_survive_export_and_trusted_reopen() {
    let dir = tempfile::tempdir().unwrap();
    for modern in [false, true] {
        for (width, height) in [
            (1, 1),
            (95251, 38107),
            (1_828_800, 914_400),
            (5_715_000, 7_620_000),
        ] {
            let m = model(width, height, modern);
            let path = dir.path().join(format!("{modern}-{width}-{height}.docx"));
            document::export_docx(&m, &path).unwrap();
            let p = parts(&path);
            let xml = String::from_utf8(p["word/document.xml"].clone()).unwrap();
            assert_eq!(
                xml.matches(&format!(r#"cx="{width}" cy="{height}""#))
                    .count(),
                4
            );
            for (index, extension) in ["png", "jpeg"].into_iter().enumerate() {
                let encoded = m.content[index].attrs["src"]
                    .as_str()
                    .unwrap()
                    .split_once(',')
                    .unwrap()
                    .1;
                assert_eq!(
                    p[&format!("word/media/image{}.{}", index + 1, extension)],
                    base64::engine::general_purpose::STANDARD
                        .decode(encoded)
                        .unwrap()
                );
            }
            assert_eq!(
                value(&document::load_authored_docx(&path).unwrap().unwrap()),
                value(&m)
            );
        }
    }
}

#[test]
fn unchanged_dimensions_absence_preserves_all_historical_package_parts() {
    let baseline: Value =
        serde_json::from_str(include_str!("fixtures/word-image-legacy-parts.json")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    for profile in ["legacy-v1", "standard-v2"] {
        let mut m = fixture();
        if profile == "standard-v2" {
            m.attrs.insert("styleProfile".into(), json!(profile));
        }
        let path = dir.path().join(format!("{profile}.docx"));
        document::export_docx(&m, &path).unwrap();
        let actual: BTreeMap<_, _> = parts(&path)
            .into_iter()
            .map(|(name, bytes)| (name, format!("{:x}", Sha256::digest(&bytes))))
            .collect();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            baseline["profiles"][profile]
        );
        assert_eq!(
            value(&document::load_authored_docx(&path).unwrap().unwrap()),
            value(&m)
        );
    }
}

#[test]
fn paired_geometry_roundtrips_drafts_and_invalid_edits_preserve_last_draft() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.json");
    for modern in [false, true] {
        let m = model(914_401, 457_207, modern);
        document::save_draft(&m, &path).unwrap();
        let original = fs::read(&path).unwrap();
        assert_eq!(
            value(&document::load_draft(&path).unwrap().unwrap()),
            value(&m)
        );
        let mut bad = m.clone();
        bad.content[0].attrs.remove("displayHeightEmu");
        assert!(document::save_draft(&bad, &path).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        let mut raw: Value = serde_json::from_slice(&original).unwrap();
        raw["document"]["content"][0]["attrs"]["displayWidthEmu"] = Value::Null;
        fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
        assert!(document::load_draft(&path).is_err());
    }
}

#[test]
fn reset_removes_both_dimensions_and_restores_legacy_output_bytes() {
    let dir = tempfile::tempdir().unwrap();
    for modern in [false, true] {
        let mut m = model(95251, 45679, modern);
        for image in &mut m.content {
            image.attrs.remove("displayWidthEmu");
            image.attrs.remove("displayHeightEmu");
        }
        let reset = dir.path().join(format!("reset-{modern}.docx"));
        document::export_docx(&m, &reset).unwrap();
        let mut original = fixture();
        if modern {
            original
                .attrs
                .insert("styleProfile".into(), json!("standard-v2"));
        }
        let baseline = dir.path().join(format!("baseline-{modern}.docx"));
        document::export_docx(&original, &baseline).unwrap();
        assert_eq!(parts(&reset), parts(&baseline));
    }
}

#[test]
fn geometry_export_does_not_overwrite_source_or_publish_invalid_documents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.docx");
    let m = model(12700, 25400, true);
    document::export_docx(&m, &path).unwrap();
    let original = fs::read(&path).unwrap();
    assert!(document::export_docx(&model(38100, 12700, true), &path).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    let invalid = dir.path().join("invalid.docx");
    assert!(document::export_docx(&model(0, 1, true), &invalid).is_err());
    assert!(!invalid.exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn model_or_xml_geometry_tampering_is_not_a_trusted_authored_reopen() {
    let dir = tempfile::tempdir().unwrap();
    for modern in [false, true] {
        let source = dir.path().join(format!("source-{modern}.docx"));
        document::export_docx(&model(914400, 457200, modern), &source).unwrap();
        let original = fs::read(&source).unwrap();
        let baseline = parts(&source);
        let model_part = if modern {
            "rust-wps/model-v2.json"
        } else {
            "rust-wps/model-v1.json"
        };
        for mutation in ["model", "outer", "inner", "both", "media"] {
            let mut p = baseline.clone();
            let mut meta: Value = serde_json::from_slice(&p[model_part]).unwrap();
            if mutation == "model" {
                meta["document"]["content"][0]["attrs"]["displayWidthEmu"] = json!(914401);
            } else if mutation == "media" {
                p.get_mut("word/media/image1.png").unwrap().push(0);
            } else {
                let xml = String::from_utf8(p["word/document.xml"].clone()).unwrap();
                let xml = match mutation {
                    "outer" => {
                        xml.replacen(r#"<wp:extent cx="914400""#, r#"<wp:extent cx="914401""#, 1)
                    }
                    "inner" => xml.replacen(r#"<a:ext cx="914400""#, r#"<a:ext cx="914401""#, 1),
                    _ => xml.replace(r#"cx="914400""#, r#"cx="914401""#),
                };
                p.insert("word/document.xml".into(), xml.into_bytes());
            }
            for (name, bytes) in &p {
                if name != model_part {
                    meta["hashes"][name] = json!(format!("{:x}", Sha256::digest(bytes)));
                }
            }
            p.insert(model_part.into(), serde_json::to_vec(&meta).unwrap());
            let forged = dir.path().join(format!("forged-{modern}-{mutation}.docx"));
            write(&forged, &p);
            assert!(
                document::load_authored_docx(&forged).is_err(),
                "{modern}: {mutation}"
            );
        }
        assert_eq!(fs::read(&source).unwrap(), original);
    }
}

#[test]
fn external_import_preserves_order_media_geometry_and_genuine_neighboring_blank_paragraphs() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = model(914400, 457200, true);
    let blank: Node = serde_json::from_value(json!({"type":"paragraph"})).unwrap();
    m.content.insert(0, blank.clone());
    m.content.insert(2, blank.clone());
    m.content.push(blank);
    m.content[3]
        .attrs
        .insert("displayWidthEmu".into(), json!(457200));
    let source = dir.path().join("source.docx");
    document::export_docx(&m, &source).unwrap();
    let original = fs::read(&source).unwrap();
    let imported = importing::import_docx(&source).unwrap();
    assert_eq!(
        imported
            .document
            .content
            .iter()
            .map(|node| node.kind.as_str())
            .collect::<Vec<_>>(),
        ["paragraph", "image", "paragraph", "image", "paragraph"]
    );
    for index in [1, 3] {
        for attr in ["src", "displayWidthEmu", "displayHeightEmu"] {
            assert_eq!(
                imported.document.content[index].attrs[attr],
                m.content[index].attrs[attr]
            );
        }
    }
    assert!(
        !imported
            .compatibility
            .risks
            .contains("Image position and display dimensions normalized")
    );
    assert!(imported.compatibility.risks.contains(
        "Image paragraph placement and formatting normalized; original layout not preserved"
    ));
    let output = dir.path().join("imported.docx");
    document::export_docx(&imported.document, &output).unwrap();
    assert_eq!(
        value(&document::load_authored_docx(&output).unwrap().unwrap()),
        value(&imported.document)
    );
    let again = importing::import_docx(&output).unwrap();
    assert_eq!(
        again
            .document
            .content
            .iter()
            .map(|node| node.kind.as_str())
            .collect::<Vec<_>>(),
        ["paragraph", "image", "paragraph", "image", "paragraph"]
    );
    assert_eq!(fs::read(&source).unwrap(), original);
}

#[test]
fn custom_sizes_reject_orientation_animation_and_ambiguous_media_containers() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = fixture();
    for (index, media) in legacy.content.iter().enumerate() {
        let source = media.attrs["src"].as_str().unwrap();
        let (header, encoded) = source.split_once(',').unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        let mut variants = vec![bytes[..bytes.len() - 1].to_vec(), {
            let mut v = bytes.clone();
            v.push(0);
            v
        }];
        if index == 0 {
            for tag in [b"eXIf", b"acTL", b"fcTL", b"fdAT"] {
                let mut added = Vec::new();
                added.extend_from_slice(&bytes[..33]);
                added.extend_from_slice(&[0, 0, 0, 0]);
                added.extend_from_slice(tag);
                added.extend_from_slice(&[0, 0, 0, 0]);
                added.extend_from_slice(&bytes[33..]);
                variants.push(added);
            }
            variants.push(b"\x89PNG\r\n\x1a\n\xff\xff\xff\xffIHDR".to_vec());
        } else {
            let app1 = [0xff, 0xe1, 0, 8, b'E', b'x', b'i', b'f', 0, 0];
            for position in [2, bytes.len() - 2] {
                let mut added = bytes[..position].to_vec();
                added.extend_from_slice(&app1);
                added.extend_from_slice(&bytes[position..]);
                variants.push(added);
            }
            variants.push(vec![0xff, 0xd8, 0xff, 0xe0, 0, 1, 0xff, 0xd9]);
        }
        for (case, bytes) in variants.iter().enumerate() {
            let mut m = model(914400, 457200, true);
            m.content[index].attrs.insert(
                "src".into(),
                json!(format!(
                    "{header},{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                )),
            );
            assert!(document::validate(&m).is_err(), "{index} case {case}");
            let output = dir.path().join(format!("invalid-{index}-{case}.docx"));
            assert!(document::export_docx(&m, &output).is_err());
            assert!(!output.exists());
            // Compatibility scope is deliberately new explicit dimensions.
            for image in &mut m.content {
                image.attrs.remove("displayWidthEmu");
                image.attrs.remove("displayHeightEmu");
            }
            document::validate(&m).unwrap();
        }
    }
}

#[test]
fn dimension_edits_apply_whole_document_media_count_and_byte_budgets_before_draft_save() {
    let dir = tempfile::tempdir().unwrap();
    let draft = dir.path().join("draft.json");
    let sized = model(95250, 95250, true);
    document::save_draft(&sized, &draft).unwrap();
    let previous = fs::read(&draft).unwrap();
    let mut many = sized.clone();
    many.content = vec![sized.content[0].clone(); 100];
    document::validate(&many).unwrap();
    many.content.push(sized.content[0].clone());
    assert!(
        document::validate(&many)
            .unwrap_err()
            .to_string()
            .contains("count")
    );
    assert!(document::save_draft(&many, &draft).is_err());
    let mut large = sized.clone();
    large.content.truncate(1);
    let mut legacy_image = fixture().content.remove(0);
    legacy_image.attrs.insert(
        "src".into(),
        json!(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(vec![0; 7 * 1024 * 1024])
        )),
    );
    large.content.extend(vec![legacy_image; 3]);
    assert!(
        document::validate(&large)
            .unwrap_err()
            .to_string()
            .contains("data exceeds")
    );
    assert!(document::save_draft(&large, &draft).is_err());
    assert_eq!(fs::read(&draft).unwrap(), previous);
}

#[test]
fn custom_sizes_enforce_bitmap_pixel_header_limits_before_draft_storage() {
    let legacy = fixture();
    for (index, image) in legacy.content.iter().enumerate() {
        let (header, encoded) = image.attrs["src"]
            .as_str()
            .unwrap()
            .split_once(',')
            .unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        for (width, height) in [(20001u32, 1u32), (20000, 2001), (1, 20001), (0, 8)] {
            let mut bytes = bytes.clone();
            if index == 0 {
                bytes[16..20].copy_from_slice(&width.to_be_bytes());
                bytes[20..24].copy_from_slice(&height.to_be_bytes());
                let mut crc = 0xffff_ffffu32;
                for byte in &bytes[12..29] {
                    crc ^= u32::from(*byte);
                    for _ in 0..8 {
                        crc = if crc & 1 != 0 {
                            (crc >> 1) ^ 0xedb8_8320
                        } else {
                            crc >> 1
                        };
                    }
                }
                bytes[29..33].copy_from_slice(&(!crc).to_be_bytes());
            } else {
                let sof = bytes.windows(2).position(|v| v == [0xff, 0xc0]).unwrap();
                bytes[sof + 5..sof + 7].copy_from_slice(&(height as u16).to_be_bytes());
                bytes[sof + 7..sof + 9].copy_from_slice(&(width as u16).to_be_bytes());
            }
            let mut candidate = model(12700, 12700, true);
            candidate.content[index].attrs.insert(
                "src".into(),
                json!(format!(
                    "{header},{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                )),
            );
            assert!(
                document::validate(&candidate).is_err(),
                "{index} {width}x{height}"
            );
        }
    }
}
