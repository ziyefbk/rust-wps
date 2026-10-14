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
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const P: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const KEYS: [&str; 3] = ["keepNext", "keepLines", "pageBreakBefore"];
type Parts = BTreeMap<String, Vec<u8>>;
fn node(v: Value) -> Node {
    serde_json::from_value(v).unwrap()
}
fn model(attrs: Value) -> Node {
    node(
        json!({"type":"doc","attrs":{"styleProfile":"standard-v2"},"content":[{"type":"paragraph","attrs":attrs,"content":[{"type":"text","text":"First"}]},{"type":"heading","attrs":{"level":1,"keepNext":false,"keepLines":true,"pageBreakBefore":false},"content":[{"type":"text","text":"Heading"}]}]}),
    )
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
fn zip(path: &Path, parts: &Parts) {
    let mut z = zip::ZipWriter::new(fs::File::create_new(path).unwrap());
    for (n, b) in parts {
        z.start_file(n, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(b).unwrap();
    }
    z.finish().unwrap();
}
fn source(body: &str, styles: &str) -> Parts {
    BTreeMap::from([
        ("[Content_Types].xml".into(), br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_vec()),
        ("_rels/.rels".into(),format!("<Relationships xmlns=\"{P}\"><Relationship Id=\"main\" Type=\"{R}/officeDocument\" Target=\"word/document.xml\"/></Relationships>").into_bytes()),
        ("word/_rels/document.xml.rels".into(),format!("<Relationships xmlns=\"{P}\"><Relationship Id=\"styles\" Type=\"{R}/styles\" Target=\"styles.xml\"/></Relationships>").into_bytes()),
        ("word/document.xml".into(),format!("<w:document xmlns:w=\"{W}\"><w:body>{body}</w:body></w:document>").into_bytes()),
        ("word/styles.xml".into(),format!("<w:styles xmlns:w=\"{W}\">{styles}</w:styles>").into_bytes()),
    ])
}
fn p(props: &str, text: &str) -> String {
    format!("<w:p><w:pPr>{props}</w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>")
}
fn import(parts: &Parts) -> anyhow::Result<Node> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("original.docx");
    zip(&path, parts);
    let bytes = fs::read(&path).unwrap();
    let result = importing::import_docx(&path).map(|r| r.document);
    assert_eq!(bytes, fs::read(&path).unwrap());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    result
}
fn change(parts: &mut Parts, name: &str, from: &str, to: &str) {
    let text = String::from_utf8(parts[name].clone()).unwrap();
    assert!(text.contains(from));
    parts.insert(name.into(), text.replace(from, to).into_bytes());
}
#[test]
fn pagination_model_requires_strict_booleans() {
    for key in KEYS {
        for value in [json!(true), json!(false)] {
            document::validate(&model(json!({key:value}))).unwrap();
        }
        for value in [
            json!(null),
            json!(0),
            json!(1),
            json!("false"),
            json!([]),
            json!({}),
            json!(0.0),
        ] {
            assert!(
                document::validate(&model(json!({key:value}))).is_err(),
                "{key}: {value}"
            );
        }
    }
}
#[test]
fn pagination_model_rejects_nested_paragraphs_and_misplaced_attrs() {
    for key in KEYS {
        for kind in ["bulletList", "table"] {
            let p = json!({"type":"paragraph","attrs":{key:false}});
            let content = if kind == "table" {
                json!({"type":"table","content":[{"type":"tableRow","content":[{"type":"tableCell","content":[p]}]}]})
            } else {
                json!({"type":"bulletList","content":[{"type":"listItem","content":[p]}]})
            };
            assert!(document::validate(&node(json!({"type":"doc","content":[content]}))).is_err());
        }
        let mut m = model(json!({}));
        m.attrs.insert(key.into(), json!(false));
        assert!(document::validate(&m).is_err());
        let mut m = model(json!({}));
        m.content[0].content[0]
            .attrs
            .insert(key.into(), json!(true));
        assert!(document::validate(&m).is_err());
    }
}
#[test]
fn pagination_explicit_false_survives_draft_and_trusted_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let m = model(json!({"keepNext":false,"keepLines":false,"pageBreakBefore":false}));
    let draft = dir.path().join("draft.json");
    document::save_draft(&m, &draft).unwrap();
    assert_eq!(
        json!(document::load_draft(&draft).unwrap().unwrap()),
        json!(m)
    );
    let output = dir.path().join("output.docx");
    document::export_docx(&m, &output).unwrap();
    assert_eq!(
        json!(document::load_authored_docx(&output).unwrap().unwrap()),
        json!(m)
    );
}
#[test]
fn pagination_invalid_draft_and_export_leave_existing_files_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let draft = dir.path().join("draft.json");
    let output = dir.path().join("output.docx");
    let valid = model(json!({"keepLines":true}));
    document::save_draft(&valid, &draft).unwrap();
    document::export_docx(&valid, &output).unwrap();
    let d = fs::read(&draft).unwrap();
    let o = fs::read(&output).unwrap();
    let invalid = model(json!({"keepLines":"true"}));
    assert!(document::save_draft(&invalid, &draft).is_err());
    assert!(document::export_docx(&invalid, &output).is_err());
    assert_eq!(d, fs::read(&draft).unwrap());
    assert_eq!(o, fs::read(&output).unwrap());
    let fresh = dir.path().join("fresh.docx");
    assert!(document::export_docx(&invalid, &fresh).is_err());
    assert!(!fresh.exists());
}
#[test]
fn pagination_export_emits_direct_flags_in_schema_order() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("output.docx");
    document::export_docx(
        &model(json!({"keepNext":true,"keepLines":false,"pageBreakBefore":true,"spaceBefore":0,"textAlign":"center"})),
        &output,
    )
    .unwrap();
    let p = parts(&output);
    let xml = String::from_utf8(p["word/document.xml"].clone()).unwrap();
    assert!(xml.contains("<w:keepNext w:val=\"1\"/><w:keepLines w:val=\"0\"/><w:pageBreakBefore w:val=\"1\"/><w:spacing"));
    assert!(xml.contains("<w:pStyle w:val=\"Heading1\"/><w:keepNext w:val=\"0\"/>"));
    assert!(xml.contains("<w:spacing w:before=\"0\"/><w:jc w:val=\"center\"/>"));
    assert!(!xml.contains("<w:br w:type=\"page\"/>"));
}
#[test]
fn pagination_absent_authored_fields_remain_absent_in_xml_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("old.docx");
    let m = node(
        json!({"type":"doc","content":[{"type":"paragraph"},{"type":"heading","attrs":{"level":1},"content":[{"type":"text","text":"Old"}]}]}),
    );
    document::export_docx(&m, &output).unwrap();
    let p = parts(&output);
    let xml = String::from_utf8(p["word/document.xml"].clone()).unwrap();
    for key in KEYS {
        assert!(!xml.contains(key));
    }
    assert_eq!(
        json!(document::load_authored_docx(&output).unwrap().unwrap()),
        json!(m)
    );
}
#[test]
fn pagination_tampered_model_and_xml_cannot_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("good.docx");
    document::export_docx(&model(json!({"keepLines":true})), &output).unwrap();
    let mut p = parts(&output);
    change(
        &mut p,
        "word/document.xml",
        "<w:keepLines w:val=\"1\"/>",
        "<w:keepLines w:val=\"0\"/>",
    );
    let bad = dir.path().join("xml.docx");
    zip(&bad, &p);
    assert!(document::load_authored_docx(&bad).is_err());
    let mut p = parts(&output);
    let mut m: Value = serde_json::from_slice(&p["rust-wps/model-v2.json"]).unwrap();
    m["document"]["content"][0]["attrs"]["keepLines"] = json!(false);
    p.insert(
        "rust-wps/model-v2.json".into(),
        serde_json::to_vec(&m).unwrap(),
    );
    let bad = dir.path().join("model.docx");
    zip(&bad, &p);
    assert!(document::load_authored_docx(&bad).is_err());
}
#[test]
fn pagination_external_absence_is_false_and_heading_false_overrides_style() {
    let styles = "<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:pPr><w:keepNext/></w:pPr></w:style>";
    let doc = import(&source(
        &(p("", "ordinary")
            + &p(
                "<w:pStyle w:val=\"Heading1\"/><w:keepNext w:val=\"0\"/>",
                "heading",
            )),
        styles,
    ))
    .unwrap();
    for n in &doc.content {
        for k in KEYS {
            assert_eq!(n.attrs[k], false);
        }
    }
    assert_eq!(doc.content[1].kind, "heading");
}
#[test]
fn pagination_actual_selected_styles_part_is_used() {
    let mut s = source(
        &p("", "A"),
        "<w:docDefaults><w:pPrDefault><w:pPr><w:keepLines/></w:pPr></w:pPrDefault></w:docDefaults>",
    );
    change(
        &mut s,
        "word/_rels/document.xml.rels",
        "Target=\"styles.xml\"",
        "Target=\"selected.xml\"",
    );
    let styles = s.remove("word/styles.xml").unwrap();
    s.insert("word/selected.xml".into(), styles);
    s.insert("word/styles.xml".into(),format!("<w:styles xmlns:w=\"{W}\"><w:docDefaults><w:pPrDefault><w:pPr><w:keepLines w:val=\"0\"/></w:pPr></w:pPrDefault></w:docDefaults></w:styles>").into_bytes());
    assert_eq!(import(&s).unwrap().content[0].attrs["keepLines"], true);
}
#[test]
fn pagination_ambiguous_foreign_external_and_missing_styles_relations_refuse() {
    for replacement in [
        format!(
            "<Relationship Id=\"styles\" Type=\"{R}/styles\" Target=\"styles.xml\"/><Relationship Id=\"s2\" Type=\"{R}/styles\" Target=\"styles.xml\"/>"
        ),
        format!(
            "<Relationship Id=\"styles\" Type=\"{R}/styles\" Target=\"styles.xml\" TargetMode=\"External\"/>"
        ),
        format!(
            "<Relationship xmlns=\"urn:wrong\" Id=\"styles\" Type=\"{R}/styles\" Target=\"styles.xml\"/>"
        ),
        format!("<Relationship Id=\"styles\" Type=\"{R}/styles\" Target=\"missing.xml\"/>"),
    ] {
        let mut s = source(&p("<w:keepNext/>", "A"), "");
        change(
            &mut s,
            "word/_rels/document.xml.rels",
            &format!("<Relationship Id=\"styles\" Type=\"{R}/styles\" Target=\"styles.xml\"/>"),
            &replacement,
        );
        assert!(import(&s).is_err());
    }
}
#[test]
fn pagination_style_chain_and_direct_overrides_materialize_fieldwise() {
    let styles = "<w:docDefaults><w:pPrDefault><w:pPr><w:keepNext/><w:keepLines/><w:pageBreakBefore/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type=\"paragraph\" w:styleId=\"Base\"><w:pPr><w:keepNext w:val=\"0\"/></w:pPr></w:style><w:style w:type=\"paragraph\" w:styleId=\"Leaf\"><w:basedOn w:val=\"Base\"/><w:pPr><w:keepLines w:val=\"off\"/></w:pPr></w:style>";
    let doc = import(&source(
        &p(
            "<w:pStyle w:val=\"Leaf\"/><w:pageBreakBefore w:val=\"false\"/>",
            "A",
        ),
        styles,
    ))
    .unwrap();
    for key in KEYS {
        assert_eq!(doc.content[0].attrs[key], false);
    }
}
#[test]
fn pagination_body_flags_remain_bound_across_manual_break_and_table() {
    let table = "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>same</w:t></w:r></w:p></w:tc></w:tr></w:tbl>";
    let body = p("<w:keepNext/>", "same")
        + "<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>"
        + table
        + &p("<w:keepLines/>", "same");
    let doc = import(&source(&body, "")).unwrap();
    assert_eq!(doc.content[0].attrs["keepNext"], true);
    assert_eq!(doc.content[1].kind, "pageBreak");
    assert_eq!(doc.content[2].kind, "table");
    assert!(
        !doc.content[2].content[0].content[0].content[0]
            .attrs
            .contains_key("keepNext")
    );
    assert_eq!(doc.content[3].attrs["keepLines"], true);
}

#[test]
fn pagination_inventory_without_content_types_cannot_enter_import() {
    let mut s = source(&p("<w:keepLines/>", "A"), "");
    s.remove("[Content_Types].xml");
    assert!(import(&s).is_err());
}
#[test]
fn pagination_empty_body_preserves_the_existing_generated_empty_paragraph() {
    let doc = import(&source("", "")).unwrap();
    assert_eq!(doc.content.len(), 1);
    assert_eq!(doc.content[0].kind, "paragraph");
    for key in KEYS {
        assert!(!doc.content[0].attrs.contains_key(key));
    }
}
#[test]
fn pagination_column_break_keeps_its_existing_normalized_empty_paragraph() {
    let body = "<w:p><w:r><w:t>A</w:t><w:br w:type=\"column\"/><w:t>B</w:t></w:r></w:p>";
    let doc = import(&source(body, "")).unwrap();
    assert_eq!(doc.content.len(), 3);
    assert!(doc.content[1].content.is_empty());
    for n in &doc.content {
        for key in KEYS {
            assert!(!n.attrs.contains_key(key));
        }
    }
}
