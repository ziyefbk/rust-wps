use rust_wps::document::{self, Node};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read, Write},
    path::Path,
};
fn model(attrs: Value) -> Node {
    serde_json::from_value(json!({"type":"doc","content":[{"type":"paragraph","attrs":attrs,"content":[{"type":"text","text":"Indent probe"}]}]})).unwrap()
}
fn table(attrs: Value, columns: usize, width: Option<Vec<u64>>) -> Node {
    let mut doc = model(json!({}));
    let p = json!({"type":"paragraph","attrs":attrs,"content":[{"type":"text","text":"Cell"}]});
    let mut table = json!({"type":"table","content":[{"type":"tableRow","content":(0..columns).map(|_|json!({"type":"tableCell","content":[p.clone()]})).collect::<Vec<_>>()}]});
    if let Some(width) = width {
        table["attrs"] = json!({"columnWidthsTwips":width});
    }
    doc.content = vec![serde_json::from_value(table).unwrap()];
    doc
}
fn page(doc: &mut Node, width: u64, height: u64) {
    doc.attrs.insert("pageGeometry".into(),json!({"widthTwips":width,"heightTwips":height,"marginLeftTwips":1440,"marginRightTwips":1440,"marginTopTwips":1440,"marginBottomTwips":1440}));
}
fn parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut z = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..z.len())
        .map(|i| {
            let mut e = z.by_index(i).unwrap();
            let mut d = Vec::new();
            e.read_to_end(&mut d).unwrap();
            (e.name().into(), d)
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
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("x.docx");
    document::export_docx(doc, &p).unwrap();
    fs::read(p).unwrap()
}
#[test]
fn canonical_types_are_strict() {
    for key in document::PARAGRAPH_INDENT_FIELDS {
        for v in [
            Value::Null,
            json!(true),
            json!(1.2),
            json!("20"),
            json!([]),
            json!(31681),
            json!(-31681),
        ] {
            assert!(
                document::validate(&model(json!({key:v}))).is_err(),
                "{key} {v}"
            );
        }
    }
}
#[test]
fn side_indents_cannot_be_negative() {
    for key in ["indentLeftTwips", "indentRightTwips"] {
        assert!(document::validate(&model(json!({key:-1}))).is_err());
    }
}
#[test]
fn attributes_require_paragraphs_or_headings() {
    let mut d = model(json!({}));
    for kind in ["doc", "text", "table", "listItem", "hardBreak"] {
        d.content[0].kind = kind.into();
        d.content[0]
            .attrs
            .insert("indentLeftTwips".into(), json!(0));
        assert!(document::validate(&d).is_err());
    }
}
#[test]
fn first_and_subsequent_lines_must_fit_separately() {
    for attrs in [
        json!({"indentLeftTwips":9026}),
        json!({"indentRightTwips":9026}),
        json!({"firstLineIndentTwips":9026}),
        json!({"firstLineIndentTwips":-1}),
        json!({"indentLeftTwips":100,"firstLineIndentTwips":-101}),
        json!({"indentLeftTwips":9026,"firstLineIndentTwips":-100}),
    ] {
        assert!(document::validate(&model(attrs)).is_err());
    }
    for attrs in [
        json!({"indentLeftTwips":9025}),
        json!({"firstLineIndentTwips":9025}),
        json!({"indentLeftTwips":720,"firstLineIndentTwips":-720}),
        json!({"indentLeftTwips":100,"indentRightTwips":8900,"firstLineIndentTwips":25}),
    ] {
        document::validate(&model(attrs)).unwrap();
    }
}
#[test]
fn page_changes_validate_existing_indentation() {
    let mut d = model(json!({"indentLeftTwips":6000}));
    document::validate(&d).unwrap();
    page(&mut d, 8391, 11906);
    assert!(document::validate(&d).is_err());
    page(&mut d, 12240, 15840);
    document::validate(&d).unwrap();
}
#[test]
fn fixed_cell_context_subtracts_margins() {
    document::validate(&table(
        json!({"indentLeftTwips":783}),
        2,
        Some(vec![1000, 2000]),
    ))
    .unwrap();
    assert!(
        document::validate(&table(
            json!({"indentLeftTwips":784}),
            2,
            Some(vec![1000, 2000])
        ))
        .is_err()
    );
    assert!(document::validate(&table(json!({"indentLeftTwips":0}), 1, Some(vec![216]))).is_err());
}
#[test]
fn auto_cell_context_matches_exact_legacy_floor() {
    document::validate(&table(json!({"indentLeftTwips":1068}), 7, None)).unwrap();
    assert!(document::validate(&table(json!({"indentLeftTwips":1069}), 7, None)).is_err());
}
#[test]
fn auto_cell_context_tracks_explicit_page() {
    let mut d = table(json!({"indentLeftTwips":4200}), 2, None);
    document::validate(&d).unwrap();
    page(&mut d, 12240, 15840);
    d.attrs.get_mut("pageGeometry").unwrap()["marginLeftTwips"] = json!(1800);
    d.attrs.get_mut("pageGeometry").unwrap()["marginRightTwips"] = json!(1800);
    assert!(document::validate(&d).is_err());
}
#[test]
fn lists_do_not_accept_body_indentation() {
    let mut d = model(json!({"indentLeftTwips":0}));
    d.content = vec![
        serde_json::from_value(
            json!({"type":"bulletList","content":[{"type":"listItem","content":d.content}]}),
        )
        .unwrap(),
    ];
    assert!(document::validate(&d).is_err());
    d.content[0].content[0].content[0].attrs.clear();
    document::validate(&d).unwrap();
}
#[test]
fn nested_tables_do_not_use_root_width() {
    let mut d = table(json!({"indentLeftTwips":0}), 1, None);
    let inner = table(json!({}), 1, None).content.remove(0);
    d.content[0].content[0].content[0].content.push(inner);
    assert!(document::validate(&d).is_err());
}
#[test]
fn explicit_zero_and_signed_values_roundtrip_with_drafts() {
    let dir = tempfile::tempdir().unwrap();
    for (index, attrs) in [
        json!({"indentLeftTwips":0,"indentRightTwips":0,"firstLineIndentTwips":0}),
        json!({"indentLeftTwips":721,"indentRightTwips":239,"firstLineIndentTwips":-360}),
        json!({"firstLineIndentTwips":241}),
    ]
    .into_iter()
    .enumerate()
    {
        for modern in [false, true] {
            let mut d = model(attrs.clone());
            if modern {
                d.attrs.insert("styleProfile".into(), json!("standard-v2"));
            }
            let draft = dir.path().join(format!("{index}-{modern}.json"));
            document::save_draft(&d, &draft).unwrap();
            assert_eq!(
                serde_json::to_value(document::load_draft(&draft).unwrap().unwrap()).unwrap(),
                serde_json::to_value(&d).unwrap()
            );
            let output = draft.with_extension("docx");
            document::export_docx(&d, &output).unwrap();
            assert_eq!(
                serde_json::to_value(document::load_authored_docx(&output).unwrap().unwrap())
                    .unwrap(),
                serde_json::to_value(&d).unwrap()
            );
            let bytes = fs::read(&output).unwrap();
            assert!(document::export_docx(&d, &output).is_err());
            assert_eq!(bytes, fs::read(&output).unwrap());
        }
    }
}
#[test]
fn export_has_one_physical_indent_and_canonical_first_slot() {
    for (attrs, expected) in [
        (
            json!({"indentLeftTwips":0,"indentRightTwips":20,"firstLineIndentTwips":0}),
            r#"<w:ind w:left="0" w:right="20" w:firstLine="0"/>"#,
        ),
        (
            json!({"indentLeftTwips":720,"firstLineIndentTwips":-360}),
            r#"<w:ind w:left="720" w:hanging="360"/>"#,
        ),
    ] {
        let p = parts(&export(&model(attrs)));
        let xml = String::from_utf8(p["word/document.xml"].clone()).unwrap();
        assert!(xml.contains(expected));
        assert_eq!(xml.matches("<w:ind ").count(), 1);
    }
}
#[test]
fn indented_tables_pin_cell_margins_only_in_new_path() {
    let old = parts(&export(&table(json!({}), 2, None)));
    let new = parts(&export(&table(json!({"indentLeftTwips":0}), 2, None)));
    assert!(!String::from_utf8_lossy(&old["word/document.xml"]).contains("tblCellMar"));
    assert!(String::from_utf8_lossy(&new["word/document.xml"]).contains(r#"<w:tblCellMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar>"#));
}
#[test]
fn invalid_indent_never_replaces_existing_draft_or_creates_output() {
    let dir = tempfile::tempdir().unwrap();
    let draft = dir.path().join("draft.json");
    let good = model(json!({"indentLeftTwips":100}));
    document::save_draft(&good, &draft).unwrap();
    let bytes = fs::read(&draft).unwrap();
    let bad = model(json!({"indentLeftTwips":9026}));
    assert!(document::save_draft(&bad, &draft).is_err());
    assert_eq!(bytes, fs::read(&draft).unwrap());
    let output = dir.path().join("bad.docx");
    assert!(document::export_docx(&bad, &output).is_err());
    assert!(!output.exists());
}
#[test]
fn forged_model_indent_fails_full_reencoding() {
    let dir = tempfile::tempdir().unwrap();
    for modern in [false, true] {
        let mut d = model(json!({"indentLeftTwips":720,"firstLineIndentTwips":-360}));
        if modern {
            d.attrs.insert("styleProfile".into(), json!("standard-v2"));
        }
        let mut p = parts(&export(&d));
        let name = if modern {
            "rust-wps/model-v2.json"
        } else {
            "rust-wps/model-v1.json"
        };
        let mut m: Value = serde_json::from_slice(&p[name]).unwrap();
        m["document"]["content"][0]["attrs"]["indentLeftTwips"] = json!(721);
        p.insert(name.into(), serde_json::to_vec(&m).unwrap());
        let path = dir.path().join(format!("{modern}.docx"));
        write_parts(&path, &p);
        assert!(document::load_authored_docx(&path).is_err());
    }
}

#[test]
fn tab_stops_are_not_claimed_by_new_indentation_but_hard_breaks_remain_supported() {
    let mut d = model(json!({}));
    d.content[0].content[0].text = Some("Left\tRight".into());
    document::validate(&d).unwrap();
    d.content[0]
        .attrs
        .insert("indentLeftTwips".into(), json!(0));
    assert!(document::validate(&d).is_err());
    d.content[0].content[0].text = Some("Left".into());
    d.content[0]
        .content
        .push(serde_json::from_value(json!({"type":"hardBreak"})).unwrap());
    d.content[0]
        .content
        .push(serde_json::from_value(json!({"type":"text","text":"Right"})).unwrap());
    document::validate(&d).unwrap();
    export(&d);
}

#[test]
fn new_indent_properties_use_spacing_indent_alignment_schema_order() {
    let d = model(json!({"indentLeftTwips":0,"spaceAfter":0,"textAlign":"right"}));
    let p = parts(&export(&d));
    let xml = String::from_utf8_lossy(&p["word/document.xml"]);
    assert!(xml.contains(
        r#"<w:pPr><w:spacing w:after="0"/><w:ind w:left="0"/><w:jc w:val="right"/></w:pPr>"#
    ));
}
