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

fn model(name: &str, modern: bool) -> Node {
    let mut value = json!({"type":"doc","content":[{"type":"paragraph","content":[
        {"type":"text","text":"Latin 中文 سلام 😀","marks":[
            {"type":"fontFamily","attrs":{"name":name}},
            {"type":"fontSize","attrs":{"halfPoints":27}},
            {"type":"textColor","attrs":{"rgb":"1A6B3C"}}
        ]}
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

fn family(node: &Node) -> Option<&str> {
    node.marks
        .iter()
        .find(|mark| mark.kind == "fontFamily")
        .map(|mark| mark.attrs["name"].as_str().unwrap())
}

fn attach_part(path: &Path, name: &str, kind: &str, xml: &str) {
    let mut archive = parts(&fs::read(path).unwrap());
    archive.insert(name.into(), xml.as_bytes().to_vec());
    let rels = String::from_utf8(archive["word/_rels/document.xml.rels"].clone()).unwrap();
    archive.insert("word/_rels/document.xml.rels".into(), rels.replace("</Relationships>", &format!(r#"<Relationship Id="{kind}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}" Target="{}"/></Relationships>"#, name.strip_prefix("word/").unwrap())).into_bytes());
    write_parts(path, &archive);
}

const FONT_THEME: &str = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Fonts"><a:themeElements><a:clrScheme name="Test"><a:dk1><a:srgbClr val="000000"/></a:dk1></a:clrScheme><a:fontScheme name="Fonts"><a:majorFont><a:latin typeface="MajorLatin"/><a:ea typeface=""/><a:cs typeface=""/><a:font script="Jpan" typeface="MajorJapanese"/><a:font script="Hans" typeface="MajorChinese"/><a:font script="Arab" typeface="MajorArabic"/></a:majorFont><a:minorFont><a:latin typeface="MinorLatin"/><a:ea typeface=""/><a:cs typeface=""/><a:font script="Jpan" typeface="MinorJapanese"/><a:font script="Hans" typeface="MinorChinese"/><a:font script="Arab" typeface="MinorArabic"/></a:minorFont></a:fontScheme></a:themeElements></a:theme>"#;

#[test]
fn theme_fonts_resolve_independent_slots_languages_and_literal_overrides() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("theme.docx");
    external(
        &source,
        r#"<w:p><w:r><w:t>Aé中س</w:t></w:r></w:p><w:p><w:r><w:rPr><w:rFonts w:ascii="Direct" w:hAnsi="High" w:eastAsia="East" w:cs="Complex"/></w:rPr><w:t>Aé中س</w:t></w:r></w:p>"#,
        r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Stale" w:asciiTheme="minorHAnsi" w:hAnsiTheme="majorHAnsi" w:eastAsiaTheme="minorEastAsia" w:cstheme="majorBidi"/></w:rPr></w:rPrDefault></w:docDefaults></w:styles>"#,
    );
    attach_part(&source, "word/theme/theme1.xml", "theme", FONT_THEME);
    attach_part(
        &source,
        "word/settings.xml",
        "settings",
        r#"<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:themeFontLang w:val="ja-JP" w:eastAsia="zh-CN" w:bidi="ar-SA"/></w:settings>"#,
    );
    let before = fs::read(&source).unwrap();
    let imported = importing::import_docx(&source).unwrap();
    assert_eq!(
        imported.document.content[0]
            .content
            .iter()
            .map(family)
            .collect::<Vec<_>>(),
        [
            Some("MinorJapanese"),
            Some("MajorJapanese"),
            Some("MinorChinese"),
            Some("MajorArabic")
        ]
    );
    assert_eq!(
        imported.document.content[1]
            .content
            .iter()
            .map(family)
            .collect::<Vec<_>>(),
        [Some("Direct"), Some("High"), Some("East"), Some("Complex")]
    );
    assert!(
        !imported
            .compatibility
            .risks
            .iter()
            .any(|risk| risk.contains("UNRESOLVED"))
    );
    assert_eq!(fs::read(source).unwrap(), before);
}

#[test]
fn unresolved_used_theme_fonts_warn_without_using_stale_literal_or_ascii() {
    let dir = tempfile::tempdir().unwrap();
    let styles = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Latin" w:eastAsia="Stale" w:eastAsiaTheme="minorEastAsia"/></w:rPr></w:rPrDefault></w:docDefaults></w:styles>"#;
    for (index, text, theme, settings, expected) in [
        (0, "A中", None, None, vec![Some("Latin"), None]),
        (1, "A中", Some(FONT_THEME), None, vec![Some("Latin"), None]),
        (
            2,
            "A中",
            Some(FONT_THEME),
            Some("w:eastAsia=\"xx-ZZ\""),
            vec![Some("Latin"), None],
        ),
        (3, "A", Some(FONT_THEME), None, vec![Some("Latin")]),
    ] {
        let source = dir.path().join(format!("unresolved-{index}.docx"));
        external(
            &source,
            &format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>"),
            styles,
        );
        if let Some(theme) = theme {
            attach_part(&source, "word/theme/theme1.xml", "theme", theme);
        }
        if let Some(settings) = settings {
            attach_part(
                &source,
                "word/settings.xml",
                "settings",
                &format!(
                    r#"<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:themeFontLang {settings}/></w:settings>"#
                ),
            );
        }
        let imported = importing::import_docx(&source).unwrap();
        assert_eq!(
            imported.document.content[0]
                .content
                .iter()
                .map(family)
                .collect::<Vec<_>>(),
            expected,
            "case {index}"
        );
        assert_eq!(
            imported
                .compatibility
                .risks
                .contains("Parser warning: DOCX_FONT_THEME_UNRESOLVED"),
            index != 3,
            "case {index}"
        );
    }
}

#[test]
fn hansi_theme_does_not_override_ascii_and_tabs_keep_their_run_formatting() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("tab.docx");
    external(
        &source,
        r#"<w:p><w:r><w:rPr><w:rFonts w:ascii="Ascii" w:hAnsiTheme="minorHAnsi"/></w:rPr><w:t>Aé</w:t></w:r></w:p><w:p><w:r><w:rPr><w:rFonts w:ascii="Courier New" w:hAnsi="Courier New" w:eastAsia="Courier New" w:cs="Courier New"/><w:sz w:val="27"/><w:color w:val="1A6B3C"/><w:b/></w:rPr><w:t>A</w:t><w:tab/><w:t>B</w:t></w:r></w:p>"#,
        r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#,
    );
    attach_part(&source, "word/theme/theme1.xml", "theme", FONT_THEME);
    let imported = importing::import_docx(&source).unwrap();
    assert_eq!(
        imported.document.content[0]
            .content
            .iter()
            .map(family)
            .collect::<Vec<_>>(),
        [Some("Ascii"), Some("MinorLatin")]
    );
    let tab = &imported.document.content[1].content;
    assert_eq!(tab.len(), 1);
    assert_eq!(tab[0].text.as_deref(), Some("A\tB"));
    assert_eq!(family(&tab[0]), Some("Courier New"));
    for kind in ["bold", "fontSize", "textColor"] {
        assert!(tab[0].marks.iter().any(|mark| mark.kind == kind));
    }
}

#[test]
fn literal_font_names_roundtrip_drafts_four_script_slots_and_both_profiles() {
    let dir = tempfile::tempdir().unwrap();
    for (index, name) in [
        "Courier New",
        "微软雅黑",
        "A&B <C> \"D\",E\\F;",
        &"字".repeat(31),
        &format!("{}A", "😀".repeat(15)),
    ]
    .iter()
    .enumerate()
    {
        for modern in [false, true] {
            let original = model(name, modern);
            let draft = dir.path().join("draft.json");
            document::save_draft(&original, &draft).unwrap();
            assert_eq!(
                serde_json::to_value(document::load_draft(&draft).unwrap().unwrap()).unwrap(),
                serde_json::to_value(&original).unwrap()
            );
            let output = dir.path().join(format!("font-{index}-{modern}.docx"));
            document::export_docx(&original, &output).unwrap();
            assert_eq!(
                serde_json::to_value(document::load_authored_docx(&output).unwrap().unwrap())
                    .unwrap(),
                serde_json::to_value(&original).unwrap()
            );
            let archive = parts(&fs::read(&output).unwrap());
            let mut reader =
                quick_xml::Reader::from_reader(archive["word/document.xml"].as_slice());
            let mut fonts = Vec::new();
            loop {
                match reader.read_event().unwrap() {
                    quick_xml::events::Event::Empty(element)
                        if element.local_name().as_ref() == "rFonts" =>
                    {
                        fonts.push(
                            element
                                .attributes()
                                .map(|attribute| {
                                    let attribute = attribute.unwrap();
                                    (
                                        attribute.key.local_name().as_ref().to_owned(),
                                        attribute
                                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                            .unwrap()
                                            .into_owned(),
                                    )
                                })
                                .collect::<BTreeMap<_, _>>(),
                        );
                    }
                    quick_xml::events::Event::Eof => break,
                    _ => {}
                }
            }
            assert_eq!(
                fonts,
                vec![BTreeMap::from_iter(
                    ["ascii", "hAnsi", "eastAsia", "cs"]
                        .map(|slot| (slot.to_string(), name.to_string()))
                )]
            );
            let converted = importing::import_docx(&output).unwrap();
            assert_eq!(converted.document.content[0].content.len(), 1);
            let text = &converted.document.content[0].content[0];
            assert_eq!(text.text, original.content[0].content[0].text);
            assert_eq!(family(text), Some(*name));
            assert_eq!(
                text.marks
                    .iter()
                    .find(|mark| mark.kind == "fontSize")
                    .unwrap()
                    .attrs["halfPoints"],
                27
            );
            assert_eq!(
                text.marks
                    .iter()
                    .find(|mark| mark.kind == "textColor")
                    .unwrap()
                    .attrs["rgb"],
                "1A6B3C"
            );
        }
    }
}

#[test]
fn invalid_or_duplicate_font_marks_never_change_a_draft_or_publish() {
    let dir = tempfile::tempdir().unwrap();
    let draft = dir.path().join("draft.json");
    let original = model("Arial", true);
    document::save_draft(&original, &draft).unwrap();
    let before = fs::read(&draft).unwrap();
    let mut invalid: Vec<Value> = [
        json!(null),
        json!(false),
        json!(3),
        json!(""),
        json!(" Arial"),
        json!("Arial "),
        json!("A\nB"),
        json!("A\u{7f}B"),
        json!("A\u{85}B"),
        json!("A\u{fffe}B"),
        json!("\u{feff}Arial"),
        json!("字".repeat(32)),
        json!("😀".repeat(16)),
    ]
    .into_iter()
    .map(|name| json!({"type":"fontFamily","attrs":{"name":name}}))
    .collect();
    invalid.extend([
        json!({"type":"fontFamily"}),
        json!({"type":"fontFamily","attrs":{"name":"Arial","extra":1}}),
    ]);
    let mut invalid_models = Vec::new();
    for mark in invalid {
        let mut doc = original.clone();
        doc.content[0].content[0].marks[0] = serde_json::from_value(mark).unwrap();
        invalid_models.push(doc);
    }
    let mut duplicate = original.clone();
    let extra = duplicate.content[0].content[0].marks[0].clone();
    duplicate.content[0].content[0].marks.push(extra);
    invalid_models.push(duplicate);
    let mut nontext = original.clone();
    let misplaced = nontext.content[0].content[0].marks[0].clone();
    nontext.content[0].marks.push(misplaced);
    invalid_models.push(nontext);
    for (index, doc) in invalid_models.iter().enumerate() {
        let output = dir.path().join(format!("bad-{index}.docx"));
        assert!(document::save_draft(doc, &draft).is_err(), "{index}");
        assert_eq!(fs::read(&draft).unwrap(), before);
        assert!(document::export_docx(doc, &output).is_err(), "{index}");
        assert!(!output.exists());
    }
}

#[test]
fn explicit_family_overrides_code_once_regardless_of_mark_order() {
    let dir = tempfile::tempdir().unwrap();
    for reverse in [false, true] {
        let mut doc = model("Courier New", true);
        let marks = &mut doc.content[0].content[0].marks;
        marks.push(serde_json::from_value(json!({"type":"code"})).unwrap());
        if reverse {
            marks.reverse();
        }
        let output = dir.path().join(format!("code-{reverse}.docx"));
        document::export_docx(&doc, &output).unwrap();
        let archive = parts(&fs::read(output).unwrap());
        let xml = std::str::from_utf8(&archive["word/document.xml"]).unwrap();
        assert_eq!(xml.matches("<w:rFonts ").count(), 1);
        assert!(!xml.contains("Consolas"));
        for slot in ["ascii", "hAnsi", "eastAsia", "cs"] {
            assert!(xml.contains(&format!("w:{slot}=\"Courier New\"")));
        }
    }
}

#[test]
fn external_font_inheritance_keeps_heading_and_each_script_font() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("scripts.docx");
    external(
        &source,
        r#"<w:p><w:pPr><w:pStyle w:val="TitleFont"/></w:pPr><w:r><w:t>A中é</w:t></w:r><w:r><w:rPr><w:rStyle w:val="CharFont"/></w:rPr><w:t>Z</w:t></w:r><w:r><w:rPr><w:rStyle w:val="CharFont"/><w:rFonts w:ascii="Direct"/></w:rPr><w:t>D</w:t></w:r></w:p><w:p><w:r><w:rPr><w:cs/></w:rPr><w:t>C</w:t></w:r></w:p>"#,
        r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Base" w:hAnsi="High" w:eastAsia="East" w:cs="Complex"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:styleId="TitleFont"><w:name w:val="heading 1"/><w:pPr><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:rFonts w:ascii="Heading" w:eastAsia="标题"/></w:rPr></w:style><w:style w:type="character" w:styleId="CharFont"><w:rPr><w:rFonts w:ascii="Character"/></w:rPr></w:style></w:styles>"#,
    );
    let before = fs::read(&source).unwrap();
    let imported = importing::import_docx(&source).unwrap();
    assert_eq!(imported.document.content[0].kind, "heading");
    let actual: Vec<_> = imported.document.content[0]
        .content
        .iter()
        .map(|node| (node.text.as_deref().unwrap(), family(node)))
        .collect();
    assert_eq!(
        actual,
        vec![
            ("A", Some("Heading")),
            ("中", Some("标题")),
            ("é", Some("High")),
            ("Z", Some("Character")),
            ("D", Some("Direct"))
        ]
    );
    assert_eq!(
        family(&imported.document.content[1].content[0]),
        Some("Complex")
    );
    assert_eq!(fs::read(source).unwrap(), before);
}

#[test]
fn different_font_ranges_are_not_coalesced_or_silently_dropped_at_the_budget() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("ranges.docx");
    let styles = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Latin" w:eastAsia="East"/></w:rPr></w:rPrDefault></w:docDefaults></w:styles>"#;
    external(&source, "<w:p><w:r><w:t>A中B</w:t></w:r></w:p>", styles);
    let imported = importing::import_docx(&source).unwrap();
    assert_eq!(
        imported.document.content[0]
            .content
            .iter()
            .map(family)
            .collect::<Vec<_>>(),
        [Some("Latin"), Some("East"), Some("Latin")]
    );
    let oversized = dir.path().join("over-budget.docx");
    external(
        &oversized,
        &"<w:p><w:r><w:t>A中B</w:t></w:r></w:p>".repeat(3000),
        styles,
    );
    let before = fs::read(&oversized).unwrap();
    assert!(
        importing::import_docx(&oversized)
            .unwrap_err()
            .to_string()
            .contains("structure exceeds limits")
    );
    assert_eq!(fs::read(oversized).unwrap(), before);
}

#[test]
fn model_only_font_tampering_cannot_replace_the_actual_package_font() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("original.docx");
    document::export_docx(&model("Arial", true), &output).unwrap();
    let mut archive = parts(&fs::read(&output).unwrap());
    let key = "rust-wps/model-v2.json";
    let mut metadata: Value = serde_json::from_slice(&archive[key]).unwrap();
    metadata["document"]["content"][0]["content"][0]["marks"][0]["attrs"]["name"] =
        json!("Courier New");
    archive.insert(key.into(), serde_json::to_vec(&metadata).unwrap());
    let modified = dir.path().join("modified.docx");
    write_parts(&modified, &archive);
    assert!(
        document::load_authored_docx(&modified)
            .unwrap_err()
            .to_string()
            .contains("Embedded model disagrees")
    );
}
