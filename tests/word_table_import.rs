use rust_wps::{compatibility, importing};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read, Write},
    path::Path,
};

const NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const RISK: &str = "Unsupported table geometry cannot be preserved in an editable copy";
const AUTOFIT: &str = "Autofit table widths normalized; fixed column widths were not inferred";
const FORMAT: &str = "Table borders and cell formatting normalized";
fn styles(body: &str) -> String {
    format!(r#"<w:styles xmlns:w="{NS}">{body}</w:styles>"#)
}
fn table(properties: &str) -> String {
    format!(
        r#"<w:tbl><w:tblPr>{properties}</w:tblPr><w:tblGrid><w:gridCol w:w="1200"/><w:gridCol w:w="2400"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:type="dxa" w:w="1200"/></w:tcPr><w:p><w:r><w:t>First</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:type="dxa" w:w="2400"/></w:tcPr><w:p><w:r><w:t>Second</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#
    )
}
fn fixed() -> String {
    table(r#"<w:tblLayout w:type="fixed"/><w:tblW w:type="dxa" w:w="3600"/>"#)
}
fn archive(body: &str, styles: &str) -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
        ("[Content_Types].xml".into(), br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#.to_vec()),
        ("_rels/.rels".into(), br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="office" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec()),
        ("word/_rels/document.xml.rels".into(), br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#.to_vec()),
        ("word/document.xml".into(), format!(r#"<w:document xmlns:w="{NS}"><w:body>{body}</w:body></w:document>"#).into_bytes()),
        ("word/styles.xml".into(), styles.as_bytes().to_vec()),
    ])
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
fn refused(body: &str, styles: &str) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.docx");
    write(&path, &archive(body, styles));
    let original = fs::read(&path).unwrap();
    let report = compatibility::audit_docx(&path).unwrap();
    assert!(
        report.risks.contains(RISK),
        "{body}\n{styles}\n{:?}",
        report.risks
    );
    let error = importing::import_docx(&path).unwrap_err().to_string();
    assert!(
        error.contains(RISK)
            || error.contains("Merged table cells")
            || error.contains("Tracked changes"),
        "{error}"
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn fixed_grid_and_cells_preserved_with_dxa_auto_zero_or_missing_total() {
    let dir = tempfile::tempdir().unwrap();
    for (index, total) in [
        r#"<w:tblW w:type="dxa" w:w="3600"/>"#,
        r#"<w:tblW w:type="auto" w:w="0"/>"#,
        "",
    ]
    .into_iter()
    .enumerate()
    {
        let source = dir.path().join(format!("fixed-{index}.docx"));
        write(
            &source,
            &archive(
                &table(&format!(r#"<w:tblLayout w:type="fixed"/>{total}"#)),
                &styles(""),
            ),
        );
        let original = fs::read(&source).unwrap();
        let report = importing::import_docx(&source).unwrap();
        assert_eq!(
            report.document.content[0].attrs["columnWidthsTwips"],
            json!([1200, 2400])
        );
        assert!(report.compatibility.risks.contains(FORMAT));
        assert!(!report.compatibility.risks.contains(AUTOFIT));
        assert!(
            !report
                .compatibility
                .risks
                .iter()
                .any(|r| r.contains("Table widths,"))
        );
        assert_eq!(
            report.document.content[0].content[0].content[0].content[0].content[0]
                .text
                .as_deref(),
            Some("First")
        );
        assert_eq!(fs::read(&source).unwrap(), original);
    }
}

#[test]
fn grid_only_and_explicit_autofit_are_not_fixed() {
    let dir = tempfile::tempdir().unwrap();
    for (index, properties) in [
        "",
        r#"<w:tblW w:type="dxa" w:w="3600"/>"#,
        r#"<w:tblLayout w:type="autofit"/>"#,
    ]
    .into_iter()
    .enumerate()
    {
        let source = dir.path().join(format!("auto-{index}.docx"));
        write(&source, &archive(&table(properties), &styles("")));
        let report = importing::import_docx(&source).unwrap();
        assert!(
            !report.document.content[0]
                .attrs
                .contains_key("columnWidthsTwips")
        );
        assert!(report.compatibility.risks.contains(AUTOFIT));
    }
}

#[test]
fn source_table_sequence_is_preserved_across_paragraphs_and_auto_tables() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("mixed.docx");
    let third = fixed()
        .replace("1200", "500")
        .replace("2400", "2500")
        .replace("3600", "3000");
    let body = format!("{}<w:p/>{}<w:p/>{}", fixed(), table(""), third);
    write(&source, &archive(&body, &styles("")));
    let report = importing::import_docx(&source).unwrap();
    let tables: Vec<_> = report
        .document
        .content
        .iter()
        .filter(|n| n.kind == "table")
        .collect();
    assert_eq!(tables.len(), 3);
    assert_eq!(tables[0].attrs["columnWidthsTwips"], json!([1200, 2400]));
    assert!(!tables[1].attrs.contains_key("columnWidthsTwips"));
    assert_eq!(tables[2].attrs["columnWidthsTwips"], json!([500, 2500]));
}

#[test]
fn fixed_width_declarations_reject_malformed_conflicting_or_missing_geometry() {
    let good = fixed();
    for bad in [
        good.replace(r#"w:w="1200""#, r#"w:w="0""#),
        good.replace(r#"w:w="1200""#, r#"w:w="-1""#),
        good.replace(r#"w:w="1200""#, r#"w:w="1.5""#),
        good.replace(r#"w:w="1200""#, r#"w:w="12pt""#),
        good.replace(r#"w:w="1200""#, r#"w:w=" 1200""#),
        good.replace(r#"w:w="1200""#, r#"w:w="31681""#),
        good.replace(r#"w:w="1200""#, r#"w:w="20000""#)
            .replace(r#"w:w="2400""#, r#"w:w="20000""#),
        good.replace(r#"<w:gridCol w:w="2400"/>"#, ""),
        good.replace(r#"<w:tcW w:type="dxa" w:w="1200"/>"#, ""),
        good.replace(
            r#"<w:tcW w:type="dxa" w:w="1200"/>"#,
            r#"<w:tcW w:type="dxa" w:w="1199"/>"#,
        ),
        good.replace(r#"w:w="3600""#, r#"w:w="3601""#),
        good.replace(r#"w:type="fixed""#, r#"w:type="unknown""#),
        good.replace(r#"<w:tblLayout w:type="fixed"/>"#, "<w:tblLayout/>"),
        good.replace(
            r#"<w:tblLayout w:type="fixed"/>"#,
            r#"<w:tblLayout w:type="fixed"/><w:tblLayout w:type="autofit"/>"#,
        ),
        good.replace(
            r#"<w:tcW w:type="dxa" w:w="1200"/>"#,
            r#"<w:tcW w:type="dxa" w:w="1200"/><w:tcW w:type="dxa" w:w="1200"/>"#,
        ),
        good.replace("</w:tblPr>", "</w:tblPr><w:tblPr/>"),
        good.replace("</w:tblGrid>", "</w:tblGrid><w:tblGrid/>"),
    ] {
        refused(&bad, &styles(""));
    }
}

#[test]
fn percentage_floating_indentation_merges_nesting_and_ragged_tables_refused() {
    let good = fixed();
    for declaration in [
        r#"<w:tblW w:type="pct" w:w="5000"/>"#,
        r#"<w:tblpPr w:leftFromText="100"/>"#,
        r#"<w:tblInd w:type="dxa" w:w="100"/>"#,
        r#"<w:tblCellSpacing w:type="dxa" w:w="20"/>"#,
        "<w:gridSpan w:val=\"2\"/>",
        "<w:vMerge/>",
        "<w:hMerge/>",
        "<w:gridBefore w:val=\"1\"/>",
        "<w:tblPrEx/>",
        "<w:tblPrChange/>",
        "<w:tblGridChange/>",
        "<w:bidiVisual/>",
    ] {
        refused(
            &good.replace("</w:tblPr>", &format!("{declaration}</w:tblPr>")),
            &styles(""),
        );
    }
    refused(
        &good.replace(
            r#"<w:tcW w:type="dxa" w:w="1200"/>"#,
            r#"<w:tcW w:type="pct" w:w="5000"/>"#,
        ),
        &styles(""),
    );
    refused(
        &good.replace("</w:tbl>", "<w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>"),
        &styles(""),
    );
    refused(
        &good.replacen("</w:tc>", &format!("{}</w:tc>", fixed()), 1),
        &styles(""),
    );
}

#[test]
fn inherited_or_conditional_layout_does_not_masquerade_as_direct_fixed() {
    for explicit in ["", r#"<w:tblLayout w:type="fixed"/>"#] {
        for property in [
            "<w:tblPr><w:tblLayout w:type=\"fixed\"/></w:tblPr>",
            "<w:tblStylePr w:type=\"firstRow\"><w:tblPr><w:tblLayout w:type=\"fixed\"/></w:tblPr></w:tblStylePr>",
        ] {
            let sheet = styles(&format!(
                r#"<w:style w:type="table" w:styleId="Base">{property}</w:style><w:style w:type="table" w:styleId="Used"><w:basedOn w:val="Base"/></w:style>"#
            ));
            refused(
                &table(&format!(r#"<w:tblStyle w:val="Used"/>{explicit}"#)),
                &sheet,
            );
        }
    }
}

#[test]
fn used_default_table_style_is_audited_but_unrelated_defaults_do_not_block() {
    let bad = r#"<w:style w:type="table" w:default="1" w:styleId="Default"><w:tblPr><w:tblLayout w:type="fixed"/></w:tblPr></w:style>"#;
    refused(&fixed(), &styles(bad));
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("unused.docx");
    write(
        &source,
        &archive(
            &table(r#"<w:tblStyle w:val="Plain"/><w:tblLayout w:type="fixed"/>"#),
            &styles(&format!(
                r#"{bad}<w:style w:type="table" w:styleId="Plain"/>"#
            )),
        ),
    );
    assert_eq!(
        importing::import_docx(&source).unwrap().document.content[0].attrs["columnWidthsTwips"],
        json!([1200, 2400])
    );
    write(&source, &archive("<w:p/>", &styles(bad)));
    assert!(importing::import_docx(&source).is_ok());
}

#[test]
fn untouched_default_table_style_zero_indent_is_supported() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("plain.docx");
    let sheet = styles(
        r#"<w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:tblPr><w:tblInd w:type="dxa" w:w="0"/><w:tblCellMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style><w:style w:type="table" w:styleId="Unused"><w:tblPr><w:tblLayout w:type="fixed"/><w:tblW w:type="pct" w:w="5000"/></w:tblPr></w:style>"#,
    );
    write(&source, &archive(&fixed(), &sheet));
    assert_eq!(
        importing::import_docx(&source).unwrap().document.content[0].attrs["columnWidthsTwips"],
        json!([1200, 2400])
    );
}

#[test]
fn table_element_and_attribute_namespace_collisions_fail_closed() {
    for layout in [
        r#"<w:tblLayout xmlns:x="urn:foreign" w:type="fixed" x:type="autofit"/>"#,
        r#"<w:tblLayout xmlns:x="http://schemas.openxmlformats.org/wordprocessingml/2006/main" x:type="fixed"/>"#,
        r#"<x:tblLayout xmlns:x="urn:foreign" w:type="fixed"/>"#,
        r#"<x:tblLayout xmlns:x="urn:foreign" w:type="autofit"/><w:tblLayout w:type="fixed"/>"#,
        r#"<w:extension><w:tblLayout w:type="autofit"/></w:extension><w:tblLayout w:type="fixed"/>"#,
    ] {
        refused(&table(layout), &styles(""));
    }
    refused(
        &fixed().replace(
            r#"<w:gridCol w:w="1200"/>"#,
            r#"<w:gridCol xmlns:x="urn:foreign" w:w="1200" x:w="2400"/>"#,
        ),
        &styles(""),
    );
    refused(
        &fixed().replace(
            r#"<w:tcW w:type="dxa" w:w="1200"/>"#,
            r#"<w:tcW xmlns:x="urn:foreign" w:type="dxa" w:w="1200" x:w="1200"/>"#,
        ),
        &styles(""),
    );
}

#[test]
fn strict_namespace_utf16_main_and_relationship_resolved_styles_use_same_parts() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("strict.docx");
    let mut archive = archive(&fixed(), &styles(""));
    let text = String::from_utf8(archive["word/document.xml"].clone())
        .unwrap()
        .replace(NS, "http://purl.oclc.org/ooxml/wordprocessingml/main");
    let mut utf16 = vec![0xff, 0xfe];
    utf16.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    archive.insert("word/document.xml".into(), utf16);
    write(&source, &archive);
    assert_eq!(
        importing::import_docx(&source).unwrap().document.content[0].attrs["columnWidthsTwips"],
        json!([1200, 2400])
    );
    archive.insert("word/actual-styles.xml".into(), styles(r#"<w:style w:type="table" w:default="1" w:styleId="Real"><w:tblPr><w:tblLayout w:type="fixed"/></w:tblPr></w:style>"#).into_bytes());
    archive.insert(
        "word/_rels/document.xml.rels".into(),
        String::from_utf8(archive["word/_rels/document.xml.rels"].clone())
            .unwrap()
            .replace("Target=\"styles.xml\"", "Target=\"actual-styles.xml\"")
            .into_bytes(),
    );
    write(&source, &archive);
    assert!(
        importing::import_docx(&source)
            .unwrap_err()
            .to_string()
            .contains(RISK)
    );
}

#[test]
fn exported_fixed_width_copy_reimports_and_source_is_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.docx");
    let output = dir.path().join("copy.docx");
    write(&source, &archive(&fixed(), &styles("")));
    let original = fs::read(&source).unwrap();
    let report = importing::import_docx(&source).unwrap();
    rust_wps::document::export_docx(&report.document, &output).unwrap();
    let reimported = importing::import_docx(&output).unwrap();
    assert_eq!(
        reimported.document.content[0].attrs["columnWidthsTwips"],
        json!([1200, 2400])
    );
    let mut zip = zip::ZipArchive::new(Cursor::new(fs::read(&output).unwrap())).unwrap();
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains(r#"<w:tblLayout w:type="fixed"/>"#));
    assert_eq!(fs::read(&source).unwrap(), original);
}

#[test]
fn non_direct_width_declarations_cannot_hide_beside_supported_geometry() {
    for property in [
        r#"<w:tblW w:type="pct" w:w="5000"/>"#,
        r#"<w:tblPr><w:tblW w:type="dxa" w:w="4000"/></w:tblPr>"#,
        r#"<w:tblStyle w:val="Missing"/>"#,
        r#"<w:tcW w:type="pct" w:w="5000"/>"#,
        r#"<w:tcPr><w:tcW w:type="dxa" w:w="4000"/></w:tcPr>"#,
        r#"<w:gridCol w:w="100"/>"#,
        r#"<w:trPr/>"#,
    ] {
        for wrapper in [
            format!("<w:unknown>{property}</w:unknown>"),
            format!(
                r#"<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Fallback>{property}</mc:Fallback></mc:AlternateContent>"#
            ),
        ] {
            refused(
                &fixed().replace("</w:tblPr>", &format!("{wrapper}</w:tblPr>")),
                &styles(""),
            );
        }
    }
}

#[test]
fn unresolved_and_ambiguous_style_identity_cannot_supply_fixed_geometry() {
    refused(
        &table(r#"<w:tblStyle w:val="Missing"/><w:tblLayout w:type="fixed"/>"#),
        &styles(""),
    );
    let body = table(r#"<w:tblStyle w:val="Used"/><w:tblLayout w:type="fixed"/>"#);
    for style in [
        r#"<w:style w:type="table" w:styleId="Used"><w:basedOn w:val="Missing"/></w:style>"#,
        r#"<w:style w:type="table" w:styleId="Used"><w:basedOn w:val="Used"/></w:style>"#,
        r#"<w:style xmlns:x="urn:foreign" w:type="table" w:styleId="Used" x:styleId="Other"/>"#,
        r#"<w:style xmlns:x="urn:foreign" w:type="table" w:styleId="Used"><w:basedOn w:val="Base" x:val="Other"/></w:style><w:style w:type="table" w:styleId="Base"/>"#,
    ] {
        refused(&body, &styles(style));
    }
    for default in [
        r#"<w:style xmlns:x="http://schemas.openxmlformats.org/wordprocessingml/2006/main" w:type="table" x:default="1" w:styleId="Default"/>"#,
        r#"<w:style xmlns:x="urn:foreign" w:type="table" w:default="0" x:default="1" w:styleId="Default"/>"#,
    ] {
        refused(&fixed(), &styles(default));
    }
}

#[test]
fn explicit_table_alignment_has_a_normalization_warning() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("aligned.docx");
    write(
        &path,
        &archive(
            &table(r#"<w:tblLayout w:type="fixed"/><w:jc w:val="right"/>"#),
            &styles(""),
        ),
    );
    let report = importing::import_docx(&path).unwrap();
    assert_eq!(
        report.document.content[0].attrs["columnWidthsTwips"],
        json!([1200, 2400])
    );
    assert!(
        report
            .compatibility
            .risks
            .contains("Table alignment normalized to editor defaults")
    );
}
