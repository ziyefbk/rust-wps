use rust_wps::{
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
const NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const FONT: &str = r#"<w:rFonts w:ascii="DejaVu Sans" w:hAnsi="DejaVu Sans"/><w:sz w:val="24"/>"#;
fn font() -> Value {
    json!({"family":"DejaVu Sans","halfPoints":24,"bold":false,"italic":false})
}
fn model() -> Node {
    serde_json::from_value(json!({"type":"doc","attrs":{"styleProfile":"standard-v2"},"content":[{"type":"paragraph","attrs":{"emptyParagraphFont":font()}}]})).unwrap()
}
fn parts(body: &str, styles: &str) -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
    ("[Content_Types].xml".into(), br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/meta/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#.to_vec()),
    ("_rels/.rels".into(),br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="office" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec()),
    ("word/_rels/document.xml.rels".into(),br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="meta/styles.xml"/></Relationships>"#.to_vec()),
    ("word/document.xml".into(),format!(r#"<w:document xmlns:w="{NS}"><w:body>{body}</w:body></w:document>"#).into_bytes()),
    ("word/meta/styles.xml".into(),format!(r#"<w:styles xmlns:w="{NS}">{styles}</w:styles>"#).into_bytes()),
])
}
fn write(path: &Path, parts: &BTreeMap<String, Vec<u8>>) {
    let mut z = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, bytes) in parts {
        z.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(bytes).unwrap();
    }
    z.finish().unwrap();
}
fn read(path: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut z = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            (f.name().to_owned(), b)
        })
        .collect()
}
fn defaults(font: &str) -> String {
    format!("<w:docDefaults><w:rPrDefault><w:rPr>{font}</w:rPr></w:rPrDefault></w:docDefaults>")
}
fn import(body: &str, styles: &str) -> importing::ImportReport {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("source.docx");
    write(&p, &parts(body, styles));
    let before = fs::read(&p).unwrap();
    let result =
        importing::import_docx(&p).unwrap_or_else(|error| panic!("{body} / {styles}: {error}"));
    assert_eq!(fs::read(&p).unwrap(), before);
    assert_eq!(fs::read_dir(d.path()).unwrap().count(), 1);
    result
}
fn fonts(n: &Node) -> Vec<&Value> {
    let mut out = Vec::new();
    if let Some(f) = n.attrs.get("emptyParagraphFont") {
        out.push(f)
    }
    for c in &n.content {
        out.extend(fonts(c));
    }
    out
}
#[test]
fn empty_font_valid_complete_model_exports_only_paragraph_mark_and_reopens() {
    for modern in [false, true] {
        for half_points in [2, 24, 3276] {
            let d = tempfile::tempdir().unwrap();
            let mut m = model();
            if !modern {
                m.attrs.clear();
            }
            m.content[0].attrs["emptyParagraphFont"]["halfPoints"] = json!(half_points);
            let p = d.path().join("out.docx");
            document::export_docx(&m, &p).unwrap();
            let zip = read(&p);
            let xml = String::from_utf8(zip["word/document.xml"].clone()).unwrap();
            assert!(xml.contains("<w:pPr><w:rPr><w:rFonts"));
            assert!(!xml.contains("<w:t"));
            assert!(!xml.contains("<w:r>"));
            assert!(xml.contains(&format!("<w:sz w:val=\"{half_points}\"/>")));
            assert_eq!(
                serde_json::to_value(document::load_authored_docx(&p).unwrap().unwrap()).unwrap(),
                serde_json::to_value(&m).unwrap()
            );
        }
    }
}
#[test]
fn empty_font_raw_null_partial_unknown_and_wrong_typed_values_reject() {
    let mut bad = vec![Value::Null, json!([]), json!({}), json!("DejaVu Sans")];
    for key in ["family", "halfPoints", "bold", "italic"] {
        let mut f = font();
        f.as_object_mut().unwrap().remove(key);
        bad.push(f);
        let mut f = font();
        f[key] = Value::Null;
        bad.push(f);
    }
    for (key, values) in [
        (
            "family",
            vec![
                json!(""),
                json!(" x"),
                json!("x\n"),
                json!("x".repeat(32)),
                json!(false),
            ],
        ),
        (
            "halfPoints",
            vec![json!(1), json!(3277), json!(24.5), json!("24")],
        ),
        ("bold", vec![json!(1), json!("false")]),
        ("italic", vec![json!(0), json!("true")]),
    ] {
        for v in values {
            let mut f = font();
            f[key] = v;
            bad.push(f);
        }
    }
    let mut unknown = font();
    unknown["size"] = json!(12);
    bad.push(unknown);
    for f in bad {
        let mut m = model();
        m.content[0].attrs["emptyParagraphFont"] = f;
        assert!(document::validate(&m).is_err());
    }
}
#[test]
fn empty_font_never_permitted_on_text_whitespace_break_or_nonparagraph_node() {
    for child in [
        json!({"type":"text","text":"x"}),
        json!({"type":"text","text":" "}),
        json!({"type":"text","text":"\t"}),
        json!({"type":"hardBreak"}),
    ] {
        let mut m = model();
        m.content[0]
            .content
            .push(serde_json::from_value(child).unwrap());
        assert!(document::validate(&m).is_err());
    }
    for kind in [
        "doc",
        "text",
        "pageBreak",
        "table",
        "tableRow",
        "tableCell",
        "bulletList",
        "listItem",
        "image",
    ] {
        let mut m = model();
        m.content[0].kind = kind.into();
        assert!(document::validate(&m).is_err());
    }
}
#[test]
fn canonical_authored_empty_heading_list_and_cell_fonts_are_supported() {
    let mut m = model();
    m.content[0].kind = "heading".into();
    m.content[0].attrs.insert("level".into(), json!(2));
    document::validate(&m).unwrap();
    for wrapper in [
        json!({"type":"bulletList","content":[{"type":"listItem","content":[serde_json::to_value(&model().content[0]).unwrap()]}]}),
        json!({"type":"table","content":[{"type":"tableRow","content":[{"type":"tableCell","content":[serde_json::to_value(&model().content[0]).unwrap()]}]}]}),
    ] {
        let mut m = model();
        m.content = vec![serde_json::from_value(wrapper).unwrap()];
        document::validate(&m).unwrap();
        let d = tempfile::tempdir().unwrap();
        document::export_docx(&m, &d.path().join("out.docx")).unwrap();
    }
}
#[test]
fn empty_font_draft_failure_keeps_previous_and_export_cannot_clobber() {
    let d = tempfile::tempdir().unwrap();
    let draft = d.path().join("draft.json");
    let p = d.path().join("source.docx");
    let mut m = model();
    document::save_draft(&m, &draft).unwrap();
    document::export_docx(&m, &p).unwrap();
    let old_draft = fs::read(&draft).unwrap();
    let old_docx = fs::read(&p).unwrap();
    assert_eq!(
        serde_json::to_value(document::load_draft(&draft).unwrap().unwrap()).unwrap(),
        serde_json::to_value(&m).unwrap()
    );
    assert!(document::export_docx(&m, &p).is_err());
    m.content[0].attrs["emptyParagraphFont"] = Value::Null;
    assert!(document::save_draft(&m, &draft).is_err());
    assert!(document::export_docx(&m, &d.path().join("invalid.docx")).is_err());
    assert_eq!(fs::read(&draft).unwrap(), old_draft);
    assert_eq!(fs::read(&p).unwrap(), old_docx);
    assert!(!d.path().join("invalid.docx").exists());
}
#[test]
fn true_empty_source_uses_related_defaults_and_never_adds_fake_text() {
    let result = import("<w:p/>", &defaults(FONT));
    assert_eq!(fonts(&result.document), [&font()]);
    assert!(result.document.content[0].content.is_empty());
    assert!(
        result
            .compatibility
            .risks
            .iter()
            .any(|r| r.contains("typing ends"))
    );
}
#[test]
fn direct_mark_partial_fields_and_false_toggles_override_style() {
    let styles = format!(
        "{}<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:rPr><w:b/><w:i/></w:rPr></w:style>",
        defaults(FONT)
    );
    let result = import(
        r#"<w:p><w:pPr><w:rPr><w:sz w:val="18"/><w:b w:val="0"/><w:i w:val="false"/></w:rPr></w:pPr></w:p>"#,
        &styles,
    );
    assert_eq!(
        fonts(&result.document),
        [&json!({"family":"DejaVu Sans","halfPoints":18,"bold":false,"italic":false})]
    );
}
#[test]
fn paragraph_style_bold_italic_follow_toggle_inheritance() {
    let styles = format!(
        "{}<w:style w:type=\"paragraph\" w:styleId=\"Base\"><w:rPr><w:b/><w:i/></w:rPr></w:style><w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:basedOn w:val=\"Base\"/><w:rPr><w:b/><w:i w:val=\"0\"/></w:rPr></w:style>",
        defaults(FONT)
    );
    let result = import("<w:p/>", &styles);
    assert_eq!(
        fonts(&result.document),
        [&json!({"family":"DejaVu Sans","halfPoints":24,"bold":false,"italic":true})]
    );
}
#[test]
fn explicit_empty_heading_keeps_kind_and_effective_font() {
    let styles = format!(
        "{}<w:style w:type=\"paragraph\" w:styleId=\"Heading2\"><w:name w:val=\"heading 2\"/><w:pPr><w:outlineLvl w:val=\"1\"/></w:pPr><w:rPr><w:sz w:val=\"30\"/><w:b/></w:rPr></w:style>",
        defaults(FONT)
    );
    let result = import(
        "<w:p><w:pPr><w:pStyle w:val=\"Heading2\"/></w:pPr></w:p>",
        &styles,
    );
    assert_eq!(result.document.content[0].kind, "heading");
    assert_eq!(fonts(&result.document)[0]["halfPoints"], json!(30));
    assert_eq!(fonts(&result.document)[0]["bold"], json!(true));
}
#[test]
fn empty_runs_whitespace_tabs_breaks_and_pagebreaks_do_not_get_blank_font() {
    for body in [
        "<w:p><w:r><w:rPr><w:sz w:val=\"40\"/></w:rPr></w:r></w:p>",
        "<w:p><w:r><w:t xml:space=\"preserve\"> </w:t></w:r></w:p>",
        "<w:p><w:r><w:tab/></w:r></w:p>",
        "<w:p><w:r><w:br/></w:r></w:p>",
        "<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>",
    ] {
        let result = import(body, &defaults(FONT));
        assert!(fonts(&result.document).is_empty(), "{body}");
    }
    let result = import("<w:p><w:r><w:rPr/></w:r></w:p>", &defaults(FONT));
    assert!(
        result
            .compatibility
            .risks
            .iter()
            .any(|r| r.contains("unproved"))
    );
}
#[test]
fn bookmark_only_blank_is_still_zero_content() {
    let result = import(
        "<w:p><w:bookmarkStart w:id=\"1\" w:name=\"Blank\"/><w:bookmarkEnd w:id=\"1\"/></w:p>",
        &defaults(FONT),
    );
    assert_eq!(fonts(&result.document), [&font()]);
}
#[test]
fn pagebreak_and_rendered_break_do_not_shift_source_blank_identity() {
    let result = import(
        "<w:p/><w:p><w:r><w:br w:type=\"page\"/></w:r></w:p><w:p><w:r><w:lastRenderedPageBreak/><w:t>After</w:t></w:r></w:p><w:p/>",
        &defaults(FONT),
    );
    assert_eq!(fonts(&result.document), [&font(), &font()]);
    assert_eq!(result.document.content[1].kind, "pageBreak");
    assert!(
        !result.document.content[2]
            .attrs
            .contains_key("emptyParagraphFont")
    );
}
#[test]
fn unsupported_primary_language_and_conflicting_slots_are_warned_without_claim() {
    for extra in [
        r#"<w:lang w:val="ja-JP"/>"#,
        r#"<w:position w:val="1"/>"#,
        r#"<w:spacing w:val="20"/>"#,
        r#"<w:rFonts w:ascii="DejaVu Sans" w:hAnsi="DejaVu Serif"/>"#,
    ] {
        let properties = if extra.contains("rFonts") {
            format!("{extra}<w:sz w:val=\"24\"/>")
        } else {
            format!("{FONT}{extra}")
        };
        let result = import("<w:p/>", &defaults(&properties));
        assert!(fonts(&result.document).is_empty());
        assert!(
            result
                .compatibility
                .risks
                .iter()
                .any(|r| r.contains("unproved"))
        );
    }
}
#[test]
fn partial_unresolved_source_is_warned_and_never_guessed() {
    for props in [
        "",
        r#"<w:sz w:val="24"/>"#,
        r#"<w:rFonts w:ascii="DejaVu Sans" w:hAnsi="DejaVu Sans"/>"#,
    ] {
        let result = import("<w:p/>", &defaults(props));
        assert!(fonts(&result.document).is_empty());
        assert!(
            result
                .compatibility
                .risks
                .iter()
                .any(|r| r.contains("unproved"))
        );
    }
}
#[test]
fn source_table_blanks_are_not_body_font_targets() {
    let body = "<w:p/><w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"2400\"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:type=\"dxa\" w:w=\"2400\"/></w:tcPr><w:p/></w:tc></w:tr></w:tbl><w:p/>";
    let result = import(body, &defaults(FONT));
    assert_eq!(fonts(&result.document), [&font(), &font()]);
    assert!(
        !result.document.content[1].content[0].content[0].content[0]
            .attrs
            .contains_key("emptyParagraphFont")
    );
    assert!(
        result
            .compatibility
            .risks
            .iter()
            .any(|r| r.contains("unproved"))
    );
}
#[test]
fn no_empty_font_reset_returns_identical_package_to_plain_model() {
    let d = tempfile::tempdir().unwrap();
    for modern in [false, true] {
        let mut a = model();
        if !modern {
            a.attrs.clear();
        }
        a.content[0].attrs.remove("emptyParagraphFont");
        let mut b: Node =
            serde_json::from_value(json!({"type":"doc","content":[{"type":"paragraph"}]})).unwrap();
        b.attrs = a.attrs.clone();
        let pa = d.path().join(format!("a-{modern}.docx"));
        let pb = d.path().join(format!("b-{modern}.docx"));
        document::export_docx(&a, &pa).unwrap();
        document::export_docx(&b, &pb).unwrap();
        assert_eq!(fs::read(pa).unwrap(), fs::read(pb).unwrap());
    }
}
#[test]
fn existing_complex_script_rejection_is_not_weakened_by_empty_fonts() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("rtl.docx");
    write(&p, &parts("<w:p/>", &defaults(&format!("{FONT}<w:rtl/>"))));
    assert!(importing::import_docx(&p).is_err());
}
#[test]
fn wrapped_defaults_and_nonbaseline_mark_properties_never_gain_preservation() {
    for styles in [
        format!(
            "<w:docDefaults><w:rPrDefault><w:wrapper><w:rPr>{FONT}</w:rPr></w:wrapper></w:rPrDefault></w:docDefaults>"
        ),
        format!(
            "{}<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:pPr><w:rPr><w:sz w:val=\"48\"/></w:rPr></w:pPr></w:style>",
            defaults(FONT)
        ),
    ] {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("bad.docx");
        write(&p, &parts("<w:p/>", &styles));
        if let Ok(result) = importing::import_docx(&p) {
            assert!(fonts(&result.document).is_empty());
            assert!(
                result
                    .compatibility
                    .risks
                    .iter()
                    .any(|r| r.contains("unproved"))
            );
        }
    }
    for properties in [
        r#"<w:textAlignment w:val="top"/>"#,
        r#"<w:rPr><w:rStyle w:val="Mark"/></w:rPr>"#,
    ] {
        let result = import(
            &format!("<w:p><w:pPr>{properties}</w:pPr></w:p>"),
            &format!(
                "{}<w:style w:type=\"character\" w:styleId=\"Mark\"><w:rPr><w:sz w:val=\"48\"/></w:rPr></w:style>",
                defaults(FONT)
            ),
        );
        assert!(fonts(&result.document).is_empty());
        assert!(
            result
                .compatibility
                .risks
                .iter()
                .any(|r| r.contains("unproved"))
        );
    }
}
#[test]
fn source_bookmarks_and_unicode_anchors_bind_actual_block_order() {
    let body = "<w:p><w:r><w:t>前 &amp; first</w:t></w:r></w:p><w:p/><w:p><w:r><w:t>After &lt; second</w:t></w:r></w:p><w:p/>";
    let result = import(body, &defaults(FONT));
    assert_eq!(fonts(&result.document), [&font(), &font()]);
    assert!(
        result.document.content[0]
            .attrs
            .get("emptyParagraphFont")
            .is_none()
    );
    assert!(
        result.document.content[2]
            .attrs
            .get("emptyParagraphFont")
            .is_none()
    );
}
#[test]
fn source_default_theme_latin_resolves_through_actual_theme_relationship() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("theme.docx");
    let mut z = parts(
        "<w:p/>",
        &defaults(
            r#"<w:rFonts w:asciiTheme="minorHAnsi" w:hAnsiTheme="minorHAnsi"/><w:sz w:val="22"/>"#,
        ),
    );
    let rels=String::from_utf8(z["word/_rels/document.xml.rels"].clone()).unwrap().replace("</Relationships>",r#"<Relationship Id="theme" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="elsewhere/theme.xml"/></Relationships>"#);
    z.insert("word/_rels/document.xml.rels".into(), rels.into_bytes());
    z.insert("word/elsewhere/theme.xml".into(),br#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements><a:fontScheme name="Proof"><a:majorFont><a:latin typeface="DejaVu Serif"/></a:majorFont><a:minorFont><a:latin typeface="DejaVu Sans"/></a:minorFont></a:fontScheme></a:themeElements></a:theme>"#.to_vec());
    write(&p, &z);
    let result = importing::import_docx(&p).unwrap();
    assert_eq!(
        fonts(&result.document),
        [&json!({"family":"DejaVu Sans","halfPoints":22,"bold":false,"italic":false})]
    );
}

#[test]
fn bare_empty_run_has_no_font_of_its_own_and_preserves_paragraph_mark_baseline() {
    let result = import("<w:p><w:r/></w:p>", &defaults(FONT));
    assert_eq!(fonts(&result.document), [&font()]);
    assert!(result.document.content[0].content.is_empty());
}

#[test]
fn per_blank_missing_font_does_not_claim_all_blanks_preserved() {
    let styles = format!(
        "<w:style w:type=\"paragraph\" w:styleId=\"Proved\"><w:rPr>{FONT}</w:rPr></w:style>"
    );
    let result = import(
        "<w:p><w:pPr><w:pStyle w:val=\"Proved\"/></w:pPr></w:p><w:p/>",
        &styles,
    );
    assert_eq!(fonts(&result.document), [&font()]);
    assert!(
        result.document.content[0]
            .attrs
            .contains_key("emptyParagraphFont")
    );
    assert!(
        !result.document.content[1]
            .attrs
            .contains_key("emptyParagraphFont")
    );
    assert!(
        result
            .compatibility
            .risks
            .iter()
            .any(|r| r.contains("unproved"))
    );
    assert!(
        result
            .compatibility
            .risks
            .iter()
            .any(|r| r.contains("typing ends"))
    );
}

#[test]
fn true_blanks_around_images_do_not_bind_to_converter_placeholders() {
    let d = tempfile::tempdir().unwrap();
    let legacy: Node =
        serde_json::from_str(include_str!("fixtures/word-image-legacy.json")).unwrap();
    let mut source = model();
    let mut blank = source.content[0].clone();
    blank.attrs.clear();
    source.content = vec![
        blank.clone(),
        legacy.content[0].clone(),
        blank.clone(),
        legacy.content[1].clone(),
        blank,
    ];
    let path = d.path().join("images-and-blanks.docx");
    document::export_docx(&source, &path).unwrap();
    let before = fs::read(&path).unwrap();
    let imported = importing::import_docx(&path).unwrap().document;
    assert_eq!(
        imported
            .content
            .iter()
            .map(|n| n.kind.as_str())
            .collect::<Vec<_>>(),
        ["paragraph", "image", "paragraph", "image", "paragraph"]
    );
    for index in [0, 2, 4] {
        assert_eq!(
            imported.content[index].attrs["emptyParagraphFont"],
            json!({"family":"Arial","halfPoints":22,"bold":false,"italic":false})
        );
    }
    for (index, previous) in [(1, 0), (3, 1)] {
        assert_eq!(
            imported.content[index].attrs["src"],
            legacy.content[previous].attrs["src"]
        );
        assert!(
            !imported.content[index]
                .attrs
                .contains_key("emptyParagraphFont")
        );
    }
    assert_eq!(fs::read(&path).unwrap(), before);
}
