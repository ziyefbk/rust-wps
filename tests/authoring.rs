use rust_wps::document::{Node, export_docx, load_draft, save_draft};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Cursor, Read},
};

fn fixture() -> Node {
    serde_json::from_str(include_str!("fixtures/authoring.json")).unwrap()
}
fn part(path: &std::path::Path, name: &str) -> String {
    let mut zip = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    let mut text = String::new();
    zip.by_name(name)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}

#[test]
fn manual_page_break_exports_and_reopens_without_losing_following_text() {
    let dir = tempfile::tempdir().unwrap();
    let doc: Node = serde_json::from_value(json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"第一章"}]},{"type":"pageBreak"},{"type":"paragraph","content":[{"type":"text","text":"第二章"}]}]})).unwrap();
    let output = dir.path().join("pages.docx");
    export_docx(&doc, &output).unwrap();
    assert_eq!(
        part(&output, "word/document.xml")
            .matches("w:type=\"page\"")
            .count(),
        1
    );
    let reopened = rust_wps::document::load_authored_docx(&output)
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_value(reopened).unwrap(),
        serde_json::to_value(&doc).unwrap()
    );
    let imported = rust_wps::importing::import_docx(&output).unwrap();
    assert_eq!(
        imported
            .document
            .content
            .iter()
            .filter(|n| n.kind == "pageBreak")
            .count(),
        1
    );
    let text = rust_wps::inspect(&output).unwrap().text;
    assert!(text.contains("第一章") && text.contains("第二章"));
    let nested: Node = serde_json::from_value(json!({"type":"doc","content":[{"type":"bulletList","content":[{"type":"listItem","content":[{"type":"paragraph"},{"type":"pageBreak"}]}]}]})).unwrap();
    assert!(rust_wps::document::validate(&nested).is_err());
    for (name, sequence) in [
        ("leading", vec!["pageBreak", "paragraph"]),
        (
            "consecutive",
            vec!["paragraph", "pageBreak", "pageBreak", "paragraph"],
        ),
        ("trailing", vec!["paragraph", "pageBreak"]),
    ] {
        let expected = sequence.iter().filter(|kind| **kind == "pageBreak").count();
        let doc: Node = serde_json::from_value(json!({"type":"doc","content":sequence.iter().map(|kind| if *kind == "paragraph" { json!({"type":"paragraph","content":[{"type":"text","text":"正文"}]}) } else { json!({"type":kind}) }).collect::<Vec<_>>()})).unwrap();
        let output = dir.path().join(format!("{name}.docx"));
        export_docx(&doc, &output).unwrap();
        let imported = rust_wps::importing::import_docx(&output).unwrap();
        assert_eq!(
            imported
                .document
                .content
                .iter()
                .filter(|n| n.kind == "pageBreak")
                .count(),
            expected,
            "{name}"
        );
        let reopened = rust_wps::document::load_authored_docx(&output)
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::to_value(reopened).unwrap(),
            serde_json::to_value(doc).unwrap()
        );
    }
}

#[test]
fn paragraph_and_heading_alignment_survives_ooxml_and_model_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    for alignment in ["left", "center", "right", "justify"] {
        for kind in ["paragraph", "heading"] {
            let mut attrs = json!({"textAlign":alignment});
            if kind == "heading" {
                attrs["level"] = json!(2);
            }
            let doc: Node = serde_json::from_value(json!({"type":"doc","content":[{"type":kind,"attrs":attrs,"content":[{"type":"text","text":"中文对齐"}]}]})).unwrap();
            let output = dir.path().join(format!("{kind}-{alignment}.docx"));
            export_docx(&doc, &output).unwrap();
            let ooxml = if alignment == "justify" {
                "both"
            } else {
                alignment
            };
            assert!(
                part(&output, "word/document.xml").contains(&format!("<w:jc w:val=\"{ooxml}\"/>"))
            );
            let imported = rust_wps::importing::import_docx(&output).unwrap();
            assert_eq!(
                imported.document.content[0]
                    .attrs
                    .get("textAlign")
                    .and_then(Value::as_str)
                    .unwrap_or("left"),
                alignment,
                "{kind}-{alignment}"
            );
            let reopened = rust_wps::document::load_authored_docx(&output)
                .unwrap()
                .unwrap();
            assert_eq!(
                serde_json::to_value(reopened).unwrap(),
                serde_json::to_value(doc).unwrap()
            );
        }
    }
}

#[test]
fn links_keep_targets_styles_and_authored_model_across_docx_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    for href in [
        "https://example.com/中文?q=1&v=2",
        "mailto:editor@example.com",
        "http://example.com",
    ] {
        let doc: Node = serde_json::from_value(json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"链接正文","marks":[{"type":"bold"},{"type":"link","attrs":{"href":href}}]}]}]})).unwrap();
        let output = dir.path().join(format!("{}.docx", href.len()));
        export_docx(&doc, &output).unwrap();
        assert!(part(&output, "word/document.xml").contains("<w:hyperlink r:id=\"link1\"><w:r>"));
        let rels = part(&output, "word/_rels/document.xml.rels");
        assert!(rels.contains("TargetMode=\"External\""));
        assert!(rels.contains(&quick_xml::escape::escape(href).to_string()));
        let reopened = rust_wps::document::load_authored_docx(&output)
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::to_value(&reopened).unwrap(),
            serde_json::to_value(&doc).unwrap()
        );
        let imported = rust_wps::importing::import_docx(&output).unwrap();
        let marks = &imported.document.content[0].content[0].marks;
        assert!(
            marks
                .iter()
                .any(|m| m.kind == "link" && m.attrs["href"] == href)
        );
        assert!(marks.iter().any(|m| m.kind == "bold"));
    }
    for href in [
        "javascript:alert(1)",
        "file:///C:/test",
        "data:text/html,test",
        "#bookmark",
        "https://example.com/\n",
    ] {
        let doc: Node = serde_json::from_value(json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"链接","marks":[{"type":"link","attrs":{"href":href}}]}]}]})).unwrap();
        let output = dir.path().join("rejected.docx");
        assert!(export_docx(&doc, &output).is_err());
        assert!(!output.exists());
    }
}

#[test]
fn exports_unicode_format_lists_tables_and_reopens() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("新文档.docx");
    let report = export_docx(&fixture(), &output).unwrap();
    assert_eq!(report.blocks, 4);
    let reopened_model = rust_wps::document::load_authored_docx(&output)
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_value(&reopened_model).unwrap(),
        serde_json::to_value(fixture()).unwrap()
    );
    let inspection = rust_wps::inspect(&output).unwrap();
    for text in [
        "Rust 办公文档",
        "中文 & <正文>",
        "第二行",
        "第一个编号项目",
        "嵌套列表",
        "姓名",
        "100",
    ] {
        assert!(
            inspection.text.contains(text),
            "missing {text}: {}",
            inspection.text
        );
    }
    let xml = part(&output, "word/document.xml");
    for token in [
        "<w:b/>",
        "<w:i/>",
        "<w:u",
        "<w:br/>",
        "<w:tab/>",
        "Heading1",
        "<w:tbl>",
        "<w:tblHeader/>",
        "<w:numPr>",
        "&amp; &lt;正文&gt;",
    ] {
        assert!(xml.contains(token), "{token}");
    }
    let nums = part(&output, "word/numbering.xml");
    assert!(nums.contains("<w:startOverride w:val=\"3\"/>"));
    assert!(nums.contains("<w:lvlOverride w:ilvl=\"1\">"));
    assert!(nums.contains("w:numId=\"2\""));
    let mut reader = quick_xml::Reader::from_str(&xml);
    while !matches!(reader.read_event().unwrap(), quick_xml::events::Event::Eof) {}
}

#[test]
fn refuses_stale_model_after_content_or_metadata_only_changes() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("original.docx");
    export_docx(&fixture(), &source).unwrap();
    let replaced = dir.path().join("replaced.docx");
    rust_wps::editing::replace_docx(&source, &replaced, "Rust", "Changed").unwrap();
    assert!(rust_wps::document::load_authored_docx(&replaced).is_err());
    assert!(
        rust_wps::inspect(&replaced)
            .unwrap()
            .text
            .contains("Changed")
    );
    let mut zip = zip::ZipArchive::new(fs::File::open(&source).unwrap()).unwrap();
    let forged = dir.path().join("model-only.docx");
    let mut out = zip::ZipWriter::new(fs::File::create(&forged).unwrap());
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        let name = entry.name().to_string();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        if name == "rust-wps/model-v1.json" {
            let mut model: Value = serde_json::from_slice(&data).unwrap();
            model["document"]["content"][0]["content"][0]["text"] = json!("Outdated metadata");
            data = serde_json::to_vec(&model).unwrap();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        out.write_all(&data).unwrap();
    }
    out.finish().unwrap();
    assert!(
        rust_wps::document::load_authored_docx(&forged)
            .unwrap_err()
            .to_string()
            .contains("disagrees")
    );
}

#[test]
fn refuses_removed_parts_even_when_embedded_hash_entries_are_removed_too() {
    use base64::Engine;
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("complete.docx");
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(12, 8)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let encoded = base64::engine::general_purpose::STANDARD.encode(png.get_ref());
    let mut document = fixture();
    document.content.push(
        serde_json::from_value(json!({
            "type":"image","attrs":{"src":format!("data:image/png;base64,{encoded}")}
        }))
        .unwrap(),
    );
    export_docx(&document, &source).unwrap();
    assert_eq!(
        serde_json::to_value(
            rust_wps::document::load_authored_docx(&source)
                .unwrap()
                .unwrap()
        )
        .unwrap(),
        serde_json::to_value(&document).unwrap(),
        "the complete package, including its model part, remains editable"
    );
    let original_bytes = fs::read(&source).unwrap();
    let mut accepted = Vec::new();
    for (index, removed) in [
        &["word/styles.xml"][..],
        &["word/media/image1.png"][..],
        &["word/styles.xml", "word/media/image1.png"][..],
    ]
    .into_iter()
    .enumerate()
    {
        let forged = dir.path().join(format!("missing-parts-{index}.docx"));
        let mut archive = zip::ZipArchive::new(Cursor::new(&original_bytes)).unwrap();
        let mut output = zip::ZipWriter::new(fs::File::create(&forged).unwrap());
        let mut removed_parts = 0;
        for part_index in 0..archive.len() {
            let mut entry = archive.by_index(part_index).unwrap();
            let name = entry.name().to_owned();
            if removed.contains(&name.as_str()) {
                removed_parts += 1;
                continue;
            }
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            if name == "rust-wps/model-v1.json" {
                let mut embedded: Value = serde_json::from_slice(&bytes).unwrap();
                for name in removed {
                    assert!(
                        embedded["hashes"]
                            .as_object_mut()
                            .unwrap()
                            .remove(*name)
                            .is_some()
                    );
                }
                bytes = serde_json::to_vec(&embedded).unwrap();
            }
            output
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            output.write_all(&bytes).unwrap();
        }
        output.finish().unwrap();
        assert_eq!(removed_parts, removed.len());
        let forged_bytes = fs::read(&forged).unwrap();
        match rust_wps::document::load_authored_docx(&forged) {
            Ok(_) => accepted.push(removed),
            Err(error) => assert!(error.to_string().contains("part set"), "{error}"),
        }
        assert_eq!(fs::read(&forged).unwrap(), forged_bytes);
    }
    assert_eq!(fs::read(&source).unwrap(), original_bytes);
    assert!(
        accepted.is_empty(),
        "accepted incomplete packages after synchronized hash removal: {accepted:?}"
    );
}

#[test]
fn embeds_png_with_relationship_and_dimensions() {
    use base64::Engine;
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("image.docx");
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(12, 8)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let encoded = base64::engine::general_purpose::STANDARD.encode(png.get_ref());
    let model: Node = serde_json::from_value(json!({"type":"doc","content":[{"type":"image","attrs":{"src":format!("data:image/png;base64,{encoded}"),"alt":"图 & 片","title":null,"width":null,"height":null}}]})).unwrap();
    let report = export_docx(&model, &output).unwrap();
    assert_eq!(report.images, 1);
    let mut zip = zip::ZipArchive::new(fs::File::open(&output).unwrap()).unwrap();
    let mut bytes = Vec::new();
    zip.by_name("word/media/image1.png")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(&bytes, png.get_ref());
    assert!(part(&output, "word/document.xml").contains("cx=\"114300\" cy=\"76200\""));
    assert!(part(&output, "word/document.xml").contains("descr=\"图 &amp; 片\""));
    assert!(part(&output, "word/_rels/document.xml.rels").contains("Target=\"media/image1.png\""));
}

#[test]
fn rejects_unsupported_structure_and_preserves_existing_output() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("output.docx");
    let examples: Vec<Value> = vec![
        json!({"type":"doc","content":[]}),
        json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"bad\u{0}"}]}]}),
        json!({"type":"doc","content":[{"type":"heading","attrs":{"level":8}}]}),
        json!({"type":"doc","content":[{"type":"paragraph","attrs":{"textAlign":"distribute"}}]}),
        json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"x","marks":[{"type":"link"}]}]}]}),
        json!({"type":"doc","content":[{"type":"image","attrs":{"src":"https://example.com/private.png"}}]}),
        json!({"type":"doc","content":[{"type":"table","content":[{"type":"tableRow","content":[{"type":"tableCell","attrs":{"colspan":2},"content":[{"type":"paragraph"}]}]}]}]}),
        json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"paragraph"}]}]}),
    ];
    for example in examples {
        let doc: Node = serde_json::from_value(example).unwrap();
        assert!(export_docx(&doc, &output).is_err());
        assert!(!output.exists());
    }
    fs::write(&output, b"original").unwrap();
    assert!(export_docx(&fixture(), &output).is_err());
    assert_eq!(fs::read(output).unwrap(), b"original");
}

#[test]
fn versioned_draft_roundtrip_atomic_replacement_and_rejection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("drafts/current.json");
    assert!(load_draft(&path).unwrap().is_none());
    let original = fixture();
    save_draft(&original, &path).unwrap();
    assert_eq!(
        serde_json::to_value(load_draft(&path).unwrap().unwrap()).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
    let minimal: Node =
        serde_json::from_value(json!({"type":"doc","content":[{"type":"paragraph"}]})).unwrap();
    save_draft(&minimal, &path).unwrap();
    assert_eq!(load_draft(&path).unwrap().unwrap().content.len(), 1);
    let before = fs::read(&path).unwrap();
    let invalid: Node = serde_json::from_value(json!({"type":"doc","content":[]})).unwrap();
    assert!(save_draft(&invalid, &path).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    fs::write(
        &path,
        b"{\"version\":99,\"document\":{\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\"}]}}",
    )
    .unwrap();
    assert!(load_draft(&path).is_err());
}

#[test]
fn script_marks_roundtrip_draft_package_and_docx_import() {
    let dir = tempfile::tempdir().unwrap();
    let doc:Node=serde_json::from_value(json!({"type":"doc","content":[{"type":"paragraph","content":[
      {"type":"text","text":"H"},{"type":"text","text":"2","marks":[{"type":"subscript"}]},
      {"type":"text","text":"O 中文 x"},{"type":"text","text":"2","marks":[{"type":"bold"},{"type":"superscript"}]}
    ]}]})).unwrap();
    let draft = dir.path().join("draft.json");
    save_draft(&doc, &draft).unwrap();
    assert_eq!(
        serde_json::to_value(load_draft(&draft).unwrap().unwrap()).unwrap(),
        serde_json::to_value(&doc).unwrap()
    );
    let output = dir.path().join("scripts.docx");
    export_docx(&doc, &output).unwrap();
    let xml = part(&output, "word/document.xml");
    assert!(xml.contains("<w:vertAlign w:val=\"superscript\"/>"));
    assert!(xml.contains("<w:vertAlign w:val=\"subscript\"/>"));
    assert_eq!(
        serde_json::to_value(
            rust_wps::document::load_authored_docx(&output)
                .unwrap()
                .unwrap()
        )
        .unwrap(),
        serde_json::to_value(&doc).unwrap()
    );
    let imported = rust_wps::importing::import_docx(&output).unwrap();
    let content = &imported.document.content[0].content;
    assert_eq!(content[1].marks[0].kind, "subscript");
    let superscript = content
        .iter()
        .find(|node| node.marks.iter().any(|mark| mark.kind == "superscript"))
        .unwrap();
    assert_eq!(superscript.text.as_deref(), Some("2"));
    assert!(
        superscript
            .marks
            .iter()
            .any(|mark| mark.kind == "superscript")
    );
    assert!(superscript.marks.iter().any(|mark| mark.kind == "bold"));
}

#[test]
fn conflicting_or_duplicate_script_marks_never_publish() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("invalid.docx");
    for marks in [
        json!([{"type":"superscript"},{"type":"subscript"}]),
        json!([{"type":"superscript"},{"type":"superscript"}]),
    ] {
        let doc:Node=serde_json::from_value(json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"2","marks":marks}]}]})).unwrap();
        assert!(export_docx(&doc, &output).is_err());
        assert!(!output.exists());
    }
}
