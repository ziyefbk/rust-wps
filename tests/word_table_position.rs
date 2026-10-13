use rust_wps::{
    compatibility,
    document::{self, Node},
    importing,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read, Write},
    path::Path,
};

const NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const RISK: &str = "Unsupported table geometry cannot be preserved in an editable copy";
const NORMALIZED_ALIGNMENT: &str = "Table alignment normalized to editor defaults";
const AUTOFIT: &str = "Autofit table widths normalized; fixed column widths were not inferred";

fn model(attrs: Value, modern: bool) -> Node {
    let cell = |text| json!({"type":"tableCell","content":[{"type":"paragraph","attrs":{"textAlign":"right"},"content":[{"type":"text","text":text}]}]});
    let mut value = json!({"type":"doc","content":[{"type":"table","attrs":attrs,"content":[
        {"type":"tableRow","content":[cell("First"),cell("Second")]},
        {"type":"tableRow","content":[cell("Third"),cell("Fourth")]}
    ]}]});
    if modern {
        value["attrs"] = json!({"styleProfile":"standard-v2"});
    }
    serde_json::from_value(value).unwrap()
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

fn write(path: &Path, archive: &BTreeMap<String, Vec<u8>>) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, data) in archive {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
}

fn styles(body: &str) -> String {
    format!(r#"<w:styles xmlns:w="{NS}">{body}</w:styles>"#)
}

fn table(properties: &str) -> String {
    format!(
        r#"<w:tbl><w:tblPr>{properties}</w:tblPr><w:tblGrid><w:gridCol w:w="1200"/><w:gridCol w:w="2400"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:type="dxa" w:w="1200"/></w:tcPr><w:p><w:r><w:t>First</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:type="dxa" w:w="2400"/></w:tcPr><w:p><w:r><w:t>Second</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#
    )
}

fn fixed(properties: &str) -> String {
    table(&format!(
        r#"<w:tblLayout w:type="fixed"/><w:tblW w:type="dxa" w:w="3600"/>{properties}"#
    ))
}

fn archive(body: &str, sheet: &str) -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
        ("[Content_Types].xml".into(), br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#.to_vec()),
        ("_rels/.rels".into(), br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="office" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec()),
        ("word/_rels/document.xml.rels".into(), br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#.to_vec()),
        ("word/document.xml".into(), format!(r#"<w:document xmlns:w="{NS}"><w:body>{body}</w:body></w:document>"#).into_bytes()),
        ("word/styles.xml".into(), sheet.as_bytes().to_vec()),
    ])
}

fn refused(body: &str, sheet: &str) {
    refused_package(&archive(body, sheet));
}

fn refused_package(package: &BTreeMap<String, Vec<u8>>) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.docx");
    write(&source, package);
    let original = fs::read(&source).unwrap();
    let audit = compatibility::audit_docx(&source).unwrap();
    assert!(
        audit.risks.contains(RISK),
        "{}\n{}\n{:?}",
        String::from_utf8_lossy(&package["word/document.xml"]),
        String::from_utf8_lossy(&package["word/styles.xml"]),
        audit.risks
    );
    let error = importing::import_docx(&source).unwrap_err().to_string();
    assert!(error.contains(RISK), "{error}");
    assert_eq!(fs::read(&source).unwrap(), original);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

fn with_settings(body: &str, settings_body: &str) -> BTreeMap<String, Vec<u8>> {
    let mut package = archive(body, &styles(""));
    // Use a non-conventional relationship target so raw filename assumptions
    // cannot cause the preflight to audit different settings from the package.
    package.insert(
        "word/source-settings.xml".into(),
        format!(r#"<w:settings xmlns:w="{NS}">{settings_body}</w:settings>"#).into_bytes(),
    );
    let content_types = String::from_utf8(package["[Content_Types].xml"].clone()).unwrap().replace("</Types>", r#"<Override PartName="/word/source-settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>"#);
    package.insert("[Content_Types].xml".into(), content_types.into_bytes());
    let relationships = String::from_utf8(package["word/_rels/document.xml.rels"].clone()).unwrap().replace("</Relationships>", r#"<Relationship Id="settings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="source-settings.xml"/></Relationships>"#);
    package.insert(
        "word/_rels/document.xml.rels".into(),
        relationships.into_bytes(),
    );
    package
}

fn first_text(table: &Node) -> &str {
    table.content[0].content[0].content[0].content[0]
        .text
        .as_deref()
        .unwrap()
}

#[test]
fn strict_position_values_and_combined_width_budget() {
    for attrs in [
        json!({}),
        json!({"tableAlign":"left"}),
        json!({"tableAlign":"center"}),
        json!({"tableAlign":"right"}),
        json!({"tableIndentTwips":0}),
        json!({"tableIndentTwips":22680}),
        json!({"tableAlign":"left","tableIndentTwips":720}),
        json!({"tableAlign":"center","tableIndentTwips":0}),
        json!({"tableAlign":"right","tableIndentTwips":0}),
        json!({"columnWidthsTwips":[1,1],"tableIndentTwips":31678}),
        json!({"columnWidthsTwips":[15840,15840],"tableIndentTwips":0}),
        // The product ceiling is deliberately not the printable page width.
        json!({"columnWidthsTwips":[12000,12000],"tableAlign":"left","tableIndentTwips":7680}),
    ] {
        document::validate(&model(attrs.clone(), true))
            .unwrap_or_else(|error| panic!("{attrs}: {error}"));
    }
    for attrs in [
        json!({"tableAlign":null}),
        json!({"tableAlign":0}),
        json!({"tableAlign":true}),
        json!({"tableAlign":""}),
        json!({"tableAlign":"Left"}),
        json!({"tableAlign":" left"}),
        json!({"tableAlign":"justify"}),
        json!({"tableAlign":"start"}),
        json!({"tableAlign":"end"}),
        json!({"tableIndentTwips":null}),
        json!({"tableIndentTwips":"720"}),
        json!({"tableIndentTwips":true}),
        json!({"tableIndentTwips":0.0}),
        json!({"tableIndentTwips":720.0}),
        json!({"tableIndentTwips":0.5}),
        json!({"tableIndentTwips":-1}),
        json!({"tableIndentTwips":31681}),
        json!({"tableIndentTwips":u64::MAX}),
        json!({"tableIndentTwips":22681}),
        json!({"tableAlign":"center","tableIndentTwips":1}),
        json!({"tableAlign":"right","tableIndentTwips":1}),
        json!({"columnWidthsTwips":[1,1],"tableIndentTwips":31679}),
        json!({"columnWidthsTwips":[15840,15840],"tableIndentTwips":1}),
    ] {
        assert!(
            document::validate(&model(attrs.clone(), true)).is_err(),
            "{attrs}"
        );
    }
}

#[test]
fn positioning_attributes_are_table_scoped_and_independent_from_paragraph_alignment() {
    for (key, value) in [
        ("tableAlign", json!("left")),
        ("tableIndentTwips", json!(0)),
    ] {
        for scope in [0, 1, 2, 3, 4] {
            let mut doc = model(json!({}), true);
            let target = match scope {
                0 => &mut doc,
                1 => &mut doc.content[0].content[0],
                2 => &mut doc.content[0].content[0].content[0],
                3 => &mut doc.content[0].content[0].content[0].content[0],
                _ => &mut doc.content[0].content[0].content[0].content[0].content[0],
            };
            target.attrs.insert(key.into(), value.clone());
            assert!(document::validate(&doc).is_err(), "{key}, scope {scope}");
        }
    }
    let doc = model(json!({"tableAlign":"left","tableIndentTwips":720}), true);
    document::validate(&doc).unwrap();
    assert_eq!(
        doc.content[0].content[0].content[0].content[0].attrs["textAlign"],
        "right"
    );
}

#[test]
fn any_explicit_position_refuses_nested_table_combinations() {
    for attrs in [
        json!({"tableAlign":"left"}),
        json!({"tableIndentTwips":0}),
        json!({"tableIndentTwips":1}),
    ] {
        for (outer_attrs, inner_attrs) in [
            (attrs.clone(), json!({})),
            (json!({}), attrs.clone()),
            (attrs.clone(), attrs.clone()),
            (json!({"columnWidthsTwips":[1200,2400]}), attrs.clone()),
            (attrs.clone(), json!({"columnWidthsTwips":[1200,2400]})),
        ] {
            let mut outer = model(outer_attrs, true);
            outer.content[0].content[0].content[0]
                .content
                .push(model(inner_attrs, true).content.remove(0));
            assert!(document::validate(&outer).is_err());
        }
    }
    let mut old = model(json!({}), false);
    old.content[0].content[0].content[0]
        .content
        .push(model(json!({}), false).content.remove(0));
    document::validate(&old).unwrap();
}

#[test]
fn positioned_models_round_trip_exactly_through_draft_trusted_docx_and_xml() {
    let dir = tempfile::tempdir().unwrap();
    let cases = [
        json!({}),
        json!({"tableAlign":"left"}),
        json!({"tableAlign":"center"}),
        json!({"tableAlign":"right"}),
        json!({"tableIndentTwips":0}),
        json!({"tableIndentTwips":721}),
        json!({"tableAlign":"left","tableIndentTwips":720}),
        json!({"tableAlign":"center","tableIndentTwips":0}),
        json!({"tableAlign":"right","tableIndentTwips":0,"columnWidthsTwips":[1201,2399]}),
    ];
    for modern in [false, true] {
        for (index, attrs) in cases.iter().enumerate() {
            let doc = model(attrs.clone(), modern);
            let expected = serde_json::to_value(&doc).unwrap();
            let draft = dir.path().join(format!("{modern}-{index}.json"));
            document::save_draft(&doc, &draft).unwrap();
            assert_eq!(
                serde_json::to_value(document::load_draft(&draft).unwrap().unwrap()).unwrap(),
                expected
            );
            let output = dir.path().join(format!("{modern}-{index}.docx"));
            document::export_docx(&doc, &output).unwrap();
            assert_eq!(
                serde_json::to_value(document::load_authored_docx(&output).unwrap().unwrap())
                    .unwrap(),
                expected
            );
            let bytes = fs::read(&output).unwrap();
            let archive = parts(&bytes);
            let xml = String::from_utf8(archive["word/document.xml"].clone()).unwrap();
            let table_properties = xml
                .split("<w:tblPr>")
                .nth(1)
                .unwrap()
                .split("</w:tblPr>")
                .next()
                .unwrap();
            match attrs.get("tableAlign") {
                Some(alignment) => assert!(table_properties.contains(&format!(
                    r#"<w:jc w:val="{}"/>"#,
                    alignment.as_str().unwrap()
                ))),
                None => assert!(!table_properties.contains("w:jc")),
            }
            match attrs.get("tableIndentTwips") {
                Some(indent) => assert!(
                    table_properties
                        .contains(&format!(r#"<w:tblInd w:w="{indent}" w:type="dxa"/>"#))
                ),
                None => assert!(!table_properties.contains("tblInd")),
            }
            assert_eq!(
                xml.matches(r#"<w:jc w:val="right"/>"#).count(),
                4 + usize::from(attrs.get("tableAlign") == Some(&json!("right")))
            );
            if attrs.get("columnWidthsTwips").is_none() {
                assert!(table_properties.contains(r#"<w:tblW w:w="9000" w:type="dxa"/>"#));
                assert!(!table_properties.contains("tblLayout"));
            }
            assert!(document::export_docx(&doc, &output).is_err());
            assert_eq!(fs::read(&output).unwrap(), bytes);
        }
    }
}

#[test]
fn invalid_position_never_replaces_draft_or_publishes_output() {
    let dir = tempfile::tempdir().unwrap();
    let draft = dir.path().join("draft.json");
    document::save_draft(
        &model(json!({"tableAlign":"left","tableIndentTwips":720}), true),
        &draft,
    )
    .unwrap();
    let before = fs::read(&draft).unwrap();
    for (index, attrs) in [
        json!({"tableAlign":null}),
        json!({"tableAlign":"justify"}),
        json!({"tableIndentTwips":0.0}),
        json!({"tableIndentTwips":22681}),
        json!({"tableAlign":"center","tableIndentTwips":720}),
    ]
    .into_iter()
    .enumerate()
    {
        let doc = model(attrs, true);
        assert!(document::save_draft(&doc, &draft).is_err());
        assert_eq!(fs::read(&draft).unwrap(), before);
        let output = dir.path().join(format!("invalid-{index}.docx"));
        assert!(document::export_docx(&doc, &output).is_err());
        assert!(!output.exists());
        fs::write(&output, b"unrelated existing file").unwrap();
        assert!(document::export_docx(&doc, &output).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"unrelated existing file");
    }
}

#[test]
fn tampered_position_model_or_rehashed_xml_cannot_be_trusted() {
    let dir = tempfile::tempdir().unwrap();
    for modern in [false, true] {
        let source = dir.path().join(format!("source-{modern}.docx"));
        let doc = model(
            json!({"tableAlign":"left","tableIndentTwips":720,"columnWidthsTwips":[1200,2400]}),
            modern,
        );
        document::export_docx(&doc, &source).unwrap();
        let original = fs::read(&source).unwrap();
        let baseline = parts(&original);
        let model_part = if modern {
            "rust-wps/model-v2.json"
        } else {
            "rust-wps/model-v1.json"
        };
        for mutation in ["model-align", "model-indent", "xml-align", "xml-indent"] {
            let mut archive = baseline.clone();
            let mut embedded: Value = serde_json::from_slice(&archive[model_part]).unwrap();
            match mutation {
                "model-align" => {
                    // Absent alignment plus a positive indent is model-valid;
                    // reopening still must detect that the stored XML differs.
                    embedded["document"]["content"][0]["attrs"]
                        .as_object_mut()
                        .unwrap()
                        .remove("tableAlign");
                }
                "model-indent" => {
                    embedded["document"]["content"][0]["attrs"]["tableIndentTwips"] = json!(721);
                }
                _ => {
                    let xml = String::from_utf8(archive["word/document.xml"].clone()).unwrap();
                    let xml = if mutation == "xml-align" {
                        xml.replacen(r#"<w:jc w:val="left"/>"#, r#"<w:jc w:val="right"/>"#, 1)
                    } else {
                        xml.replacen(r#"<w:tblInd w:w="720""#, r#"<w:tblInd w:w="721""#, 1)
                    };
                    archive.insert("word/document.xml".into(), xml.into_bytes());
                    embedded["hashes"]["word/document.xml"] = json!(format!(
                        "{:x}",
                        Sha256::digest(&archive["word/document.xml"])
                    ));
                }
            }
            archive.insert(model_part.into(), serde_json::to_vec(&embedded).unwrap());
            let corrupt = dir.path().join(format!("corrupt-{modern}-{mutation}.docx"));
            write(&corrupt, &archive);
            assert!(
                document::load_authored_docx(&corrupt).is_err(),
                "{modern}: {mutation}"
            );
        }
        assert_eq!(fs::read(&source).unwrap(), original);
    }
}

#[test]
fn direct_alignment_and_indent_survive_fixed_and_automatic_import() {
    let dir = tempfile::tempdir().unwrap();
    let cases = [
        ("", None, None),
        (r#"<w:jc w:val="left"/>"#, Some("left"), None),
        (r#"<w:jc w:val="center"/>"#, Some("center"), None),
        (r#"<w:jc w:val="right"/>"#, Some("right"), None),
        (r#"<w:tblInd w:type="dxa" w:w="0"/>"#, None, Some(0)),
        (r#"<w:tblInd w:type="dxa" w:w="721"/>"#, None, Some(721)),
        (
            r#"<w:jc w:val="left"/><w:tblInd w:type="dxa" w:w="720"/>"#,
            Some("left"),
            Some(720),
        ),
        (
            r#"<w:jc w:val="center"/><w:tblInd w:type="dxa" w:w="0"/>"#,
            Some("center"),
            Some(0),
        ),
        (
            r#"<w:jc w:val="right"/><w:tblInd w:type="dxa" w:w="0"/>"#,
            Some("right"),
            Some(0),
        ),
    ];
    for fixed_layout in [false, true] {
        for (index, (properties, alignment, indent)) in cases.iter().enumerate() {
            let body = if fixed_layout {
                fixed(properties)
            } else {
                table(properties)
            };
            let source = dir
                .path()
                .join(format!("source-{fixed_layout}-{index}.docx"));
            write(&source, &archive(&body, &styles("")));
            let original = fs::read(&source).unwrap();
            let report = importing::import_docx(&source).unwrap();
            let attrs = &report.document.content[0].attrs;
            assert_eq!(
                attrs.get("tableAlign"),
                alignment.map(|v| json!(v)).as_ref()
            );
            assert_eq!(
                attrs.get("tableIndentTwips"),
                indent.map(|v| json!(v)).as_ref()
            );
            assert_eq!(
                attrs.get("columnWidthsTwips"),
                fixed_layout.then(|| json!([1200, 2400])).as_ref()
            );
            assert_eq!(report.compatibility.risks.contains(AUTOFIT), !fixed_layout);
            assert!(!report.compatibility.risks.contains(NORMALIZED_ALIGNMENT));
            let output = dir.path().join(format!("copy-{fixed_layout}-{index}.docx"));
            document::export_docx(&report.document, &output).unwrap();
            let reimported = importing::import_docx(&output).unwrap();
            assert_eq!(reimported.document.content[0].attrs, *attrs);
            assert_eq!(fs::read(&source).unwrap(), original);
        }
    }
}

#[test]
fn imports_keep_table_position_identity_across_paragraphs_and_auto_tables() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("ordered.docx");
    let body = format!(
        "{}<w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr><w:r><w:t>Between</w:t></w:r></w:p>{}<w:p/>{}",
        fixed(r#"<w:jc w:val="right"/>"#).replace("First", "Right fixed"),
        table(r#"<w:jc w:val="left"/><w:tblInd w:type="dxa" w:w="721"/>"#)
            .replace("First", "Left automatic"),
        fixed(r#"<w:jc w:val="center"/><w:tblInd w:type="dxa" w:w="0"/>"#)
            .replace("First", "Center fixed")
            .replace("1200", "1000")
            .replace("2400", "3000")
            .replace("3600", "4000")
    );
    write(&source, &archive(&body, &styles("")));
    let original = fs::read(&source).unwrap();
    let report = importing::import_docx(&source).unwrap();
    let tables: Vec<_> = report
        .document
        .content
        .iter()
        .filter(|n| n.kind == "table")
        .collect();
    assert_eq!(tables.len(), 3);
    assert_eq!(first_text(tables[0]), "Right fixed");
    assert_eq!(first_text(tables[1]), "Left automatic");
    assert_eq!(first_text(tables[2]), "Center fixed");
    assert_eq!(tables[0].attrs["tableAlign"], "right");
    assert!(!tables[0].attrs.contains_key("tableIndentTwips"));
    assert_eq!(tables[0].attrs["columnWidthsTwips"], json!([1200, 2400]));
    assert_eq!(tables[1].attrs["tableAlign"], "left");
    assert_eq!(tables[1].attrs["tableIndentTwips"], 721);
    assert!(!tables[1].attrs.contains_key("columnWidthsTwips"));
    assert_eq!(tables[2].attrs["tableAlign"], "center");
    assert_eq!(tables[2].attrs["tableIndentTwips"], 0);
    assert_eq!(tables[2].attrs["columnWidthsTwips"], json!([1000, 3000]));
    assert_eq!(fs::read(&source).unwrap(), original);
}

#[test]
fn cell_paragraph_alignment_is_not_mistaken_for_table_position() {
    let dir = tempfile::tempdir().unwrap();
    for positioned in [false, true] {
        let body = fixed(if positioned {
            r#"<w:jc w:val="left"/>"#
        } else {
            ""
        })
        .replace(
            "<w:p><w:r><w:t>First",
            r#"<w:p><w:pPr><w:jc w:val="right"/></w:pPr><w:r><w:t>First"#,
        )
        .replace(
            "<w:p><w:r><w:t>Second",
            r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>Second"#,
        );
        let source = dir.path().join(format!("paragraph-{positioned}.docx"));
        write(&source, &archive(&body, &styles("")));
        let report = importing::import_docx(&source).unwrap();
        let table = &report.document.content[0];
        assert_eq!(
            table.attrs.get("tableAlign"),
            positioned.then(|| json!("left")).as_ref()
        );
        assert!(!table.attrs.contains_key("tableIndentTwips"));
        assert_eq!(
            table.content[0].content[0].content[0].attrs["textAlign"],
            "right"
        );
        assert_eq!(
            table.content[0].content[1].content[0].attrs["textAlign"],
            "center"
        );
    }
}

#[test]
fn malformed_repeated_or_conflicting_direct_positions_are_refused() {
    for property in [
        "<w:jc/>",
        r#"<w:jc w:val=""/>"#,
        r#"<w:jc w:val="justify"/>"#,
        r#"<w:jc w:val="both"/>"#,
        r#"<w:jc w:val="Left"/>"#,
        r#"<w:jc w:val="left"><w:jc w:val="right"/></w:jc>"#,
        r#"<w:jc w:val="left"/><w:jc w:val="left"/>"#,
        r#"<w:jc w:val="left" w:unknown="1"/>"#,
        "<w:tblInd/>",
        r#"<w:tblInd w:w="720"/>"#,
        r#"<w:tblInd w:type="dxa"/>"#,
        r#"<w:tblInd w:type="auto" w:w="0"/>"#,
        r#"<w:tblInd w:type="pct" w:w="0"/>"#,
        r#"<w:tblInd w:type="nil" w:w="0"/>"#,
        r#"<w:tblInd w:type="dxa" w:w="-1"/>"#,
        r#"<w:tblInd w:type="dxa" w:w="+1"/>"#,
        r#"<w:tblInd w:type="dxa" w:w="1.0"/>"#,
        r#"<w:tblInd w:type="dxa" w:w="1pt"/>"#,
        r#"<w:tblInd w:type="dxa" w:w=" 1"/>"#,
        r#"<w:tblInd w:type="dxa" w:w="1 "/>"#,
        r#"<w:tblInd w:type="dxa" w:w="31681"/>"#,
        r#"<w:tblInd w:type="dxa" w:w="18446744073709551616"/>"#,
        r#"<w:tblInd w:type="dxa" w:w="0" w:unknown="1"/>"#,
        r#"<w:tblInd w:type="dxa" w:w="0"><w:foo/></w:tblInd>"#,
        r#"<w:tblInd w:type="dxa" w:w="0"/><w:tblInd w:type="dxa" w:w="0"/>"#,
        r#"<w:jc w:val="center"/><w:tblInd w:type="dxa" w:w="1"/>"#,
        r#"<w:jc w:val="right"/><w:tblInd w:type="dxa" w:w="1"/>"#,
    ] {
        refused(&fixed(property), &styles(""));
    }
}

#[test]
fn non_direct_position_declarations_cannot_hide_beside_supported_geometry() {
    for property in [
        r#"<w:jc w:val="left"/>"#,
        r#"<w:tblInd w:type="dxa" w:w="0"/>"#,
    ] {
        for wrapper in [
            format!("<w:unknown>{property}</w:unknown>"),
            format!(
                r#"<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Fallback>{property}</mc:Fallback></mc:AlternateContent>"#
            ),
        ] {
            refused(&fixed(&wrapper), &styles(""));
            refused(
                &fixed("").replacen("</w:tcPr>", &format!("{wrapper}</w:tcPr>"), 1),
                &styles(""),
            );
        }
        refused(
            &fixed("").replace("</w:tblPr>", &format!("</w:tblPr>{property}")),
            &styles(""),
        );
    }
}

#[test]
fn namespace_collisions_and_alternate_attribute_prefixes_fail_closed() {
    for property in [
        r#"<w:jc xmlns:x="urn:foreign" w:val="left" x:val="right"/>"#,
        r#"<w:jc xmlns:x="http://schemas.openxmlformats.org/wordprocessingml/2006/main" x:val="left"/>"#,
        r#"<w:jc val="left"/>"#,
        r#"<x:jc xmlns:x="urn:foreign" w:val="left"/>"#,
        r#"<w:tblInd xmlns:x="urn:foreign" w:type="dxa" w:w="0" x:w="0"/>"#,
        r#"<w:tblInd xmlns:x="urn:foreign" w:type="dxa" x:type="dxa" w:w="0"/>"#,
        r#"<w:tblInd xmlns:x="http://schemas.openxmlformats.org/wordprocessingml/2006/main" x:type="dxa" w:w="0"/>"#,
        r#"<w:tblInd xmlns:x="http://schemas.openxmlformats.org/wordprocessingml/2006/main" w:type="dxa" x:w="0"/>"#,
        r#"<x:tblInd xmlns:x="urn:foreign" w:type="dxa" w:w="0"/>"#,
    ] {
        refused(&fixed(property), &styles(""));
    }
}

#[test]
fn inherited_and_conditional_non_neutral_position_is_refused() {
    for property in [
        r#"<w:jc w:val="center"/>"#,
        r#"<w:jc w:val="right"/>"#,
        r#"<w:tblInd w:type="dxa" w:w="1"/>"#,
    ] {
        for wrapped in [
            format!("<w:tblPr>{property}</w:tblPr>"),
            format!(
                r#"<w:tblStylePr w:type="firstRow"><w:tblPr>{property}</w:tblPr></w:tblStylePr>"#
            ),
        ] {
            let sheet = styles(&format!(
                r#"<w:style w:type="table" w:styleId="Base">{wrapped}</w:style><w:style w:type="table" w:styleId="Used"><w:basedOn w:val="Base"/></w:style>"#
            ));
            refused(&fixed(r#"<w:tblStyle w:val="Used"/>"#), &sheet);
            refused(
                &fixed(
                    r#"<w:tblStyle w:val="Used"/><w:jc w:val="left"/><w:tblInd w:type="dxa" w:w="0"/>"#,
                ),
                &sheet,
            );
            let default = styles(&format!(
                r#"<w:style w:type="table" w:default="1" w:styleId="Default">{wrapped}</w:style>"#
            ));
            refused(&fixed(""), &default);
        }
    }
}

#[test]
fn neutral_styles_do_not_materialize_absent_direct_position_or_poison_unrelated_tables() {
    let dir = tempfile::tempdir().unwrap();
    for (index, sheet) in [
        styles(r#"<w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:tblPr><w:jc w:val="left"/><w:tblInd w:type="dxa" w:w="0"/></w:tblPr></w:style>"#),
        styles(r#"<w:style w:type="table" w:styleId="Unused"><w:tblPr><w:jc w:val="right"/><w:tblInd w:type="dxa" w:w="720"/></w:tblPr></w:style>"#),
    ].into_iter().enumerate() {
        let source = dir.path().join(format!("neutral-{index}.docx"));
        write(&source, &archive(&fixed(""), &sheet));
        let report = importing::import_docx(&source).unwrap();
        assert!(!report.document.content[0].attrs.contains_key("tableAlign"));
        assert!(!report.document.content[0].attrs.contains_key("tableIndentTwips"));
        assert!(!report.compatibility.risks.contains(NORMALIZED_ALIGNMENT));
    }
}

#[test]
fn imported_fixed_and_automatic_width_plus_indent_use_the_product_budget() {
    let dir = tempfile::tempdir().unwrap();
    for (fixed_layout, maximum) in [(true, 28080), (false, 22680)] {
        let good_property = format!(r#"<w:tblInd w:type="dxa" w:w="{maximum}"/>"#);
        let good = if fixed_layout {
            fixed(&good_property)
        } else {
            table(&good_property)
        };
        let source = dir.path().join(format!("boundary-{fixed_layout}.docx"));
        write(&source, &archive(&good, &styles("")));
        let report = importing::import_docx(&source).unwrap();
        assert_eq!(
            report.document.content[0].attrs["tableIndentTwips"],
            maximum
        );
        let bad_property = format!(r#"<w:tblInd w:type="dxa" w:w="{}"/>"#, maximum + 1);
        let bad = if fixed_layout {
            fixed(&bad_property)
        } else {
            table(&bad_property)
        };
        refused(&bad, &styles(""));
    }
}

#[test]
fn strict_namespace_utf16_position_and_relationship_resolved_style_are_audited() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("strict.docx");
    let mut package = archive(
        &fixed(r#"<w:jc w:val="left"/><w:tblInd w:type="dxa" w:w="721"/>"#),
        &styles(""),
    );
    let text = String::from_utf8(package["word/document.xml"].clone())
        .unwrap()
        .replace(NS, "http://purl.oclc.org/ooxml/wordprocessingml/main");
    let mut utf16 = vec![0xff, 0xfe];
    utf16.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    package.insert("word/document.xml".into(), utf16);
    write(&source, &package);
    let report = importing::import_docx(&source).unwrap();
    assert_eq!(report.document.content[0].attrs["tableAlign"], "left");
    assert_eq!(report.document.content[0].attrs["tableIndentTwips"], 721);
    package.insert("word/actual-styles.xml".into(), styles(r#"<w:style w:type="table" w:default="1" w:styleId="Real"><w:tblPr><w:jc w:val="center"/></w:tblPr></w:style>"#).into_bytes());
    package.insert(
        "word/_rels/document.xml.rels".into(),
        String::from_utf8(package["word/_rels/document.xml.rels"].clone())
            .unwrap()
            .replace("Target=\"styles.xml\"", "Target=\"actual-styles.xml\"")
            .into_bytes(),
    );
    write(&source, &package);
    let original = fs::read(&source).unwrap();
    assert!(
        importing::import_docx(&source)
            .unwrap_err()
            .to_string()
            .contains(RISK)
    );
    assert_eq!(fs::read(&source).unwrap(), original);
}

#[test]
fn position_absence_keeps_historical_v1_and_v2_encoded_parts() {
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/word-spacing-legacy-parts.json")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    for profile in ["legacy-v1", "standard-v2"] {
        let mut doc: Node = serde_json::from_str(include_str!("fixtures/authoring.json")).unwrap();
        if profile == "standard-v2" {
            doc.attrs.insert("styleProfile".into(), json!(profile));
        }
        let output = dir.path().join(format!("{profile}.docx"));
        document::export_docx(&doc, &output).unwrap();
        let actual: BTreeMap<_, _> = parts(&fs::read(&output).unwrap())
            .into_iter()
            .map(|(name, data)| (name, format!("{:x}", Sha256::digest(data))))
            .collect();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            expected["profiles"][profile]
        );
        assert_eq!(
            serde_json::to_value(document::load_authored_docx(&output).unwrap().unwrap()).unwrap(),
            serde_json::to_value(&doc).unwrap()
        );
    }
}

#[test]
fn start_and_end_map_to_physical_positions_only_in_supported_ltr_tables() {
    let dir = tempfile::tempdir().unwrap();
    for (source_alignment, canonical) in [("start", "left"), ("end", "right")] {
        let property =
            format!(r#"<w:jc w:val="{source_alignment}"/><w:tblInd w:type="dxa" w:w="0"/>"#);
        let source = dir.path().join(format!("{source_alignment}.docx"));
        write(&source, &archive(&fixed(&property), &styles("")));
        let report = importing::import_docx(&source).unwrap();
        assert_eq!(report.document.content[0].attrs["tableAlign"], canonical);
        assert_eq!(report.document.content[0].attrs["tableIndentTwips"], 0);
        for bidi in [
            "<w:bidiVisual/>",
            r#"<w:bidiVisual w:val="1"/>"#,
            r#"<w:bidiVisual w:val="0"/>"#,
        ] {
            refused(&fixed(&format!("{property}{bidi}")), &styles(""));
            refused(
                &fixed(&property),
                &styles(&format!(
                    r#"<w:style w:type="table" w:default="1" w:styleId="Bidi"><w:tblPr>{bidi}</w:tblPr></w:style>"#
                )),
            );
        }
    }
    refused(
        &fixed(r#"<w:jc w:val="end"/><w:tblInd w:type="dxa" w:w="1"/>"#),
        &styles(""),
    );
}

#[test]
fn source_compatibility_mode_is_relationship_resolved_before_preserving_indent() {
    let dir = tempfile::tempdir().unwrap();
    for indent in [0, 720] {
        let body = fixed(&format!(r#"<w:tblInd w:type="dxa" w:w="{indent}"/>"#));
        for mode in ["11", "12", "14"] {
            let settings = format!(
                r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{mode}"/></w:compat>"#
            );
            let source = dir.path().join(format!("mode-{mode}-{indent}.docx"));
            write(&source, &with_settings(&body, &settings));
            let report = importing::import_docx(&source).unwrap();
            assert_eq!(report.document.content[0].attrs["tableIndentTwips"], indent);
        }
        for settings in [
            r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#,
            r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="99"/></w:compat>"#,
            r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word"/></w:compat>"#,
            r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="urn:foreign" w:val="14"/></w:compat>"#,
            r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="14"/><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="14"/></w:compat>"#,
            r#"<w:compat><w:compatSetting xmlns:x="urn:foreign" w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="14" x:val="15"/></w:compat>"#,
        ] {
            refused_package(&with_settings(&body, settings));
        }
    }
    // Alignment without an indentation claim need not assume a text-edge model.
    let source = dir.path().join("modern-alignment.docx");
    write(
        &source,
        &with_settings(
            &fixed(r#"<w:jc w:val="right"/>"#),
            r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#,
        ),
    );
    let report = importing::import_docx(&source).unwrap();
    assert_eq!(report.document.content[0].attrs["tableAlign"], "right");
    assert!(
        !report.document.content[0]
            .attrs
            .contains_key("tableIndentTwips")
    );
}

#[test]
fn explicit_indent_preserves_only_proven_baseline_horizontal_margins() {
    let dir = tempfile::tempdir().unwrap();
    for indent in [0, 720] {
        let position = format!(r#"<w:tblInd w:type="dxa" w:w="{indent}"/>"#);
        for (index, margins) in [
            r#"<w:left w:type="dxa" w:w="108"/><w:right w:type="dxa" w:w="108"/>"#,
            r#"<w:start w:type="dxa" w:w="108"/><w:end w:type="dxa" w:w="108"/>"#,
        ]
        .into_iter()
        .enumerate()
        {
            let source = dir.path().join(format!("margins-{indent}-{index}.docx"));
            write(
                &source,
                &archive(
                    &fixed(&format!("{position}<w:tblCellMar>{margins}</w:tblCellMar>")),
                    &styles(""),
                ),
            );
            let report = importing::import_docx(&source).unwrap();
            assert_eq!(report.document.content[0].attrs["tableIndentTwips"], indent);
        }
        for margins in [
            r#"<w:left w:type="dxa" w:w="0"/>"#,
            r#"<w:right w:type="dxa" w:w="109"/>"#,
            r#"<w:start w:type="pct" w:w="108"/>"#,
            r#"<w:end w:type="dxa" w:w="1.0"/>"#,
            r#"<w:left w:type="dxa" w:w="108"/><w:left w:type="dxa" w:w="108"/>"#,
            r#"<w:left w:type="dxa" w:w="108"/><w:start w:type="dxa" w:w="108"/>"#,
            r#"<w:right w:type="dxa" w:w="108"/><w:end w:type="dxa" w:w="108"/>"#,
            r#"<w:left xmlns:x="urn:foreign" w:type="dxa" w:w="108" x:w="0"/>"#,
            r#"<x:left xmlns:x="urn:foreign" w:type="dxa" w:w="108"/>"#,
        ] {
            refused(
                &fixed(&format!("{position}<w:tblCellMar>{margins}</w:tblCellMar>")),
                &styles(""),
            );
            refused(
                &fixed(&position).replacen(
                    "</w:tcPr>",
                    &format!("<w:tcMar>{margins}</w:tcMar></w:tcPr>"),
                    1,
                ),
                &styles(""),
            );
            for inherited in [
                format!("<w:tblPr><w:tblCellMar>{margins}</w:tblCellMar></w:tblPr>"),
                format!(
                    r#"<w:tblStylePr w:type="firstRow"><w:tcPr><w:tcMar>{margins}</w:tcMar></w:tcPr></w:tblStylePr>"#
                ),
            ] {
                let sheet = styles(&format!(
                    r#"<w:style w:type="table" w:styleId="Base">{inherited}</w:style><w:style w:type="table" w:styleId="Used"><w:basedOn w:val="Base"/></w:style>"#
                ));
                refused(
                    &fixed(&format!(r#"<w:tblStyle w:val="Used"/>{position}"#)),
                    &sheet,
                );
            }
        }
    }
}

#[test]
fn malformed_neutral_inherited_position_cannot_hide_in_styles() {
    for property in [
        r#"<w:jc w:val="left" w:unknown="1"/>"#,
        r#"<w:jc w:val="left"><w:unknown/></w:jc>"#,
        r#"<w:jc w:val="left"/><w:jc w:val="left"/>"#,
        r#"<w:jc xmlns:x="urn:foreign" w:val="left" x:val="left"/>"#,
        r#"<w:tblInd w:type="dxa" w:w="0" w:unknown="1"/>"#,
        r#"<w:tblInd w:type="dxa" w:w="0"/><w:tblInd w:type="dxa" w:w="0"/>"#,
    ] {
        refused(
            &fixed(""),
            &styles(&format!(
                r#"<w:style w:type="table" w:default="1" w:styleId="Default"><w:tblPr>{property}</w:tblPr></w:style>"#
            )),
        );
    }
}

#[test]
fn malformed_or_non_direct_compatibility_settings_fail_closed_for_indentation() {
    let body = fixed(r#"<w:tblInd w:type="dxa" w:w="0"/>"#);
    let mode = r#"<w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="14"/>"#;
    for settings in [
        mode.to_string(),
        format!("<w:unknown><w:compat>{mode}</w:compat></w:unknown>"),
        format!("<w:compat><w:unknown>{mode}</w:unknown></w:compat>"),
        format!("<w:compat>{mode}</w:compat><w:compat/>"),
        format!(r#"<x:compat xmlns:x="urn:foreign">{mode}</x:compat>"#),
        r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="14" w:unknown="1"/></w:compat>"#.into(),
        r#"<w:compat><w:compatSetting xmlns:x="urn:foreign" x:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="14"/></w:compat>"#.into(),
        r#"<w:compat><w:compatSetting xmlns:x="http://schemas.openxmlformats.org/wordprocessingml/2006/main" x:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="14"/></w:compat>"#.into(),
        r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="14"><w:unknown/></w:compatSetting></w:compat>"#.into(),
    ] {
        refused_package(&with_settings(&body, &settings));
    }
    let good = format!("<w:compat>{mode}</w:compat>");
    for settings in [
        format!(r#"<w:settings xmlns:w="urn:foreign">{good}</w:settings>"#),
        format!(r#"<w:notSettings xmlns:w="{NS}">{good}</w:notSettings>"#),
        format!(r#"<w:unknown xmlns:w="{NS}"><w:settings>{good}</w:settings></w:unknown>"#),
    ] {
        let mut package = with_settings(&body, &good);
        package.insert("word/source-settings.xml".into(), settings.into_bytes());
        refused_package(&package);
    }
}

#[test]
fn duplicate_or_external_settings_relationships_do_not_imply_legacy_indent_semantics() {
    let body = fixed(r#"<w:tblInd w:type="dxa" w:w="720"/>"#);
    let good_settings = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="14"/></w:compat>"#;
    for extra in [
        r#"<Relationship Id="settings2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="source-settings.xml"/>"#,
        r#"<Relationship Id="settings2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="https://example.invalid/settings.xml" TargetMode="External"/>"#,
    ] {
        let mut package = with_settings(&body, good_settings);
        let relationships = String::from_utf8(package["word/_rels/document.xml.rels"].clone())
            .unwrap()
            .replace("</Relationships>", &format!("{extra}</Relationships>"));
        package.insert(
            "word/_rels/document.xml.rels".into(),
            relationships.into_bytes(),
        );
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.docx");
        write(&source, &package);
        let original = fs::read(&source).unwrap();
        let audit = compatibility::audit_docx(&source);
        assert!(
            audit
                .as_ref()
                .map_or(true, |report| report.risks.contains(RISK))
        );
        assert!(importing::import_docx(&source).is_err());
        assert_eq!(fs::read(&source).unwrap(), original);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}

#[test]
fn relocated_style_settings_parts_and_strict_namespace_keep_position_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let body = fixed(r#"<w:jc w:val="start"/><w:tblInd w:type="dxa" w:w="721"/>"#);
    let settings = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="14"/></w:compat>"#;
    for relocated in [false, true] {
        let mut package = with_settings(&body, settings);
        if relocated {
            let sheet = package.remove("word/styles.xml").unwrap();
            let settings = package.remove("word/source-settings.xml").unwrap();
            let relationships =
                String::from_utf8(package.remove("word/_rels/document.xml.rels").unwrap())
                    .unwrap()
                    .replace("Target=\"styles.xml\"", "Target=\"meta/source-styles.xml\"")
                    .replace(
                        "Target=\"source-settings.xml\"",
                        "Target=\"meta/source-settings.xml\"",
                    );
            package.insert("word/meta/source-styles.xml".into(), sheet);
            package.insert("word/meta/source-settings.xml".into(), settings);
            package.insert(
                "word/_rels/document.xml.rels".into(),
                relationships.into_bytes(),
            );
            let text = String::from_utf8(package["[Content_Types].xml"].clone())
                .unwrap()
                .replace("word/styles.xml", "word/meta/source-styles.xml")
                .replace("word/source-settings.xml", "word/meta/source-settings.xml");
            package.insert("[Content_Types].xml".into(), text.into_bytes());
        } else {
            for name in [
                "word/document.xml",
                "word/styles.xml",
                "word/source-settings.xml",
            ] {
                let text = String::from_utf8(package[name].clone())
                    .unwrap()
                    .replace(NS, "http://purl.oclc.org/ooxml/wordprocessingml/main");
                package.insert(name.into(), text.into_bytes());
            }
        }
        let source = dir.path().join(format!("opc-{relocated}.docx"));
        write(&source, &package);
        let original = fs::read(&source).unwrap();
        let report = importing::import_docx(&source).unwrap();
        assert_eq!(report.document.content[0].attrs["tableAlign"], "left");
        assert_eq!(report.document.content[0].attrs["tableIndentTwips"], 721);
        assert_eq!(
            report.document.content[0].attrs["columnWidthsTwips"],
            json!([1200, 2400])
        );
        assert_eq!(fs::read(&source).unwrap(), original);
    }
}

const INHERITED_INDENT_NORMALIZED: &str =
    "Inherited table indentation normalized; automatic table geometry not preserved";

fn zero_indent_style_chain() -> String {
    styles(
        r#"<w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:tblPr><w:tblInd w:type="dxa" w:w="0"/><w:tblCellMar><w:left w:type="dxa" w:w="108"/><w:right w:type="dxa" w:w="108"/></w:tblCellMar></w:tblPr></w:style><w:style w:type="table" w:styleId="TableGrid"><w:basedOn w:val="TableNormal"/></w:style><w:style w:type="table" w:styleId="Custom"><w:basedOn w:val="TableGrid"/></w:style>"#,
    )
}

#[test]
fn explicitly_selected_zero_indent_style_is_distinct_from_implicit_default() {
    let dir = tempfile::tempdir().unwrap();
    for fixed_layout in [false, true] {
        for (index, selected) in [None, Some("TableNormal"), Some("TableGrid"), Some("Custom")]
            .into_iter()
            .enumerate()
        {
            let property = selected.map_or(String::new(), |name| {
                format!(r#"<w:tblStyle w:val="{name}"/>"#)
            });
            let body = if fixed_layout {
                fixed(&property)
            } else {
                table(&property)
            };
            let source = dir
                .path()
                .join(format!("style-zero-{fixed_layout}-{index}.docx"));
            write(&source, &archive(&body, &zero_indent_style_chain()));
            let original = fs::read(&source).unwrap();
            let report = importing::import_docx(&source).unwrap();
            let attrs = &report.document.content[0].attrs;
            assert_eq!(
                attrs.get("tableIndentTwips"),
                selected.map(|_| json!(0)).as_ref()
            );
            assert!(!attrs.contains_key("tableAlign"));
            assert_eq!(
                attrs.get("columnWidthsTwips"),
                fixed_layout.then(|| json!([1200, 2400])).as_ref()
            );
            assert!(
                !report
                    .compatibility
                    .risks
                    .contains(INHERITED_INDENT_NORMALIZED)
            );
            let output = dir
                .path()
                .join(format!("style-copy-{fixed_layout}-{index}.docx"));
            document::export_docx(&report.document, &output).unwrap();
            let restored = importing::import_docx(&output).unwrap();
            assert_eq!(restored.document.content[0].attrs, *attrs);
            assert_eq!(fs::read(&source).unwrap(), original);
        }
    }
}

#[test]
fn direct_indent_takes_precedence_over_selected_zero_indent_style_chain() {
    let dir = tempfile::tempdir().unwrap();
    for fixed_layout in [false, true] {
        for selected in ["TableNormal", "TableGrid", "Custom"] {
            for indent in [0, 721] {
                let property = format!(
                    r#"<w:tblStyle w:val="{selected}"/><w:jc w:val="left"/><w:tblInd w:type="dxa" w:w="{indent}"/>"#
                );
                let body = if fixed_layout {
                    fixed(&property)
                } else {
                    table(&property)
                };
                let mut package = with_settings(
                    &body,
                    r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="14"/></w:compat>"#,
                );
                package.insert(
                    "word/styles.xml".into(),
                    zero_indent_style_chain().into_bytes(),
                );
                let source = dir
                    .path()
                    .join(format!("override-{fixed_layout}-{selected}-{indent}.docx"));
                write(&source, &package);
                let report = importing::import_docx(&source).unwrap();
                assert_eq!(report.document.content[0].attrs["tableIndentTwips"], indent);
                assert_eq!(report.document.content[0].attrs["tableAlign"], "left");
                assert!(
                    !report
                        .compatibility
                        .risks
                        .contains(INHERITED_INDENT_NORMALIZED)
                );
            }
        }
    }
}

#[test]
fn unproven_inherited_zero_refuses_fixed_but_warns_for_automatic_normalization() {
    let dir = tempfile::tempdir().unwrap();
    for proof_failure in ["modern-mode", "table-margin", "style-margin"] {
        for fixed_layout in [false, true] {
            let properties = format!(
                r#"<w:tblStyle w:val="Custom"/><w:jc w:val="right"/>{}"#,
                if proof_failure == "table-margin" {
                    r#"<w:tblCellMar><w:left w:type="dxa" w:w="0"/></w:tblCellMar>"#
                } else {
                    ""
                }
            );
            let body = if fixed_layout {
                fixed(&properties)
            } else {
                table(&properties)
            };
            let mode = if proof_failure == "modern-mode" {
                15
            } else {
                14
            };
            let mut package = with_settings(
                &body,
                &format!(
                    r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{mode}"/></w:compat>"#
                ),
            );
            let sheet = if proof_failure == "style-margin" {
                zero_indent_style_chain().replacen(
                    r#"<w:left w:type="dxa" w:w="108"/>"#,
                    r#"<w:left w:type="dxa" w:w="0"/>"#,
                    1,
                )
            } else {
                zero_indent_style_chain()
            };
            package.insert("word/styles.xml".into(), sheet.into_bytes());
            if fixed_layout {
                refused_package(&package);
            } else {
                let source = dir.path().join(format!("normalized-{proof_failure}.docx"));
                write(&source, &package);
                let original = fs::read(&source).unwrap();
                let audit = compatibility::audit_docx(&source).unwrap();
                assert!(audit.risks.contains(INHERITED_INDENT_NORMALIZED));
                assert!(!audit.risks.contains(RISK));
                let report = importing::import_docx(&source).unwrap();
                assert!(
                    report
                        .compatibility
                        .risks
                        .contains(INHERITED_INDENT_NORMALIZED)
                );
                assert!(report.compatibility.risks.contains(AUTOFIT));
                assert_eq!(report.document.content[0].attrs["tableAlign"], "right");
                assert!(
                    !report.document.content[0]
                        .attrs
                        .contains_key("tableIndentTwips")
                );
                assert!(
                    !report.document.content[0]
                        .attrs
                        .contains_key("columnWidthsTwips")
                );
                assert_eq!(fs::read(&source).unwrap(), original);
            }
        }
    }
    // An unselected default style remains absence, even with modern settings.
    let mut package = with_settings(
        &fixed(""),
        r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#,
    );
    package.insert(
        "word/styles.xml".into(),
        zero_indent_style_chain().into_bytes(),
    );
    let source = dir.path().join("implicit-modern.docx");
    write(&source, &package);
    let report = importing::import_docx(&source).unwrap();
    assert!(
        !report.document.content[0]
            .attrs
            .contains_key("tableIndentTwips")
    );
    assert!(
        !report
            .compatibility
            .risks
            .contains(INHERITED_INDENT_NORMALIZED)
    );
}

#[test]
fn direct_indent_cannot_downgrade_to_inherited_auto_normalization() {
    for fixed_layout in [false, true] {
        for indent in [0, 721] {
            for proof_failure in ["modern-mode", "custom-margin"] {
                let properties = format!(
                    r#"<w:tblStyle w:val="Custom"/><w:tblInd w:type="dxa" w:w="{indent}"/>{}"#,
                    if proof_failure == "custom-margin" {
                        r#"<w:tblCellMar><w:left w:type="dxa" w:w="0"/></w:tblCellMar>"#
                    } else {
                        ""
                    }
                );
                let body = if fixed_layout {
                    fixed(&properties)
                } else {
                    table(&properties)
                };
                let mode = if proof_failure == "modern-mode" {
                    15
                } else {
                    14
                };
                let mut package = with_settings(
                    &body,
                    &format!(
                        r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{mode}"/></w:compat>"#
                    ),
                );
                package.insert(
                    "word/styles.xml".into(),
                    zero_indent_style_chain().into_bytes(),
                );
                refused_package(&package);
            }
        }
    }
}

#[test]
fn conditional_zero_indent_is_never_promoted_even_with_direct_override() {
    for fixed_layout in [false, true] {
        for selected in [false, true] {
            for direct in [
                "",
                r#"<w:tblInd w:type="dxa" w:w="0"/>"#,
                r#"<w:tblInd w:type="dxa" w:w="721"/>"#,
            ] {
                let property = format!(
                    "{}{direct}",
                    if selected {
                        r#"<w:tblStyle w:val="Used"/>"#
                    } else {
                        ""
                    }
                );
                let body = if fixed_layout {
                    fixed(&property)
                } else {
                    table(&property)
                };
                let sheet = styles(
                    r#"<w:style w:type="table" w:default="1" w:styleId="Base"><w:tblPr><w:tblInd w:type="dxa" w:w="0"/></w:tblPr><w:tblStylePr w:type="firstRow"><w:tblPr><w:tblInd w:type="dxa" w:w="0"/></w:tblPr></w:tblStylePr></w:style><w:style w:type="table" w:styleId="Used"><w:basedOn w:val="Base"/></w:style>"#,
                );
                refused(&body, &sheet);
            }
        }
    }
}

#[test]
fn hidden_horizontal_margins_and_non_direct_margin_containers_fail_closed() {
    for container in ["tblCellMar", "tcMar"] {
        let valid = format!(r#"<w:{container}><w:left w:type="dxa" w:w="108"/></w:{container}>"#);
        let cases = [
            format!(
                r#"<w:{container}><w:unknown><w:left w:type="dxa" w:w="0"/></w:unknown></w:{container}>"#
            ),
            format!(
                r#"<w:{container}><w:unknown><w:left w:type="dxa" w:w="108"/></w:unknown></w:{container}>"#
            ),
            format!(
                r#"<w:{container}><mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Fallback><w:left w:type="dxa" w:w="0"/></mc:Fallback></mc:AlternateContent></w:{container}>"#
            ),
            format!("<w:unknown>{valid}</w:unknown>"),
            format!(
                r#"<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Fallback>{valid}</mc:Fallback></mc:AlternateContent>"#
            ),
        ];
        for declaration in cases {
            for fixed_layout in [false, true] {
                for indent in [0, 720] {
                    let position = format!(r#"<w:tblInd w:type="dxa" w:w="{indent}"/>"#);
                    let base = if fixed_layout {
                        fixed(&position)
                    } else {
                        table(&position)
                    };
                    let body = if container == "tblCellMar" {
                        base.replace("</w:tblPr>", &format!("{declaration}</w:tblPr>"))
                    } else {
                        base.replacen("</w:tcPr>", &format!("{declaration}</w:tcPr>"), 1)
                    };
                    refused(&body, &styles(""));
                }
            }
            let parent = if container == "tblCellMar" {
                "tblPr"
            } else {
                "tcPr"
            };
            let sheet = styles(&format!(
                r#"<w:style w:type="table" w:styleId="Used"><w:{parent}>{declaration}</w:{parent}></w:style>"#
            ));
            refused(
                &fixed(r#"<w:tblStyle w:val="Used"/><w:tblInd w:type="dxa" w:w="0"/>"#),
                &sheet,
            );
        }
    }
}

#[test]
fn ambiguous_or_malformed_margin_containers_cannot_supply_indent_proof() {
    for container in ["tblCellMar", "tcMar"] {
        let valid = format!(r#"<w:{container}><w:left w:type="dxa" w:w="108"/></w:{container}>"#);
        for declaration in [
            format!("{valid}{valid}"),
            format!(
                r#"<w:{container} w:unknown="1"><w:left w:type="dxa" w:w="108"/></w:{container}>"#
            ),
            format!(
                r#"<w:{container} xmlns:x="urn:foreign" x:unknown="1"><w:left w:type="dxa" w:w="108"/></w:{container}>"#
            ),
            format!(
                r#"<x:{container} xmlns:x="urn:foreign"><w:left w:type="dxa" w:w="108"/></x:{container}>"#
            ),
            format!(
                r#"<w:{container}><w:left w:type="dxa" w:w="108" w:unknown="1"/></w:{container}>"#
            ),
            format!(
                r#"<w:{container}><x:left xmlns:x="urn:foreign" w:type="dxa" w:w="108"/></w:{container}>"#
            ),
            format!(
                r#"<w:{container}><w:left w:type="dxa" w:w="108"><w:unknown/></w:left></w:{container}>"#
            ),
            format!(r#"<w:{container}><w:unknown w:type="dxa" w:w="108"/></w:{container}>"#),
        ] {
            let base = fixed(r#"<w:tblInd w:type="dxa" w:w="0"/>"#);
            let body = if container == "tblCellMar" {
                base.replace("</w:tblPr>", &format!("{declaration}</w:tblPr>"))
            } else {
                base.replacen("</w:tcPr>", &format!("{declaration}</w:tcPr>"), 1)
            };
            refused(&body, &styles(""));
            let parent = if container == "tblCellMar" {
                "tblPr"
            } else {
                "tcPr"
            };
            let sheet = styles(&format!(
                r#"<w:style w:type="table" w:styleId="Used"><w:{parent}>{declaration}</w:{parent}></w:style>"#
            ));
            refused(
                &fixed(r#"<w:tblStyle w:val="Used"/><w:tblInd w:type="dxa" w:w="0"/>"#),
                &sheet,
            );
        }
        // A valid container under the other property type is still ambiguous.
        let base = fixed(r#"<w:tblInd w:type="dxa" w:w="0"/>"#);
        let wrong_parent = if container == "tblCellMar" {
            base.replacen("</w:tcPr>", &format!("{valid}</w:tcPr>"), 1)
        } else {
            base.replace("</w:tblPr>", &format!("{valid}</w:tblPr>"))
        };
        refused(&wrong_parent, &styles(""));
    }
}
