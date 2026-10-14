use rust_wps::{document, importing};
use std::io::Write;

#[test]
fn coalescing_equal_runs_preserves_styles_and_hard_break_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("runs.docx");
    let mut document: document::Node = serde_json::from_value(
        serde_json::json!({"type":"doc","content":[{"type":"paragraph","content":[
            {"type":"text","text":"甲A","marks":[{"type":"bold"}]},
            {"type":"text","text":"乙B","marks":[{"type":"bold"}]},
            {"type":"hardBreak"},
            {"type":"text","text":"丙C","marks":[{"type":"italic"}]},
            {"type":"text","text":"丁D","marks":[{"type":"bold"}]}
        ]}]}),
    )
    .unwrap();
    // Equal formatting includes equal script fonts. Different font ranges
    // must now remain separate, covered in word_font_family.rs.
    for node in &mut document.content[0].content {
        if node.kind == "text" {
            node.marks.push(
                serde_json::from_value(
                    serde_json::json!({"type":"fontFamily","attrs":{"name":"Courier New"}}),
                )
                .unwrap(),
            );
        }
    }
    document::export_docx(&document, &source).unwrap();
    let imported = importing::import_docx(&source).unwrap();
    let content = &imported.document.content[0].content;
    assert_eq!(content.len(), 4);
    assert_eq!(content[0].text.as_deref(), Some("甲A乙B"));
    assert_eq!(content[0].marks[0].kind, "bold");
    assert_eq!(content[1].kind, "hardBreak");
    assert_eq!(content[2].text.as_deref(), Some("丙C"));
    assert_eq!(content[2].marks[0].kind, "italic");
    assert_eq!(content[3].text.as_deref(), Some("丁D"));
    assert_eq!(content[3].marks[0].kind, "bold");
}

#[test]
fn same_font_mixed_script_paragraphs_coalesce_within_node_limit() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("mixed.docx");
    let content: Vec<_> = (0..4000).map(|i| serde_json::json!({"type":"paragraph","content":[{"type":"text","text":format!("段落 {i}: Rust office 中文长文档读取与结构转换性能测试。"),"marks":[{"type":"fontFamily","attrs":{"name":"Courier New"}}] }]})).collect();
    let document: document::Node =
        serde_json::from_value(serde_json::json!({"type":"doc","content":content})).unwrap();
    document::export_docx(&document, &source).unwrap();
    let imported = importing::import_docx(&source).unwrap();
    assert_eq!(imported.document.content.len(), 4000);
    for (original, converted) in document.content.iter().zip(&imported.document.content) {
        assert_eq!(converted.content.len(), 1);
        assert_eq!(converted.content[0].text, original.content[0].text);
        assert_eq!(
            serde_json::to_value(&converted.content[0].marks).unwrap(),
            serde_json::json!([
                {"type":"fontSize","attrs":{"halfPoints":22}},
                {"type":"fontFamily","attrs":{"name":"Courier New"}}
            ])
        );
    }
    document::validate(&imported.document).unwrap();
}

#[test]
fn external_hyperlink_runs_keep_destinations_and_reject_unresolved_targets() {
    let dir = tempfile::tempdir().unwrap();
    for (case, target, mode, reference, supported) in [
        (
            "web",
            "https://example.com/?a=1&amp;b=2",
            "External",
            "r:id=\"rId2\"",
            true,
        ),
        (
            "missing",
            "https://example.com",
            "External",
            "r:id=\"missing\"",
            false,
        ),
        (
            "file",
            "file:///C:/private.txt",
            "External",
            "r:id=\"rId2\"",
            false,
        ),
        (
            "bookmark",
            "https://example.com",
            "External",
            "w:anchor=\"bookmark\"",
            false,
        ),
        (
            "internal",
            "https://example.com",
            "Internal",
            "r:id=\"rId2\"",
            false,
        ),
    ] {
        let source = dir.path().join(format!("{case}.docx"));
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&source).unwrap());
        let main = format!(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body><w:p><w:hyperlink {reference}><w:r><w:rPr><w:b/></w:rPr><w:t>中文</w:t></w:r><w:r><w:t>链接</w:t></w:r></w:hyperlink></w:p></w:body></w:document>"#
        );
        let rels = format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="{target}" TargetMode="{mode}"/></Relationships>"#
        );
        for (name, xml) in [
            (
                "[Content_Types].xml",
                r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
            ),
            (
                "_rels/.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
            ),
            ("word/document.xml", main.as_str()),
            ("word/_rels/document.xml.rels", rels.as_str()),
        ] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(xml.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        let before = std::fs::read(&source).unwrap();
        let result = importing::import_docx(&source);
        if supported {
            let report = result.unwrap();
            let runs = &report.document.content[0].content;
            assert_eq!(
                runs.iter()
                    .map(|r| r.text.as_deref().unwrap_or(""))
                    .collect::<String>(),
                "中文链接"
            );
            for run in runs {
                assert!(
                    run.marks
                        .iter()
                        .any(|m| m.kind == "link"
                            && m.attrs["href"] == "https://example.com/?a=1&b=2")
                );
            }
            assert!(runs[0].marks.iter().any(|m| m.kind == "bold"));
        } else {
            assert!(
                result.unwrap_err().to_string().contains("hyperlink target"),
                "{case}"
            );
        }
        assert_eq!(before, std::fs::read(source).unwrap());
    }
}

#[test]
fn docx_import_uses_bounded_content_with_explicit_format() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("document.snapshot");
    let model: document::Node =
        serde_json::from_str(include_str!("fixtures/authoring.json")).unwrap();
    let generated = dir.path().join("generated.docx");
    document::export_docx(&model, &generated).unwrap();
    std::fs::rename(generated, &source).unwrap();
    let before = std::fs::read(&source).unwrap();
    let imported = importing::import_docx(&source).unwrap();
    document::validate(&imported.document).unwrap();
    assert_eq!(before, std::fs::read(&source).unwrap());

    let large = dir.path().join("large.docx");
    std::fs::File::create(&large)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    assert!(
        importing::import_docx(&large)
            .unwrap_err()
            .to_string()
            .contains("64 MiB")
    );
}

#[test]
fn legacy_import_rejects_disguised_corrupt_and_oversized_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.doc");
    for bytes in [
        b"{\\rtf1 not binary DOC}".as_slice(),
        &[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1],
    ] {
        std::fs::write(&path, bytes).unwrap();
        assert!(importing::import_doc(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
    std::fs::File::create(&path)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    assert!(
        importing::import_doc(&path)
            .unwrap_err()
            .to_string()
            .contains("64 MiB")
    );
}

#[test]
fn legacy_manual_page_break_preserves_neighboring_real_blank_paragraph() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("blank.doc");
    let bytes = include_bytes!("fixtures/doc-pagination/blank.doc");
    std::fs::write(&source, bytes).unwrap();
    let report = importing::import_doc(&source).unwrap();
    let expected = serde_json::json!({"type":"doc","attrs":{"styleProfile":"standard-v2"},"content":[
        {"type":"paragraph","content":[{"type":"text","text":"hello."}]},
        {"type":"paragraph"},
        {"type":"pageBreak"},
        {"type":"paragraph","content":[{"type":"text","text":"world."}]}
    ]});
    assert_eq!(serde_json::to_value(&report.document).unwrap(), expected);
    assert!(!report.compatibility.fidelity_verified);
    let output = dir.path().join("converted.docx");
    document::export_docx(&report.document, &output).unwrap();
    let reopened = importing::import_docx(&output).unwrap();
    let mut exported_expected = expected;
    // External re-import retains the explicit exported page as well as V2 defaults.
    exported_expected["attrs"]["pageGeometry"] = serde_json::json!({"widthTwips":11906,"heightTwips":16838,"marginTopTwips":1440,"marginRightTwips":1440,"marginBottomTwips":1440,"marginLeftTwips":1440});
    for index in [0, 1, 3] {
        exported_expected["content"][index]["attrs"] = serde_json::json!({
            "spaceBefore":0,"spaceAfter":120,"lineSpacing":360,
            "keepNext":false,"keepLines":false,"pageBreakBefore":false
        });
    }
    // The independently exported DOCX declares the empty paragraph's Arial
    // 11 pt font through V2 defaults, so external import now preserves it.
    exported_expected["content"][1]["attrs"]["emptyParagraphFont"] =
        serde_json::json!({"family":"Arial","halfPoints":22,"bold":false,"italic":false});
    for index in [0, 3] {
        exported_expected["content"][index]["content"][0]["marks"] = serde_json::json!([
            {"type":"fontSize","attrs":{"halfPoints":22}},
            {"type":"textColor","attrs":{"rgb":"000000"}},
            {"type":"fontFamily","attrs":{"name":"Arial"}}
        ]);
    }
    assert_eq!(
        serde_json::to_value(reopened.document).unwrap(),
        exported_expected
    );
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
}

#[test]
fn legacy_partial_structured_view_is_refused_despite_complete_flat_text() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("partial.doc");
    let bytes = include_bytes!("fixtures/doc-pagination/partial.doc");
    std::fs::write(&source, bytes).unwrap();
    let error = importing::import_doc(&source).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("DOC_STRUCTURED_TEXT_INCOMPLETE:"),
        "{error}"
    );
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
}

#[test]
fn refuses_content_that_the_parser_might_omit() {
    let dir = tempfile::tempdir().unwrap();
    for tag in [
        "ins",
        "fldSimple",
        "oMath",
        "sdt",
        "anchor",
        "vMerge",
        "hyperlink",
    ] {
        let source = dir.path().join(format!("{tag}.docx"));
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&source).unwrap());
        zip.start_file(
            "word/document.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        write!(zip, "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p><w:{tag}/></w:p></w:body></w:document>").unwrap();
        zip.finish().unwrap();
        assert!(importing::import_docx(&source).is_err(), "{tag}");
    }
}

#[test]
fn manual_page_break_is_preserved_and_text_survives() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("pages.docx");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&source).unwrap());
    for (name, xml) in [
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        (
            "word/document.xml",
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>第一页</w:t><w:br w:type="page"/></w:r></w:p><w:p><w:r><w:t>第二页</w:t></w:r></w:p></w:body></w:document>"#,
        ),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    let report = importing::import_docx(&source).unwrap();
    assert_eq!(
        report
            .document
            .content
            .iter()
            .filter(|n| n.kind == "pageBreak")
            .count(),
        1
    );
    let json = serde_json::to_string(&report.document).unwrap();
    assert!(json.contains("第一页") && json.contains("第二页"));
}

#[test]
fn imports_actual_ooxml_structure_without_embedded_model() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.docx");
    let model: document::Node =
        serde_json::from_str(include_str!("fixtures/authoring.json")).unwrap();
    document::export_docx(&model, &source).unwrap();
    // The importer reads actual OOXML, not this application's embedded model.
    let before = std::fs::read(&source).unwrap();
    let report = importing::import_docx(&source).unwrap();
    document::validate(&report.document).unwrap();
    let json = serde_json::to_string(&report.document).unwrap();
    for kind in [
        "heading",
        "bold",
        "italic",
        "underline",
        "orderedList",
        "bulletList",
        "table",
    ] {
        assert!(json.contains(kind), "Missing {kind}: {json}");
    }
    let output = dir.path().join("copy.docx");
    document::export_docx(&report.document, &output).unwrap();
    assert_eq!(before, std::fs::read(source).unwrap());
    assert!(!report.compatibility.fidelity_verified);
    assert_eq!(
        rust_wps::inspect(&output).unwrap().text.trim(),
        rust_wps::inspect(&dir.path().join("source.docx"))
            .unwrap()
            .text
            .trim()
    );
}

#[test]
fn external_run_scripts_and_explicit_baseline_keep_distinct_marks() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("external-scripts.docx");
    let file = std::fs::File::create(&source).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    for (name, xml) in [
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        (
            "word/document.xml",
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:t>x</w:t></w:r><w:r><w:rPr><w:vertAlign w:val="baseline"/></w:rPr><w:t>0</w:t></w:r><w:r><w:rPr><w:b/><w:vertAlign w:val="subscript"/></w:rPr><w:t>2</w:t></w:r></w:p></w:body></w:document>"#,
        ),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    let before = std::fs::read(&source).unwrap();
    let imported = importing::import_docx(&source).unwrap();
    let content = &imported.document.content[0].content;
    assert_eq!(content.len(), 3);
    assert_eq!(content[0].marks[0].kind, "superscript");
    assert!(content[1].marks.is_empty());
    assert!(content[2].marks.iter().any(|mark| mark.kind == "subscript"));
    assert!(content[2].marks.iter().any(|mark| mark.kind == "bold"));
    assert_eq!(std::fs::read(&source).unwrap(), before);
}
