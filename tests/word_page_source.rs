//! Source page values require a complete, bounded, namespace-aware proof.
use rust_wps::{compatibility, importing};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, io::Write, path::Path};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const P: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const BLOCK: &str = "Unsupported page geometry cannot be preserved in an editable copy";
const NORMALIZED: &str =
    "Source page geometry is incomplete; editable copy uses default A4 margins";
const METADATA: &str = "Paper code and header/footer distance metadata normalized; only page size and body margins preserved";
const TEXT: &str = "<w:p><w:r><w:t>PageProbe</w:t></w:r></w:p>";
fn geometry(w: u64, h: u64) -> Value {
    json!({"widthTwips":w,"heightTwips":h,"marginTopTwips":720,"marginRightTwips":1440,"marginBottomTwips":1800,"marginLeftTwips":2160})
}
fn size(w: u64, h: u64, orient: &str) -> String {
    format!(r#"<w:pgSz w:w="{w}" w:h="{h}"{orient}/>"#)
}
fn margins() -> &'static str {
    r#"<w:pgMar w:top="720" w:right="1440" w:bottom="1800" w:left="2160"/>"#
}
fn section() -> String {
    format!(
        "<w:sectPr>{}{}</w:sectPr>",
        size(11906, 16838, ""),
        margins()
    )
}
fn package(body: &str) -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
        ("[Content_Types].xml".into(), br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_vec()),
        ("_rels/.rels".into(),format!(r#"<Relationships xmlns="{P}"><Relationship Id="main" Type="{R}/officeDocument" Target="word/document.xml"/></Relationships>"#).into_bytes()),
        ("word/document.xml".into(),format!(r#"<w:document xmlns:w="{W}" xmlns:x="urn:foreign" xmlns:same="{W}"><w:body>{body}</w:body></w:document>"#).into_bytes()),
    ])
}
fn rels(items: &str) -> Vec<u8> {
    format!(r#"<Relationships xmlns="{P}">{items}</Relationships>"#).into_bytes()
}
fn relation(kind: &str, target: &str) -> String {
    format!(r#"<Relationship Id="{kind}" Type="{R}/{kind}" Target="{target}"/>"#)
}
fn settings(content: &str) -> Vec<u8> {
    format!(
        r#"<w:settings xmlns:w="{W}" xmlns:x="urn:foreign" xmlns:same="{W}">{content}</w:settings>"#
    )
    .into_bytes()
}
fn add_settings(parts: &mut BTreeMap<String, Vec<u8>>, content: &str) {
    parts.insert(
        "word/_rels/document.xml.rels".into(),
        rels(&relation("settings", "../custom/layout.xml")),
    );
    parts.insert("custom/layout.xml".into(), settings(content));
}
fn write(path: &Path, parts: &BTreeMap<String, Vec<u8>>) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, bytes) in parts {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}
fn with_source<T>(parts: &BTreeMap<String, Vec<u8>>, f: impl FnOnce(&Path) -> T) -> T {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.docx");
    write(&path, parts);
    let before = fs::read(&path).unwrap();
    let output = f(&path);
    assert_eq!(before, fs::read(&path).unwrap());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    output
}
fn refused(parts: &BTreeMap<String, Vec<u8>>) {
    with_source(parts, |path| {
        let audit = compatibility::audit_docx(path).unwrap();
        assert!(audit.risks.contains(BLOCK), "{:?}", audit.risks);
        assert!(importing::import_docx(path).is_err());
    });
}
#[test]
fn canonical_and_common_rounding_sizes_preserve_exact_source_values() {
    for (w, h) in [
        (11906, 16838),
        (11907, 16840),
        (12240, 15840),
        (8391, 11906),
        (8390, 11907),
    ] {
        for landscape in [false, true] {
            let (w, h) = if landscape { (h, w) } else { (w, h) };
            let sect = format!(
                "<w:sectPr>{}{}</w:sectPr>",
                size(
                    w,
                    h,
                    if landscape {
                        r#" w:orient="landscape""#
                    } else {
                        ""
                    }
                ),
                margins()
            );
            with_source(&package(&format!("{TEXT}{sect}")), |path| {
                let imported = importing::import_docx(path).unwrap();
                assert_eq!(imported.document.attrs["pageGeometry"], geometry(w, h));
                assert!(!imported.compatibility.risks.contains(NORMALIZED));
                assert!(
                    !imported
                        .compatibility
                        .risks
                        .contains("Section and page geometry")
                );
                assert!(!imported.compatibility.fidelity_verified);
            });
        }
    }
}
#[test]
fn absent_sections_and_missing_whole_geometry_elements_are_explicitly_normalized() {
    for sect in [
        String::new(),
        "<w:sectPr/>".into(),
        format!("<w:sectPr>{}</w:sectPr>", margins()),
        format!("<w:sectPr>{}</w:sectPr>", size(11906, 16838, "")),
    ] {
        with_source(&package(&format!("{TEXT}{sect}")), |path| {
            let imported = importing::import_docx(path).unwrap();
            assert!(!imported.document.attrs.contains_key("pageGeometry"));
            assert!(imported.compatibility.risks.contains(NORMALIZED));
        });
    }
}
#[test]
fn harmless_inactive_section_and_settings_metadata_does_not_block_preservation() {
    let sect=section().replace("w:h=\"16838\"", "w:h=\"16838\" w:code=\"9\"")
        .replace("w:top=\"720\"", "w:top=\"720\" w:header=\"709\" w:footer=\"709\" w:gutter=\"0\"")
        .replace("</w:sectPr>",r#"<w:type w:val="nextPage"/><w:cols w:space="708"/><w:docGrid w:linePitch="360"/><w:titlePg w:val="0"/><w:formProt w:val="false"/><w:bidi w:val="false"/><w:rtlGutter w:val="off"/><w:vAlign w:val="top"/><w:textDirection w:val="lrTb"/></w:sectPr>"#);
    let mut parts = package(&format!("{TEXT}{sect}"));
    add_settings(
        &mut parts,
        r#"<w:mirrorMargins w:val="false"/><w:gutterAtTop w:val="0"/><w:evenAndOddHeaders w:val="off"/><w:bookFoldPrinting w:val="0"/><w:bookFoldPrintingSheets w:val="0"/><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#,
    );
    with_source(&parts, |path| {
        let imported = importing::import_docx(path).unwrap();
        assert_eq!(
            imported.document.attrs["pageGeometry"],
            geometry(11906, 16838)
        );
        assert!(imported.compatibility.risks.contains(METADATA));
    });
}
#[test]
fn malformed_duplicate_wrapped_and_conflicting_geometry_is_refused() {
    let good = section();
    let variants = vec![
        good.replace("11906", "10000"),
        good.replace("11906", "11909"),
        good.replace("11906", "-11906"),
        good.replace("11906", "11906.0"),
        good.replace("11906", "11906pt"),
        good.replace("11906", " 11906"),
        good.replace("w:w=\"11906\"", ""),
        good.replace("w:top=\"720\"", ""),
        good.replace("w:top=\"720\"", "w:top=\"-1\""),
        good.replace("w:top=\"720\"", "w:top=\"4321\""),
        good.replace("w:pgSz", "x:pgSz"),
        good.replace("w:pgMar", "x:pgMar"),
        good.replace("w:sectPr", "x:sectPr"),
        good.replace("w:w=\"11906\"", "x:w=\"11906\""),
        good.replace("w:w=\"11906\"", "same:w=\"11906\""),
        good.replace("w:w=\"11906\"", "w:w=\"11906\" x:w=\"11906\""),
        good.replace("w:top=\"720\"", "w:top=\"720\" same:top=\"720\""),
        good.replace("w:h=\"16838\"", "w:h=\"16838\" w:orient=\"landscape\""),
        good.replace("w:h=\"16838\"", "w:h=\"16838\" w:orient=\"unknown\""),
        good.replace("<w:pgSz", "<w:wrap><w:pgSz")
            .replace("<w:pgMar", "</w:wrap><w:pgMar"),
        good.replace(
            "</w:sectPr>",
            &format!("{}</w:sectPr>", size(11906, 16838, "")),
        ),
        good.replace("</w:sectPr>", &format!("{}</w:sectPr>", margins())),
        good.replace("w:h=\"16838\"", "w:h=\"16838\" w:unknown=\"0\""),
        good.replace("<w:pgSz", "<w:pgSz xmlns:w=\"urn:foreign\""),
    ];
    for bad in variants {
        refused(&package(&format!("{TEXT}{bad}")));
    }
    refused(&package(&format!(
        "{TEXT}<w:sectPr>{}{}</w:sectPr>",
        size(16838, 11906, ""),
        margins()
    )));
}
#[test]
fn multiple_interior_nonterminal_and_wrapped_sections_are_refused() {
    for body in [
        format!("{TEXT}{}{}", section(), section()),
        format!("{}{}", section(), TEXT),
        format!("{TEXT}<w:wrap>{}</w:wrap>", section()),
        format!("<w:p><w:pPr>{}</w:pPr></w:p>", section()),
        format!("{TEXT}{}<w:p/>", section()),
        format!(
            "{TEXT}{}",
            section().replace("</w:sectPr>", "<w:sectPr/></w:sectPr>")
        ),
    ] {
        refused(&package(&body));
    }
}
#[test]
fn active_section_features_and_settings_fail_closed() {
    for property in [
        r#"<w:type w:val="continuous"/>"#,
        r#"<w:type w:val="oddPage"/>"#,
        r#"<w:cols w:num="2"/>"#,
        r#"<w:cols><w:col w:w="8000"/></w:cols>"#,
        r#"<w:cols w:sep="1"/>"#,
        r#"<w:cols w:equalWidth="0"/>"#,
        r#"<w:docGrid w:type="lines"/>"#,
        r#"<w:titlePg/>"#,
        r#"<w:formProt/>"#,
        r#"<w:bidi/>"#,
        r#"<w:rtlGutter/>"#,
        r#"<w:textDirection w:val="tbRl"/>"#,
        r#"<w:vAlign w:val="center"/>"#,
        r#"<w:pgBorders/>"#,
        r#"<w:paperSrc/>"#,
        r#"<w:pgNumType w:start="2"/>"#,
        r#"<w:sectPrChange/>"#,
        r#"<w:headerReference/>"#,
        r#"<w:footerReference/>"#,
    ] {
        refused(&package(&format!(
            "{TEXT}{}",
            section().replace("</w:sectPr>", &format!("{property}</w:sectPr>"))
        )));
    }
    refused(&package(&format!(
        "{TEXT}{}",
        section().replace("w:top=\"720\"", "w:top=\"720\" w:gutter=\"1\"")
    )));
    for property in [
        "<w:mirrorMargins/>",
        "<w:gutterAtTop/>",
        "<w:bookFoldPrinting/>",
        "<w:bookFoldRevPrinting/>",
        "<w:printTwoOnOne/>",
        "<w:evenAndOddHeaders/>",
        r#"<w:bookFoldPrintingSheets w:val="2"/>"#,
        r#"<x:mirrorMargins w:val="0"/>"#,
        r#"<w:mirrorMargins same:val="0"/>"#,
        r#"<w:mirrorMargins w:val="0" x:val="0"/>"#,
        r#"<w:mirrorMargins w:val="0"/><w:mirrorMargins w:val="0"/>"#,
        r#"<w:wrap><w:mirrorMargins w:val="0"/></w:wrap>"#,
        r#"<w:mirrorMargins w:val="unknown"/>"#,
    ] {
        let mut parts = package(&format!("{TEXT}{}", section()));
        add_settings(&mut parts, property);
        refused(&parts);
    }
}
#[test]
fn settings_relationship_identity_cannot_be_replaced_by_a_familiar_filename() {
    let mut parts = package(&format!("{TEXT}{}", section()));
    add_settings(&mut parts, "<w:mirrorMargins/>");
    parts.insert("word/settings.xml".into(), settings(""));
    refused(&parts);
    // An orphan familiar filename does not override the actual neutral target.
    add_settings(&mut parts, "");
    parts.insert("word/settings.xml".into(), settings("<w:mirrorMargins/>"));
    with_source(&parts, |path| {
        assert_eq!(
            importing::import_docx(path).unwrap().document.attrs["pageGeometry"],
            geometry(11906, 16838)
        )
    });
    for rel in [
        relation("settings", "missing.xml"),
        format!(
            "{}{}",
            relation("settings", "../custom/layout.xml"),
            relation("settings", "settings.xml").replace("Id=\"settings\"", "Id=\"second\"")
        ),
        relation("settings", "../custom/layout.xml").replace("/>", r#" TargetMode="External"/>"#),
        relation("settings", "../custom/layout.xml").replace("/>", r#" TargetMode="unknown"/>"#),
        format!(
            "<Wrap>{}</Wrap>",
            relation("settings", "../custom/layout.xml")
        ),
        relation("settings", "../custom/layout.xml")
            .replace("<Relationship", "<Relationship xmlns=\"urn:foreign\""),
    ] {
        parts.insert("word/_rels/document.xml.rels".into(), rels(&rel));
        refused(&parts);
    }
}
#[test]
fn unused_styles_do_not_activate_geometry_but_used_ancestry_does() {
    let mut parts = package(&format!("{TEXT}{}", section()));
    parts.insert(
        "word/_rels/document.xml.rels".into(),
        rels(&relation("styles", "../custom/styles.xml")),
    );
    let style = r#"<w:style w:type="paragraph" w:styleId="Bad"><w:pPr><w:sectPr><w:pgSz w:w="10000" w:h="10000"/></w:sectPr></w:pPr></w:style>"#;
    parts.insert(
        "custom/styles.xml".into(),
        format!(r#"<w:styles xmlns:w="{W}">{style}</w:styles>"#).into_bytes(),
    );
    with_source(&parts, |path| {
        assert_eq!(
            importing::import_docx(path).unwrap().document.attrs["pageGeometry"],
            geometry(11906, 16838)
        )
    });
    let styled = TEXT.replace(
        "<w:p>",
        r#"<w:p><w:pPr><w:pStyle w:val="Derived"/></w:pPr>"#,
    );
    parts.insert(
        "word/document.xml".into(),
        package(&format!("{styled}{}", section()))
            .remove("word/document.xml")
            .unwrap(),
    );
    parts.insert("custom/styles.xml".into(),format!(r#"<w:styles xmlns:w="{W}">{style}<w:style w:type="paragraph" w:styleId="Derived"><w:basedOn w:val="Bad"/></w:style></w:styles>"#).into_bytes());
    refused(&parts);
}
#[test]
fn strict_relationship_types_and_utf16_main_use_the_same_normalized_snapshot() {
    let mut parts = package(&format!("{TEXT}{}", section()));
    add_settings(&mut parts, "");
    let rel = String::from_utf8(parts.remove("word/_rels/document.xml.rels").unwrap())
        .unwrap()
        .replace(R, "http://purl.oclc.org/ooxml/officeDocument/relationships");
    parts.insert("word/_rels/document.xml.rels".into(), rel.into_bytes());
    let main = String::from_utf8(parts.remove("word/document.xml").unwrap())
        .unwrap()
        .replace(W, "http://purl.oclc.org/ooxml/wordprocessingml/main");
    let mut utf16 = vec![0xff, 0xfe];
    for unit in format!("<?xml version=\"1.0\" encoding=\"UTF-16\"?>{main}").encode_utf16() {
        utf16.extend(unit.to_le_bytes());
    }
    parts.insert("word/document.xml".into(), utf16);
    with_source(&parts, |path| {
        assert_eq!(
            importing::import_docx(path).unwrap().document.attrs["pageGeometry"],
            geometry(11906, 16838)
        )
    });
}

#[test]
fn bounded_resource_limits_and_duplicate_body_cannot_supply_geometry() {
    let duplicate = package(&format!("{TEXT}{}<w:body/>", section()));
    refused(&duplicate);
    let deeply_wrapped = format!(
        "{}{}{}",
        "<w:wrap>".repeat(130),
        TEXT,
        "</w:wrap>".repeat(130)
    );
    with_source(
        &package(&format!("{deeply_wrapped}{}", section())),
        |path| {
            // The shared bounded table/main reader may hit its equal depth budget
            // before the page-specific proof is reached.
            assert!(compatibility::audit_docx(path).is_err());
            assert!(importing::import_docx(path).is_err());
        },
    );
}

#[test]
fn bound_alternative_element_prefixes_match_the_downstream_local_name_reader() {
    let section = section()
        .replace("w:pgSz", "same:pgSz")
        .replace("w:pgMar", "same:pgMar");
    with_source(&package(&format!("{TEXT}{section}")), |path| {
        assert_eq!(
            importing::import_docx(path).unwrap().document.attrs["pageGeometry"],
            geometry(11906, 16838)
        );
    });
}
