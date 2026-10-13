use rust_wps::document::{self, Node};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read, Write},
    path::Path,
};

const ATTR: &str = "tableCellMarginsTwips";
fn model(margins: Option<Value>, fixed: bool, modern: bool) -> Node {
    let p = json!({"type":"paragraph","content":[{"type":"text","text":"Cell text"}]});
    let cell = json!({"type":"tableCell","content":[p]});
    let mut table = json!({"type":"table","attrs":{},"content":[
        {"type":"tableRow","content":[cell.clone(),cell.clone()]},
        {"type":"tableRow","content":[cell.clone(),cell]}
    ]});
    if let Some(margins) = margins {
        table["attrs"][ATTR] = margins;
    }
    if fixed {
        table["attrs"]["columnWidthsTwips"] = json!([1800, 4200]);
    }
    let mut doc = json!({"type":"doc","content":[table]});
    if modern {
        doc["attrs"] = json!({"styleProfile":"standard-v2"});
    }
    serde_json::from_value(doc).unwrap()
}
fn value(doc: &Node) -> Value {
    serde_json::to_value(doc).unwrap()
}
fn parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut e = zip.by_index(i).unwrap();
            let mut b = vec![];
            e.read_to_end(&mut b).unwrap();
            (e.name().into(), b)
        })
        .collect()
}
fn write_parts(path: &Path, parts: &BTreeMap<String, Vec<u8>>) {
    let mut z = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (n, b) in parts {
        z.start_file(n, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(b).unwrap();
    }
    z.finish().unwrap();
}
fn export(doc: &Node) -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("x.docx");
    document::export_docx(doc, &p).unwrap();
    fs::read(p).unwrap()
}
fn page(doc: &mut Node, width: u64, height: u64) {
    doc.attrs.insert("pageGeometry".into(),json!({"widthTwips":width,"heightTwips":height,"marginLeftTwips":1440,"marginRightTwips":1440,"marginTopTwips":1440,"marginBottomTwips":1440}));
}
#[test]
fn paired_physical_integer_margins_are_strict() {
    for m in [
        Value::Null,
        json!({}),
        json!([]),
        json!(108),
        json!({"left":0}),
        json!({"right":0}),
        json!({"left":0,"right":0,"top":0}),
    ] {
        assert!(
            document::validate(&model(Some(m.clone()), true, true)).is_err(),
            "{m}"
        );
    }
    for side in ["left", "right"] {
        for invalid in [
            Value::Null,
            json!(true),
            json!("108"),
            json!(-1),
            json!(1.0),
            json!(0.5),
            json!(31681),
            json!(u64::MAX),
            json!([]),
            json!({}),
        ] {
            let mut m = json!({"left":0,"right":0});
            m[side] = invalid;
            assert!(
                document::validate(&model(Some(m.clone()), true, true)).is_err(),
                "{m}"
            );
        }
    }
    for m in [
        json!({"left":0,"right":0}),
        json!({"left":0,"right":108}),
        json!({"left":180,"right":240}),
        json!({"left":1799,"right":0}),
    ] {
        document::validate(&model(Some(m), true, true)).unwrap();
    }
}
#[test]
fn margin_resource_bound_still_requires_positive_actual_cell_content() {
    for m in [
        json!({"left":900,"right":900}),
        json!({"left":31680,"right":0}),
        json!({"left":1,"right":1799}),
    ] {
        assert!(document::validate(&model(Some(m), true, false)).is_err());
    }
    let mut doc = model(Some(json!({"left":31679,"right":0})), true, false);
    doc.content[0]
        .attrs
        .insert("columnWidthsTwips".into(), json!([31680]));
    for row in &mut doc.content[0].content {
        row.content.truncate(1);
    }
    document::validate(&doc).unwrap();
}
#[test]
fn margins_cannot_be_misplaced_or_attached_to_list_or_nested_tables() {
    let original = model(None, false, false);
    for path in [
        vec![],
        vec![0, 0],
        vec![0, 0, 0],
        vec![0, 0, 0, 0],
        vec![0, 0, 0, 0, 0],
    ] {
        let mut doc = original.clone();
        let mut node = &mut doc;
        for i in path {
            node = &mut node.content[i];
        }
        node.attrs.insert(ATTR.into(), json!({"left":0,"right":0}));
        assert!(document::validate(&doc).is_err());
    }
    let with = model(Some(json!({"left":180,"right":240})), false, false)
        .content
        .remove(0);
    for outer_has in [false, true] {
        let mut doc = if outer_has {
            model(Some(json!({"left":0,"right":0})), false, false)
        } else {
            original.clone()
        };
        doc.content[0].content[0].content[0]
            .content
            .push(if outer_has {
                original.content[0].clone()
            } else {
                with.clone()
            });
        assert!(document::validate(&doc).is_err());
    }
    let mut doc = original;
    doc.content=vec![serde_json::from_value(json!({"type":"bulletList","content":[{"type":"listItem","content":[{"type":"paragraph"},value(&with)]}]})).unwrap()];
    assert!(document::validate(&doc).is_err());
}
#[test]
fn explicit_margins_preserve_simple_rectangular_structure_guards() {
    let original = model(Some(json!({"left":0,"right":0})), false, false);
    let mut ragged = original.clone();
    ragged.content[0].content[1].content.pop();
    assert!(document::validate(&ragged).is_err());
    for (key, val) in [
        ("colspan", json!(2)),
        ("rowspan", json!(2)),
        ("colwidth", json!([10])),
    ] {
        let mut doc = original.clone();
        doc.content[0].content[0].content[0]
            .attrs
            .insert(key.into(), val);
        assert!(document::validate(&doc).is_err());
    }
}
#[test]
fn exact_margin_xml_has_no_global_settings_or_indent_translation() {
    for modern in [false, true] {
        for fixed in [false, true] {
            for (left, right) in [(0, 0), (0, 108), (180, 240), (108, 108)] {
                let mut doc = model(Some(json!({"left":left,"right":right})), fixed, modern);
                doc.content[0]
                    .attrs
                    .insert("tableIndentTwips".into(), json!(720));
                let actual = parts(&export(&doc));
                let xml = String::from_utf8(actual["word/document.xml"].clone()).unwrap();
                assert!(xml.contains(&format!(r#"<w:tblCellMar><w:left w:w="{left}" w:type="dxa"/><w:right w:w="{right}" w:type="dxa"/></w:tblCellMar>"#)));
                assert!(xml.contains(r#"<w:tblInd w:w="720" w:type="dxa"/>"#));
                assert!(!xml.contains("<w:tcMar>"));
                let mut before = doc.clone();
                before.content[0].attrs.remove(ATTR);
                let baseline = parts(&export(&before));
                assert_eq!(
                    actual.get("word/settings.xml"),
                    baseline.get("word/settings.xml")
                );
                for (name, data) in &baseline {
                    if !name.contains("model-v") && name != "word/document.xml" {
                        assert_eq!(actual.get(name), Some(data), "{name}");
                    }
                }
            }
        }
    }
}
#[test]
fn absent_zero_and_explicit_default_margins_survive_draft_and_trusted_reload() {
    let dir = tempfile::tempdir().unwrap();
    for modern in [false, true] {
        for (i, margins) in [
            None,
            Some(json!({"left":0,"right":0})),
            Some(json!({"left":108,"right":108})),
            Some(json!({"left":180,"right":240})),
        ]
        .into_iter()
        .enumerate()
        {
            let doc = model(margins, true, modern);
            let draft = dir.path().join(format!("{modern}-{i}.json"));
            document::save_draft(&doc, &draft).unwrap();
            assert_eq!(
                value(&document::load_draft(&draft).unwrap().unwrap()),
                value(&doc)
            );
            let path = draft.with_extension("docx");
            document::export_docx(&doc, &path).unwrap();
            assert_eq!(
                value(&document::load_authored_docx(&path).unwrap().unwrap()),
                value(&doc)
            );
            let bytes = fs::read(&path).unwrap();
            assert!(document::export_docx(&doc, &path).is_err());
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
    }
}
#[test]
fn resetting_margins_restores_exact_historical_document_bytes() {
    for modern in [false, true] {
        for fixed in [false, true] {
            for position in [false, true] {
                let mut old = model(None, fixed, modern);
                if position {
                    old.content[0]
                        .attrs
                        .insert("tableIndentTwips".into(), json!(0));
                }
                let bytes = export(&old);
                let mut changed = old.clone();
                changed.content[0]
                    .attrs
                    .insert(ATTR.into(), json!({"left":0,"right":240}));
                assert_ne!(export(&changed), bytes);
                changed.content[0].attrs.remove(ATTR);
                assert_eq!(export(&changed), bytes);
            }
        }
    }
}
#[test]
fn invalid_margins_never_replace_draft_or_create_output() {
    let dir = tempfile::tempdir().unwrap();
    let draft = dir.path().join("draft.json");
    let output = dir.path().join("bad.docx");
    let good = model(Some(json!({"left":0,"right":0})), true, true);
    document::save_draft(&good, &draft).unwrap();
    let original = fs::read(&draft).unwrap();
    let bad = model(Some(json!({"left":1800,"right":0})), true, true);
    assert!(document::save_draft(&bad, &draft).is_err());
    assert_eq!(fs::read(draft).unwrap(), original);
    assert!(document::export_docx(&bad, &output).is_err());
    assert!(!output.exists());
}
#[test]
fn changed_margin_model_and_rehashed_xml_cannot_pass_trusted_reload() {
    let dir = tempfile::tempdir().unwrap();
    for modern in [false, true] {
        let doc = model(Some(json!({"left":180,"right":240})), true, modern);
        let original = parts(&export(&doc));
        let key = if modern {
            "rust-wps/model-v2.json"
        } else {
            "rust-wps/model-v1.json"
        };
        for target in ["model", "xml"] {
            let mut changed = original.clone();
            if target == "model" {
                let mut m: Value = serde_json::from_slice(&changed[key]).unwrap();
                m["document"]["content"][0]["attrs"][ATTR]["left"] = json!(181);
                changed.insert(key.into(), serde_json::to_vec(&m).unwrap());
            } else {
                use sha2::{Digest, Sha256};
                let xml = String::from_utf8(changed["word/document.xml"].clone())
                    .unwrap()
                    .replace(r#"w:left w:w="180""#, r#"w:left w:w="181""#);
                let bytes = xml.into_bytes();
                let hash = format!("{:x}", Sha256::digest(&bytes));
                changed.insert("word/document.xml".into(), bytes);
                let mut m: Value = serde_json::from_slice(&changed[key]).unwrap();
                m["hashes"]["word/document.xml"] = json!(hash);
                changed.insert(key.into(), serde_json::to_vec(&m).unwrap());
            }
            let path = dir.path().join(format!("{modern}-{target}.docx"));
            write_parts(&path, &changed);
            assert!(document::load_authored_docx(&path).is_err());
        }
    }
}
#[test]
fn cell_paragraph_line_frames_use_custom_not_default_margins() {
    let mut doc = model(Some(json!({"left":0,"right":0})), true, true);
    doc.content[0].content[0].content[0].content[0]
        .attrs
        .insert("indentLeftTwips".into(), json!(1799));
    document::validate(&doc).unwrap();
    doc.content[0]
        .attrs
        .insert(ATTR.into(), json!({"left":1,"right":0}));
    assert!(document::validate(&doc).is_err());
    doc.content[0].content[0].content[0].content[0]
        .attrs
        .insert("indentLeftTwips".into(), json!(1000));
    doc.content[0].content[0].content[0].content[0]
        .attrs
        .insert("firstLineIndentTwips".into(), json!(379));
    doc.content[0]
        .attrs
        .insert(ATTR.into(), json!({"left":180,"right":240}));
    document::validate(&doc).unwrap();
    doc.content[0].content[0].content[0].content[0]
        .attrs
        .insert("firstLineIndentTwips".into(), json!(380));
    assert!(document::validate(&doc).is_err());
}
#[test]
fn narrowing_page_or_column_rechecks_margins_and_existing_paragraph_indents() {
    let mut doc = model(Some(json!({"left":180,"right":240})), true, true);
    doc.content[0].content[0].content[0].content[0]
        .attrs
        .insert("indentLeftTwips".into(), json!(1000));
    document::validate(&doc).unwrap();
    doc.content[0]
        .attrs
        .insert("columnWidthsTwips".into(), json!([1420, 4200]));
    assert!(document::validate(&doc).is_err());
    let mut auto = model(Some(json!({"left":1380,"right":1380})), false, true);
    page(&mut auto, 12240, 15840);
    document::validate(&auto).unwrap();
    page(&mut auto, 8391, 11906);
    assert!(document::validate(&auto).is_err());
    let mut auto = model(Some(json!({"left":180,"right":240})), false, true);
    page(&mut auto, 12240, 15840);
    auto.content[0].content[0].content[1].content[0]
        .attrs
        .insert("indentLeftTwips".into(), json!(2400));
    document::validate(&auto).unwrap();
    page(&mut auto, 8391, 11906);
    assert!(document::validate(&auto).is_err());
}
#[test]
fn normalized_auto_grid_remainder_is_used_for_margin_fit() {
    let mut doc = model(Some(json!({"left":2755,"right":0})), false, true);
    page(&mut doc, 8391, 11906);
    // A5 body5511 => columns2756/2755. Every column, not just first, must fit.
    assert!(document::validate(&doc).is_err());
    doc.content[0]
        .attrs
        .insert(ATTR.into(), json!({"left":2754,"right":0}));
    document::validate(&doc).unwrap();
}
#[test]
fn mixed_tables_and_unrelated_paragraphs_keep_their_own_margin_metadata() {
    let mut doc = model(None, true, true);
    let mut custom = model(Some(json!({"left":0,"right":240})), true, true)
        .content
        .remove(0);
    custom.attrs.insert("tableIndentTwips".into(), json!(0));
    doc.content.push(custom);
    doc.content.push(serde_json::from_value(json!({"type":"paragraph","attrs":{"indentLeftTwips":360},"content":[{"type":"text","text":"Unrelated"}]})).unwrap());
    let xml = String::from_utf8(parts(&export(&doc))["word/document.xml"].clone()).unwrap();
    assert_eq!(xml.matches("<w:tblCellMar>").count(), 1);
    assert_eq!(xml.matches(r#"<w:ind w:left="360"/>"#).count(), 1);
}
