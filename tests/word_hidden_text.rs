//! Hidden source text must not disappear while creating an editable copy.
use rust_wps::{compatibility, document, importing};
use std::{collections::BTreeMap, fs, io::Write, path::Path};

const RISK: &str = "Hidden text cannot be preserved in an editable copy";
const WML: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn fixture(path: &Path, body: &str, styles: &str, style_target: &str) {
    let style_path = office_oxide::core::opc::PartName::new("/word/document.xml")
        .unwrap()
        .resolve_relative(style_target)
        .unwrap();
    let parts = BTreeMap::from([
        ("[Content_Types].xml".to_owned(), format!(r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="{}" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#, style_path.as_str())),
        ("_rels/.rels".into(), r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="office" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.into()),
        ("word/_rels/document.xml.rels".into(), format!(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="{style_target}"/></Relationships>"#)),
        ("word/document.xml".into(), format!(r#"<w:document xmlns:w="{WML}" xmlns:x="{WML}"><w:body>{body}</w:body></w:document>"#)),
        (style_path.as_str().trim_start_matches('/').to_owned(), format!(r#"<w:styles xmlns:w="{WML}" xmlns:x="{WML}">{styles}</w:styles>"#)),
    ]);
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, xml) in parts {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

fn body(property: &str) -> String {
    format!(
        r#"<w:p><w:r><w:t>VisibleProbe</w:t></w:r><w:r><w:rPr>{property}</w:rPr><w:t>HiddenProbe</w:t></w:r></w:p>"#
    )
}

fn assert_refused(path: &Path) {
    let before = fs::read(path).unwrap();
    assert!(
        compatibility::audit_docx(path)
            .unwrap()
            .risks
            .contains(RISK)
    );
    let error = importing::import_docx(path).unwrap_err().to_string();
    assert!(error.contains(RISK), "{error}");
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn hidden_direct_runs_are_refused_before_the_ir_can_drop_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hidden.docx");
    fixture(&path, &body("<w:vanish/>"), "", "styles.xml");
    // This is the underlying loss mechanism, not a fidelity oracle: the
    // source contains both runs but the upstream IR intentionally omits one.
    let parsed = office_oxide::Document::from_reader(
        std::io::Cursor::new(fs::read(&path).unwrap()),
        office_oxide::DocumentFormat::Docx,
    )
    .unwrap();
    let ir = serde_json::to_string(&parsed.to_ir()).unwrap();
    assert!(ir.contains("VisibleProbe"));
    assert!(!ir.contains("HiddenProbe"));
    assert_refused(&path);
}

#[test]
fn enabled_ambiguous_and_alternate_prefix_toggles_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("toggles.docx");
    for property in [
        "<w:vanish/>",
        "<w:vanish></w:vanish>",
        r#"<w:vanish w:val="true"/>"#,
        r#"<w:vanish w:val="1"/>"#,
        r#"<w:vanish w:val="on"/>"#,
        r#"<w:vanish w:val=""/>"#,
        r#"<w:vanish w:val="FALSE"/>"#,
        r#"<w:vanish w:val="no"/>"#,
        r#"<w:vanish w:val=" false "/>"#,
        "<x:vanish/>",
        r#"<x:vanish x:val="false"/>"#,
        r#"<w:vanish val="0"/>"#,
        r#"<w:vanish w:val="false" x:val="true"/>"#,
        r#"<w:vanish x:val="false" w:val="false"/>"#,
    ] {
        fixture(&path, &body(property), "", "styles.xml");
        assert_refused(&path);
    }
    fixture(
        &path,
        &body(r#"<w:vanish w:val="false" w:val="true"/>"#),
        "",
        "styles.xml",
    );
    let before = fs::read(&path).unwrap();
    assert!(compatibility::audit_docx(&path).is_err());
    assert!(importing::import_docx(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn hidden_styles_defaults_and_relocated_style_parts_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("styles.docx");
    for styles in [
        r#"<w:docDefaults><w:rPrDefault><w:rPr><w:vanish/></w:rPr></w:rPrDefault></w:docDefaults>"#,
        r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:rPr><w:vanish/></w:rPr></w:style>"#,
        r#"<w:style w:type="character" w:styleId="Secret"><w:rPr><w:vanish/></w:rPr></w:style>"#,
        r#"<w:style w:type="paragraph" w:styleId="HiddenBase"><w:rPr><w:vanish/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Derived"><w:basedOn w:val="HiddenBase"/></w:style>"#,
        r#"<w:style w:type="table" w:styleId="SecretTable"><w:tblStylePr w:type="firstRow"><w:rPr><w:vanish/></w:rPr></w:tblStylePr></w:style>"#,
    ] {
        // Includes unused declarations and direct false overriding inherited
        // true: deliberately refuse rather than claim effective-style proof.
        for target in [
            "styles.xml",
            "../custom/style-data.bin",
            "/custom/styles.xml",
        ] {
            fixture(&path, &body(r#"<w:vanish w:val="false"/>"#), styles, target);
            assert_refused(&path);
        }
    }
}

#[test]
fn false_only_declarations_preserve_text_drafts_and_export_without_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    for (index, property) in [
        r#"<w:vanish w:val="0"/>"#,
        r#"<w:vanish w:val="false"/>"#,
        r#"<w:vanish w:val="off"></w:vanish>"#,
        r#"<x:vanish w:val="false"/>"#,
        r#"<w:vanish w:val="f&#97;lse"/>"#,
    ]
    .iter()
    .enumerate()
    {
        let source = dir.path().join(format!("visible-{index}.docx"));
        let target = match index % 3 {
            0 => "styles.xml",
            1 => "../custom/styles.xml",
            _ => "../custom/styles.bin",
        };
        fixture(
            &source,
            &body(property),
            &format!(
                "<w:docDefaults><w:rPrDefault><w:rPr>{property}</w:rPr></w:rPrDefault></w:docDefaults>"
            ),
            target,
        );
        let before = fs::read(&source).unwrap();
        assert!(
            !compatibility::audit_docx(&source)
                .unwrap()
                .risks
                .contains(RISK)
        );
        let model = importing::import_docx(&source).unwrap().document;
        let json = serde_json::to_string(&model).unwrap();
        assert!(json.contains("VisibleProbe") && json.contains("HiddenProbe"));
        let draft = dir.path().join(format!("draft-{index}.json"));
        document::save_draft(&model, &draft).unwrap();
        assert_eq!(
            serde_json::to_string(&document::load_draft(&draft).unwrap().unwrap()).unwrap(),
            json
        );
        let output = dir.path().join(format!("export-{index}.docx"));
        document::export_docx(&model, &output).unwrap();
        assert_eq!(
            serde_json::to_string(&document::load_authored_docx(&output).unwrap().unwrap())
                .unwrap(),
            json
        );
        let exported = fs::read(&output).unwrap();
        assert!(document::export_docx(&model, &output).is_err());
        assert!(document::export_docx(&model, &source).is_err());
        assert_eq!(fs::read(&output).unwrap(), exported);
        assert_eq!(fs::read(&source).unwrap(), before);
    }
}

#[test]
fn style_visibility_flags_do_not_mean_hidden_run_text() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("style-ui.docx");
    fixture(
        &source,
        &body(""),
        r#"<w:style w:type="paragraph" w:styleId="Unused"><w:semiHidden/><w:unhideWhenUsed/></w:style>"#,
        "styles.xml",
    );
    assert!(
        !compatibility::audit_docx(&source)
            .unwrap()
            .risks
            .contains(RISK)
    );
    assert!(importing::import_docx(&source).is_ok());
}

fn rewrite_parts(path: &Path, change: impl FnOnce(&mut BTreeMap<String, Vec<u8>>)) {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(fs::read(path).unwrap())).unwrap();
    let mut parts = BTreeMap::new();
    for index in 0..zip.len() {
        let mut part = zip.by_index(index).unwrap();
        let mut bytes = Vec::new();
        part.read_to_end(&mut bytes).unwrap();
        parts.insert(part.name().to_owned(), bytes);
    }
    change(&mut parts);
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, bytes) in parts {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn actual_main_part_cannot_bypass_audit_using_a_standard_named_decoy() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("relocated-main.docx");
    fixture(&source, &body("<w:vanish/>"), "", "styles.xml");
    rewrite_parts(&source, |parts| {
        let actual = parts["word/document.xml"].clone();
        parts.insert("custom/main.bin".into(), actual);
        parts.insert(
            "word/document.xml".into(),
            format!(r#"<w:document xmlns:w="{WML}"><w:body><w:p/></w:body></w:document>"#)
                .into_bytes(),
        );
        for name in ["_rels/.rels", "[Content_Types].xml"] {
            let xml = String::from_utf8(parts[name].clone()).unwrap();
            parts.insert(
                name.into(),
                xml.replace("word/document.xml", "custom/main.bin")
                    .into_bytes(),
            );
        }
    });
    let before = fs::read(&source).unwrap();
    for error in [
        compatibility::audit_docx(&source).unwrap_err(),
        importing::import_docx(&source).unwrap_err(),
    ] {
        assert!(
            error.to_string().contains("standard word/document.xml"),
            "{error}"
        );
    }
    assert_eq!(fs::read(&source).unwrap(), before);
}

#[test]
fn inventory_only_zip_without_content_types_cannot_be_imported() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("incomplete.docx");
    fixture(&source, &body(""), "", "styles.xml");
    rewrite_parts(&source, |parts| {
        parts.remove("[Content_Types].xml");
    });
    let before = fs::read(&source).unwrap();
    assert!(compatibility::audit_docx(&source).is_ok());
    assert!(importing::import_docx(&source).is_err());
    assert_eq!(fs::read(&source).unwrap(), before);
}

#[test]
fn content_type_casing_and_utf16_main_are_audited_as_the_parser_reads_them() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("utf16.docx");
    fixture(&source, &body("<w:vanish/>"), "", "styles.xml");
    rewrite_parts(&source, |parts| {
        let types = parts.remove("[Content_Types].xml").unwrap();
        parts.insert("[content_types].xml".into(), types);
        let main = String::from_utf8(parts["word/document.xml"].clone()).unwrap();
        let xml = format!("<?xml version=\"1.0\" encoding=\"UTF-16\"?>{main}");
        let mut bytes = vec![0xff, 0xfe];
        for unit in xml.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        parts.insert("word/document.xml".into(), bytes);
    });
    assert_refused(&source);

    // With the same case-variant Content Types, a real nonstandard main must
    // still be rejected even though a standard-named part is present.
    rewrite_parts(&source, |parts| {
        parts.insert("custom/main.xml".into(), parts["word/document.xml"].clone());
        let rels = String::from_utf8(parts["_rels/.rels"].clone()).unwrap();
        parts.insert(
            "_rels/.rels".into(),
            rels.replace("word/document.xml", "custom/main.xml")
                .into_bytes(),
        );
    });
    let before = fs::read(&source).unwrap();
    let error = importing::import_docx(&source).unwrap_err().to_string();
    assert!(error.contains("standard word/document.xml"), "{error}");
    assert_eq!(fs::read(&source).unwrap(), before);
}
