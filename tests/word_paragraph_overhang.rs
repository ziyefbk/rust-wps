//! Bounded physical-page overhang, deliberately excluding cells/lists/images.
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
const RISK: &str = "Unsupported paragraph indentation cannot be preserved in an editable copy";
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
fn page() -> Value {
    json!({"widthTwips":11906,"heightTwips":16838,"marginLeftTwips":1417,"marginRightTwips":1417,"marginTopTwips":1417,"marginBottomTwips":1134})
}
fn model(attrs: Value) -> Node {
    serde_json::from_value(json!({"type":"doc","attrs":{"pageGeometry":page()},"content":[{"type":"paragraph","attrs":attrs,"content":[{"type":"text","text":"Margin overhang"}]}]})).unwrap()
}
fn parts(path: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut z = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    (0..z.len())
        .map(|i| {
            let mut e = z.by_index(i).unwrap();
            let mut b = vec![];
            e.read_to_end(&mut b).unwrap();
            (e.name().into(), b)
        })
        .collect()
}
fn zip(path: &Path, parts: &BTreeMap<String, Vec<u8>>) {
    let mut z = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (n, b) in parts {
        z.start_file(n, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(b).unwrap();
    }
    z.finish().unwrap();
}
fn source(body: &str, styles: &str, extra: &str) -> BTreeMap<String, Vec<u8>> {
    let r = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    let p = "http://schemas.openxmlformats.org/package/2006/relationships";
    BTreeMap::from([
        ("[Content_Types].xml".into(), br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_vec()),
        ("_rels/.rels".into(),format!(r#"<Relationships xmlns="{p}"><Relationship Id="main" Type="{r}/officeDocument" Target="word/document.xml"/></Relationships>"#).into_bytes()),
        ("word/document.xml".into(),format!(r#"<w:document xmlns:w="{W}"><w:body>{body}{extra}</w:body></w:document>"#).into_bytes()),
        ("word/_rels/document.xml.rels".into(),format!(r#"<Relationships xmlns="{p}"><Relationship Id="styles" Type="{r}/styles" Target="styles.xml"/><Relationship Id="numbering" Type="{r}/numbering" Target="numbering.xml"/></Relationships>"#).into_bytes()),
        ("word/styles.xml".into(),format!(r#"<w:styles xmlns:w="{W}">{styles}</w:styles>"#).into_bytes()),
        ("word/numbering.xml".into(),format!(r#"<w:numbering xmlns:w="{W}"/>"#).into_bytes()),
    ])
}
fn section() -> &'static str {
    r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:left="1417" w:right="1417" w:top="1417" w:bottom="1134"/></w:sectPr>"#
}
fn para(props: &str) -> String {
    format!("<w:p><w:pPr>{props}</w:pPr><w:r><w:t>Margin overhang</w:t></w:r></w:p>")
}
fn with_source<T>(parts: &BTreeMap<String, Vec<u8>>, f: impl FnOnce(&Path) -> T) -> T {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("source.docx");
    zip(&p, parts);
    let before = fs::read(&p).unwrap();
    let result = f(&p);
    assert_eq!(before, fs::read(&p).unwrap());
    assert_eq!(fs::read_dir(d.path()).unwrap().count(), 1);
    result
}
fn accepted(parts: &BTreeMap<String, Vec<u8>>) -> Node {
    with_source(parts, |p| {
        let audit = compatibility::audit_docx(p).unwrap();
        assert!(!audit.risks.contains(RISK), "{:?}", audit.risks);
        importing::import_docx(p).unwrap().document
    })
}
fn refused(parts: &BTreeMap<String, Vec<u8>>) {
    with_source(parts, |p| {
        assert!(compatibility::audit_docx(p).unwrap().risks.contains(RISK));
        assert!(importing::import_docx(p).is_err());
    })
}
#[test]
fn exact_physical_edges_are_valid_for_body_and_heading() {
    for attrs in [
        json!({"indentLeftTwips":-1417}),
        json!({"indentRightTwips":-1417}),
        json!({"firstLineIndentTwips":-1417}),
        json!({"indentLeftTwips":-567,"firstLineIndentTwips":-850}),
        json!({"indentLeftTwips":-1417,"indentRightTwips":-1417,"firstLineIndentTwips":11905}),
    ] {
        let mut d = model(attrs);
        document::validate(&d).unwrap();
        d.content[0].kind = "heading".into();
        d.content[0].attrs.insert("level".into(), json!(2));
        document::validate(&d).unwrap();
    }
}
#[test]
fn each_physical_edge_and_line_width_rejects_one_twip_over() {
    for attrs in [
        json!({"indentLeftTwips":-1418}),
        json!({"indentRightTwips":-1418}),
        json!({"firstLineIndentTwips":-1418}),
        json!({"indentLeftTwips":-567,"firstLineIndentTwips":-851}),
        json!({"indentLeftTwips":-1417,"indentRightTwips":-1417,"firstLineIndentTwips":11906}),
        json!({"indentLeftTwips":10489,"indentRightTwips":-1417}),
    ] {
        assert!(document::validate(&model(attrs)).is_err());
    }
}
#[test]
fn signed_components_still_require_strict_bounded_integers() {
    for key in document::PARAGRAPH_INDENT_FIELDS {
        for value in [
            json!(-31681),
            json!(31681),
            json!(-1.5),
            json!("-1"),
            Value::Null,
            json!(false),
        ] {
            assert!(document::validate(&model(json!({key:value}))).is_err());
        }
    }
}
#[test]
fn absent_or_invalid_page_never_proves_overhang() {
    for attrs in [
        json!({"indentLeftTwips":-1}),
        json!({"indentRightTwips":-1}),
        json!({"firstLineIndentTwips":-1}),
    ] {
        let mut d = model(attrs);
        d.attrs.clear();
        assert!(document::validate(&d).is_err());
        d.attrs.insert("pageGeometry".into(), page());
        d.attrs.get_mut("pageGeometry").unwrap()["marginLeftTwips"] = json!(-1);
        assert!(document::validate(&d).is_err());
    }
}
#[test]
fn page_margin_edits_recheck_both_sides_without_clamping() {
    for (field, attrs) in [
        ("marginLeftTwips", json!({"indentLeftTwips":-1417})),
        ("marginLeftTwips", json!({"firstLineIndentTwips":-1417})),
        ("marginRightTwips", json!({"indentRightTwips":-1417})),
    ] {
        let mut d = model(attrs);
        document::validate(&d).unwrap();
        d.attrs.get_mut("pageGeometry").unwrap()[field] = json!(1416);
        assert!(document::validate(&d).is_err());
    }
}
#[test]
fn signed_cell_and_list_indentation_never_borrows_page_margins() {
    for attrs in [
        json!({"indentLeftTwips":-1}),
        json!({"indentRightTwips":-1}),
        json!({"firstLineIndentTwips":-1}),
    ] {
        let d = model(attrs);
        for wrapped in [
            json!({"type":"table","content":[{"type":"tableRow","content":[{"type":"tableCell","content":d.content}]}]}),
            json!({"type":"bulletList","content":[{"type":"listItem","content":d.content}]}),
        ] {
            let mut d = d.clone();
            d.content = vec![serde_json::from_value(wrapped).unwrap()];
            assert!(document::validate(&d).is_err());
        }
    }
}
#[test]
fn signed_body_values_roundtrip_draft_and_trusted_docx_both_profiles() {
    let dir = tempfile::tempdir().unwrap();
    for modern in [false, true] {
        let mut d = model(
            json!({"indentLeftTwips":-567,"indentRightTwips":-1417,"firstLineIndentTwips":-567}),
        );
        if modern {
            d.attrs.insert("styleProfile".into(), json!("standard-v2"));
        }
        let draft = dir.path().join(format!("{modern}.json"));
        document::save_draft(&d, &draft).unwrap();
        assert_eq!(
            serde_json::to_value(document::load_draft(&draft).unwrap().unwrap()).unwrap(),
            serde_json::to_value(&d).unwrap()
        );
        let p = draft.with_extension("docx");
        document::export_docx(&d, &p).unwrap();
        assert_eq!(
            serde_json::to_value(document::load_authored_docx(&p).unwrap().unwrap()).unwrap(),
            serde_json::to_value(&d).unwrap()
        );
        let part = parts(&p);
        let xml = String::from_utf8_lossy(&part["word/document.xml"]);
        assert!(xml.contains(r#"<w:ind w:left="-567" w:right="-1417" w:hanging="567"/>"#));
        let before = fs::read(&p).unwrap();
        assert!(document::export_docx(&d, &p).is_err());
        assert_eq!(before, fs::read(&p).unwrap());
    }
}
#[test]
fn invalid_overhang_cannot_change_draft_or_create_export() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("draft.json");
    document::save_draft(&model(json!({"indentLeftTwips":-1417})), &p).unwrap();
    let before = fs::read(&p).unwrap();
    let bad = model(json!({"indentLeftTwips":-1418}));
    assert!(document::save_draft(&bad, &p).is_err());
    assert_eq!(before, fs::read(p).unwrap());
    let out = d.path().join("bad.docx");
    assert!(document::export_docx(&bad, &out).is_err());
    assert!(!out.exists());
}
#[test]
fn tampered_negative_model_or_page_fails_full_reencoding() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("good.docx");
    document::export_docx(&model(json!({"indentLeftTwips":-567})), &p).unwrap();
    for field in ["indent", "page"] {
        let mut parts = parts(&p);
        let mut m: Value = serde_json::from_slice(&parts["rust-wps/model-v1.json"]).unwrap();
        if field == "indent" {
            m["document"]["content"][0]["attrs"]["indentLeftTwips"] = json!(-568);
        } else {
            m["document"]["attrs"]["pageGeometry"]["marginLeftTwips"] = json!(1418);
        }
        parts.insert(
            "rust-wps/model-v1.json".into(),
            serde_json::to_vec(&m).unwrap(),
        );
        let p = d.path().join(format!("{field}.docx"));
        zip(&p, &parts);
        assert!(document::load_authored_docx(&p).is_err());
    }
}
#[test]
fn direct_signed_source_preserves_exact_components() {
    for (ind, attrs) in [
        (
            r#"<w:ind w:left="-1134" w:right="-567" w:firstLine="567"/>"#,
            json!({"indentLeftTwips":-1134,"indentRightTwips":-567,"firstLineIndentTwips":567}),
        ),
        (
            r#"<w:ind w:left="-567" w:hanging="567"/>"#,
            json!({"indentLeftTwips":-567,"firstLineIndentTwips":-567}),
        ),
        (
            r#"<w:ind w:hanging="1417"/>"#,
            json!({"firstLineIndentTwips":-1417}),
        ),
    ] {
        let d = accepted(&source(&para(ind), "", section()));
        for (key, v) in attrs.as_object().unwrap() {
            assert_eq!(&d.content[0].attrs[key], v);
        }
    }
}
#[test]
fn inherited_signed_values_and_explicit_zero_remain_fieldwise() {
    let styles = r#"<w:docDefaults><w:pPrDefault><w:pPr><w:ind w:left="-1134" w:right="-567" w:hanging="200"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:styleId="Over"><w:pPr><w:ind w:left="-567"/></w:pPr></w:style>"#;
    let d = accepted(&source(
        &para(r#"<w:pStyle w:val="Over"/><w:ind w:firstLine="0"/>"#),
        styles,
        section(),
    ));
    assert_eq!(d.content[0].attrs["indentLeftTwips"], -567);
    assert_eq!(d.content[0].attrs["indentRightTwips"], -567);
    assert_eq!(d.content[0].attrs["firstLineIndentTwips"], 0);
}
#[test]
fn signed_source_requires_complete_proved_page_and_physical_fit() {
    for (ind, sect) in [
        (r#"<w:ind w:left="-1"/>"#, "".to_owned()),
        (
            r#"<w:ind w:right="-1"/>"#,
            section().replace(" w:left=\"1417\"", ""),
        ),
        (r#"<w:ind w:left="-1418"/>"#, section().to_owned()),
        (r#"<w:ind w:right="-1418"/>"#, section().to_owned()),
        (
            r#"<w:ind w:left="-567" w:hanging="851"/>"#,
            section().to_owned(),
        ),
        (
            r#"<w:ind w:left="-1417" w:right="-1417" w:firstLine="11906"/>"#,
            section().to_owned(),
        ),
    ] {
        refused(&source(&para(ind), "", &sect));
    }
}
#[test]
fn signed_source_keeps_direction_tab_units_and_conflict_refusals() {
    for props in [
        r#"<w:ind w:left="-10"/><w:bidi/>"#,
        r#"<w:ind w:left="-10"/><w:mirrorIndents/>"#,
        r#"<w:ind w:left="-10"/><w:tabs><w:tab w:val="left" w:pos="500"/></w:tabs>"#,
        r#"<w:ind w:left="-10" w:firstLine="0" w:hanging="0"/>"#,
        r#"<w:ind w:left="-1.5"/>"#,
        r#"<w:ind w:left="-1pt"/>"#,
        r#"<w:ind w:start="-1"/>"#,
        r#"<w:ind w:left="-1" w:firstLine="-1"/>"#,
        r#"<w:ind w:left="-1" w:hanging="-1"/>"#,
    ] {
        refused(&source(&para(props), "", section()));
    }
}
#[test]
fn negative_list_paragraph_and_numbering_level_keep_prior_refusal() {
    let num = r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr>"#;
    for side in ["left", "right"] {
        let definition = format!(
            r#"<w:numbering xmlns:w="{W}"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:pPr><w:ind w:{side}="-1"/></w:pPr></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#
        );
        let mut p = source(&para(num), "", section());
        p.insert("word/numbering.xml".into(), definition.into_bytes());
        refused(&p);
        refused(&source(
            &para(&format!(r#"{num}<w:ind w:{side}="-1"/>"#)),
            "",
            section(),
        ));
    }
}
#[test]
fn negative_cells_and_overridden_inherited_negative_cells_still_refuse() {
    let styles = r#"<w:style w:type="paragraph" w:styleId="Over"><w:pPr><w:ind w:left="-1"/></w:pPr></w:style>"#;
    for props in [
        r#"<w:ind w:left="-1"/>"#,
        r#"<w:ind w:right="-1"/>"#,
        r#"<w:pStyle w:val="Over"/><w:ind w:left="0"/>"#,
    ] {
        let body = format!(
            r#"<w:tbl><w:tblPr><w:tblW w:type="auto" w:w="0"/></w:tblPr><w:tblGrid><w:gridCol w:w="9000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:type="auto" w:w="0"/></w:tcPr>{}</w:tc></w:tr></w:tbl>"#,
            para(props)
        );
        refused(&source(&body, styles, section()));
    }
}
#[test]
fn negative_image_paragraph_is_not_silently_normalized() {
    let body =
        para(r#"<w:ind w:left="-1"/>"#).replace("<w:t>Margin overhang</w:t>", "<w:drawing/>");
    refused(&source(&body, "", section()));
}
#[test]
fn tabs_and_page_breaks_still_reject_signed_body_source() {
    for child in [
        "<w:tab/>",
        r#"<w:br w:type="page"/>"#,
        r#"<w:br w:type="column"/>"#,
    ] {
        let body = para(r#"<w:ind w:left="-1"/>"#).replace("</w:t>", &format!("</w:t>{child}"));
        refused(&source(&body, "", section()));
    }
}
#[test]
fn legacy_nonnegative_indent_xml_order_remains_unchanged() {
    // Frozen XML declaration ordering is unchanged, including explicit zero.
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("old.docx");
    let mut model =
        model(json!({"indentLeftTwips":0,"indentRightTwips":20,"firstLineIndentTwips":0}));
    model.attrs.clear();
    document::export_docx(&model, &p).unwrap();
    let bytes = fs::read(&p).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains(r#"<w:ind w:left="0" w:right="20" w:firstLine="0"/>"#));
}

#[test]
fn hanging_only_overhang_requires_direct_body_and_proved_page() {
    let p = para(r#"<w:ind w:hanging="1"/>"#);
    refused(&source(&p, "", ""));
    refused(&source(
        &format!("<w:customXml>{p}</w:customXml>"),
        "",
        section(),
    ));
    accepted(&source(&p, "", section()));
}
#[test]
fn unsupported_section_settings_cannot_supply_physical_bounds() {
    for section in [
        section().replace("</w:sectPr>", "<w:cols w:num=\"2\"/></w:sectPr>"),
        format!("{}{}", section(), section()),
        section().replace("</w:sectPr>", "<w:bidi/></w:sectPr>"),
    ] {
        let p = source(&para(r#"<w:ind w:left="-1"/>"#), "", &section);
        refused(&p);
    }
}
#[test]
fn negative_style_overridden_in_list_or_image_remains_refused() {
    let styles = r#"<w:style w:type="paragraph" w:styleId="Over"><w:pPr><w:ind w:left="-1"/></w:pPr></w:style>"#;
    let props = r#"<w:pStyle w:val="Over"/><w:ind w:left="0"/>"#;
    refused(&source(
        &para(&format!(
            r#"{props}<w:numPr><w:numId w:val="1"/></w:numPr>"#
        )),
        styles,
        section(),
    ));
    refused(&source(
        &para(props).replace("<w:t>Margin overhang</w:t>", "<w:drawing/>"),
        styles,
        section(),
    ));
}
