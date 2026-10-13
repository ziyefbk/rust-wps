use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

fn fixture(path: &std::path::Path, signed: bool) {
    let mut writer = ZipWriter::new(fs::File::create(path).unwrap());
    let ct = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="bin" ContentType="application/octet-stream"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rel = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let body = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:rPr><w:b/></w:rPr><w:t>你好</w:t></w:r><w:r><w:t>世界 &amp; Office</w:t></w:r></w:p><w:p><w:r><w:t>你好</w:t></w:r></w:p><w:p><w:r><w:t>世界</w:t></w:r></w:p></w:body></w:document>"#;
    for (name, bytes) in [
        ("[Content_Types].xml", ct.as_bytes()),
        ("_rels/.rels", rel.as_bytes()),
        ("word/document.xml", body.as_bytes()),
        (
            "customXml/item1.xml",
            b"<custom unknown=\"preserve\">opaque payload</custom>",
        ),
        ("word/media/image1.bin", &[0, 255, 1, 2, 3]),
    ] {
        writer
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    if signed {
        writer
            .start_file("_xmlsignatures/sig1.xml", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"<Signature/>").unwrap();
    }
    writer.finish().unwrap();
}

fn parts(path: &std::path::Path) -> BTreeMap<String, Vec<u8>> {
    let mut archive = ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    let mut result = BTreeMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        result.insert(entry.name().into(), data);
    }
    result
}

#[test]
fn replaces_across_runs_keeps_format_and_opaque_parts() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.docx");
    let output = dir.path().join("edited.docx");
    fixture(&source, false);
    let original = fs::read(&source).unwrap();
    let report =
        rust_wps::editing::replace_docx(&source, &output, "你好世界", " <新 & 文本> ").unwrap();
    assert_eq!(report.replacements, 1); // Never matches across the two later paragraphs.
    assert_eq!(report.changed_parts, vec!["word/document.xml"]);
    assert_eq!(report.preserved_parts, 4);
    assert_eq!(original, fs::read(&source).unwrap());
    let before = parts(&source);
    let after = parts(&output);
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>()
    );
    for (name, bytes) in &before {
        if name != "word/document.xml" {
            assert_eq!(bytes, &after[name], "{name}");
        }
    }
    let xml = String::from_utf8(after["word/document.xml"].clone()).unwrap();
    assert!(xml.contains("<w:b/>"));
    assert!(xml.contains("xml:space=\"preserve\""));
    assert!(xml.contains("&lt;新 &amp; 文本&gt;"));
    assert!(
        rust_wps::inspect(&output)
            .unwrap()
            .text
            .contains("<新 & 文本>")
    );
}

#[test]
fn failures_leave_source_and_existing_destination_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.docx");
    fixture(&source, false);
    let bytes = fs::read(&source).unwrap();
    let output = dir.path().join("edited.docx");
    for (find, replacement) in [
        ("", "x"),
        ("你好", "bad\0"),
        ("你好", "new\nline"),
        ("missing", "x"),
        ("你好", "你好"),
    ] {
        assert!(rust_wps::editing::replace_docx(&source, &output, find, replacement).is_err());
        assert!(!output.exists());
        assert_eq!(bytes, fs::read(&source).unwrap());
    }
    fs::write(&output, b"existing data").unwrap();
    assert!(rust_wps::editing::replace_docx(&source, &output, "你好", "Hello").is_err());
    assert_eq!(fs::read(&output).unwrap(), b"existing data");
    assert!(rust_wps::editing::replace_docx(&source, &source, "你好", "Hello").is_err());
    assert_eq!(bytes, fs::read(&source).unwrap());
    let alias = dir.path().join("alias.docx");
    fs::hard_link(&source, &alias).unwrap();
    assert!(rust_wps::editing::replace_docx(&source, &alias, "你好", "Hello").is_err());
    assert_eq!(bytes, fs::read(&source).unwrap());
}

#[test]
fn refuses_signed_packages() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("signed.docx");
    let output = dir.path().join("edited.docx");
    fixture(&source, true);
    let error = rust_wps::editing::replace_docx(&source, &output, "你好", "Hello").unwrap_err();
    assert!(error.to_string().contains("Signed DOCX"));
    assert!(!output.exists());
}

#[test]
fn rejects_false_matches_across_tabs_breaks_and_fields() {
    let dir = tempfile::tempdir().unwrap();
    for (index, barrier) in [
        "<w:tab/>",
        "<w:br/>",
        "<w:fldChar w:fldCharType=\"begin\"/>",
        "<w:drawing/>",
    ]
    .iter()
    .enumerate()
    {
        let source = dir.path().join(format!("barrier-{index}.docx"));
        let output = dir.path().join(format!("out-{index}.docx"));
        fixture(&source, false);
        let mut package = parts(&source);
        let xml = String::from_utf8(package["word/document.xml"].clone()).unwrap();
        package.insert(
            "word/document.xml".into(),
            xml.replacen("<w:t>世界", &format!("{barrier}<w:t>世界"), 1)
                .into_bytes(),
        );
        let mut zip = ZipWriter::new(fs::File::create(&source).unwrap());
        for (name, bytes) in &package {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        let error = rust_wps::editing::replace_docx(&source, &output, "你好世界", "X").unwrap_err();
        assert!(error.to_string().contains("cross a tab"), "{error}");
        assert!(!output.exists());
    }
}
