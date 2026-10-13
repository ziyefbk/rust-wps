use rust_wps::{
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
const NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
fn parts(body: &str, styles: &str) -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
("[Content_Types].xml".into(),br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/meta/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#.to_vec()),
("_rels/.rels".into(),br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="office" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec()),
("word/_rels/document.xml.rels".into(),br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="meta/styles.xml"/></Relationships>"#.to_vec()),
("word/document.xml".into(),format!(r#"<w:document xmlns:w="{NS}"><w:body>{body}</w:body></w:document>"#).into_bytes()),
("word/meta/styles.xml".into(),format!(r#"<w:styles xmlns:w="{NS}">{styles}</w:styles>"#).into_bytes())])
}
fn table(shd: &str, p: &str) -> String {
    format!(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2400"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:type="dxa" w:w="2400"/>{shd}</w:tcPr>{p}</w:tc></w:tr></w:tbl>"#
    )
}
fn write(path: &Path, parts: &BTreeMap<String, Vec<u8>>) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, value) in parts {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(value).unwrap();
    }
    zip.finish().unwrap();
}
fn import(body: &str, styles: &str) -> rust_wps::importing::ImportReport {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.docx");
    write(&path, &parts(body, styles));
    let before = fs::read(&path).unwrap();
    let result = importing::import_docx(&path).unwrap();
    assert_eq!(fs::read(&path).unwrap(), before);
    result
}
fn cell(doc: &Node) -> &Node {
    &doc.content[0].content[0].content[0]
}
fn model(fill: Option<Value>, auto: Option<Value>) -> Node {
    let mut cell = json!({"type":"tableCell","content":[{"type":"paragraph","content":[{"type":"text","text":"Auto"},{"type":"text","text":"Black","marks":[{"type":"textColor","attrs":{"rgb":"000000"}}]},{"type":"text","text":"Green","marks":[{"type":"textColor","attrs":{"rgb":"008000"}}]},{"type":"text","text":"Link","marks":[{"type":"link","attrs":{"href":"https://example.com"}}]}]}]});
    if let Some(fill) = fill {
        cell["attrs"]["cellFillRgb"] = fill;
    }
    if let Some(auto) = auto {
        cell["attrs"]["cellTextColorMode"] = auto;
    }
    serde_json::from_value(json!({"type":"doc","attrs":{"styleProfile":"standard-v2"},"content":[{"type":"table","content":[{"type":"tableRow","content":[cell]}]}]})).unwrap()
}
fn encode(doc: &Node) -> anyhow::Result<(Vec<u8>, document::ExportReport)> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("output.docx");
    let report = document::export_docx(doc, &path)?;
    Ok((fs::read(path)?, report))
}
fn unzip(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut e = zip.by_index(i).unwrap();
            let mut b = Vec::new();
            e.read_to_end(&mut b).unwrap();
            (e.name().to_owned(), b)
        })
        .collect()
}
const FILL: &str = r#"<w:shd w:val="clear" w:color="auto" w:fill="E97132"/>"#;
const P: &str = "<w:p><w:r><w:t>Cell</w:t></w:r></w:p>";
#[test]
fn direct_fill_and_proved_source_automatic_are_separate() {
    let report = import(&table(FILL, P), "");
    assert_eq!(cell(&report.document).attrs["cellFillRgb"], "E97132");
    assert_eq!(cell(&report.document).attrs["cellTextColorMode"], "auto");
    assert!(
        report
            .compatibility
            .risks
            .iter()
            .any(|s| s.contains("preserved locally"))
    );
}
#[test]
fn explicit_run_rgb_wins_beside_automatic_text() {
    let p = r#"<w:p><w:r><w:t>Auto</w:t></w:r><w:r><w:rPr><w:color w:val="000000"/></w:rPr><w:t>Black</w:t></w:r><w:r><w:rPr><w:color w:val="008000"/></w:rPr><w:t>Green</w:t></w:r></w:p>"#;
    let report = import(&table(FILL, p), "");
    let c = cell(&report.document);
    assert_eq!(c.attrs["cellTextColorMode"], "auto");
    assert_eq!(
        c.content[0].content[1]
            .marks
            .iter()
            .find(|m| m.kind == "textColor")
            .unwrap()
            .attrs["rgb"],
        "000000"
    );
}
#[test]
fn explicit_default_color_is_never_inferred_as_automatic() {
    for value in ["000000", "008000"] {
        let styles = format!(
            r#"<w:docDefaults><w:rPrDefault><w:rPr><w:color w:val="{value}"/></w:rPr></w:rPrDefault></w:docDefaults>"#
        );
        let report = import(&table(FILL, P), &styles);
        assert_eq!(cell(&report.document).attrs["cellFillRgb"], "E97132");
        assert!(
            !cell(&report.document)
                .attrs
                .contains_key("cellTextColorMode")
        );
    }
}
#[test]
fn active_paragraph_style_rgb_is_not_automatic() {
    let styles = r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:rPr><w:color w:val="008000"/></w:rPr></w:style>"#;
    let report = import(&table(FILL, P), styles);
    assert!(
        !cell(&report.document)
            .attrs
            .contains_key("cellTextColorMode")
    );
}
#[test]
fn unused_conditional_style_does_not_disable_proved_fill() {
    let styles = r#"<w:style w:type="table" w:styleId="Unused"><w:tblStylePr w:type="firstRow"><w:tcPr><w:shd w:val="pct20" w:fill="123456"/></w:tcPr><w:rPr><w:color w:val="008000"/></w:rPr></w:tblStylePr></w:style>"#;
    let report = import(&table(FILL, P), styles);
    assert_eq!(cell(&report.document).attrs["cellFillRgb"], "E97132");
    assert_eq!(cell(&report.document).attrs["cellTextColorMode"], "auto");
}
#[test]
fn active_conditional_shading_keeps_warned_old_normalization() {
    let styles = r#"<w:style w:type="table" w:default="1" w:styleId="Active"><w:tblStylePr w:type="firstRow"><w:tcPr><w:shd w:val="pct20" w:fill="123456"/></w:tcPr></w:tblStylePr></w:style>"#;
    let report = import(&table(FILL, P), styles);
    assert!(!cell(&report.document).attrs.contains_key("cellFillRgb"));
    assert!(
        report
            .compatibility
            .risks
            .iter()
            .any(|s| s.contains("fills are unproved"))
    );
}
#[test]
fn active_table_foreground_does_not_block_fill_but_warns_unproved_auto() {
    let styles = r#"<w:style w:type="table" w:default="1" w:styleId="Active"><w:rPr><w:color w:val="008000"/></w:rPr></w:style>"#;
    let report = import(&table(FILL, P), styles);
    assert_eq!(cell(&report.document).attrs["cellFillRgb"], "E97132");
    assert!(
        !cell(&report.document)
            .attrs
            .contains_key("cellTextColorMode")
    );
    assert!(
        report
            .compatibility
            .risks
            .iter()
            .any(|s| s.contains("automatic text colors are unproved"))
    );
}
#[test]
fn patterns_missing_fill_and_transforms_are_never_averaged_into_rgb() {
    for shd in [
        r#"<w:shd w:val="pct20" w:color="FF0000" w:fill="000000"/>"#,
        r#"<w:shd w:fill="E97132"/>"#,
        r#"<w:shd w:val="clear" w:fill="auto"/>"#,
        r#"<w:shd w:val="clear" w:fill="E97132" w:themeFillTint="80"/>"#,
    ] {
        let report = import(&table(shd, P), "");
        assert!(!cell(&report.document).attrs.contains_key("cellFillRgb"));
        assert!(
            report
                .compatibility
                .risks
                .iter()
                .any(|s| s.contains("fills are unproved"))
        );
    }
}
#[test]
fn local_auto_export_keeps_explicit_rgb_links_and_global_defaults() {
    let source = model(Some(json!("E97132")), Some(json!("auto")));
    let (bytes, _) = encode(&source).unwrap();
    let parts = unzip(&bytes);
    let xml = String::from_utf8(parts["word/document.xml"].clone()).unwrap();
    assert_eq!(xml.matches("<w:color w:val=\"auto\"/>").count(), 1);
    for rgb in ["000000", "008000", "0563C1"] {
        assert!(xml.contains(&format!("<w:color w:val=\"{rgb}\"/>")));
    }
    assert!(xml.contains(FILL));
    assert_eq!(
        parts["word/styles.xml"],
        unzip(&encode(&model(None, None)).unwrap().0)["word/styles.xml"]
    );
}
#[test]
fn fill_only_does_not_invent_auto_and_auto_does_not_require_fill() {
    for (fill, auto) in [(Some(json!("000000")), None), (None, Some(json!("auto")))] {
        let doc = model(fill, auto);
        let (bytes, _) = encode(&doc).unwrap();
        let xml = String::from_utf8(unzip(&bytes)["word/document.xml"].clone()).unwrap();
        assert_eq!(
            xml.contains("<w:color w:val=\"auto\"/>"),
            cell(&doc).attrs.contains_key("cellTextColorMode")
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("output.docx");
        fs::write(&path, bytes).unwrap();
        assert_eq!(
            serde_json::to_value(document::load_authored_docx(&path).unwrap().unwrap()).unwrap(),
            serde_json::to_value(&doc).unwrap()
        );
    }
}
#[test]
fn model_values_are_canonical_and_wrong_node_attributes_reject() {
    for fill in [
        json!(null),
        json!("abcDEF"),
        json!("#ABCDEF"),
        json!("ABCDE"),
        json!("ABCDEG"),
        json!(42),
    ] {
        assert!(document::validate(&model(Some(fill), None)).is_err());
    }
    for auto in [
        json!(null),
        json!(false),
        json!("Auto"),
        json!("default"),
        json!("000000"),
    ] {
        assert!(document::validate(&model(None, Some(auto))).is_err());
    }
    let mut doc = model(None, None);
    doc.attrs.insert("cellFillRgb".into(), json!("ABCDEF"));
    assert!(document::validate(&doc).is_err());
}
#[test]
fn new_colors_require_simple_top_level_table() {
    let mut doc = model(Some(json!("FFFFFF")), Some(json!("auto")));
    let nested = doc.content[0].clone();
    doc.content[0].content[0].content[0].content.push(nested);
    assert!(document::validate(&doc).is_err());
}
#[test]
fn fill_and_auto_survive_draft_and_resets_restore_old_bytes() {
    let original = model(None, None);
    let old = encode(&original).unwrap().0;
    let mut doc = model(Some(json!("000000")), Some(json!("auto")));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.json");
    document::save_draft(&doc, &path).unwrap();
    assert_eq!(
        serde_json::to_value(document::load_draft(&path).unwrap().unwrap()).unwrap(),
        serde_json::to_value(&doc).unwrap()
    );
    doc.content[0].content[0].content[0]
        .attrs
        .remove("cellFillRgb");
    assert!(document::validate(&doc).is_ok());
    doc.content[0].content[0].content[0]
        .attrs
        .remove("cellTextColorMode");
    assert_eq!(old, encode(&doc).unwrap().0);
}

#[test]
fn filled_numbered_cell_keeps_legacy_acceptance_with_atomic_warning_fallback() {
    let p = r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Listed</w:t></w:r></w:p>"#;
    for kind in ["decimal", "bullet"] {
        let mut package = parts(&table(FILL, p), "");
        let rels=String::from_utf8(package["word/_rels/document.xml.rels"].clone()).unwrap().replace("</Relationships>",r#"<Relationship Id="numbering" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbers.xml"/></Relationships>"#);
        package.insert("word/_rels/document.xml.rels".into(), rels.into_bytes());
        package.insert("word/numbers.xml".into(),format!(r#"<w:numbering xmlns:w="{NS}"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="{kind}"/><w:lvlText w:val="%1."/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#).into_bytes());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.docx");
        write(&path, &package);
        let before = fs::read(&path).unwrap();
        let report = importing::import_docx(&path).unwrap();
        assert_eq!(before, fs::read(&path).unwrap());
        assert!(!cell(&report.document).attrs.contains_key("cellFillRgb"));
        assert!(
            report
                .compatibility
                .risks
                .iter()
                .any(|s| s.contains("identity could not be bound"))
        );
        assert!(matches!(
            cell(&report.document).content[0].kind.as_str(),
            "orderedList" | "bulletList"
        ));
    }
}
#[test]
fn inherited_background_never_proves_automatic_foreground() {
    let styles = r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:pPr><w:shd w:val="clear" w:fill="000000"/></w:pPr></w:style>"#;
    let report = import(&table(FILL, P), styles);
    assert_eq!(cell(&report.document).attrs["cellFillRgb"], "E97132");
    assert!(
        !cell(&report.document)
            .attrs
            .contains_key("cellTextColorMode")
    );
    assert!(
        report
            .compatibility
            .risks
            .iter()
            .any(|s| s.contains("automatic text colors are unproved"))
    );
}
#[test]
fn property_tampering_even_with_repaired_hash_does_not_trust_stale_model() {
    use sha2::{Digest, Sha256};
    let doc = model(Some(json!("E97132")), Some(json!("auto")));
    let (original, _) = encode(&doc).unwrap();
    let dir = tempfile::tempdir().unwrap();
    for (from, to) in [
        ("E97132", "FFFFFF"),
        (r#"<w:color w:val="auto"/>"#, r#"<w:color w:val="000000"/>"#),
    ] {
        let mut p = unzip(&original);
        let raw = String::from_utf8(p["word/document.xml"].clone())
            .unwrap()
            .replace(from, to);
        p.insert("word/document.xml".into(), raw.into_bytes());
        let mut embedded: Value = serde_json::from_slice(&p["rust-wps/model-v2.json"]).unwrap();
        embedded["hashes"]["word/document.xml"] =
            json!(format!("{:x}", Sha256::digest(&p["word/document.xml"])));
        p.insert(
            "rust-wps/model-v2.json".into(),
            serde_json::to_vec(&embedded).unwrap(),
        );
        let path = dir
            .path()
            .join(format!("{}.docx", p["word/document.xml"].len()));
        write(&path, &p);
        assert!(document::load_authored_docx(&path).is_err());
    }
}
