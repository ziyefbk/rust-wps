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

const RISK: &str = "Unsupported paragraph spacing cannot be preserved in an editable copy";
const STYLES: &str =
    r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#;
fn model(attrs: Value, modern: bool) -> Node {
    let mut value = json!({"type":"doc","content":[{"type":"paragraph","attrs":attrs,"content":[{"type":"text","text":"Spacing"}]}]});
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

fn write_parts(path: &Path, archive: &BTreeMap<String, Vec<u8>>) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, bytes) in archive {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}

fn external(path: &Path, body: &str, styles: &str) {
    let archive = BTreeMap::from([
        ("[Content_Types].xml".into(), br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#.to_vec()),
        ("_rels/.rels".into(), br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="office" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec()),
        ("word/_rels/document.xml.rels".into(), br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#.to_vec()),
        ("word/document.xml".into(), format!(r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#).into_bytes()),
        ("word/styles.xml".into(), styles.as_bytes().to_vec()),
    ]);
    write_parts(path, &archive);
}

#[test]
fn strict_canonical_attributes_and_limits() {
    for (key, min, max) in [
        ("spaceBefore", 0, 31680),
        ("spaceAfter", 0, 31680),
        ("lineSpacing", 120, 2400),
    ] {
        for value in [json!(min), json!(max)] {
            document::validate(&model(json!({key:value}), true)).unwrap();
        }
        for value in [
            Value::Null,
            json!(-1),
            json!(max + 1),
            json!(1.5),
            json!("240"),
            json!(true),
            json!([]),
        ] {
            assert!(
                document::validate(&model(json!({key:value}), true)).is_err(),
                "{key}: {value}"
            );
        }
        let mut bad = model(json!({}), true);
        bad.attrs.insert(key.into(), json!(240));
        assert!(document::validate(&bad).is_err());
    }
    assert!(document::validate(&model(json!({"lineSpacing":119}), true)).is_err());
}

#[test]
fn explicit_zero_and_fractional_line_units_survive_draft_export_and_authored_reopen() {
    let dir = tempfile::tempdir().unwrap();
    for modern in [false, true] {
        let doc = model(
            json!({"spaceBefore":0,"spaceAfter":31680,"lineSpacing":241}),
            modern,
        );
        let draft = dir.path().join(format!("{modern}.json"));
        document::save_draft(&doc, &draft).unwrap();
        let restored = document::load_draft(&draft).unwrap().unwrap();
        assert_eq!(
            serde_json::to_value(&restored).unwrap(),
            serde_json::to_value(&doc).unwrap()
        );
        let output = dir.path().join(format!("{modern}.docx"));
        document::export_docx(&doc, &output).unwrap();
        let archive = parts(&fs::read(&output).unwrap());
        let xml = String::from_utf8(archive["word/document.xml"].clone()).unwrap();
        assert!(xml.contains(
            r#"<w:spacing w:before="0" w:after="31680" w:line="241" w:lineRule="auto"/>"#
        ));
        assert_eq!(
            serde_json::to_value(document::load_authored_docx(&output).unwrap().unwrap()).unwrap(),
            serde_json::to_value(&doc).unwrap()
        );
        let original = fs::read(&output).unwrap();
        assert!(document::export_docx(&doc, &output).is_err());
        assert_eq!(fs::read(&output).unwrap(), original);
        // Forging only the embedded model cannot bypass re-encoding checks.
        let model_part = if modern {
            "rust-wps/model-v2.json"
        } else {
            "rust-wps/model-v1.json"
        };
        let mut forged = archive;
        let mut metadata: Value = serde_json::from_slice(&forged[model_part]).unwrap();
        metadata["document"]["content"][0]["attrs"]["spaceBefore"] = json!(20);
        forged.insert(model_part.into(), serde_json::to_vec(&metadata).unwrap());
        let corrupt = dir.path().join(format!("forged-{modern}.docx"));
        write_parts(&corrupt, &forged);
        assert!(document::load_authored_docx(&corrupt).is_err());
    }
}

#[test]
fn unsupported_raw_spacing_fails_closed_without_modifying_source() {
    let dir = tempfile::tempdir().unwrap();
    for (index, property) in [
        r#"<w:spacing w:before="-1"/>"#,
        r#"<w:spacing w:after="31681"/>"#,
        r#"<w:spacing w:line="119"/>"#,
        r#"<w:spacing w:line="2401"/>"#,
        r#"<w:spacing w:line="240" w:lineRule="exact"/>"#,
        r#"<w:spacing w:line="240" w:lineRule="atLeast"/>"#,
        r#"<w:spacing w:line="240" w:lineRule="mystery"/>"#,
        r#"<w:spacing w:lineRule="auto"/>"#,
        r#"<w:spacing w:before="12.5"/>"#,
        r#"<w:spacing w:before="1pt"/>"#,
        r#"<w:spacing w:before=" 20"/>"#,
        r#"<w:spacing w:beforeAutospacing="1"/>"#,
        r#"<w:spacing w:afterAutospacing="true"/>"#,
        r#"<w:spacing w:beforeLines="0"/>"#,
        r#"<w:spacing w:afterLines="100"/>"#,
        r#"<w:spacing xmlns:x="http://schemas.openxmlformats.org/wordprocessingml/2006/main" x:before="20"/>"#,
    ].iter().enumerate() {
        let source = dir.path().join(format!("bad-{index}.docx"));
        external(&source,&format!("<w:p><w:pPr>{property}</w:pPr><w:r><w:t>Source</w:t></w:r></w:p>"),STYLES);
        let bytes = fs::read(&source).unwrap();
        assert!(importing::import_docx(&source).unwrap_err().to_string().contains(RISK), "{property}");
        assert_eq!(fs::read(&source).unwrap(),bytes);
    }
}

#[test]
fn supported_disabled_flags_and_default_grid_remain_importable() {
    let dir = tempfile::tempdir().unwrap();
    for (index, grid) in [
        "",
        r#"<w:docGrid w:linePitch="360"/>"#,
        r#"<w:docGrid w:type="default" w:linePitch="360"/>"#,
    ]
    .iter()
    .enumerate()
    {
        let source = dir.path().join(format!("okay-{index}.docx"));
        external(
            &source,
            &format!(
                r#"<w:p><w:pPr><w:spacing w:before="0" w:after="120" w:line="276" w:beforeAutospacing="false" w:afterAutospacing="off"/><w:contextualSpacing w:val="0"/><w:snapToGrid w:val="0"/></w:pPr><w:r><w:t>Source</w:t></w:r></w:p><w:sectPr>{grid}</w:sectPr>"#
            ),
            STYLES,
        );
        let imported = importing::import_docx(&source).unwrap();
        assert_eq!(
            imported.document.content[0].attrs,
            json!({"spaceBefore":0,"spaceAfter":120,"lineSpacing":276,"keepNext":false,"keepLines":false,"pageBreakBefore":false})
                .as_object()
                .unwrap()
                .clone()
        );
    }
    for grid in ["lines", "linesAndChars", "snapToChars", "invalid"] {
        let source = dir.path().join(format!("grid-{grid}.docx"));
        external(
            &source,
            &format!(
                r#"<w:p/><w:sectPr><w:docGrid w:type="{grid}" w:linePitch="360"/></w:sectPr>"#
            ),
            STYLES,
        );
        assert!(
            importing::import_docx(&source)
                .unwrap_err()
                .to_string()
                .contains(RISK)
        );
    }
}

#[test]
fn used_style_chain_cascade_and_explicit_zero_are_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("cascade.docx");
    external(
        &source,
        r#"<w:p><w:pPr><w:pStyle w:val="Child"/><w:spacing w:after="0"/></w:pPr><w:r><w:t>Cascade</w:t></w:r></w:p>"#,
        r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:pPrDefault><w:pPr><w:spacing w:before="80" w:after="100" w:line="276" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:pPr><w:spacing w:before="120"/></w:pPr></w:style><w:style w:type="paragraph" w:styleId="Base"><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:line="480" w:lineRule="auto"/></w:pPr></w:style><w:style w:type="paragraph" w:styleId="Child"><w:basedOn w:val="Base"/><w:pPr><w:spacing w:before="0"/></w:pPr></w:style><w:style w:type="paragraph" w:styleId="Unused"><w:pPr><w:contextualSpacing/><w:spacing w:line="240" w:lineRule="exact"/></w:pPr></w:style></w:styles>"#,
    );
    let imported = importing::import_docx(&source).unwrap();
    assert_eq!(
        imported.document.content[0].attrs,
        json!({"spaceBefore":0,"spaceAfter":0,"lineSpacing":480,"keepNext":false,"keepLines":false,"pageBreakBefore":false})
            .as_object()
            .unwrap()
            .clone()
    );
    let mut archive = parts(&fs::read(&source).unwrap());
    let styles = String::from_utf8(archive["word/styles.xml"].clone())
        .unwrap()
        .replace(
            r#"<w:basedOn w:val="Base"/>"#,
            r#"<w:basedOn w:val="Unused"/>"#,
        );
    archive.insert("word/styles.xml".into(), styles.into_bytes());
    let bad = dir.path().join("used-unsupported.docx");
    write_parts(&bad, &archive);
    assert!(
        importing::import_docx(&bad)
            .unwrap_err()
            .to_string()
            .contains(RISK)
    );
}

#[test]
fn headings_table_cells_lists_and_soft_breaks_keep_spacing_and_structure() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = model(
        json!({"spaceBefore":60,"spaceAfter":80,"lineSpacing":360}),
        true,
    );
    let paragraph = doc.content[0].clone();
    let mut heading = paragraph.clone();
    heading.kind = "heading".into();
    heading.attrs.insert("level".into(), json!(2));
    let mut list = json!({"type":"orderedList","attrs":{"start":4},"content":[{"type":"listItem","content":[serde_json::to_value(&paragraph).unwrap()]}]});
    list["content"][0]["content"][0]["content"] = json!([{"type":"text","text":"First"},{"type":"hardBreak"},{"type":"text","text":"Second"}]);
    let table = json!({"type":"table","content":[{"type":"tableRow","content":[{"type":"tableCell","content":[serde_json::to_value(&paragraph).unwrap()]}]}]});
    doc.content.extend([
        heading,
        serde_json::from_value(list).unwrap(),
        serde_json::from_value(table).unwrap(),
    ]);
    let output = dir.path().join("structures.docx");
    document::export_docx(&doc, &output).unwrap();
    let archive = parts(&fs::read(&output).unwrap());
    let xml = String::from_utf8(archive["word/document.xml"].clone()).unwrap();
    assert_eq!(xml.matches("<w:spacing ").count(), 4);
    assert!(xml.contains("<w:numPr>"));
    assert!(xml.contains("<w:br/>"));
    assert!(xml.contains("<w:tbl>"));
    assert_eq!(
        serde_json::to_value(document::load_authored_docx(&output).unwrap().unwrap()).unwrap(),
        serde_json::to_value(doc).unwrap()
    );
    let imported = importing::import_docx(&output).unwrap();
    fn check(node: &Node) {
        if matches!(node.kind.as_str(), "paragraph" | "heading") {
            assert_eq!(node.attrs["spaceBefore"], 60);
            assert_eq!(node.attrs["spaceAfter"], 80);
            assert_eq!(node.attrs["lineSpacing"], 360);
        }
        for child in &node.content {
            check(child);
        }
    }
    check(&imported.document);
}

#[test]
fn external_missing_spacing_materializes_ooxml_defaults_not_editor_defaults() {
    let dir = tempfile::tempdir().unwrap();
    for (index, spacing, expected) in [
        (
            0,
            "",
            json!({"spaceBefore":0,"spaceAfter":0,"lineSpacing":240,"keepNext":false,"keepLines":false,"pageBreakBefore":false}),
        ),
        (
            1,
            r#"<w:spacing w:before="160"/>"#,
            json!({"spaceBefore":160,"spaceAfter":0,"lineSpacing":240,"keepNext":false,"keepLines":false,"pageBreakBefore":false}),
        ),
        (
            2,
            r#"<w:spacing w:after="80"/>"#,
            json!({"spaceBefore":0,"spaceAfter":80,"lineSpacing":240,"keepNext":false,"keepLines":false,"pageBreakBefore":false}),
        ),
    ] {
        let source = dir.path().join(format!("defaults-{index}.docx"));
        external(
            &source,
            &format!("<w:p><w:pPr>{spacing}</w:pPr><w:r><w:t>Default</w:t></w:r></w:p>"),
            STYLES,
        );
        let imported = importing::import_docx(&source).unwrap();
        assert_eq!(
            imported.document.content[0].attrs,
            expected.as_object().unwrap().clone()
        );
        let output = dir.path().join(format!("defaults-{index}-out.docx"));
        document::export_docx(&imported.document, &output).unwrap();
        assert_eq!(
            importing::import_docx(&output).unwrap().document.content[0].attrs,
            expected.as_object().unwrap().clone()
        );
    }
}

#[test]
fn active_style_identity_uses_vendor_keys_and_unused_default_is_not_applied() {
    let dir = tempfile::tempdir().unwrap();
    for (index, style_attrs, reference) in [
        (
            0,
            r#"w:styleId="Used" x:styleId="Unused""#,
            r#"w:val="Used""#,
        ),
        (1, r#"w:styleId="Used""#, r#"w:val="Used" x:val="Unused""#),
        (
            2,
            r#"x:styleId="Unused" w:styleId="Used""#,
            r#"x:val="Unused" w:val="Used""#,
        ),
    ] {
        let source = dir.path().join(format!("shadow-{index}.docx"));
        external(
            &source,
            &format!(
                r#"<w:p xmlns:x="urn:unrelated"><w:pPr><w:pStyle {reference}/></w:pPr><w:r><w:t>Shadow</w:t></w:r></w:p>"#
            ),
            &format!(
                r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:x="urn:unrelated"><w:style w:type="paragraph" {style_attrs}><w:pPr><w:contextualSpacing/></w:pPr></w:style></w:styles>"#
            ),
        );
        assert!(
            importing::import_docx(&source)
                .unwrap_err()
                .to_string()
                .contains(RISK)
        );
    }
    let source = dir.path().join("unused-default.docx");
    external(
        &source,
        r#"<w:p><w:pPr><w:pStyle w:val="Safe"/></w:pPr><w:r><w:t>Safe</w:t></w:r></w:p>"#,
        r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:pPr><w:contextualSpacing/></w:pPr></w:style><w:style w:type="paragraph" w:styleId="Safe"/></w:styles>"#,
    );
    importing::import_docx(&source).unwrap();
}

#[test]
fn only_used_numbering_levels_block_for_unrepresented_spacing() {
    let dir = tempfile::tempdir().unwrap();
    for (index, reference, unsupported_level, rejected) in [
        (0, "", 0, false),
        (
            1,
            r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="7"/></w:numPr>"#,
            1,
            false,
        ),
        (
            2,
            r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="7"/></w:numPr>"#,
            0,
            true,
        ),
        (
            3,
            r#"<w:numPr><w:ilvl w:val="1"/><w:numId w:val="7"/></w:numPr>"#,
            1,
            true,
        ),
    ] {
        let source = dir.path().join(format!("numbers-{index}.docx"));
        external(
            &source,
            &format!("<w:p><w:pPr>{reference}</w:pPr><w:r><w:t>Number</w:t></w:r></w:p>"),
            STYLES,
        );
        let mut archive = parts(&fs::read(&source).unwrap());
        let mut levels = String::new();
        for level in 0..2 {
            let spacing = if level == unsupported_level {
                r#"<w:pPr><w:spacing w:before="120"/><w:contextualSpacing/></w:pPr>"#
            } else {
                ""
            };
            levels.push_str(&format!(r#"<w:lvl w:ilvl="{level}"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/>{spacing}</w:lvl>"#));
        }
        archive.insert("word/numbering.xml".into(),format!(r#"<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:abstractNum w:abstractNumId="4">{levels}</w:abstractNum><w:num w:numId="7"><w:abstractNumId w:val="4"/></w:num></w:numbering>"#).into_bytes());
        let rels = String::from_utf8(archive["word/_rels/document.xml.rels"].clone()).unwrap();
        archive.insert("word/_rels/document.xml.rels".into(),rels.replace("</Relationships>",r#"<Relationship Id="numbering" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/></Relationships>"#).into_bytes());
        if index == 2 {
            let original = String::from_utf8(archive["word/numbering.xml"].clone()).unwrap();
            for (case, xml) in [
                (
                    "missing-level",
                    original.replace(r#"<w:lvl w:ilvl="0">"#, "<w:lvl>"),
                ),
                (
                    "bad-level",
                    original.replace(r#"<w:lvl w:ilvl="0">"#, r#"<w:lvl w:ilvl="bad">"#),
                ),
                (
                    "bad-abstract",
                    original
                        .replace(r#"w:abstractNumId="4""#, r#"w:abstractNumId="bad""#)
                        .replace(
                            r#"<w:abstractNumId w:val="4"/>"#,
                            r#"<w:abstractNumId w:val="0"/>"#,
                        ),
                ),
            ] {
                let mut changed = archive.clone();
                changed.insert("word/numbering.xml".into(), xml.into_bytes());
                let path = dir.path().join(format!("numbers-{case}.docx"));
                write_parts(&path, &changed);
                assert!(
                    importing::import_docx(&path)
                        .unwrap_err()
                        .to_string()
                        .contains(RISK),
                    "{case}"
                );
            }
        }
        write_parts(&source, &archive);
        let result = importing::import_docx(&source);
        if rejected {
            assert!(result.unwrap_err().to_string().contains(RISK));
        } else {
            result.unwrap();
        }
    }
}

#[test]
fn optional_settings_part_has_exact_activation_and_integrity_coverage() {
    let dir = tempfile::tempdir().unwrap();
    for modern in [false, true] {
        for active in [false, true] {
            let doc = model(
                if active {
                    json!({"spaceBefore":0})
                } else {
                    json!({})
                },
                modern,
            );
            let output = dir.path().join(format!("settings-{modern}-{active}.docx"));
            document::export_docx(&doc, &output).unwrap();
            let archive = parts(&fs::read(&output).unwrap());
            assert_eq!(archive.contains_key("word/settings.xml"), active);
            for part in ["word/_rels/document.xml.rels", "[Content_Types].xml"] {
                assert_eq!(
                    String::from_utf8(archive[part].clone())
                        .unwrap()
                        .contains("settings.xml"),
                    active
                );
            }
            if !active {
                continue;
            }
            let key = if modern {
                "rust-wps/model-v2.json"
            } else {
                "rust-wps/model-v1.json"
            };
            for remove in [false, true] {
                let mut forged = archive.clone();
                let mut metadata: Value = serde_json::from_slice(&forged[key]).unwrap();
                if remove {
                    forged.remove("word/settings.xml");
                    metadata["hashes"]
                        .as_object_mut()
                        .unwrap()
                        .remove("word/settings.xml");
                } else {
                    use sha2::{Digest, Sha256};
                    let data = br#"<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:compat/></w:settings>"#.to_vec();
                    metadata["hashes"]["word/settings.xml"] =
                        json!(format!("{:x}", Sha256::digest(&data)));
                    forged.insert("word/settings.xml".into(), data);
                }
                forged.insert(key.into(), serde_json::to_vec(&metadata).unwrap());
                let changed = dir
                    .path()
                    .join(format!("settings-{modern}-{remove}-forged.docx"));
                write_parts(&changed, &forged);
                assert!(document::load_authored_docx(&changed).is_err());
            }
        }
    }
}

#[test]
fn historical_and_foreign_style_markers_cannot_shadow_current_spacing_audit() {
    let dir = tempfile::tempdir().unwrap();
    let styles = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:style w:type="paragraph" w:styleId="Used"><w:pPr><w:contextualSpacing/></w:pPr></w:style><w:style w:type="paragraph" w:styleId="Unused"/></w:styles>"#;
    for (index, nested) in [
        r#"<w:pPrChange w:id="1" w:author="Test"><w:pPr><w:pStyle w:val="Unused"/></w:pPr></w:pPrChange>"#,
        r#"<x:pStyle xmlns:x="urn:foreign" w:val="Unused"/>"#,
        r#"<w:unknown><w:pStyle w:val="Unused"/></w:unknown>"#,
    ].iter().enumerate() {
        let source = dir.path().join(format!("nested-style-{index}.docx"));
        external(&source,&format!(r#"<w:p><w:pPr><w:pStyle w:val="Used"/>{nested}</w:pPr><w:r><w:t>Source</w:t></w:r></w:p>"#),styles);
        let error = importing::import_docx(&source).unwrap_err().to_string();
        assert!(error.contains(RISK) || error.contains("Tracked changes"), "{error}");
    }
}

#[test]
fn strict_main_uses_the_same_active_style_audit() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("strict.docx");
    external(
        &source,
        r#"<w:p><w:pPr><w:pStyle w:val="Used"/></w:pPr><w:r><w:t>Strict</w:t></w:r></w:p>"#,
        r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:style w:type="paragraph" w:styleId="Used"><w:pPr><w:contextualSpacing/></w:pPr></w:style></w:styles>"#,
    );
    let mut archive = parts(&fs::read(&source).unwrap());
    for part in ["word/document.xml", "word/styles.xml"] {
        let xml = String::from_utf8(archive[part].clone()).unwrap().replace(
            "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
            "http://purl.oclc.org/ooxml/wordprocessingml/main",
        );
        archive.insert(part.into(), xml.into_bytes());
    }
    write_parts(&source, &archive);
    let imported = importing::import_docx(&source).unwrap();
    assert_eq!(imported.document.content[0].attrs["spaceBefore"], 0);
    assert_eq!(imported.document.content[0].attrs["spaceAfter"], 0);
    assert!(
        imported
            .compatibility
            .risks
            .iter()
            .any(|risk| risk.contains("Contextual"))
    );
}

#[test]
fn default_table_spacing_is_refused_only_when_an_implicit_table_uses_it() {
    let dir = tempfile::tempdir().unwrap();
    let styles = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:style w:type="table" w:default="1" w:styleId="DefaultTable"><w:pPr><w:spacing w:before="120"/></w:pPr></w:style></w:styles>"#;
    for (index, body, rejected) in [
        (0, "<w:p/>", false),
        (
            1,
            "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"9000\"/></w:tblGrid><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>",
            true,
        ),
        (
            2,
            "<w:tbl><w:tblPr><w:tblStyle w:val=\"DefaultTable\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"9000\"/></w:tblGrid><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>",
            false,
        ),
    ] {
        let source = dir.path().join(format!("default-table-{index}.docx"));
        external(&source, body, styles);
        let result = importing::import_docx(&source);
        if rejected {
            assert!(result.unwrap_err().to_string().contains(RISK));
        } else {
            let model = result.unwrap().document;
            if index == 2 {
                assert_eq!(
                    model.content[0].content[0].content[0].content[0].attrs["spaceBefore"],
                    120
                );
            }
        }
    }
}

#[test]
fn v1_and_v2_without_spacing_match_pre_feature_part_hashes() {
    use sha2::{Digest, Sha256};
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
        let actual: BTreeMap<_, _> = parts(&fs::read(output).unwrap())
            .into_iter()
            .map(|(name, data)| (name, format!("{:x}", Sha256::digest(data))))
            .collect();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            expected["profiles"][profile]
        );
    }
}
