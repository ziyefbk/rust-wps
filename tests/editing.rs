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

const REPLACEMENT_MARKERS: [&str; 6] = [
    "<w:softHyphen/>",
    "<w:softHyphen></w:softHyphen>",
    "<w:noBreakHyphen/>",
    "<w:noBreakHyphen></w:noBreakHyphen>",
    "<w:sym w:font=\"Symbol\" w:char=\"F0B7\"/>",
    "<w:sym w:font=\"Symbol\" w:char=\"F0B7\"></w:sym>",
];

fn write_replacement_package(path: &std::path::Path, package: &BTreeMap<String, Vec<u8>>) {
    let mut writer = ZipWriter::new(fs::File::create(path).unwrap());
    for (name, bytes) in package {
        writer
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap();
}

fn replacement_marker_fixture(path: &std::path::Path, body: &str) {
    fixture(path, false);
    let mut package = parts(path);
    package.insert(
        "word/document.xml".into(),
        format!("<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}</w:body></w:document>").into_bytes(),
    );
    write_replacement_package(path, &package);
}

fn assert_marker_refusal(
    source: &std::path::Path,
    output: &std::path::Path,
    find: &str,
    replacement: &str,
) {
    let original = fs::read(source).unwrap();
    let error = rust_wps::editing::replace_docx(source, output, find, replacement).unwrap_err();
    assert!(
        error.to_string().contains("hyphen or symbol marker"),
        "{error}"
    );
    assert!(!output.exists());
    assert_eq!(fs::read(source).unwrap(), original);
}

#[test]
fn replacement_refuses_crossing_hyphen_and_symbol_markers_in_both_xml_forms() {
    let dir = tempfile::tempdir().unwrap();
    for (index, marker) in REPLACEMENT_MARKERS.iter().enumerate() {
        let source = dir.path().join(format!("marker-{index}.docx"));
        let output = dir.path().join(format!("out-{index}.docx"));
        replacement_marker_fixture(
            &source,
            &format!("<w:p><w:r><w:t>AB</w:t>{marker}<w:t>CD</w:t></w:r></w:p>"),
        );
        assert_marker_refusal(&source, &output, "BC", "X");
    }
}

#[test]
fn replacement_allows_exact_marker_edges_and_preserves_every_other_payload() {
    let dir = tempfile::tempdir().unwrap();
    for (index, marker) in REPLACEMENT_MARKERS.iter().enumerate() {
        let source = dir.path().join(format!("marker-{index}.docx"));
        replacement_marker_fixture(
            &source,
            &format!("<w:p><w:r><w:t>AB</w:t>{marker}<w:t>CD</w:t></w:r></w:p>"),
        );
        let original = fs::read(&source).unwrap();
        let before = parts(&source);
        for find in ["AB", "CD"] {
            let output = dir.path().join(format!("out-{index}-{find}.docx"));
            let report = rust_wps::editing::replace_docx(&source, &output, find, "XY").unwrap();
            assert_eq!(report.replacements, 1);
            assert_eq!(report.changed_parts, vec!["word/document.xml"]);
            let mut expected = before.clone();
            expected.insert(
                "word/document.xml".into(),
                String::from_utf8(before["word/document.xml"].clone())
                    .unwrap()
                    .replace(&format!("<w:t>{find}</w:t>"), "<w:t>XY</w:t>")
                    .into_bytes(),
            );
            assert_eq!(parts(&output), expected);
            assert_eq!(fs::read(&source).unwrap(), original);
            let output_bytes = fs::read(&output).unwrap();
            assert!(rust_wps::editing::replace_docx(&source, &output, find, "Z").is_err());
            assert_eq!(fs::read(&output).unwrap(), output_bytes);
        }
    }
}

#[test]
fn replacement_marker_refusal_is_atomic_across_safe_matches_and_empty_replacement() {
    let dir = tempfile::tempdir().unwrap();
    for (index, marker) in REPLACEMENT_MARKERS.iter().enumerate() {
        let source = dir.path().join(format!("marker-{index}.docx"));
        replacement_marker_fixture(
            &source,
            &format!(
                "<w:p><w:r><w:t>ABC</w:t></w:r></w:p><w:p><w:r><w:t>AB</w:t>{marker}<w:t>C</w:t></w:r></w:p>"
            ),
        );
        for (case, replacement) in ["X", ""].iter().enumerate() {
            assert_marker_refusal(
                &source,
                &dir.path().join(format!("out-{index}-{case}.docx")),
                "ABC",
                replacement,
            );
        }
    }
}

#[test]
fn replacement_marker_offsets_use_decoded_unicode_and_leave_literal_hyphens_matchable() {
    let dir = tempfile::tempdir().unwrap();
    for (index, marker) in REPLACEMENT_MARKERS.iter().enumerate() {
        let source = dir.path().join(format!("marker-{index}.docx"));
        replacement_marker_fixture(
            &source,
            &format!(
                "<w:p><w:r><w:t>甲&#x1F642;&amp;</w:t>{marker}<w:t>乙&#x4E19;</w:t></w:r></w:p>"
            ),
        );
        assert_marker_refusal(
            &source,
            &dir.path().join(format!("out-{index}.docx")),
            "🙂&乙",
            "X",
        );
    }
    for (index, text) in ["A\u{00ad}B", "A\u{2011}B"].iter().enumerate() {
        let source = dir.path().join(format!("literal-{index}.docx"));
        let output = dir.path().join(format!("literal-out-{index}.docx"));
        replacement_marker_fixture(&source, &format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>"));
        let original = fs::read(&source).unwrap();
        assert_eq!(
            rust_wps::editing::replace_docx(&source, &output, text, "XY")
                .unwrap()
                .replacements,
            1
        );
        assert_eq!(fs::read(&source).unwrap(), original);
        assert_eq!(rust_wps::inspect(&output).unwrap().text, "XY");
    }
}

#[test]
fn replacement_markers_are_local_and_escaped_text_is_not_a_marker() {
    let dir = tempfile::tempdir().unwrap();
    for (index, marker) in REPLACEMENT_MARKERS.iter().enumerate() {
        let source = dir.path().join(format!("marker-{index}.docx"));
        let output = dir.path().join(format!("out-{index}.docx"));
        replacement_marker_fixture(
            &source,
            &format!(
                "<w:p><w:r><w:t>AB</w:t>{marker}{marker}<w:t>CD</w:t></w:r></w:p><w:p><w:r><w:t>SAFE &lt;w:softHyphen/&gt; TEXT</w:t></w:r></w:p>"
            ),
        );
        assert_marker_refusal(
            &source,
            &dir.path().join(format!("blocked-{index}.docx")),
            "BC",
            "X",
        );
        let original = fs::read(&source).unwrap();
        let report =
            rust_wps::editing::replace_docx(&source, &output, "SAFE <w:softHyphen/> TEXT", "XY")
                .unwrap();
        assert_eq!(report.replacements, 1);
        assert_eq!(fs::read(&source).unwrap(), original);
        let mut expected = parts(&source);
        let xml = String::from_utf8(expected["word/document.xml"].clone()).unwrap();
        expected.insert(
            "word/document.xml".into(),
            xml.replace("SAFE &lt;w:softHyphen/&gt; TEXT", "XY")
                .into_bytes(),
        );
        assert_eq!(parts(&output), expected);
    }
}

#[test]
fn replacement_marker_in_any_referenced_story_refuses_the_whole_package() {
    let dir = tempfile::tempdir().unwrap();
    for (index, (kind, root, wrapper)) in [
        ("header", "hdr", None),
        ("footer", "ftr", None),
        ("footnotes", "footnotes", Some("footnote")),
        ("endnotes", "endnotes", Some("endnote")),
        ("comments", "comments", Some("comment")),
    ]
    .iter()
    .enumerate()
    {
        let source = dir.path().join(format!("story-{index}.docx"));
        let output = dir.path().join(format!("story-out-{index}.docx"));
        replacement_marker_fixture(&source, "<w:p><w:r><w:t>ABC</w:t></w:r></w:p>");
        let mut package = parts(&source);
        let paragraph = "<w:p><w:r><w:t>AB</w:t><w:softHyphen/><w:t>C</w:t></w:r></w:p>";
        let content = wrapper.map_or_else(
            || paragraph.to_string(),
            |name| format!("<w:{name} w:id=\"1\">{paragraph}</w:{name}>"),
        );
        package.insert(format!("word/renamed-{index}.xml"), format!("<w:{root} xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">{content}</w:{root}>").into_bytes());
        package.insert("word/_rels/document.xml.rels".into(), format!("<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"story\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}\" Target=\"renamed-{index}.xml\"/></Relationships>").into_bytes());
        let content_types = String::from_utf8(package["[Content_Types].xml"].clone()).unwrap();
        package.insert("[Content_Types].xml".into(), content_types.replace("</Types>", &format!("<Override PartName=\"/word/renamed-{index}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.{kind}+xml\"/></Types>")).into_bytes());
        write_replacement_package(&source, &package);
        assert_marker_refusal(&source, &output, "ABC", "X");
    }
}
