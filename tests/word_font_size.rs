mod common;
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

fn model(marks: Value) -> Node {
    serde_json::from_value(
        json!({"type":"doc","content":[{"type":"paragraph","content":[
            {"type":"text","text":"中文 Latin سلام 😀","marks":marks}
        ]}]}),
    )
    .unwrap()
}

fn parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..zip.len())
        .map(|index| {
            let mut entry = zip.by_index(index).unwrap();
            let mut data = Vec::new();
            entry.read_to_end(&mut data).unwrap();
            (entry.name().to_owned(), data)
        })
        .collect()
}

fn size(node: &Node) -> Option<u64> {
    node.marks
        .iter()
        .find(|mark| mark.kind == "fontSize")
        .map(|mark| mark.attrs["halfPoints"].as_u64().unwrap())
}

fn external_docx(path: &Path, body: &str, styles: Option<&str>) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let main = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#
    );
    for (name, xml) in [
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="office" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        (
            "word/_rels/document.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#,
        ),
        ("word/document.xml", &main),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    let styles = styles.unwrap_or(
        r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#,
    );
    zip.start_file("word/styles.xml", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(styles.as_bytes()).unwrap();
    zip.finish().unwrap();
}

#[test]
fn half_point_sizes_roundtrip_draft_ooxml_and_authored_package() {
    let dir = tempfile::tempdir().unwrap();
    for half_points in [2, 22, 27, 144, 3276] {
        let original = model(json!([
            {"type":"bold"}, {"type":"fontSize","attrs":{"halfPoints":half_points}},
            {"type":"link","attrs":{"href":"https://example.com/字号"}}
        ]));
        let draft = dir.path().join("draft.json");
        document::save_draft(&original, &draft).unwrap();
        assert_eq!(
            serde_json::to_value(document::load_draft(&draft).unwrap().unwrap()).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        let output = dir.path().join(format!("size-{half_points}.docx"));
        document::export_docx(&original, &output).unwrap();
        let archive = parts(&fs::read(&output).unwrap());
        let xml = std::str::from_utf8(&archive["word/document.xml"]).unwrap();
        assert_eq!(
            xml.matches(&format!("<w:sz w:val=\"{half_points}\"/>"))
                .count(),
            1
        );
        assert_eq!(
            xml.matches(&format!("<w:szCs w:val=\"{half_points}\"/>"))
                .count(),
            1
        );
        assert_eq!(
            serde_json::to_value(document::load_authored_docx(&output).unwrap().unwrap()).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        let imported = importing::import_docx(&output).unwrap();
        let text = &imported.document.content[0].content;
        assert_eq!(
            text.iter()
                .filter_map(|node| node.text.as_deref())
                .collect::<String>(),
            original.content[0].content[0].text.as_deref().unwrap()
        );
        for run in text {
            assert_eq!(size(run), Some(half_points));
            assert!(run.marks.iter().any(
                |mark| mark.kind == "link" && mark.attrs["href"] == "https://example.com/字号"
            ));
        }
    }
}

#[test]
fn invalid_font_sizes_never_replace_a_draft_or_publish_a_package() {
    let dir = tempfile::tempdir().unwrap();
    let draft = dir.path().join("draft.json");
    document::save_draft(&model(json!([])), &draft).unwrap();
    let before = fs::read(&draft).unwrap();
    let mut invalid: Vec<Value> = [
        json!(null),
        json!(true),
        json!("27"),
        json!("13.5pt"),
        json!(-2),
        json!(0),
        json!(1),
        json!(3277),
        json!(27.0),
        json!(27.5),
        json!(u64::MAX),
    ]
    .into_iter()
    .map(|value| json!([{"type":"fontSize","attrs":{"halfPoints":value}}]))
    .collect();
    invalid.extend([
        json!([{"type":"fontSize"}]),
        json!([{"type":"fontSize","attrs":{"halfPoints":27,"size":27}}]),
        json!([{"type":"fontSize","attrs":{"size":27}}]),
        json!([{"type":"fontSize","attrs":{"halfPoints":27}},{"type":"fontSize","attrs":{"halfPoints":27}}]),
        json!([{"type":"fontSize","attrs":{"halfPoints":27}},{"type":"fontSize","attrs":{"halfPoints":30}}]),
    ]);
    for (index, marks) in invalid.into_iter().enumerate() {
        let original = model(marks);
        assert!(document::validate(&original).is_err(), "case {index}");
        assert!(
            document::save_draft(&original, &draft).is_err(),
            "case {index}"
        );
        assert_eq!(fs::read(&draft).unwrap(), before, "case {index}");
        let output = dir.path().join(format!("invalid-{index}.docx"));
        assert!(
            document::export_docx(&original, &output).is_err(),
            "case {index}"
        );
        assert!(!output.exists(), "case {index}");
        fs::write(&output, b"existing document").unwrap();
        assert!(document::export_docx(&original, &output).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"existing document");
    }
}

#[test]
fn previous_export_without_size_marks_still_reopens_and_encodes_identical_parts() {
    // Captured with the unmodified CLI before adding fontSize support, from the
    // project's self-authored authoring.json. No external corpus or tmp needed.
    let baseline = include_bytes!("fixtures/authoring-before-font-size.docx");
    let original: Node = serde_json::from_str(include_str!("fixtures/authoring.json")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("old.docx");
    fs::write(&source, baseline).unwrap();
    assert_eq!(
        serde_json::to_value(document::load_authored_docx(&source).unwrap().unwrap()).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
    let output = dir.path().join("new.docx");
    document::export_docx(&original, &output).unwrap();
    assert_eq!(parts(baseline), parts(&fs::read(&output).unwrap()));
    assert_eq!(fs::read(&source).unwrap(), baseline);
}

#[test]
fn explicit_styles_and_script_sizes_reach_the_editable_model() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("styles.docx");
    external_docx(
        &source,
        r#"
        <w:p><w:r><w:t>默认</w:t></w:r></w:p>
        <w:p><w:pPr><w:pStyle w:val="Child"/></w:pPr><w:r><w:t>继承</w:t></w:r></w:p>
        <w:p><w:pPr><w:pStyle w:val="Child"/></w:pPr><w:r><w:rPr><w:rStyle w:val="Accent"/></w:rPr><w:t>字符</w:t></w:r></w:p>
        <w:p><w:pPr><w:pStyle w:val="Child"/></w:pPr><w:r><w:rPr><w:rStyle w:val="Accent"/><w:sz w:val="45"/></w:rPr><w:t>直接</w:t></w:r></w:p>
        <w:p><w:r><w:rPr><w:sz w:val="26"/><w:szCs w:val="31"/></w:rPr><w:t>Latin中文سلام</w:t></w:r></w:p>
        <w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:t>标题Latinسلام</w:t></w:r></w:p>
        <w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:rPr><w:rStyle w:val="Accent"/><w:sz w:val="29"/></w:rPr><w:t>直接标题سلام</w:t></w:r></w:p>
    "#,
        Some(
            r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
        <w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults>
        <w:style w:type="paragraph" w:default="1" w:styleId="CustomDefault"><w:rPr><w:sz w:val="33"/><w:szCs w:val="37"/></w:rPr></w:style>
        <w:style w:type="paragraph" w:styleId="Base"><w:rPr><w:sz w:val="24"/></w:rPr></w:style>
        <w:style w:type="paragraph" w:styleId="Child"><w:basedOn w:val="Base"/><w:rPr><w:sz w:val="27"/></w:rPr></w:style>
        <w:style w:type="character" w:styleId="Accent"><w:rPr><w:sz w:val="35"/></w:rPr></w:style>
        <w:style w:type="paragraph" w:styleId="Heading2"><w:basedOn w:val="Child"/><w:pPr><w:outlineLvl w:val="1"/></w:pPr><w:rPr><w:b/><w:szCs w:val="39"/></w:rPr></w:style>
    </w:styles>"#,
        ),
    );
    let before = fs::read(&source).unwrap();
    let imported = importing::import_docx(&source).unwrap();
    for (index, expected) in [33, 27, 35, 45].into_iter().enumerate() {
        assert_eq!(
            size(&imported.document.content[index].content[0]),
            Some(expected)
        );
    }
    let mixed = &imported.document.content[4].content;
    assert_eq!(mixed.len(), 2);
    assert_eq!(
        (mixed[0].text.as_deref(), size(&mixed[0])),
        (Some("Latin中文"), Some(26))
    );
    assert_eq!(
        (mixed[1].text.as_deref(), size(&mixed[1])),
        (Some("سلام"), Some(31))
    );
    for (index, latin_size) in [(5, 27), (6, 29)] {
        let heading = &imported.document.content[index];
        assert_eq!(heading.kind, "heading");
        assert_eq!(heading.content.len(), 2);
        assert_eq!(size(&heading.content[0]), Some(latin_size));
        assert_eq!(size(&heading.content[1]), Some(39));
    }
    let output = dir.path().join("converted.docx");
    document::export_docx(&imported.document, &output).unwrap();
    assert_eq!(
        serde_json::to_value(document::load_authored_docx(&output).unwrap().unwrap()).unwrap(),
        serde_json::to_value(&imported.document).unwrap()
    );
    let mut expected = imported.document.clone();
    common::materialize_standard_spacing(&mut expected);
    for block in &mut expected.content {
        for run in &mut block.content {
            // External re-conversion resolves V2's declared black default.
            run.marks.push(
                serde_json::from_value(json!({"type":"textColor","attrs":{"rgb":"000000"}}))
                    .unwrap(),
            );
        }
    }
    assert_eq!(
        common::without_font_segmentation(&importing::import_docx(&output).unwrap().document),
        common::without_font_segmentation(&expected)
    );
    assert_eq!(fs::read(&source).unwrap(), before);
}

#[test]
fn unknown_size_is_not_invented_and_invalid_external_sizes_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("unsized.docx");
    external_docx(
        &source,
        "<w:p><w:r><w:t>unknown size</w:t></w:r></w:p>",
        None,
    );
    let imported = importing::import_docx(&source).unwrap();
    assert_eq!(size(&imported.document.content[0].content[0]), None);
    for value in [0, 1, 3277, u32::MAX] {
        let source = dir.path().join(format!("invalid-{value}.docx"));
        external_docx(
            &source,
            &format!(
                "<w:p><w:r><w:rPr><w:sz w:val=\"{value}\"/></w:rPr><w:t>invalid size</w:t></w:r></w:p>"
            ),
            None,
        );
        let before = fs::read(&source).unwrap();
        assert!(
            importing::import_docx(&source)
                .unwrap_err()
                .to_string()
                .contains("font size outside")
        );
        assert_eq!(fs::read(&source).unwrap(), before);
    }
}

#[test]
fn external_size_units_are_exact_and_unrepresentable_sizes_do_not_import() {
    let dir = tempfile::tempdir().unwrap();
    for (index, value, expected) in [(0, "13.5pt", 27), (1, "1in", 144), (2, "2.54cm", 144)] {
        let source = dir.path().join(format!("units-{index}.docx"));
        external_docx(
            &source,
            &format!(
                "<w:p><w:r><w:rPr><w:sz w:val=\"{value}\"/><w:szCs w:val=\"{value}\"/></w:rPr><w:t>Latinسلام</w:t></w:r></w:p>"
            ),
            None,
        );
        let imported = importing::import_docx(&source).unwrap();
        assert!(
            imported.document.content[0]
                .content
                .iter()
                .all(|node| size(node) == Some(expected))
        );
    }
    for (index, value) in ["13.25pt", "27.5", "13px", "13.5000000000000001pt"]
        .into_iter()
        .enumerate()
    {
        let source = dir.path().join(format!("invalid-unit-{index}.docx"));
        external_docx(
            &source,
            &format!(
                "<w:p><w:r><w:rPr><w:szCs w:val=\"{value}\"/></w:rPr><w:t>سلام</w:t></w:r></w:p>"
            ),
            None,
        );
        let before = fs::read(&source).unwrap();
        assert!(
            importing::import_docx(&source).is_err(),
            "must not round or strip units: {value}"
        );
        assert_eq!(fs::read(&source).unwrap(), before);
    }
}

#[test]
fn changing_only_embedded_font_size_cannot_override_the_visible_document() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.docx");
    document::export_docx(
        &model(json!([{"type":"fontSize","attrs":{"halfPoints":27}}])),
        &source,
    )
    .unwrap();
    let mut archive = parts(&fs::read(&source).unwrap());
    let mut metadata: Value = serde_json::from_slice(&archive["rust-wps/model-v1.json"]).unwrap();
    metadata["document"]["content"][0]["content"][0]["marks"][0]["attrs"]["halfPoints"] = json!(29);
    archive.insert(
        "rust-wps/model-v1.json".into(),
        serde_json::to_vec(&metadata).unwrap(),
    );
    let forged = dir.path().join("forged.docx");
    let mut zip = zip::ZipWriter::new(fs::File::create(&forged).unwrap());
    for (name, data) in archive {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&data).unwrap();
    }
    zip.finish().unwrap();
    assert!(
        document::load_authored_docx(&forged)
            .unwrap_err()
            .to_string()
            .contains("disagrees")
    );
}
