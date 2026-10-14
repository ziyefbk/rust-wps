//! Dedicated list metadata never enters the ordinary paragraph-indent contract.
use rust_wps::{
    compatibility,
    document::{self, Node},
    importing,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::Path,
};
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const P: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const BLOCK: &str = "Unsupported paragraph indentation cannot be preserved in an editable copy";
type Parts = BTreeMap<String, Vec<u8>>;
fn model() -> Value {
    json!({"type":"doc","attrs":{"styleProfile":"standard-v2","pageGeometry":{"widthTwips":12240,"heightTwips":15840,"marginTopTwips":1134,"marginBottomTwips":1134,"marginLeftTwips":1701,"marginRightTwips":850}},"content":[{"type":"orderedList","attrs":{"start":1,"markerFont":{"fontFamily":"Calibri","fontSize":22}},"content":[{"type":"listItem","content":[{"type":"paragraph","attrs":{"listGeometry":{"leftTwips":1134,"rightTwips":1134,"hangingTwips":567},"spaceBefore":0,"spaceAfter":160,"lineSpacing":259},"content":[{"type":"text","text":"first"}]}]},{"type":"listItem","content":[{"type":"paragraph","attrs":{"listGeometry":{"leftTwips":3969,"rightTwips":2835,"hangingTwips":2268}},"content":[{"type":"text","text":"second"}]}]}]}]})
}
fn node(v: Value) -> Node {
    serde_json::from_value(v).unwrap()
}
fn geometry(v: &mut Value) -> &mut Value {
    &mut v["content"][0]["content"][0]["content"][0]["attrs"]["listGeometry"]
}
fn parts(path: &Path) -> Parts {
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
fn zip(path: &Path, p: &Parts) {
    let mut z = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (n, b) in p {
        z.start_file(n, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(b).unwrap();
    }
    z.finish().unwrap();
}
fn source() -> Parts {
    let para = |s: &str, t: &str| {
        format!("<w:p><w:pPr><w:pStyle w:val=\"{s}\"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>")
    };
    let body = format!(
        "{}{}<w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/><w:pgMar w:left=\"1701\" w:right=\"850\" w:top=\"1134\" w:bottom=\"1134\"/></w:sectPr>",
        para("A", "first"),
        para("B", "second")
    );
    let styles = r#"<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="A"><w:name w:val="A"/><w:basedOn w:val="Normal"/><w:pPr><w:numPr><w:numId w:val="1"/></w:numPr><w:ind w:left="1134" w:right="1134" w:hanging="567"/></w:pPr></w:style><w:style w:type="paragraph" w:styleId="B"><w:name w:val="B"/><w:basedOn w:val="A"/><w:pPr><w:ind w:left="3969" w:right="2835" w:hanging="2268"/></w:pPr></w:style>"#;
    let numbering = r#"<w:abstractNum w:abstractNumId="0"><w:multiLevelType w:val="multilevel"/><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:pStyle w:val="A"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>"#;
    BTreeMap::from([
        ("[Content_Types].xml".into(),br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_vec()),
        ("_rels/.rels".into(),format!(r#"<Relationships xmlns="{P}"><Relationship Id="main" Type="{R}/officeDocument" Target="word/document.xml"/></Relationships>"#).into_bytes()),
        ("word/_rels/document.xml.rels".into(),format!(r#"<Relationships xmlns="{P}"><Relationship Id="styles" Type="{R}/styles" Target="styles.xml"/><Relationship Id="numbering" Type="{R}/numbering" Target="numbering.xml"/></Relationships>"#).into_bytes()),
        ("word/document.xml".into(),format!(r#"<w:document xmlns:w="{W}"><w:body>{body}</w:body></w:document>"#).into_bytes()),
        ("word/styles.xml".into(),format!(r#"<w:styles xmlns:w="{W}">{styles}</w:styles>"#).into_bytes()),
        ("word/numbering.xml".into(),format!(r#"<w:numbering xmlns:w="{W}">{numbering}</w:numbering>"#).into_bytes()),
    ])
}
fn change(p: &mut Parts, key: &str, old: &str, new: &str) {
    let s = String::from_utf8(p[key].clone()).unwrap();
    assert!(s.contains(old));
    p.insert(key.into(), s.replace(old, new).into_bytes());
}
fn import(p: &Parts) -> anyhow::Result<Node> {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("source.docx");
    zip(&path, p);
    let before = fs::read(&path).unwrap();
    let r = importing::import_docx(&path).map(|r| r.document);
    assert_eq!(before, fs::read(&path).unwrap());
    assert_eq!(1, fs::read_dir(d.path()).unwrap().count());
    r
}
fn refused(p: &Parts) {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("source.docx");
    zip(&path, p);
    if let Ok(a) = compatibility::audit_docx(&path) {
        assert!(a.risks.contains(BLOCK), "{:?}", a.risks);
    }
    assert!(import(p).is_err());
}
#[test]
fn model_preserves_complete_different_item_frames() {
    document::validate(&node(model())).unwrap();
}
#[test]
fn exact_zero_geometry_is_distinct_from_absence() {
    let mut v = model();
    *geometry(&mut v) = json!({"leftTwips":0,"rightTwips":0,"hangingTwips":0});
    document::validate(&node(v)).unwrap();
}
#[test]
fn invalid_geometry_values_are_rejected_atomically() {
    for g in [
        json!(null),
        json!({}),
        json!({"leftTwips":1,"rightTwips":0}),
        json!({"leftTwips":-1,"rightTwips":0,"hangingTwips":0}),
        json!({"leftTwips":1.2,"rightTwips":0,"hangingTwips":0}),
        json!({"leftTwips":1,"rightTwips":0,"hangingTwips":2}),
        json!({"leftTwips":31681,"rightTwips":0,"hangingTwips":0}),
        json!({"leftTwips":1,"rightTwips":0,"hangingTwips":0,"extra":0}),
    ] {
        let mut v = model();
        *geometry(&mut v) = g;
        assert!(document::validate(&node(v)).is_err());
    }
}
#[test]
fn invalid_marker_fonts_are_rejected() {
    for f in [
        json!(null),
        json!({}),
        json!({"fontFamily":"Calibri","fontSize":0}),
        json!({"fontFamily":"Calibri","fontSize":22.5}),
        json!({"fontFamily":"Calibri","fontSize":3277}),
        json!({"fontFamily":" Calibri","fontSize":22}),
        json!({"fontFamily":"Calibri","fontSize":22,"bold":true}),
    ] {
        let mut v = model();
        v["content"][0]["attrs"]["markerFont"] = f;
        assert!(document::validate(&node(v)).is_err());
    }
}
#[test]
fn missing_page_and_page_shrink_refuse() {
    let mut v = model();
    v["attrs"].as_object_mut().unwrap().remove("pageGeometry");
    assert!(document::validate(&node(v)).is_err());
    let mut v = model();
    v["attrs"]["pageGeometry"]["marginRightTwips"] = json!(4000);
    assert!(document::validate(&node(v)).is_err());
}
#[test]
fn absent_sibling_geometry_refuses() {
    let mut v = model();
    v["content"][0]["content"][1]["content"][0]["attrs"]
        .as_object_mut()
        .unwrap()
        .remove("listGeometry");
    assert!(document::validate(&node(v)).is_err());
}
#[test]
fn body_lift_bullet_nesting_and_multiple_paragraphs_refuse() {
    let mut v = model();
    v["content"][0] = v["content"][0]["content"][0]["content"][0].clone();
    assert!(document::validate(&node(v)).is_err());
    let mut v = model();
    v["content"][0]["type"] = json!("bulletList");
    assert!(document::validate(&node(v)).is_err());
    let mut v = model();
    let p = v["content"][0]["content"][0]["content"][0].clone();
    v["content"][0]["content"][0]["content"]
        .as_array_mut()
        .unwrap()
        .push(p);
    assert!(document::validate(&node(v)).is_err());
}
#[test]
fn conflicting_indent_font_alignment_and_tab_refuse() {
    for (k, x) in [
        ("indentLeftTwips", json!(0)),
        (
            "emptyParagraphFont",
            json!({"family":"Calibri","halfPoints":22,"bold":false,"italic":false}),
        ),
        ("textAlign", json!("right")),
    ] {
        let mut v = model();
        v["content"][0]["content"][0]["content"][0]["attrs"][k] = x;
        assert!(document::validate(&node(v)).is_err());
    }
    let mut v = model();
    v["content"][0]["content"][0]["content"][0]["content"][0]["text"] = json!("a\tb");
    assert!(document::validate(&node(v)).is_err());
}
#[test]
fn authored_roundtrip_real_numbering_and_no_literal_markers() {
    let d = tempfile::tempdir().unwrap();
    let out = d.path().join("list.docx");
    let n = node(model());
    document::export_docx(&n, &out).unwrap();
    assert_eq!(
        serde_json::to_value(document::load_authored_docx(&out).unwrap().unwrap()).unwrap(),
        model()
    );
    let p = parts(&out);
    let main = String::from_utf8(p["word/document.xml"].clone()).unwrap();
    let numbers = String::from_utf8(p["word/numbering.xml"].clone()).unwrap();
    assert!(main.contains(r#"<w:ind w:left="1134" w:right="1134" w:hanging="567"/>"#));
    assert!(main.contains(r#"<w:ind w:left="3969" w:right="2835" w:hanging="2268"/>"#));
    assert_eq!(main.matches("<w:numPr>").count(), 2);
    assert!(!main.contains("1. first"));
    assert!(
        numbers.contains(
            r#"<w:abstractNum w:abstractNumId="2"><w:multiLevelType w:val="singleLevel"/>"#
        )
    );
    assert!(numbers.contains(r#"w:ascii="Calibri""#));
    assert!(numbers.contains(r#"<w:sz w:val="22"/>"#));
    let before = fs::read(&out).unwrap();
    assert!(document::export_docx(&n, &out).is_err());
    assert_eq!(before, fs::read(&out).unwrap());
}
#[test]
fn draft_roundtrip_and_invalid_save_preserve_existing_draft() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("draft.json");
    document::save_draft(&node(model()), &p).unwrap();
    let before = fs::read(&p).unwrap();
    assert_eq!(
        serde_json::to_value(document::load_draft(&p).unwrap().unwrap()).unwrap(),
        model()
    );
    let mut v = model();
    *geometry(&mut v) = json!(null);
    assert!(document::save_draft(&node(v), &p).is_err());
    assert_eq!(before, fs::read(&p).unwrap());
}
#[test]
fn tampered_model_and_numbering_cannot_reopen() {
    for part in ["rust-wps/model-v2.json", "word/numbering.xml"] {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("authored.docx");
        document::export_docx(&node(model()), &p).unwrap();
        let mut data = parts(&p);
        if part.ends_with("json") {
            change(&mut data, part, "1134", "1135");
        } else {
            change(&mut data, part, "Calibri", "Arial");
        }
        let out = d.path().join("tampered.docx");
        zip(&out, &data);
        assert!(document::load_authored_docx(&out).is_err());
    }
}
#[test]
fn source_associated_styles_keep_complete_physical_geometry() {
    let n = import(&source()).unwrap();
    assert_eq!(
        n.content[0].attrs["markerFont"],
        json!({"fontFamily":"Calibri","fontSize":22})
    );
    assert_eq!(
        n.content[0].content[0].content[0].attrs["listGeometry"],
        geometry(&mut model()).clone()
    );
    assert_eq!(
        n.content[0].content[1].content[0].attrs["listGeometry"],
        json!({"leftTwips":3969,"rightTwips":2835,"hangingTwips":2268})
    );
}
#[test]
fn source_complete_direct_override_is_preserved() {
    let mut p = source();
    change(
        &mut p,
        "word/document.xml",
        r#"<w:pStyle w:val="A"/>"#,
        r#"<w:pStyle w:val="A"/><w:ind w:left="1200" w:right="1000" w:hanging="400"/>"#,
    );
    let n = import(&p).unwrap();
    assert_eq!(
        n.content[0].content[0].content[0].attrs["listGeometry"]["leftTwips"],
        1200
    );
}
#[test]
fn source_partial_and_negative_even_overridden_refuse() {
    for ind in [
        r#"<w:ind w:left="100"/>"#,
        r#"<w:ind w:left="-100" w:right="0" w:hanging="0"/>"#,
        r#"<w:ind w:left="0" w:right="0" w:hanging="1"/>"#,
    ] {
        let mut p = source();
        change(
            &mut p,
            "word/styles.xml",
            r#"<w:ind w:left="1134" w:right="1134" w:hanging="567"/>"#,
            ind,
        );
        refused(&p);
    }
}
#[test]
fn source_unsupported_numbering_semantics_refuse() {
    for (old, new) in [
        (r#"w:val="decimal""#, r#"w:val="upperRoman""#),
        (r#"w:val="%1.""#, r#"w:val="(%1)""#),
        (r#"<w:lvlJc w:val="left"/>"#, r#"<w:lvlJc w:val="right"/>"#),
        (r#"<w:lvlText"#, r#"<w:suff w:val="space"/><w:lvlText"#),
        (r#"<w:pStyle w:val="A"/>"#, r#"<w:pStyle w:val="Other"/>"#),
        (r#"</w:lvl>"#, r#"<w:rPr><w:b/></w:rPr></w:lvl>"#),
        (
            r#"</w:num>"#,
            r#"<w:lvlOverride w:ilvl="0"><w:startOverride w:val="2"/></w:lvlOverride></w:num>"#,
        ),
    ] {
        let mut p = source();
        change(&mut p, "word/numbering.xml", old, new);
        refused(&p);
    }
}
#[test]
fn source_style_and_numbering_ambiguity_refuse() {
    for (part, old, new) in [
        (
            "word/styles.xml",
            r#"<w:basedOn w:val="A"/>"#,
            r#"<w:basedOn w:val="B"/>"#,
        ),
        (
            "word/numbering.xml",
            r#"</w:numbering>"#,
            r#"<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#,
        ),
        (
            "word/numbering.xml",
            r#"</w:abstractNum>"#,
            r#"<w:lvl w:ilvl="1"><w:pStyle w:val="A"/></w:lvl></w:abstractNum>"#,
        ),
    ] {
        let mut p = source();
        change(&mut p, part, old, new);
        refused(&p);
    }
}
#[test]
fn source_custom_tabs_direction_and_markers_refuse() {
    for extra in [
        r#"<w:tabs><w:tab w:val="left" w:pos="2000"/></w:tabs>"#,
        r#"<w:bidi/>"#,
        r#"<w:rPr><w:b/></w:rPr>"#,
    ] {
        let mut p = source();
        change(
            &mut p,
            "word/document.xml",
            r#"<w:pStyle w:val="A"/>"#,
            &format!(r#"<w:pStyle w:val="A"/>{extra}"#),
        );
        refused(&p);
    }
}
#[test]
fn source_resumed_numbering_and_nested_wrappers_refuse() {
    let mut p = source();
    change(
        &mut p,
        "word/document.xml",
        "</w:p><w:p>",
        "</w:p><w:p><w:r><w:t>separator</w:t></w:r></w:p><w:p>",
    );
    refused(&p);
    let mut p = source();
    change(
        &mut p,
        "word/document.xml",
        "<w:body>",
        "<w:body><w:sdt><w:sdtContent>",
    );
    change(
        &mut p,
        "word/document.xml",
        "<w:sectPr>",
        "</w:sdtContent></w:sdt><w:sectPr>",
    );
    assert!(import(&p).is_err());
}
#[test]
fn source_marker_theme_relationship_ambiguity_refuses_even_literal_fonts() {
    for rel in [
        format!(
            r#"<Relationship Id="theme1" Type="{R}/theme" Target="theme.xml" TargetMode="External"/>"#
        ),
        format!(
            r#"<Relationship Id="theme1" Type="{R}/theme" Target="theme.xml"/><Relationship Id="theme2" Type="{R}/theme" Target="theme.xml"/>"#
        ),
    ] {
        let mut p = source();
        change(
            &mut p,
            "word/_rels/document.xml.rels",
            "</Relationships>",
            &format!("{rel}</Relationships>"),
        );
        p.insert("word/theme.xml".into(), b"<unused/>".to_vec());
        refused(&p);
    }
}
#[test]
fn source_marker_font_must_be_plain_and_resolved() {
    for old in [
        r#"<w:sz w:val="22"/>"#,
        r#"<w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/>"#,
    ] {
        let mut p = source();
        change(&mut p, "word/styles.xml", old, "");
        refused(&p);
    }
    let mut p = source();
    change(
        &mut p,
        "word/styles.xml",
        r#"<w:sz w:val="22"/>"#,
        r#"<w:sz w:val="22"/><w:color w:val="FF0000"/>"#,
    );
    refused(&p);
}
#[test]
fn absent_metadata_keeps_legacy_list_validation() {
    let mut v = model();
    v["attrs"].as_object_mut().unwrap().remove("pageGeometry");
    v["content"][0]["attrs"]
        .as_object_mut()
        .unwrap()
        .remove("markerFont");
    for i in v["content"][0]["content"].as_array_mut().unwrap() {
        i["content"][0]["attrs"]
            .as_object_mut()
            .unwrap()
            .remove("listGeometry");
    }
    document::validate(&node(v)).unwrap();
}

#[test]
fn carriage_return_is_rejected_in_model_and_source() {
    let mut v = model();
    v["content"][0]["content"][0]["content"][0]["content"][0]["text"] = json!("a\rb");
    assert!(document::validate(&node(v)).is_err());
    let mut p = source();
    change(&mut p, "word/document.xml", ">first<", ">a&#13;b<");
    refused(&p);
}
#[test]
fn source_association_must_resolve_active_numbering() {
    let mut p = source();
    change(
        &mut p,
        "word/numbering.xml",
        r#"<w:pStyle w:val="A"/>"#,
        r#"<w:pStyle w:val="Normal"/>"#,
    );
    refused(&p);
}
fn settings_part(p: &mut Parts, settings: &str) {
    change(
        p,
        "word/_rels/document.xml.rels",
        "</Relationships>",
        &format!(
            r#"<Relationship Id="settings" Type="{R}/settings" Target="settings.xml"/></Relationships>"#
        ),
    );
    p.insert(
        "word/settings.xml".into(),
        format!(r#"<w:settings xmlns:w="{W}">{settings}</w:settings>"#).into_bytes(),
    );
}
#[test]
fn default_tab_stop_and_compatibility_flags_are_proved() {
    for settings in [
        r#"<w:defaultTabStop w:val="360"/>"#,
        r#"<w:defaultTabStop w:val="720"/><w:defaultTabStop w:val="720"/>"#,
        r#"<w:compat><w:noTabHangInd/></w:compat>"#,
        r#"<w:compat><w:doNotUseIndentAsNumberingTabStop/></w:compat>"#,
        r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="12"/></w:compat>"#,
    ] {
        let mut p = source();
        settings_part(&mut p, settings);
        refused(&p);
    }
    let mut p = source();
    settings_part(&mut p, r#"<w:defaultTabStop w:val="720"/>"#);
    import(&p).unwrap();
}
#[test]
fn repeated_numbering_audit_has_cumulative_work_limit() {
    let mut p = source();
    let source_p = r#"<w:p><w:pPr><w:pStyle w:val="A"/></w:pPr><w:r><w:t>first</w:t></w:r></w:p>"#;
    change(
        &mut p,
        "word/document.xml",
        source_p,
        &source_p.repeat(2000),
    );
    let unused = (2..502)
        .map(|n| format!(r#"<w:num w:numId="{n}"><w:abstractNumId w:val="0"/></w:num>"#))
        .collect::<String>();
    change(
        &mut p,
        "word/numbering.xml",
        "</w:numbering>",
        &format!("{unused}</w:numbering>"),
    );
    refused(&p);
}

#[test]
fn empty_whitespace_break_only_and_empty_formatted_runs_refuse_source_preservation() {
    for replacement in [
        "",
        "<w:r/>",
        "<w:r><w:t>   </w:t></w:r>",
        "<w:r><w:br/></w:r>",
        "<w:r><w:br/><w:t>first</w:t></w:r>",
        "<w:r><w:t>first</w:t></w:r><w:r><w:rPr><w:sz w:val=\"100\"/></w:rPr></w:r>",
    ] {
        let mut p = source();
        change(
            &mut p,
            "word/document.xml",
            "<w:r><w:t>first</w:t></w:r>",
            replacement,
        );
        refused(&p);
    }
}
