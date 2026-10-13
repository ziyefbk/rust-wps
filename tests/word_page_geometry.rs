use rust_wps::document::{self, Node};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::Path,
};

fn page() -> Value {
    json!({"widthTwips":12240,"heightTwips":15840,"marginTopTwips":720,"marginRightTwips":1080,"marginBottomTwips":1440,"marginLeftTwips":1800})
}
fn model() -> Node {
    serde_json::from_value(json!({"type":"doc","attrs":{"styleProfile":"standard-v2","pageGeometry":page()},"content":[{"type":"paragraph","content":[{"type":"text","text":"Page proof"}]}]})).unwrap()
}
fn parts(path: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut zip = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    (0..zip.len())
        .map(|index| {
            let mut item = zip.by_index(index).unwrap();
            let mut bytes = vec![];
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
fn export(model: &Node, path: &Path) {
    document::export_docx(model, path).unwrap();
}

#[test]
fn all_nominal_sizes_orientations_and_twip_tolerances_preserve_raw_values() {
    for (w, h) in [(11906u64, 16838u64), (12240, 15840), (8391, 11906)] {
        for landscape in [false, true] {
            for dw in [-2i64, 0, 2] {
                for dh in [-2i64, 0, 2] {
                    let mut m = model();
                    let (w, h) = if landscape { (h, w) } else { (w, h) };
                    m.attrs["pageGeometry"]["widthTwips"] = json!(w as i64 + dw);
                    m.attrs["pageGeometry"]["heightTwips"] = json!(h as i64 + dh);
                    document::validate(&m).unwrap();
                    let p = document::page_geometry(&m).unwrap().unwrap();
                    assert_eq!(p.width_twips, (w as i64 + dw) as u64);
                    assert_eq!(p.height_twips, (h as i64 + dh) as u64);
                }
            }
        }
    }
}
#[test]
fn raw_page_geometry_rejects_null_partial_unknown_and_noninteger_fields() {
    let mut bad = vec![Value::Null, json!([]), json!({}), json!(true), json!("A4")];
    for key in page().as_object().unwrap().keys() {
        let mut p = page();
        p.as_object_mut().unwrap().remove(key);
        bad.push(p);
        for value in [
            Value::Null,
            json!("1440"),
            json!(1440.0),
            json!(-1),
            json!(true),
            json!(u64::MAX),
        ] {
            let mut p = page();
            p[key] = value;
            bad.push(p);
        }
    }
    let mut p = page();
    p["orientation"] = json!("portrait");
    bad.push(p);
    for p in bad {
        let mut m = model();
        m.attrs["pageGeometry"] = p.clone();
        assert!(document::validate(&m).is_err(), "accepted {p}");
    }
}
#[test]
fn geometry_rejects_unrecognized_paper_excess_margins_and_small_content() {
    for patch in [
        json!({"widthTwips":12243}),
        json!({"heightTwips":15843}),
        json!({"widthTwips":10000}),
        json!({"marginTopTwips":4321}),
        json!({"widthTwips":8391,"heightTwips":11906,"marginLeftTwips":4000,"marginRightTwips":4000}),
    ] {
        let mut m = model();
        for (k, v) in patch.as_object().unwrap() {
            m.attrs["pageGeometry"][k] = v.clone();
        }
        assert!(document::validate(&m).is_err());
    }
}
#[test]
fn page_geometry_only_allowed_on_root() {
    for target in [0, 1] {
        let mut m = model();
        let p = m.attrs.remove("pageGeometry").unwrap();
        if target == 0 {
            m.content[0].attrs.insert("pageGeometry".into(), p);
        } else {
            m.content[0].content[0]
                .attrs
                .insert("pageGeometry".into(), p);
        }
        assert!(document::validate(&m).is_err());
    }
}
#[test]
fn page_margins_accept_zero_and_exact_minimum_content_boundary() {
    let mut m = model();
    m.attrs["pageGeometry"] = json!({"widthTwips":8391,"heightTwips":11906,"marginTopTwips":0,"marginRightTwips":3000,"marginBottomTwips":4320,"marginLeftTwips":3951});
    document::validate(&m).unwrap();
    m.attrs["pageGeometry"]["marginLeftTwips"] = json!(3952);
    assert!(document::validate(&m).is_err());
}
#[test]
fn export_and_trusted_reopen_preserve_exact_page_geometry_for_both_profiles() {
    let dir = tempfile::tempdir().unwrap();
    for modern in [false, true] {
        for landscape in [false, true] {
            let mut m = model();
            if !modern {
                m.attrs.remove("styleProfile");
            }
            if landscape {
                m.attrs["pageGeometry"]["widthTwips"] = json!(15840);
                m.attrs["pageGeometry"]["heightTwips"] = json!(12240);
            }
            let path = dir.path().join(format!("{modern}-{landscape}.docx"));
            export(&m, &path);
            assert_eq!(
                serde_json::to_value(document::load_authored_docx(&path).unwrap().unwrap())
                    .unwrap(),
                serde_json::to_value(&m).unwrap()
            );
            let parts = parts(&path);
            let xml = std::str::from_utf8(&parts["word/document.xml"]).unwrap();
            assert!(xml.contains(r#"w:top="720" w:right="1080" w:bottom="1440" w:left="1800""#));
            assert_eq!(xml.contains(r#"w:orient="landscape""#), landscape);
        }
    }
}
#[test]
fn page_draft_roundtrip_and_failed_candidate_preserve_prior_draft() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.json");
    let mut m = model();
    document::save_draft(&m, &path).unwrap();
    let before = fs::read(&path).unwrap();
    assert_eq!(
        serde_json::to_value(document::load_draft(&path).unwrap().unwrap()).unwrap(),
        serde_json::to_value(&m).unwrap()
    );
    m.attrs["pageGeometry"]["marginTopTwips"] = json!(4321);
    assert!(document::save_draft(&m, &path).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
}
fn table(width: Option<u64>, indent: Option<u64>) -> Node {
    let mut table:Node=serde_json::from_value(json!({"type":"table","content":[{"type":"tableRow","content":[{"type":"tableCell","content":[{"type":"paragraph"}]}]}]})).unwrap();
    if let Some(w) = width {
        table.attrs.insert("columnWidthsTwips".into(), json!([w]));
    }
    if let Some(i) = indent {
        table.attrs.insert("tableIndentTwips".into(), json!(i));
    }
    table
}
#[test]
fn table_and_indent_fit_checks_reject_without_resizing_including_auto_tables() {
    let mut m = model();
    m.content.push(table(Some(9360), None));
    document::validate(&m).unwrap();
    m.content[1].attrs["columnWidthsTwips"] = json!([9361]);
    assert!(document::validate(&m).is_err());
    m.content[1] = table(Some(9000), Some(360));
    document::validate(&m).unwrap();
    m.content[1].attrs["tableIndentTwips"] = json!(361);
    assert!(document::validate(&m).is_err());
    m.content[1] = table(None, Some(0));
    m.attrs["pageGeometry"]["marginRightTwips"] = json!(1441);
    assert!(document::validate(&m).is_err());
    m.attrs.remove("pageGeometry");
    document::validate(&m).unwrap();
}
#[test]
fn explicit_image_frames_fit_both_axes_and_legacy_omission_remains_unchanged() {
    let mut m = model();
    let images: Node =
        serde_json::from_str(include_str!("fixtures/word-image-legacy.json")).unwrap();
    let mut image = images.content[0].clone();
    image.attrs.insert("displayWidthEmu".into(), json!(1));
    image
        .attrs
        .insert("displayHeightEmu".into(), json!(7_620_000));
    m.content.push(image);
    document::validate(&m).unwrap();
    m.attrs["pageGeometry"]["heightTwips"] = json!(12240);
    m.attrs["pageGeometry"]["widthTwips"] = json!(15840);
    assert!(document::validate(&m).is_err());
    m.attrs.remove("pageGeometry");
    document::validate(&m).unwrap();
}
#[test]
fn natural_image_frame_page_check_does_not_add_explicit_dimensions_or_change_media() {
    let mut m: model_alias::Model =
        serde_json::from_str(include_str!("fixtures/word-image-legacy.json")).unwrap();
    m.attrs.insert("pageGeometry".into(), page());
    let before = serde_json::to_value(&m).unwrap();
    document::validate(&m).unwrap();
    assert_eq!(serde_json::to_value(&m).unwrap(), before);
}
mod model_alias {
    pub type Model = rust_wps::document::Node;
}
#[test]
fn failed_export_leaves_no_output_and_valid_export_never_clobbers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.docx");
    let mut m = model();
    m.attrs["pageGeometry"] = Value::Null;
    assert!(document::export_docx(&m, &path).is_err());
    assert!(!path.exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    export(&model(), &path);
    let before = fs::read(&path).unwrap();
    assert!(document::export_docx(&model(), &path).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
}
#[test]
fn package_page_tamper_cannot_be_trusted_even_with_refreshed_hash() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("original.docx");
    export(&model(), &original);
    let mut entries = parts(&original);
    let data = String::from_utf8(entries["word/document.xml"].clone())
        .unwrap()
        .replace("w:top=\"720\"", "w:top=\"721\"");
    entries.insert("word/document.xml".into(), data.into_bytes());
    let mut embedded: Value = serde_json::from_slice(&entries["rust-wps/model-v2.json"]).unwrap();
    embedded["hashes"]["word/document.xml"] = json!(format!(
        "{:x}",
        Sha256::digest(&entries["word/document.xml"])
    ));
    entries.insert(
        "rust-wps/model-v2.json".into(),
        serde_json::to_vec(&embedded).unwrap(),
    );
    let path = dir.path().join("tampered.docx");
    write(&path, &entries);
    assert!(document::load_authored_docx(&path).is_err());
}
#[test]
fn model_only_page_tamper_cannot_be_trusted() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("original.docx");
    export(&model(), &original);
    let mut entries = parts(&original);
    let mut embedded: Value = serde_json::from_slice(&entries["rust-wps/model-v2.json"]).unwrap();
    embedded["document"]["attrs"]["pageGeometry"]["marginTopTwips"] = json!(721);
    entries.insert(
        "rust-wps/model-v2.json".into(),
        serde_json::to_vec(&embedded).unwrap(),
    );
    let path = dir.path().join("tampered.docx");
    write(&path, &entries);
    assert!(document::load_authored_docx(&path).is_err());
}

#[test]
fn unpositioned_automatic_tables_reflow_only_with_explicit_pages() {
    let mut m = model();
    m.content.push(table(None, None));
    for (w, h, left, right, expected) in [
        (12240, 15840, 1800, 1800, 8640),
        (8391, 11906, 1080, 1080, 6231),
        (16838, 11906, 1440, 1440, 9000),
    ] {
        m.attrs["pageGeometry"] = json!({"widthTwips":w,"heightTwips":h,"marginTopTwips":1440,"marginRightTwips":right,"marginBottomTwips":1440,"marginLeftTwips":left});
        document::validate(&m).unwrap();
        let page = document::page_geometry(&m).unwrap().unwrap();
        assert_eq!(
            document::table_layout_width(&m.content[1], Some(&page)).unwrap(),
            expected
        );
        assert!(m.content[1].attrs.is_empty());
    }
    m.attrs.remove("pageGeometry");
    assert_eq!(
        document::table_layout_width(&m.content[1], None).unwrap(),
        9000
    );
}
#[test]
fn page_auto_normalization_exports_exact_remainder_and_reset_restores_historical_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = model();
    m.attrs.remove("pageGeometry");
    let mut t = table(None, None);
    let extra = t.content[0].content[0].clone();
    t.content[0].content.push(extra);
    m.content.push(t);
    let baseline = dir.path().join("baseline.docx");
    export(&m, &baseline);
    let before = fs::read(&baseline).unwrap();
    m.attrs.insert("pageGeometry".into(),json!({"widthTwips":8391,"heightTwips":11906,"marginTopTwips":1440,"marginRightTwips":1080,"marginBottomTwips":1440,"marginLeftTwips":1080}));
    let output = dir.path().join("a5.docx");
    export(&m, &output);
    let entries = parts(&output);
    let xml = std::str::from_utf8(&entries["word/document.xml"]).unwrap();
    assert!(xml.contains(r#"<w:tblW w:w="6231" w:type="dxa"/>"#));
    assert!(xml.contains(r#"<w:gridCol w:w="3116"/><w:gridCol w:w="3115"/>"#));
    assert!(xml.contains(r#"<w:tcW w:w="3116" w:type="dxa"/>"#));
    assert!(!xml.contains("<w:tblLayout"));
    assert_eq!(
        serde_json::to_value(document::load_authored_docx(&output).unwrap().unwrap()).unwrap(),
        serde_json::to_value(&m).unwrap()
    );
    m.attrs.remove("pageGeometry");
    let reset = dir.path().join("reset.docx");
    export(&m, &reset);
    assert_eq!(fs::read(reset).unwrap(), before);
}
#[test]
fn page_auto_normalization_never_rewrites_explicit_position_or_fixed_width() {
    let mut m = model();
    m.attrs["pageGeometry"]["marginLeftTwips"] = json!(1800);
    m.attrs["pageGeometry"]["marginRightTwips"] = json!(1800);
    for t in [table(Some(9000), None), table(None, Some(0))] {
        m.content = vec![t];
        assert!(document::validate(&m).is_err());
    }
    for align in ["left", "center", "right"] {
        let mut t = table(None, None);
        t.attrs.insert("tableAlign".into(), json!(align));
        m.content = vec![t];
        assert!(document::validate(&m).is_err());
    }
    m.content = vec![table(None, None)];
    document::validate(&m).unwrap();
}
#[test]
fn explicit_page_does_not_treat_container_width_as_body_width() {
    let mut m = model();
    let mut outer = table(None, None);
    outer.content[0].content[0].content.push(table(None, None));
    m.content = vec![outer];
    assert!(
        document::validate(&m)
            .unwrap_err()
            .to_string()
            .contains("top-level tables")
    );
    m.attrs.remove("pageGeometry");
    document::validate(&m).unwrap();
}
