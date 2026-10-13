mod common;
use rust_wps::{
    compatibility,
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

fn write_parts(path: &Path, parts: &BTreeMap<String, Vec<u8>>) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, data) in parts {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
}

fn color(node: &Node) -> Option<&str> {
    node.marks
        .iter()
        .find(|mark| mark.kind == "textColor")
        .map(|mark| mark.attrs["rgb"].as_str().unwrap())
}

fn external_docx(path: &Path, body: &str) {
    let main = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#
    );
    let archive = BTreeMap::from([
        ("[Content_Types].xml".into(), br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#.to_vec()),
        ("_rels/.rels".into(), br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="office" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec()),
        ("word/_rels/document.xml.rels".into(), br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#.to_vec()),
        ("word/styles.xml".into(), br#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults></w:styles>"#.to_vec()),
        ("word/document.xml".into(), main.into_bytes()),
    ]);
    write_parts(path, &archive);
}

const COLOR_NORMALIZATION_RISK: &str =
    "Theme and automatic text colors normalized to fixed RGB or editor defaults";
const CUSTOM_THEME_MAPPING_RISK: &str = "Custom theme color mapping is unsupported";

// A real theme relationship is required: putting theme XML in the ZIP alone
// would not exercise how the DOCX reader discovers and resolves that theme.
const TEST_THEME: &str = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Test"><a:themeElements><a:clrScheme name="Test">
    <a:dk1><a:srgbClr val="102030"/></a:dk1><a:lt1><a:srgbClr val="F0E0D0"/></a:lt1>
    <a:dk2><a:srgbClr val="203040"/></a:dk2><a:lt2><a:srgbClr val="E0D0C0"/></a:lt2>
    <a:accent1><a:srgbClr val="4F81BD"/></a:accent1><a:accent2><a:srgbClr val="FF8080"/></a:accent2>
    <a:accent3><a:srgbClr val="29A17B"/></a:accent3><a:accent4><a:srgbClr val="8064A2"/></a:accent4>
    <a:accent5><a:srgbClr val="C0504D"/></a:accent5>
    <a:hlink><a:srgbClr val="123ABC"/></a:hlink><a:folHlink><a:srgbClr val="804080"/></a:folHlink>
</a:clrScheme><a:fontScheme name="Test"><a:majorFont><a:latin typeface="Calibri"/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/></a:minorFont></a:fontScheme></a:themeElements></a:theme>"#;

fn external_docx_with_formatting(
    path: &Path,
    body: &str,
    styles: Option<&str>,
    theme: Option<&str>,
    mapping: Option<&str>,
) {
    external_docx(path, body);
    let mut archive = parts(&fs::read(path).unwrap());
    if let Some(styles) = styles {
        archive.insert("word/styles.xml".into(), styles.as_bytes().to_vec());
    }
    let settings = mapping.map(|attributes| format!(
        r#"<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:clrSchemeMapping {attributes}/></w:settings>"#
    ));
    for (target, relation, content_type, xml) in [
        (
            "theme/theme1.xml",
            "theme",
            "application/vnd.openxmlformats-officedocument.theme+xml",
            theme,
        ),
        (
            "settings.xml",
            "settings",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml",
            settings.as_deref(),
        ),
    ] {
        let Some(xml) = xml else { continue };
        archive.insert(format!("word/{target}"), xml.as_bytes().to_vec());
        let relationships =
            String::from_utf8(archive["word/_rels/document.xml.rels"].clone()).unwrap();
        archive.insert(
            "word/_rels/document.xml.rels".into(),
            relationships.replace("</Relationships>", &format!(
                r#"<Relationship Id="{relation}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{relation}" Target="{target}"/></Relationships>"#
            )).into_bytes(),
        );
        let content_types = String::from_utf8(archive["[Content_Types].xml"].clone()).unwrap();
        archive.insert(
            "[Content_Types].xml".into(),
            content_types.replace("</Types>", &format!(
                r#"<Override PartName="/word/{target}" ContentType="{content_type}"/></Types>"#
            )).into_bytes(),
        );
    }
    write_parts(path, &archive);
}

fn assert_color_copy_roundtrip(document: &Node, path: &Path) {
    document::export_docx(document, path).unwrap();
    assert_eq!(
        serde_json::to_value(document::load_authored_docx(path).unwrap().unwrap()).unwrap(),
        serde_json::to_value(document).unwrap(),
        "direct authored reopening preserves default versus explicit marks"
    );
    let mut expected = document.clone();
    // External conversion resolves the new profile's declared default color.
    // A direct authored reopen above must still preserve the original model.
    fn materialize_default(node: &mut Node) {
        if node.kind == "text" && color(node).is_none() {
            let link = node.marks.iter().position(|mark| mark.kind == "link");
            node.marks.insert(
                link.unwrap_or(node.marks.len()),
                serde_json::from_value(json!({
                    "type":"textColor", "attrs":{"rgb":if link.is_some() {"0563C1"} else {"000000"}}
                }))
                .unwrap(),
            );
        }
        for child in &mut node.content {
            materialize_default(child);
        }
    }
    materialize_default(&mut expected);
    assert_eq!(
        common::without_font_segmentation(&importing::import_docx(path).unwrap().document),
        common::without_font_segmentation(&expected),
        "external conversion preserves explicit colors and resolves the profile default"
    );
}

#[test]
fn rgb_colors_roundtrip_draft_package_and_ooxml_with_other_marks() {
    let dir = tempfile::tempdir().unwrap();
    for rgb in [
        "000000", "FFFFFF", "FF0000", "00FF00", "0000FF", "ABCDEF", "012345",
    ] {
        let original = model(json!([
            {"type":"bold"}, {"type":"italic"}, {"type":"strike"}, {"type":"superscript"},
            {"type":"fontSize","attrs":{"halfPoints":27}}, {"type":"textColor","attrs":{"rgb":rgb}}
        ]));
        let draft = dir.path().join("draft.json");
        document::save_draft(&original, &draft).unwrap();
        assert_eq!(
            serde_json::to_value(document::load_draft(&draft).unwrap().unwrap()).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        let output = dir.path().join(format!("{rgb}.docx"));
        document::export_docx(&original, &output).unwrap();
        let archive = parts(&fs::read(&output).unwrap());
        let xml = std::str::from_utf8(&archive["word/document.xml"]).unwrap();
        assert_eq!(xml.matches("<w:color ").count(), 1);
        assert!(xml.contains(&format!("<w:color w:val=\"{rgb}\"/>")));
        assert_eq!(
            serde_json::to_value(document::load_authored_docx(&output).unwrap().unwrap()).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        let imported = importing::import_docx(&output).unwrap();
        let runs = &imported.document.content[0].content;
        assert_eq!(
            runs.iter()
                .filter_map(|node| node.text.as_deref())
                .collect::<String>(),
            original.content[0].content[0].text.as_deref().unwrap()
        );
        for run in runs {
            assert_eq!(color(run), Some(rgb));
            for kind in ["bold", "italic", "strike", "superscript", "fontSize"] {
                assert!(
                    run.marks.iter().any(|mark| mark.kind == kind),
                    "missing {kind}"
                );
            }
            assert_eq!(
                run.marks
                    .iter()
                    .find(|mark| mark.kind == "fontSize")
                    .unwrap()
                    .attrs["halfPoints"],
                27
            );
        }
    }
}

#[test]
fn explicit_color_wins_over_link_blue_without_duplicate_ooxml_color() {
    let dir = tempfile::tempdir().unwrap();
    for (index, explicit) in [
        None,
        Some("000000"),
        Some("FFFFFF"),
        Some("FF0000"),
        Some("0563C1"),
    ]
    .into_iter()
    .enumerate()
    {
        for reversed in [false, true] {
            let mut marks = vec![
                json!({"type":"bold"}),
                json!({"type":"link","attrs":{"href":"https://example.com/颜色?a=1&b=2"}}),
            ];
            if let Some(rgb) = explicit {
                marks.push(json!({"type":"textColor","attrs":{"rgb":rgb}}));
            }
            if reversed {
                marks.reverse();
            }
            let original = model(json!(marks));
            let output = dir.path().join(format!("link-{index}-{reversed}.docx"));
            document::export_docx(&original, &output).unwrap();
            let archive = parts(&fs::read(&output).unwrap());
            let xml = std::str::from_utf8(&archive["word/document.xml"]).unwrap();
            let expected = explicit.unwrap_or("0563C1");
            assert_eq!(xml.matches("<w:color ").count(), 1);
            assert!(xml.contains(&format!("<w:color w:val=\"{expected}\"/>")));
            assert!(xml.contains("<w:u w:val=\"single\"/>"));
            if explicit.is_none() {
                assert!(
                    xml.contains(
                        "<w:rPr><w:color w:val=\"0563C1\"/><w:u w:val=\"single\"/><w:b/></w:rPr>"
                    ),
                    "old uncolored link output stays byte-identical"
                );
            }
            assert_eq!(
                serde_json::to_value(document::load_authored_docx(&output).unwrap().unwrap())
                    .unwrap(),
                serde_json::to_value(&original).unwrap()
            );
            let imported = importing::import_docx(&output).unwrap();
            for run in &imported.document.content[0].content {
                assert_eq!(color(run), Some(expected));
                assert!(run.marks.iter().any(|mark| mark.kind == "link"
                    && mark.attrs["href"] == "https://example.com/颜色?a=1&b=2"));
                assert!(run.marks.iter().any(|mark| mark.kind == "bold"));
                let names = run
                    .marks
                    .iter()
                    .map(|mark| mark.kind.as_str())
                    .collect::<Vec<_>>();
                assert!(
                    names.iter().position(|name| *name == "fontSize").unwrap()
                        < names.iter().position(|name| *name == "textColor").unwrap()
                );
                assert!(
                    names.iter().position(|name| *name == "textColor").unwrap()
                        < names.iter().position(|name| *name == "link").unwrap()
                );
            }
        }
    }
}

#[test]
fn invalid_colors_never_replace_a_draft_or_publish_a_package() {
    let dir = tempfile::tempdir().unwrap();
    let draft = dir.path().join("draft.json");
    document::save_draft(&model(json!([])), &draft).unwrap();
    let before = fs::read(&draft).unwrap();
    let mut invalid: Vec<Value> = [
        json!(null),
        json!(true),
        json!(0),
        json!(123456),
        json!([0, 0, 0]),
        json!(""),
        json!("fff"),
        json!("ABC"),
        json!("abcdef"),
        json!("AbCDEF"),
        json!("#ABCDEF"),
        json!("ABCDEG"),
        json!(" ABCDEF"),
        json!("ABCDEF "),
        json!("ABCDEF\n"),
        json!("ＡＢＣＤＥＦ"),
        json!("rgb(0,0,0)"),
        json!("auto"),
        json!("FFFFFF;display:none"),
    ]
    .into_iter()
    .map(|value| json!([{"type":"textColor","attrs":{"rgb":value}}]))
    .collect();
    invalid.extend([
        json!([{"type":"textColor"}]),
        json!([{"type":"textColor","attrs":{"color":"ABCDEF"}}]),
        json!([{"type":"textColor","attrs":{"rgb":"ABCDEF","theme":null}}]),
        json!([{"type":"textColor","attrs":{"rgb":"ABCDEF"}},{"type":"textColor","attrs":{"rgb":"ABCDEF"}}]),
        json!([{"type":"textColor","attrs":{"rgb":"ABCDEF"}},{"type":"textColor","attrs":{"rgb":"000000"}}]),
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
    for kind in ["hardBreak", "paragraph", "pageBreak"] {
        let invalid: Node = serde_json::from_value(json!({"type":"doc","content":[
            {"type":kind,"marks":[{"type":"textColor","attrs":{"rgb":"ABCDEF"}}]}
        ]}))
        .unwrap();
        assert!(document::validate(&invalid).is_err(), "nontext {kind}");
    }
}

#[test]
fn import_normalizes_rgb_coalesces_equal_runs_and_keeps_default_distinct_from_black() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("external.docx");
    external_docx(
        &source,
        r#"<w:p>
        <w:r><w:rPr><w:b/><w:sz w:val="27"/><w:color w:val="aabbcc"/></w:rPr><w:t>甲A</w:t></w:r>
        <w:r><w:rPr><w:b/><w:sz w:val="27"/><w:color w:val="AABBCC"/></w:rPr><w:t>乙B</w:t></w:r>
        <w:r><w:rPr><w:b/><w:sz w:val="27"/><w:color w:val="000000"/></w:rPr><w:t>黑色</w:t><w:br/><w:t>仍然黑色</w:t></w:r>
        <w:r><w:rPr><w:color w:val="auto"/></w:rPr><w:t>自动</w:t></w:r>
        <w:r><w:t>缺省</w:t></w:r>
    </w:p>"#,
    );
    let before = fs::read(&source).unwrap();
    let report = importing::import_docx(&source).unwrap();
    assert!(
        report
            .compatibility
            .risks
            .contains(COLOR_NORMALIZATION_RISK)
    );
    let runs = &report.document.content[0].content;
    assert_eq!(runs.len(), 5);
    assert_eq!(
        (runs[0].text.as_deref(), color(&runs[0])),
        (Some("甲A乙B"), Some("AABBCC"))
    );
    assert_eq!(
        (runs[1].text.as_deref(), color(&runs[1])),
        (Some("黑色"), Some("000000"))
    );
    assert_eq!(runs[2].kind, "hardBreak");
    assert!(runs[2].marks.is_empty());
    assert_eq!(
        (runs[3].text.as_deref(), color(&runs[3])),
        (Some("仍然黑色"), Some("000000"))
    );
    // The current IR resolves auto to None. It must never be invented as black;
    // this is a static editable copy, not preservation of dynamic auto semantics.
    assert_eq!(
        (runs[4].text.as_deref(), color(&runs[4])),
        (Some("自动缺省"), None)
    );
    let output = dir.path().join("converted.docx");
    assert_color_copy_roundtrip(&report.document, &output);
    assert_eq!(fs::read(&source).unwrap(), before);
}

#[test]
fn default_and_heading_colors_keep_inheritance_character_and_direct_precedence() {
    let dir = tempfile::tempdir().unwrap();
    let styles = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
        <w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/><w:color w:val="111213"/></w:rPr></w:rPrDefault></w:docDefaults>
        <w:style w:type="paragraph" w:default="1" w:styleId="Body"><w:rPr><w:color w:val="112233"/></w:rPr></w:style>
        <w:style w:type="paragraph" w:styleId="HeadingBase"><w:rPr><w:b/><w:sz w:val="30"/><w:color w:val="95B3D7" w:themeColor="accent1" w:themeTint="99" w:themeShade="00"/></w:rPr></w:style>
        <w:style w:type="paragraph" w:styleId="Heading2"><w:basedOn w:val="HeadingBase"/></w:style>
        <w:style w:type="character" w:styleId="Accent"><w:rPr><w:i/><w:color w:val="DEADBE" w:themeColor="accent2" w:themeShade="80"/></w:rPr></w:style>
    </w:styles>"#;
    let body = r#"
        <w:p><w:r><w:t>default before</w:t></w:r></w:p>
        <w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:t>inherited heading 中文 سلام</w:t></w:r></w:p>
        <w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:rPr><w:rStyle w:val="Accent"/></w:rPr><w:t>character override</w:t></w:r></w:p>
        <w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:rPr><w:rStyle w:val="Accent"/><w:color w:val="ABCDEF"/></w:rPr><w:t>direct override</w:t></w:r></w:p>
        <w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:rPr><w:color w:val="auto"/></w:rPr><w:t>automatic override</w:t></w:r></w:p>
        <w:p><w:r><w:t>default after</w:t></w:r></w:p>
        <w:p><w:pPr><w:pStyle w:val="Unknown"/><w:outlineLvl w:val="1"/></w:pPr><w:r><w:t>document defaults</w:t></w:r></w:p>
    "#;
    for with_theme in [true, false] {
        let source = dir.path().join(format!("styled-{with_theme}.docx"));
        external_docx_with_formatting(
            &source,
            body,
            Some(styles),
            with_theme.then_some(TEST_THEME),
            None,
        );
        let before = fs::read(&source).unwrap();
        let report = importing::import_docx(&source).unwrap();
        assert!(
            report
                .compatibility
                .risks
                .contains(COLOR_NORMALIZATION_RISK)
        );
        let expected = [
            Some("112233"),
            Some("95B3D7"),
            Some(if with_theme { "C00000" } else { "DEADBE" }),
            Some("ABCDEF"),
            None,
            Some("112233"),
            Some("111213"),
        ];
        assert_eq!(report.document.content.len(), expected.len());
        for (index, rgb) in expected.into_iter().enumerate() {
            let block = &report.document.content[index];
            assert_eq!(
                block.kind,
                if matches!(index, 0 | 5) {
                    "paragraph"
                } else {
                    "heading"
                }
            );
            if block.kind == "heading" {
                assert_eq!(block.attrs["level"], 2);
            }
            assert!(!block.content.is_empty());
            for run in &block.content {
                assert_eq!(color(run), rgb, "block {index}, theme {with_theme}");
                assert_eq!(
                    run.marks
                        .iter()
                        .find(|mark| mark.kind == "fontSize")
                        .unwrap()
                        .attrs["halfPoints"],
                    if (1..=4).contains(&index) { 30 } else { 22 },
                    "color preservation must not discard heading size"
                );
                assert!(
                    !run.marks.iter().any(|mark| mark.kind == "bold"),
                    "paragraph heading emphasis normalization stays unchanged"
                );
                assert_eq!(
                    run.marks.iter().any(|mark| mark.kind == "italic"),
                    matches!(index, 2 | 3)
                );
            }
        }
        assert_color_copy_roundtrip(
            &report.document,
            &dir.path().join(format!("styled-copy-{with_theme}.docx")),
        );
        assert_eq!(fs::read(&source).unwrap(), before);
    }
}

#[test]
fn theme_colors_use_word_hsl_transforms_and_do_not_transform_final_fallback_twice() {
    let dir = tempfile::tempdir().unwrap();
    // Fixed expectations were independently computed with Python's colorsys:
    // h,l,s = colorsys.rgb_to_hls(*(int(rgb[i:i+2],16)/255 for i in (0,2,4)))
    // a = int(transform,16)/255; l = l*a+1-a if tint else l*a
    // ''.join(f'{math.floor(c*255+0.5):02X}' for c in colorsys.hls_to_rgb(h,l,s))
    // In particular C0504D/BF is 953735 using full precision, and 95B3D7
    // is already the final fallback: applying tint 99 again would give BFD1E7.
    let cases = [
        (
            r#"w:val="95B3D7" w:themeColor="accent1" w:themeTint="99""#,
            Some("95B3D7"),
            Some("95B3D7"),
        ),
        (
            r#"w:val="112233" w:themeColor="accent2" w:themeShade="80""#,
            Some("C00000"),
            Some("112233"),
        ),
        (
            r#"w:val="112233" w:themeColor="accent5" w:themeShade="BF""#,
            Some("953735"),
            Some("112233"),
        ),
        (
            r#"w:val="112233" w:themeColor="accent3" w:themeTint="40""#,
            Some("C1EFE1"),
            Some("112233"),
        ),
        (
            r#"w:val="112233" w:themeColor="accent4" w:themeShade="A0""#,
            Some("503E67"),
            Some("112233"),
        ),
        (
            r#"w:val="112233" w:themeColor="accent1" w:themeTint="F""#,
            Some("F5F8FB"),
            Some("112233"),
        ),
        (
            r#"w:val="112233" w:themeColor="accent1" w:themeTint="FF" w:themeShade="00""#,
            Some("4F81BD"),
            Some("112233"),
        ),
        (
            r#"w:val="112233" w:themeColor="accent1" w:themeTint="00""#,
            Some("FFFFFF"),
            Some("112233"),
        ),
        (
            r#"w:val="112233" w:themeColor="accent1" w:themeShade="00""#,
            Some("000000"),
            Some("112233"),
        ),
        (
            r#"w:val="112233" w:themeColor="accent1" w:themeShade="FF""#,
            Some("4F81BD"),
            Some("112233"),
        ),
        (
            r#"w:val="112233" w:themeColor="dark1""#,
            Some("102030"),
            Some("112233"),
        ),
        (
            r#"w:val="112233" w:themeColor="text1""#,
            Some("102030"),
            Some("112233"),
        ),
        (
            r#"w:val="112233" w:themeColor="light1""#,
            Some("F0E0D0"),
            Some("112233"),
        ),
        (
            r#"w:val="112233" w:themeColor="background1""#,
            Some("F0E0D0"),
            Some("112233"),
        ),
        (
            r#"w:val="112233" w:themeColor="dark2""#,
            Some("203040"),
            Some("112233"),
        ),
        (
            r#"w:val="112233" w:themeColor="text2""#,
            Some("203040"),
            Some("112233"),
        ),
        (
            r#"w:val="112233" w:themeColor="light2""#,
            Some("E0D0C0"),
            Some("112233"),
        ),
        (
            r#"w:val="112233" w:themeColor="background2""#,
            Some("E0D0C0"),
            Some("112233"),
        ),
        (
            r#"w:val="auto" w:themeColor="hyperlink""#,
            Some("123ABC"),
            None,
        ),
        (
            r#"w:val="auto" w:themeColor="followedHyperlink""#,
            Some("804080"),
            None,
        ),
        (
            r#"w:val="667788" w:themeColor="accent6" w:themeTint="99""#,
            Some("667788"),
            Some("667788"),
        ),
        (
            r#"w:val="auto" w:themeColor="accent6" w:themeTint="99""#,
            None,
            None,
        ),
        (
            r#"w:val="123456" w:themeColor="none" w:themeTint="99""#,
            Some("123456"),
            Some("123456"),
        ),
        (r#"w:val="auto""#, None, None),
    ];
    let body = cases.iter().enumerate().map(|(index, (attributes, _, _))| format!(
        r#"<w:p><w:r><w:rPr><w:color {attributes}/></w:rPr><w:t>case {index}</w:t></w:r></w:p>"#
    )).collect::<String>();
    for with_theme in [true, false] {
        let source = dir.path().join(format!("theme-{with_theme}.docx"));
        external_docx_with_formatting(&source, &body, None, with_theme.then_some(TEST_THEME), None);
        let before = fs::read(&source).unwrap();
        let report = importing::import_docx(&source).unwrap();
        assert!(
            report
                .compatibility
                .risks
                .contains(COLOR_NORMALIZATION_RISK)
        );
        assert_eq!(report.document.content.len(), cases.len());
        for (index, (_, themed, fallback)) in cases.iter().enumerate() {
            let block = &report.document.content[index];
            assert_eq!(block.content.len(), 1);
            assert_eq!(
                block.content[0].text.as_deref(),
                Some(format!("case {index}").as_str())
            );
            assert_eq!(
                color(&block.content[0]),
                if with_theme { *themed } else { *fallback },
                "case {index}, theme {with_theme}"
            );
        }
        assert_color_copy_roundtrip(
            &report.document,
            &dir.path().join(format!("theme-copy-{with_theme}.docx")),
        );
        assert_eq!(fs::read(&source).unwrap(), before);
    }
}

#[test]
fn custom_theme_mapping_refuses_editable_import_and_standard_defaults_are_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let body = r#"<w:p><w:r><w:rPr><w:color w:val="ABCDEF" w:themeColor="accent1"/></w:rPr><w:t>mapped theme</w:t></w:r></w:p>"#;
    let defaults = r#"w:bg1="light1" w:t1="dark1" w:bg2="light2" w:t2="dark2" w:accent1="accent1" w:accent2="accent2" w:accent3="accent3" w:accent4="accent4" w:accent5="accent5" w:accent6="accent6" w:hyperlink="hyperlink" w:followedHyperlink="followedHyperlink""#;
    for (index, mapping) in [
        None,
        Some(""),
        Some(defaults),
        Some(r#"w:bg1="light1" w:hyperlink="hyperlink""#),
    ]
    .into_iter()
    .enumerate()
    {
        let source = dir.path().join(format!("standard-mapping-{index}.docx"));
        external_docx_with_formatting(&source, body, None, Some(TEST_THEME), mapping);
        let before = fs::read(&source).unwrap();
        let audit = compatibility::audit_docx(&source).unwrap();
        assert!(!audit.risks.contains(CUSTOM_THEME_MAPPING_RISK));
        let report = importing::import_docx(&source).unwrap();
        assert!(
            !report
                .compatibility
                .risks
                .contains(CUSTOM_THEME_MAPPING_RISK)
        );
        assert_eq!(
            color(&report.document.content[0].content[0]),
            Some("4F81BD")
        );
        assert_eq!(fs::read(&source).unwrap(), before);
    }
    for (key, value) in [
        ("bg1", "dark1"),
        ("t1", "light1"),
        ("bg2", "light1"),
        ("t2", "dark1"),
        ("accent1", "accent2"),
        ("accent2", "accent3"),
        ("accent3", "accent4"),
        ("accent4", "accent5"),
        ("accent5", "accent6"),
        ("accent6", "accent1"),
        ("hyperlink", "followedHyperlink"),
        ("followedHyperlink", "hyperlink"),
    ] {
        let source = dir.path().join(format!("custom-mapping-{key}.docx"));
        let mapping = format!(r#"w:{key}="{value}""#);
        external_docx_with_formatting(&source, body, None, Some(TEST_THEME), Some(&mapping));
        let before = fs::read(&source).unwrap();
        let audit = compatibility::audit_docx(&source).unwrap();
        assert!(
            audit.risks.contains(CUSTOM_THEME_MAPPING_RISK),
            "mapping {key}"
        );
        let error = importing::import_docx(&source).unwrap_err().to_string();
        assert!(
            error.contains(CUSTOM_THEME_MAPPING_RISK),
            "mapping {key}: {error}"
        );
        assert!(error.contains("source remains read-only"), "{error}");
        assert_eq!(fs::read(&source).unwrap(), before, "mapping {key}");
    }
}

#[test]
fn theme_scheme_transforms_refuse_only_imports_that_rely_on_them() {
    let dir = tempfile::tempdir().unwrap();
    let scheme_transform = TEST_THEME.replace(
        r#"<a:srgbClr val="4F81BD"/>"#,
        r#"<a:srgbClr val="4F81BD"><a:lumMod val="50000"/></a:srgbClr>"#,
    );
    let format_transform = TEST_THEME.replace(
        "</a:themeElements>",
        r#"<a:fmtScheme name="Test"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"><a:lumMod val="50000"/></a:schemeClr></a:solidFill></a:fillStyleLst></a:fmtScheme></a:themeElements>"#,
    );
    for (name, theme, uses_theme, refused) in [
        ("scheme-used", scheme_transform.as_str(), true, true),
        ("scheme-unused", scheme_transform.as_str(), false, false),
        ("format-used", format_transform.as_str(), true, false),
        ("format-unused", format_transform.as_str(), false, false),
    ] {
        let source = dir.path().join(format!("{name}.docx"));
        let color_attributes = if uses_theme {
            r#"w:val="ABCDEF" w:themeColor="accent1""#
        } else {
            r#"w:val="ABCDEF""#
        };
        let body = format!(
            r#"<w:p><w:r><w:rPr><w:color {color_attributes}/></w:rPr><w:t>scheme transform</w:t></w:r></w:p>"#
        );
        external_docx_with_formatting(&source, &body, None, Some(theme), None);
        let before = fs::read(&source).unwrap();
        if refused {
            let audit = compatibility::audit_docx(&source).unwrap();
            assert!(
                audit
                    .risks
                    .contains("Unsupported theme color scheme content")
            );
            let error = importing::import_docx(&source).unwrap_err().to_string();
            assert!(
                error.contains("Unsupported theme color scheme content"),
                "{error}"
            );
            assert!(error.contains("source remains read-only"), "{error}");
        } else {
            let report = importing::import_docx(&source).unwrap();
            assert_eq!(
                color(&report.document.content[0].content[0]),
                Some(if uses_theme { "4F81BD" } else { "ABCDEF" }),
                "{name}"
            );
            assert_color_copy_roundtrip(
                &report.document,
                &dir.path().join(format!("{name}-copy.docx")),
            );
        }
        assert_eq!(fs::read(&source).unwrap(), before, "{name}");
    }
}

#[test]
fn changing_only_color_metadata_or_visible_xml_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.docx");
    document::export_docx(
        &model(json!([{"type":"textColor","attrs":{"rgb":"123456"}}])),
        &source,
    )
    .unwrap();
    let original = parts(&fs::read(&source).unwrap());
    for metadata_only in [true, false] {
        let mut archive = original.clone();
        if metadata_only {
            let mut metadata: Value =
                serde_json::from_slice(&archive["rust-wps/model-v1.json"]).unwrap();
            metadata["document"]["content"][0]["content"][0]["marks"][0]["attrs"]["rgb"] =
                json!("654321");
            archive.insert(
                "rust-wps/model-v1.json".into(),
                serde_json::to_vec(&metadata).unwrap(),
            );
        } else {
            let xml = String::from_utf8(archive["word/document.xml"].clone()).unwrap();
            archive.insert(
                "word/document.xml".into(),
                xml.replace("123456", "654321").into_bytes(),
            );
        }
        let forged = dir.path().join(format!("forged-{metadata_only}.docx"));
        write_parts(&forged, &archive);
        let error = document::load_authored_docx(&forged)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(if metadata_only {
                "disagrees"
            } else {
                "modified"
            }),
            "{error}"
        );
    }
}
