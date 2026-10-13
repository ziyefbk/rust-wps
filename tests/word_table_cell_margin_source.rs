//! Owned synthetic source packages; no upstream DOCX bytes are redistributed.
use rust_wps::{compatibility, importing};
use serde_json::json;
use std::{collections::BTreeMap, fs, io::Write};
const NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const RISK: &str = "Unsupported table geometry cannot be preserved in an editable copy";
const PARA_RISK: &str = "Unsupported paragraph indentation cannot be preserved in an editable copy";
fn pair(left: u64, right: u64) -> String {
    format!(r#"<w:left w:w="{left}" w:type="dxa"/><w:right w:w="{right}" w:type="dxa"/>"#)
}
fn table(pr: &str, cell_margins: [&str; 2], paragraph_indent: bool) -> String {
    let cells: String = cell_margins.iter().enumerate().map(|(i, margin)| format!(
        r#"<w:tc><w:tcPr><w:tcW w:type="dxa" w:w="2400"/>{margin}</w:tcPr><w:p><w:pPr>{}</w:pPr><w:r><w:t>Cell{i}</w:t></w:r></w:p></w:tc>"#,
        if paragraph_indent { r#"<w:ind w:left="120"/>"# } else { "" })).collect();
    format!(
        r#"<w:tbl><w:tblPr><w:tblLayout w:type="fixed"/>{pr}</w:tblPr><w:tblGrid><w:gridCol w:w="2400"/><w:gridCol w:w="2400"/></w:tblGrid><w:tr>{cells}</w:tr></w:tbl>"#
    )
}
fn package(body: &str, styles: &str, mode: Option<u8>) -> BTreeMap<String, Vec<u8>> {
    let mut parts = BTreeMap::from([
        ("[Content_Types].xml".into(), br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/meta/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#.to_vec()),
        ("_rels/.rels".into(), br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="main" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec()),
        ("word/_rels/document.xml.rels".into(), br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="meta/styles.xml"/></Relationships>"#.to_vec()),
        ("word/document.xml".into(), format!(r#"<w:document xmlns:w="{NS}"><w:body>{body}</w:body></w:document>"#).into_bytes()),
        ("word/meta/styles.xml".into(), format!(r#"<w:styles xmlns:w="{NS}">{styles}</w:styles>"#).into_bytes()),
    ]);
    if let Some(mode) = mode {
        parts.insert("word/meta/settings.xml".into(), format!(r#"<w:settings xmlns:w="{NS}"><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{mode}"/></w:compat></w:settings>"#).into_bytes());
        let rels = String::from_utf8(parts["word/_rels/document.xml.rels"].clone()).unwrap().replace("</Relationships>", r#"<Relationship Id="settings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="meta/settings.xml"/></Relationships>"#);
        parts.insert("word/_rels/document.xml.rels".into(), rels.into_bytes());
    }
    parts
}
fn imported_with_audit(
    parts: &BTreeMap<String, Vec<u8>>,
    risk: Option<&str>,
    expected_warning: Option<&str>,
) -> Option<serde_json::Value> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.docx");
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    for (name, bytes) in parts {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    let before = fs::read(&path).unwrap();
    let audit = compatibility::audit_docx(&path).unwrap();
    if let Some(warning) = expected_warning {
        assert!(audit.risks.contains(warning), "{:?}", audit.risks);
    }
    let report = importing::import_docx(&path);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    if let Some(risk) = risk {
        assert!(audit.risks.contains(risk), "{:?}", audit.risks);
        assert!(report.unwrap_err().to_string().contains(risk));
        None
    } else {
        Some(serde_json::to_value(report.unwrap().document).unwrap())
    }
}
fn imported(parts: &BTreeMap<String, Vec<u8>>, risk: Option<&str>) -> Option<serde_json::Value> {
    imported_with_audit(parts, risk, None)
}

fn strict(pr: &str, margins: [&str; 2]) -> String {
    table(
        &format!(r#"<w:tblInd w:type="dxa" w:w="0"/>{pr}"#),
        margins,
        false,
    )
}
#[test]
fn paired_custom_physical_margins_preserve_zero_and_asymmetry() {
    for (left, right) in [(0, 0), (0, 108), (180, 240), (240, 180), (108, 108)] {
        for position in [
            "",
            r#"<w:tblInd w:type="dxa" w:w="0"/>"#,
            r#"<w:tblInd w:type="dxa" w:w="720"/>"#,
        ] {
            let body = table(
                &format!(
                    "{position}<w:tblCellMar>{}</w:tblCellMar>",
                    pair(left, right)
                ),
                ["", ""],
                false,
            );
            let doc = imported(&package(&body, "", Some(14)), None).unwrap();
            let actual = &doc["content"][0]["attrs"]["tableCellMarginsTwips"];
            if (left, right) == (108, 108) {
                assert!(actual.is_null());
            } else {
                assert_eq!(*actual, json!({"left":left,"right":right}));
            }
        }
    }
}
#[test]
fn uniform_cells_override_table_and_combine_with_paragraph_indentation() {
    let cell = format!("<w:tcMar>{}</w:tcMar>", pair(120, 180));
    let body = table(
        &format!("<w:tblCellMar>{}</w:tblCellMar>", pair(360, 240)),
        [&cell, &cell],
        true,
    );
    let doc = imported(&package(&body, "", None), None).unwrap();
    assert_eq!(
        doc["content"][0]["attrs"]["tableCellMarginsTwips"],
        json!({"left":120,"right":180})
    );
    for cell in doc["content"][0]["content"][0]["content"]
        .as_array()
        .unwrap()
    {
        assert_eq!(cell["content"][0]["attrs"]["indentLeftTwips"], 120);
    }
}
#[test]
fn selected_style_chain_proves_both_sides_but_implicit_default_does_not() {
    let styles = r#"<w:style w:type="table" w:default="1" w:styleId="Base"><w:tblPr><w:tblInd w:type="dxa" w:w="0"/><w:tblCellMar><w:left w:type="dxa" w:w="180"/></w:tblCellMar></w:tblPr></w:style><w:style w:type="table" w:styleId="Used"><w:basedOn w:val="Base"/><w:tblPr><w:tblCellMar><w:right w:type="dxa" w:w="240"/></w:tblCellMar></w:tblPr></w:style>"#;
    let selected = table(r#"<w:tblStyle w:val="Used"/>"#, ["", ""], false);
    let doc = imported(&package(&selected, styles, Some(14)), None).unwrap();
    assert_eq!(
        doc["content"][0]["attrs"]["tableCellMarginsTwips"],
        json!({"left":180,"right":240})
    );
    assert_eq!(doc["content"][0]["attrs"]["tableIndentTwips"], 0);
    imported(
        &package(&strict("", ["", ""]), styles, Some(14)),
        Some(RISK),
    );
    let doc = imported(
        &package(&table("", ["", ""], false), styles, Some(14)),
        None,
    )
    .unwrap();
    assert!(doc["content"][0]["attrs"]["tableCellMarginsTwips"].is_null());
}
#[test]
fn incomplete_or_nonuniform_margins_refuse_preservation_but_keep_plain_normalization() {
    for (pr, cells) in [
        (r#"<w:tblCellMar><w:left w:type="dxa" w:w="180"/></w:tblCellMar>"#.to_owned(), [String::new(), String::new()]),
        (format!("<w:tblCellMar>{}</w:tblCellMar>", pair(180, 240)), [format!("<w:tcMar>{}</w:tcMar>", pair(0, 0)), String::new()]),
        (r#"<w:tblCellMar><w:start w:type="dxa" w:w="180"/><w:end w:type="dxa" w:w="240"/></w:tblCellMar>"#.to_owned(), [String::new(), String::new()]),
    ] {
        imported(&package(&strict(&pr, [&cells[0], &cells[1]]), "", None), Some(RISK));
        imported(&package(&table(&pr, [&cells[0], &cells[1]], true), "", None), Some(PARA_RISK));
        let doc = imported(&package(&table(&pr, [&cells[0], &cells[1]], false), "", None), None).unwrap();
        assert!(doc["content"][0]["attrs"]["tableCellMarginsTwips"].is_null());
    }
}
#[test]
fn malformed_margin_values_aliases_and_locations_cannot_prove_indentation() {
    for side in [
        r#"<w:left w:type="pct" w:w="180"/>"#,
        r#"<w:left w:type="dxa" w:w="-1"/>"#,
        r#"<w:left w:type="dxa" w:w="31681"/>"#,
        r#"<w:left w:type="dxa" w:w="1.0"/>"#,
        r#"<w:left w:type="dxa" w:w="+1"/>"#,
        r#"<w:left w:type="dxa"/>"#,
        r#"<w:left w:w="180"/>"#,
        r#"<w:left w:type="dxa" w:w="180" w:other="1"/>"#,
        r#"<w:left w:type="dxa" w:w="180">hidden</w:left>"#,
        r#"<w:left w:type="dxa" w:w="180"><w:right w:type="dxa" w:w="1"/></w:left>"#,
        r#"<w:left w:type="dxa" w:w="180"/><w:left w:type="dxa" w:w="180"/>"#,
        r#"<w:left w:type="dxa" w:w="180"/><w:start w:type="dxa" w:w="108"/>"#,
        r#"<x:left xmlns:x="http://schemas.openxmlformats.org/wordprocessingml/2006/main" w:type="dxa" w:w="180"/>"#,
        r#"<w:left xmlns:x="urn:foreign" w:type="dxa" w:w="180" x:w="180"/>"#,
        r#"<w:left xmlns:x="http://schemas.openxmlformats.org/wordprocessingml/2006/main" w:type="dxa" x:w="180"/>"#,
        r#"<w:wrap><w:left w:type="dxa" w:w="180"/></w:wrap>"#,
    ] {
        let margin =
            format!(r#"<w:tblCellMar>{side}<w:right w:type="dxa" w:w="240"/></w:tblCellMar>"#);
        imported(&package(&strict(&margin, ["", ""]), "", None), Some(RISK));
    }
    let margin = format!("<w:tblCellMar>{}</w:tblCellMar>", pair(180, 240));
    for misplaced in [
        format!("<w:wrap>{margin}</w:wrap>"),
        format!("{margin}{margin}"),
    ] {
        imported(
            &package(&strict(&misplaced, ["", ""]), "", None),
            Some(RISK),
        );
    }
}
#[test]
fn custom_conditional_and_ambiguous_references_never_become_a_margin_pair() {
    let selected = strict(r#"<w:tblStyle w:val="Used"/>"#, ["", ""]);
    for style in [
        format!(r#"<w:style w:type="table" w:styleId="Used"><w:tblStylePr w:type="firstRow"><w:tcPr><w:tcMar>{}</w:tcMar></w:tcPr></w:tblStylePr></w:style>"#, pair(180,240)),
        r#"<w:style w:type="table" w:styleId="Used"><w:basedOn w:val="Missing"/></w:style>"#.to_owned(),
        r#"<w:style w:type="table" w:styleId="Used"><w:basedOn w:val="Used"/></w:style>"#.to_owned(),
        r#"<w:style w:type="table" w:styleId="Used"/><w:style xmlns:x="urn:foreign" w:type="table" x:styleId="Used"/>"#.to_owned(),
        r#"<w:wrap><w:style w:type="table" w:styleId="Used"/></w:wrap>"#.to_owned(),
        r#"<w:style w:type="table" w:styleId="Used"><w:basedOn w:val="Base" w:extra="1"/></w:style><w:style w:type="table" w:styleId="Base"/>"#.to_owned(),
    ] { imported(&package(&selected, &style, None), Some(RISK)); }
}
#[test]
fn modern_indentation_remains_refused_while_margins_alone_are_independent() {
    let margins = format!("<w:tblCellMar>{}</w:tblCellMar>", pair(180, 240));
    imported(
        &package(&strict(&margins, ["", ""]), "", Some(15)),
        Some(RISK),
    );
    let doc = imported_with_audit(
        &package(&table(&margins, ["", ""], false), "", Some(15)),
        None,
        Some("Horizontal cell margins preserved; source compatibility-dependent table position normalized"),
    )
    .unwrap();
    assert_eq!(
        doc["content"][0]["attrs"]["tableCellMarginsTwips"],
        json!({"left":180,"right":240})
    );
    assert!(doc["content"][0]["attrs"]["tableIndentTwips"].is_null());
}
#[test]
fn baseline_logical_conditional_and_vertical_normalization_stay_compatible() {
    let style = r#"<w:style w:type="table" w:styleId="Used"><w:tblStylePr w:type="firstRow"><w:tcPr><w:tcMar><w:start w:type="dxa" w:w="108"/><w:end w:type="dxa" w:w="108"/></w:tcMar></w:tcPr></w:tblStylePr></w:style>"#;
    let pr = r#"<w:tblStyle w:val="Used"/><w:tblCellMar><w:top w:type="pct" w:w="50"/><w:bottom w:unknown="1"/><w:left w:type="dxa" w:w="108"/><w:right w:type="dxa" w:w="108"/></w:tblCellMar>"#;
    let doc = imported(&package(&strict(pr, ["", ""]), style, None), None).unwrap();
    assert!(doc["content"][0]["attrs"]["tableCellMarginsTwips"].is_null());
    assert_eq!(doc["content"][0]["attrs"]["tableIndentTwips"], 0);
}

#[test]
fn physical_components_cascade_independently_through_style_table_and_cells() {
    let styles = r#"<w:style w:type="table" w:styleId="Base"><w:tblPr><w:tblCellMar><w:left w:type="dxa" w:w="180"/><w:right w:type="dxa" w:w="240"/></w:tblCellMar></w:tblPr></w:style><w:style w:type="table" w:styleId="Used"><w:basedOn w:val="Base"/><w:tblPr><w:tblCellMar><w:left w:type="dxa" w:w="200"/></w:tblCellMar></w:tblPr></w:style>"#;
    let pr = r#"<w:tblStyle w:val="Used"/><w:tblCellMar><w:right w:type="dxa" w:w="300"/></w:tblCellMar>"#;
    let cell = r#"<w:tcMar><w:left w:type="dxa" w:w="0"/></w:tcMar>"#;
    let body = strict(pr, [cell, cell]);
    let doc = imported(&package(&body, styles, None), None).unwrap();
    assert_eq!(
        doc["content"][0]["attrs"]["tableCellMarginsTwips"],
        json!({"left":0,"right":300})
    );
}

#[test]
fn mixed_source_tables_consume_their_own_margin_and_paragraph_proofs_in_order() {
    let proved = table(
        &format!("<w:tblCellMar>{}</w:tblCellMar>", pair(180, 240)),
        ["", ""],
        true,
    );
    let partial = table(
        r#"<w:tblCellMar><w:left w:type="dxa" w:w="300"/></w:tblCellMar>"#,
        ["", ""],
        false,
    );
    let baseline = table(
        r#"<w:tblCellMar><w:start w:type="dxa" w:w="108"/><w:end w:type="dxa" w:w="108"/></w:tblCellMar>"#,
        ["", ""],
        true,
    );
    let body = format!("{proved}<w:p/>{partial}<w:p/>{baseline}");
    let doc = imported(&package(&body, "", None), None).unwrap();
    let tables: Vec<_> = doc["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|node| node["type"] == "table")
        .collect();
    assert_eq!(tables.len(), 3);
    assert_eq!(
        tables[0]["attrs"]["tableCellMarginsTwips"],
        json!({"left":180,"right":240})
    );
    assert!(tables[1]["attrs"]["tableCellMarginsTwips"].is_null());
    assert!(tables[2]["attrs"]["tableCellMarginsTwips"].is_null());
    for index in [0, 2] {
        assert_eq!(
            tables[index]["content"][0]["content"][0]["content"][0]["attrs"]["indentLeftTwips"],
            120
        );
    }
}

#[test]
fn conditional_custom_margins_without_strict_dependency_keep_warned_normalization() {
    let style = format!(
        r#"<w:style w:type="table" w:styleId="Used"><w:tblStylePr w:type="firstRow"><w:tcPr><w:tcMar>{}</w:tcMar></w:tcPr></w:tblStylePr></w:style>"#,
        pair(180, 240)
    );
    let selected = r#"<w:tblStyle w:val="Used"/>"#;
    imported(
        &package(&strict(selected, ["", ""]), &style, None),
        Some(RISK),
    );
    imported(
        &package(&table(selected, ["", ""], true), &style, None),
        Some(PARA_RISK),
    );
    let doc = imported(
        &package(&table(selected, ["", ""], false), &style, None),
        None,
    )
    .unwrap();
    assert!(doc["content"][0]["attrs"]["tableCellMarginsTwips"].is_null());
}
