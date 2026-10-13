//! Source indentation is independently audited before lossy DOCX conversion.
use rust_wps::{compatibility, importing};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::Path,
};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const P: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const BLOCK: &str = "Unsupported paragraph indentation cannot be preserved in an editable copy";
type Parts = BTreeMap<String, Vec<u8>>;
fn para(props: &str, text: &str) -> String {
    format!("<w:p><w:pPr>{props}</w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>")
}
fn package(body: &str) -> Parts {
    BTreeMap::from([
        ("[Content_Types].xml".into(), br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_vec()),
        ("_rels/.rels".into(),format!(r#"<Relationships xmlns="{P}"><Relationship Id="main" Type="{R}/officeDocument" Target="word/document.xml"/></Relationships>"#).into_bytes()),
        ("word/document.xml".into(), format!(r#"<w:document xmlns:w="{W}" xmlns:x="urn:foreign" xmlns:same="{W}"><w:body>{body}</w:body></w:document>"#).into_bytes()),
    ])
}
fn add_part(parts: &mut Parts, kind: &str, path: &str, content: &str) {
    let key = "word/_rels/document.xml.rels";
    let existing = parts
        .get(key)
        .map(|b| String::from_utf8(b.clone()).unwrap())
        .unwrap_or_else(|| format!(r#"<Relationships xmlns="{P}"></Relationships>"#));
    parts.insert(key.into(), existing.replace("</Relationships>", &format!(r#"<Relationship Id="{kind}" Type="{R}/{kind}" Target="../{path}"/></Relationships>"#)).into_bytes());
    parts.insert(
        path.into(),
        format!(
            r#"<w:{kind} xmlns:w="{W}" xmlns:x="urn:foreign" xmlns:same="{W}">{content}</w:{kind}>"#
        )
        .into_bytes(),
    );
}
fn styles(parts: &mut Parts, content: &str) {
    add_part(parts, "styles", "custom/actual-styles.xml", content);
}
fn settings(parts: &mut Parts, content: &str) {
    add_part(parts, "settings", "custom/actual-settings.xml", content);
}
fn numbers(parts: &mut Parts, content: &str) {
    add_part(parts, "numbering", "custom/actual-numbering.xml", content);
}
fn with_source<T>(parts: &Parts, f: impl FnOnce(&Path) -> T) -> T {
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
    let result = f(&path);
    assert_eq!(before, fs::read(&path).unwrap());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    result
}
fn imported(parts: &Parts) -> Value {
    with_source(parts, |path| {
        let report = importing::import_docx(path).unwrap();
        assert!(!report.compatibility.risks.contains(BLOCK));
        serde_json::to_value(report.document).unwrap()
    })
}
fn refused(parts: &Parts) {
    with_source(parts, |path| {
        // XML/OPC ambiguity may already be rejected by the common reader.
        if let Ok(report) = compatibility::audit_docx(path) {
            assert!(
                report.risks.contains(BLOCK),
                "Expected indentation risk, got {:?}",
                report.risks
            );
        }
        assert!(importing::import_docx(path).is_err());
    });
}
fn attrs(doc: &Value) -> &Value {
    &doc["content"][0]["attrs"]
}
fn style(id: &str, props: &str, base: Option<&str>) -> String {
    format!(
        r#"<w:style w:type="paragraph" w:styleId="{id}">{}<w:pPr>{props}</w:pPr></w:style>"#,
        base.map(|b| format!(r#"<w:basedOn w:val="{b}"/>"#))
            .unwrap_or_default()
    )
}
fn list_props() -> &'static str {
    r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr>"#
}
fn numbering_definition(ind: &str) -> String {
    format!(
        r#"<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:pPr>{ind}</w:pPr></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>"#
    )
}
fn table(properties: &str, cell: &str, cell_props: &str) -> String {
    format!(
        r#"<w:tbl><w:tblPr>{properties}<w:tblW w:type="auto" w:w="0"/></w:tblPr><w:tblGrid><w:gridCol w:w="9000"/></w:tblGrid><w:tr><w:tc><w:tcPr>{cell_props}<w:tcW w:type="auto" w:w="0"/></w:tcPr>{cell}</w:tc></w:tr></w:tbl>"#
    )
}

#[test]
fn physical_body_indents_and_explicit_zero_preserve_exact_components() {
    for (declaration, expected) in [
        (
            r#"<w:ind w:left="720" w:right="360" w:firstLine="240"/>"#,
            json!([720, 360, 240]),
        ),
        (
            r#"<w:ind w:left="720" w:right="0" w:hanging="360"/>"#,
            json!([720, 0, -360]),
        ),
        (
            r#"<w:ind w:left="720" w:hanging="180"/>"#,
            json!([720, null, -180]),
        ),
        (
            r#"<w:ind w:left="0" w:right="0" w:firstLine="0"/>"#,
            json!([0, 0, 0]),
        ),
        (r#"<w:ind w:hanging="0"/>"#, json!([null, null, 0])),
    ] {
        let doc = imported(&package(&para(declaration, "IndentProbe")));
        let a = attrs(&doc);
        assert_eq!(
            json!([
                a["indentLeftTwips"],
                a["indentRightTwips"],
                a["firstLineIndentTwips"]
            ]),
            expected
        );
    }
}
#[test]
fn fieldwise_defaults_style_chain_and_direct_firstline_slot_are_resolved() {
    let defaults = r#"<w:docDefaults><w:pPrDefault><w:pPr><w:ind w:left="720" w:right="360" w:hanging="180"/></w:pPr></w:pPrDefault></w:docDefaults>"#;
    for (base, child, direct, expected) in [
        (
            r#"<w:ind w:firstLine="240"/>"#,
            r#"<w:ind w:right="0"/>"#,
            r#"<w:ind w:left="900"/>"#,
            json!([900, 0, 240]),
        ),
        (
            r#"<w:ind w:firstLine="240"/>"#,
            "",
            r#"<w:ind w:hanging="0"/>"#,
            json!([720, 360, 0]),
        ),
        (
            r#"<w:ind w:hanging="240"/>"#,
            "",
            r#"<w:ind w:firstLine="0"/>"#,
            json!([720, 360, 0]),
        ),
        (
            r#"<w:ind w:hanging="240"/>"#,
            "",
            r#"<w:ind w:firstLine="120"/>"#,
            json!([720, 360, 120]),
        ),
        (
            r#"<w:ind w:firstLine="240"/>"#,
            "",
            r#"<w:ind w:hanging="120"/>"#,
            json!([720, 360, -120]),
        ),
    ] {
        let mut parts = package(&para(
            &format!(r#"<w:pStyle w:val="Child"/>{direct}"#),
            "Cascade",
        ));
        styles(
            &mut parts,
            &format!(
                "{defaults}{}{}",
                style("Base", base, None),
                style("Child", child, Some("Base"))
            ),
        );
        let doc = imported(&parts);
        let a = attrs(&doc);
        assert_eq!(
            json!([
                a["indentLeftTwips"],
                a["indentRightTwips"],
                a["firstLineIndentTwips"]
            ]),
            expected
        );
    }
}
#[test]
fn empty_paragraph_heading_and_character_entities_keep_target_identity() {
    let body = format!(
        r#"<w:p><w:pPr><w:ind w:left="0"/></w:pPr></w:p>{}"#,
        para(
            r#"<w:pStyle w:val="Heading1"/><w:ind w:left="360"/>"#,
            "A &amp; B &#x4e2d; &#25991;"
        )
    );
    let mut parts = package(&body);
    styles(&mut parts, &style("Heading1", "", None));
    let doc = imported(&parts);
    assert_eq!(doc["content"][0]["attrs"]["indentLeftTwips"], 0);
    assert_eq!(doc["content"][1]["type"], "heading");
    assert_eq!(doc["content"][1]["attrs"]["indentLeftTwips"], 360);
    assert_eq!(doc["content"][1]["content"][0]["text"], "A & B 中 文");
}
#[test]
fn ordinary_unindented_break_paragraphs_do_not_break_target_mapping() {
    let body = format!(
        r#"<w:p><w:r><w:br w:type="page"/><w:t>AfterBreak</w:t></w:r></w:p>{}<w:p><w:r><w:t>Before</w:t><w:br w:type="column"/><w:t>After</w:t></w:r></w:p>{}"#,
        para(r#"<w:ind w:left="360"/>"#, "FirstTarget"),
        para(r#"<w:ind w:right="120"/>"#, "SecondTarget")
    );
    let doc = imported(&package(&body));
    assert_eq!(
        doc["content"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["attrs"].get("indentLeftTwips").is_some()
                || p["attrs"].get("indentRightTwips").is_some())
            .count(),
        2
    );
}
#[test]
fn line_breaks_and_empty_tab_declarations_are_supported_but_tabs_are_not() {
    let body = r#"<w:p><w:pPr><w:ind w:left="360"/><w:tabs/></w:pPr><w:r><w:t>A</w:t><w:br/><w:t>B</w:t></w:r></w:p>"#;
    let doc = imported(&package(body));
    assert_eq!(attrs(&doc)["indentLeftTwips"], 360);
    refused(&package(&body.replace("<w:br/>", "<w:tab/>")));
    refused(&package(&body.replace(
        "<w:tabs/>",
        r#"<w:tabs><w:tab w:val="right" w:pos="1440"/></w:tabs>"#,
    )));
    let mut parts = package(&para(
        r#"<w:pStyle w:val="Tabs"/><w:ind w:left="120"/>"#,
        "Tabs",
    ));
    styles(
        &mut parts,
        &style(
            "Tabs",
            r#"<w:tabs><w:tab w:val="right" w:pos="1440"/></w:tabs>"#,
            None,
        ),
    );
    refused(&parts);
}

#[test]
fn actual_opc_style_and_settings_targets_override_unused_conventional_files() {
    let mut parts = package(&para(r#"<w:pStyle w:val="Used"/>"#, "ActualTarget"));
    styles(&mut parts, &style("Used", r#"<w:ind w:left="480"/>"#, None));
    settings(&mut parts, r#"<w:mirrorMargins w:val="0"/>"#);
    parts.insert(
        "word/styles.xml".into(),
        format!(
            r#"<w:styles xmlns:w="{W}">{}</w:styles>"#,
            style("Used", r#"<w:ind w:start="999"/>"#, None)
        )
        .into_bytes(),
    );
    parts.insert(
        "word/settings.xml".into(),
        format!(r#"<w:settings xmlns:w="{W}"><w:mirrorMargins/></w:settings>"#).into_bytes(),
    );
    assert_eq!(attrs(&imported(&parts))["indentLeftTwips"], 480);
}
#[test]
fn strict_and_utf16_source_parts_are_supported() {
    for utf16 in [false, true] {
        let mut parts = package(&para(r#"<w:ind w:left="360" w:hanging="120"/>"#, "Strict"));
        for bytes in parts.values_mut() {
            let text = String::from_utf8(bytes.clone())
                .unwrap()
                .replace(W, "http://purl.oclc.org/ooxml/wordprocessingml/main")
                .replace(R, "http://purl.oclc.org/ooxml/officeDocument/relationships");
            *bytes = text.into_bytes();
        }
        if utf16 {
            let text = String::from_utf8(parts["word/document.xml"].clone()).unwrap();
            let mut data = vec![0xff, 0xfe];
            for unit in format!(r#"<?xml version="1.0" encoding="UTF-16"?>{text}"#).encode_utf16() {
                data.extend(unit.to_le_bytes());
            }
            parts.insert("word/document.xml".into(), data);
        }
        assert_eq!(attrs(&imported(&parts))["firstLineIndentTwips"], -120);
    }
}
#[test]
fn unused_template_styles_default_tables_and_numbering_levels_are_ignored() {
    let mut parts = package(&para(r#"<w:ind w:left="120"/>"#, "Body"));
    styles(
        &mut parts,
        &format!(
            r#"{}<w:style w:type="table" w:default="1" w:styleId="UnusedTable"><w:pPr><w:ind w:start="500"/></w:pPr><w:tblStylePr w:type="firstRow"><w:pPr><w:ind w:leftChars="100"/></w:pPr></w:tblStylePr></w:style>"#,
            style("Unused", r#"<w:ind w:leftChars="100"/><w:bidi/>"#, None)
        ),
    );
    numbers(
        &mut parts,
        &numbering_definition(r#"<w:ind w:start="600"/>"#),
    );
    assert_eq!(attrs(&imported(&parts))["indentLeftTwips"], 120);
}
#[test]
fn ordinary_list_level_indents_keep_existing_normalization_route() {
    for ind in [
        r#"<w:ind w:left="360" w:hanging="360"/>"#,
        r#"<w:ind w:left="720" w:hanging="360"/>"#,
        r#"<w:ind w:left="1440" w:right="120" w:hanging="480"/>"#,
    ] {
        let mut parts = package(&para(list_props(), "ListItem"));
        numbers(&mut parts, &numbering_definition(ind));
        with_source(&parts, |path| {
            assert!(
                importing::import_docx(path)
                    .unwrap()
                    .compatibility
                    .risks
                    .contains("List numbering indentation normalized to editor defaults")
            );
        });
        let doc = imported(&parts);
        assert_eq!(doc["content"][0]["type"], "orderedList");
        assert!(
            doc["content"][0]["content"][0]["content"][0]["attrs"]
                .get("indentLeftTwips")
                .is_none()
        );
    }
}
#[test]
fn active_numbering_unsupported_forms_and_custom_paragraph_list_indents_refuse() {
    for ind in [
        r#"<w:ind w:start="720"/>"#,
        r#"<w:ind w:leftChars="100"/>"#,
        r#"<w:ind w:left="-1"/>"#,
        r#"<w:ind w:left="720"/><w:bidi/>"#,
    ] {
        let mut parts = package(&para(list_props(), "List"));
        numbers(&mut parts, &numbering_definition(ind));
        refused(&parts);
    }
    let mut parts = package(&para(
        &format!(r#"{}<w:ind w:left="900"/>"#, list_props()),
        "CustomList",
    ));
    numbers(
        &mut parts,
        &numbering_definition(r#"<w:ind w:left="720" w:hanging="360"/>"#),
    );
    refused(&parts);
}
#[test]
fn unused_levels_in_active_numbering_definition_do_not_block() {
    let mut parts = package(&para(list_props(), "List"));
    let defs = numbering_definition(r#"<w:ind w:left="720" w:hanging="360"/>"#).replace(
        "</w:abstractNum>",
        r#"<w:lvl w:ilvl="4"><w:pPr><w:ind w:leftChars="100"/></w:pPr></w:lvl></w:abstractNum>"#,
    );
    numbers(&mut parts, &defs);
    assert_eq!(imported(&parts)["content"][0]["type"], "orderedList");
}
#[test]
fn active_style_chain_rejects_unsupported_indents_but_unused_default_does_not() {
    let mut parts = package(&para(r#"<w:pStyle w:val="Child"/>"#, "Style"));
    styles(
        &mut parts,
        &format!(
            "{}{}",
            style("Base", r#"<w:ind w:start="360"/>"#, None),
            style("Child", "", Some("Base"))
        ),
    );
    refused(&parts);
    let mut parts = package(&para(
        r#"<w:pStyle w:val="Safe"/><w:ind w:left="120"/>"#,
        "Safe",
    ));
    styles(
        &mut parts,
        &format!(
            r#"<w:style w:type="paragraph" w:default="1" w:styleId="Default"><w:pPr><w:ind w:start="360"/></w:pPr></w:style>{}"#,
            style("Safe", "", None)
        ),
    );
    assert_eq!(attrs(&imported(&parts))["indentLeftTwips"], 120);
}
#[test]
fn unsupported_units_dual_firstline_logical_and_character_indents_refuse() {
    for declaration in [
        r#"w:left="-1""#,
        r#"w:right="-1""#,
        r#"w:left="31681""#,
        r#"w:firstLine="31681""#,
        r#"w:firstLine="-31681""#,
        r#"w:firstLine="-1""#,
        r#"w:hanging="-1""#,
        r#"w:left="1.5""#,
        r#"w:left="1pt""#,
        r#"w:left=" 720""#,
        r#"w:left="+720""#,
        r#"w:start="0""#,
        r#"w:end="0""#,
        r#"w:leftChars="0""#,
        r#"w:rightChars="0""#,
        r#"w:firstLineChars="0""#,
        r#"w:hangingChars="0""#,
        r#"w:firstLine="0" w:hanging="0""#,
        r#"w:firstLine="120" w:hanging="240""#,
        r#"w:unknown="0""#,
    ] {
        refused(&package(&para(&format!("<w:ind {declaration}/>"), "Bad")));
    }
}
#[test]
fn malformed_duplicate_foreign_and_wrapped_indent_declarations_refuse() {
    for props in [
        r#"<w:ind w:left="120"/><w:ind w:right="120"/>"#,
        r#"<w:ind w:left="120" w:left="240"/>"#,
        r#"<w:ind w:left="120" x:left="240"/>"#,
        r#"<w:ind x:left="120"/>"#,
        r#"<w:ind same:left="120"/>"#,
        r#"<x:ind w:left="120"/>"#,
        r#"<same:ind w:left="120"/>"#,
        r#"<w:wrap><w:ind w:left="120"/></w:wrap>"#,
        r#"<w:ind w:left="120"><w:other/></w:ind>"#,
        r#"<w:ind w:left="120">text</w:ind>"#,
        r#"<w:pPrChange><w:pPr><w:ind w:left="120"/></w:pPr></w:pPrChange>"#,
    ] {
        refused(&package(&para(props, "Bad")));
    }
}
#[test]
fn rtl_mirror_and_adjusted_indentation_are_refused_but_explicit_off_is_allowed() {
    for setting in ["bidi", "mirrorIndents", "adjustRightInd"] {
        refused(&package(&para(
            &format!(r#"<w:ind w:left="120"/><w:{setting}/>"#),
            "Bad",
        )));
        assert_eq!(
            attrs(&imported(&package(&para(
                &format!(r#"<w:ind w:left="120"/><w:{setting} w:val="false"/>"#),
                "Good"
            ))))["indentLeftTwips"],
            120
        );
    }
    let mut parts = package(&para(r#"<w:ind w:left="120"/>"#, "Mirror"));
    settings(&mut parts, "<w:mirrorMargins/>");
    refused(&parts);
}
#[test]
fn active_style_identifier_default_and_basedon_ambiguities_refuse() {
    for content in [
        format!(
            "{}{}",
            style("Used", r#"<w:ind w:left="120"/>"#, None),
            style("Used", "", None)
        ),
        style("Used", r#"<w:ind w:left="120"/>"#, Some("Used")),
        style("Used", r#"<w:ind w:left="120"/>"#, Some("Base")).replace(
            r#"<w:basedOn w:val="Base"/>"#,
            r#"<w:wrap><w:basedOn w:val="Base"/></w:wrap>"#,
        ),
        style("Used", r#"<w:ind w:left="120"/>"#, Some("Base"))
            .replace(r#"w:val="Base""#, r#"same:val="Base""#),
        style("Used", r#"<w:ind w:left="120"/>"#, None).replace(
            r#"w:styleId="Used""#,
            r#"w:styleId="Used" x:styleId="Fake""#,
        ),
    ] {
        let mut parts = package(&para(r#"<w:pStyle w:val="Used"/>"#, "Bad"));
        styles(&mut parts, &content);
        refused(&parts);
    }
    let mut parts = package(&para("", "Defaults"));
    styles(
        &mut parts,
        r#"<w:style w:type="paragraph" w:default="1" w:styleId="A"><w:pPr><w:ind w:left="120"/></w:pPr></w:style><w:style w:type="paragraph" w:default="1" w:styleId="B"/>"#,
    );
    refused(&parts);
}
#[test]
fn numbering_leading_zero_collisions_wrapped_ids_and_wrapped_levels_refuse() {
    for defs in [
        numbering_definition(r#"<w:ind w:start="120"/>"#).replace(r#"w:numId="1""#,r#"w:numId="01""#),
        numbering_definition(r#"<w:ind w:left="720"/>"#).replace("</w:num>",r#"</w:num><w:num w:numId="01"><w:abstractNumId w:val="0"/></w:num>"#),
        numbering_definition(r#"<w:ind w:left="720"/>"#).replace(r#"<w:abstractNumId w:val="0"/>"#,r#"<w:wrap><w:abstractNumId w:val="0"/></w:wrap>"#),
        numbering_definition(r#"<w:ind w:left="720"/>"#).replace("<w:lvl ","<w:wrap><w:lvl ").replace("</w:lvl>","</w:lvl></w:wrap>"),
        numbering_definition(r#"<w:ind w:left="720"/>"#).replace("</w:num>",r#"<w:wrap><w:lvlOverride w:ilvl="0"><w:lvl w:ilvl="0"><w:pPr><w:ind w:start="120"/></w:pPr></w:lvl></w:lvlOverride></w:wrap></w:num>"#),
    ] { let mut parts = package(&para(list_props(),"List"));numbers(&mut parts,&defs);refused(&parts); }
}
#[test]
fn direct_and_unconditional_table_cell_indents_are_supported() {
    let cell = para(
        r#"<w:ind w:left="120" w:right="60" w:firstLine="30"/>"#,
        "Cell",
    );
    let doc = imported(&package(&table("", &cell, "")));
    assert_eq!(
        doc["content"][0]["content"][0]["content"][0]["content"][0]["attrs"]["indentLeftTwips"],
        120
    );
    let mut parts = package(&table(
        r#"<w:tblStyle w:val="TableIndent"/>"#,
        &para("", "StyledCell"),
        "",
    ));
    styles(
        &mut parts,
        r#"<w:style w:type="table" w:styleId="TableIndent"><w:pPr><w:ind w:left="120"/></w:pPr></w:style>"#,
    );
    assert_eq!(
        imported(&parts)["content"][0]["content"][0]["content"][0]["content"][0]["attrs"]["indentLeftTwips"],
        120
    );
}
#[test]
fn indented_cells_require_default_margins_even_for_explicit_zero() {
    for indent in [0, 120] {
        let cell = para(&format!(r#"<w:ind w:left="{indent}"/>"#), "Cell");
        for margin in [
            r#"<w:tcMar><w:left w:type="dxa" w:w="0"/></w:tcMar>"#,
            r#"<w:tcMar><w:left w:type="dxa" w:w="108"/><w:start w:type="dxa" w:w="108"/></w:tcMar>"#,
            r#"<w:wrap><w:tcMar><w:left w:type="dxa" w:w="108"/></w:tcMar></w:wrap>"#,
            r#"<w:tcMar/><w:tcMar/>"#,
        ] {
            refused(&package(&table("", &cell, margin)));
        }
        let accepted = table(
            r#"<w:tblCellMar><w:left w:type="dxa" w:w="108"/><w:right w:type="dxa" w:w="108"/></w:tblCellMar>"#,
            &cell,
            "",
        );
        assert_eq!(
            imported(&package(&accepted))["content"][0]["content"][0]["content"][0]["content"][0]["attrs"]
                ["indentLeftTwips"],
            indent
        );
    }
}
#[test]
fn conditional_and_implicit_table_indents_refuse_only_when_used() {
    let mut parts = package(&table(
        r#"<w:tblStyle w:val="Conditional"/>"#,
        &para("", "Cell"),
        "",
    ));
    styles(
        &mut parts,
        r#"<w:style w:type="table" w:styleId="Conditional"><w:tblStylePr w:type="firstRow"><w:pPr><w:ind w:left="120"/></w:pPr></w:tblStylePr></w:style>"#,
    );
    refused(&parts);
    let mut parts = package(&table("", &para("", "Cell"), ""));
    styles(
        &mut parts,
        r#"<w:style w:type="table" w:default="1" w:styleId="DefaultTable"><w:pPr><w:ind w:left="120"/></w:pPr></w:style>"#,
    );
    refused(&parts);
}
#[test]
fn indented_hard_breaks_cannot_silently_drop_metadata() {
    for body in [
        r#"<w:p><w:pPr><w:ind w:left="120"/></w:pPr><w:r><w:t>A</w:t><w:br w:type="page"/><w:t>B</w:t></w:r></w:p>"#,
        r#"<w:p><w:pPr><w:ind w:left="0"/></w:pPr><w:r><w:br w:type="column"/></w:r></w:p>"#,
    ] {
        refused(&package(body));
    }
}

#[test]
fn literal_tabs_and_clear_only_tab_stops_refuse_in_the_new_subset() {
    refused(&package(&para(r#"<w:ind w:left="0"/>"#, "A&#9;B")));
    refused(&package(&para(
        r#"<w:ind w:left="120"/><w:tabs><w:tab w:val="clear" w:pos="1440"/></w:tabs>"#,
        "Tabs",
    )));
    // The prior ordinary-paragraph normalization route remains available.
    assert_eq!(
        imported(&package(&para("", "A&#9;B")))["content"][0]["content"][0]["text"],
        "A\tB"
    );
}
#[test]
fn raw_indentation_relationship_duplicates_aliases_and_modes_refuse() {
    let mut base = package(&para(r#"<w:ind w:left="120"/>"#, "Relationships"));
    numbers(&mut base, &numbering_definition(r#"<w:ind w:left="720"/>"#));
    let key = "word/_rels/document.xml.rels";
    let good = String::from_utf8(base[key].clone()).unwrap();
    let relation = format!(
        r#"<Relationship Id="numbering" Type="{R}/numbering" Target="../custom/actual-numbering.xml"/>"#
    );
    for bad in [
        good.replace("</Relationships>", &format!(r#"<Relationship Id="another" Type="{R}/numbering" Target="../custom/actual-numbering.xml"/></Relationships>"#)),
        good.replace("</Relationships>", &format!(r#"<Relationship Id="numbering" Type="{R}/theme" Target="../custom/actual-numbering.xml"/></Relationships>"#)),
        good.replace(&relation, &format!("<wrap>{relation}</wrap>")),
        good.replace("<Relationship Id",r#"<Relationship xmlns="urn:foreign" Id"#),
        good.replace(r#"Id="numbering""#,r#"Id="numbering" TargetMode="External""#),
        good.replace(r#"Id="numbering""#,r#"Id="numbering" TargetMode="internal""#),
        good.replace(r#"Id="numbering""#,r#"xmlns:x="urn:foreign" Id="numbering" x:Id="shadow""#),
        good.replace(" Type=",r#" xmlns:x="urn:foreign" x:Type="#),
    ] {
        let mut parts = base.clone(); parts.insert(key.into(), bad.into_bytes()); refused(&parts);
    }
}
#[test]
fn repeated_paragraphs_with_large_unused_template_do_not_rescan_all_styles() {
    // This guards the cached/indexed path: a common active style should not
    // trigger a complete scan of the 2,000 unused template definitions per p.
    let mut parts = package(&para(r#"<w:pStyle w:val="Normal"/>"#, "Repeated").repeat(1500));
    let mut sheet = style("Normal", r#"<w:ind w:left="120"/>"#, None);
    for i in 0..2000 {
        sheet.push_str(&style(
            &format!("Unused{i}"),
            r#"<w:ind w:start="120"/>"#,
            None,
        ));
    }
    styles(&mut parts, &sheet);
    let doc = imported(&parts);
    assert_eq!(doc["content"].as_array().unwrap().len(), 1500);
    assert!(
        doc["content"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["attrs"]["indentLeftTwips"] == 120)
    );
}

#[test]
fn pathological_numbering_audit_work_is_bounded() {
    let mut body = String::new();
    let mut definitions = String::from(
        r#"<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl></w:abstractNum>"#,
    );
    for id in 1..=2000 {
        body.push_str(&para(
            &format!(r#"<w:numPr><w:numId w:val="{id}"/></w:numPr>"#),
            "List",
        ));
        definitions.push_str(&format!(
            r#"<w:num w:numId="{id}"><w:abstractNumId w:val="0"/></w:num>"#
        ));
    }
    let mut parts = package(&body);
    numbers(&mut parts, &definitions);
    refused(&parts);
}

fn image_package() -> Parts {
    let mut model: rust_wps::document::Node =
        serde_json::from_str(include_str!("fixtures/word-image-legacy.json")).unwrap();
    model.content.truncate(1);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.docx");
    rust_wps::document::export_docx(&model, &path).unwrap();
    let mut zip = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    (0..zip.len())
        .map(|index| {
            let mut part = zip.by_index(index).unwrap();
            let mut bytes = Vec::new();
            part.read_to_end(&mut bytes).unwrap();
            (part.name().to_owned(), bytes)
        })
        .collect()
}
#[test]
fn real_image_direct_and_inherited_indents_keep_warned_normalization_and_nearby_blank() {
    const WARNING: &str = "Image-only paragraph indentation normalized; text paragraph indentation preservation does not include images";
    for inherited in [false, true] {
        let mut parts = image_package();
        let main = String::from_utf8(parts["word/document.xml"].clone()).unwrap();
        assert!(main.contains("<w:p><w:r>"));
        let properties = if inherited {
            r#"<w:pStyle w:val="IndentedImage"/>"#
        } else {
            r#"<w:ind w:left="480" w:hanging="120"/>"#
        };
        let main = main.replacen(
            "<w:p>",
            &format!(
                r#"<w:p><w:pPr><w:ind w:left="0"/></w:pPr></w:p><w:p><w:pPr>{properties}</w:pPr>"#
            ),
            1,
        );
        parts.insert("word/document.xml".into(), main.into_bytes());
        if inherited {
            let styles = String::from_utf8(parts["word/styles.xml"].clone()).unwrap();
            parts.insert(
                "word/styles.xml".into(),
                styles
                    .replace(
                        "</w:styles>",
                        &format!(
                            "{}</w:styles>",
                            style(
                                "IndentedImage",
                                r#"<w:ind w:left="480" w:hanging="120"/>"#,
                                None
                            )
                        ),
                    )
                    .into_bytes(),
            );
        }
        with_source(&parts, |path| {
            let report = importing::import_docx(path).unwrap();
            assert!(report.compatibility.risks.contains(WARNING));
            assert!(!report.compatibility.risks.contains(BLOCK));
            let doc = serde_json::to_value(report.document).unwrap();
            assert_eq!(doc["content"].as_array().unwrap().len(), 2);
            assert_eq!(doc["content"][0]["type"], "paragraph");
            assert_eq!(doc["content"][0]["attrs"]["indentLeftTwips"], 0);
            assert_eq!(doc["content"][1]["type"], "image");
            assert!(doc["content"][1]["attrs"].get("indentLeftTwips").is_none());
        });
    }
}
#[test]
fn drawing_target_exclusion_does_not_bypass_mixed_multiple_or_malformed_image_guards() {
    let base = image_package();
    let main = String::from_utf8(base["word/document.xml"].clone()).unwrap();
    let main = main.replacen("<w:p>", r#"<w:p><w:pPr><w:ind w:left="480"/></w:pPr>"#, 1);
    let start = main.find("<w:drawing>").unwrap();
    let end = main.find("</w:drawing>").unwrap() + "</w:drawing>".len();
    let drawing = &main[start..end];
    for invalid in [
        main.replace("</w:p>", "<w:r><w:t>Mixed</w:t></w:r></w:p>"),
        main.replace("</w:p>", &format!("<w:r>{drawing}</w:r></w:p>")),
        main.replace(drawing, "<w:drawing/>"),
    ] {
        let mut parts = base.clone();
        parts.insert("word/document.xml".into(), invalid.into_bytes());
        with_source(&parts, |path| {
            let report = compatibility::audit_docx(path).unwrap();
            assert!(
                report
                    .risks
                    .contains("Unsupported image geometry cannot be preserved in an editable copy")
            );
            assert!(importing::import_docx(path).is_err());
        });
    }
    // Image-only normalization still cannot conceal unsupported raw indentation.
    let mut parts = base.clone();
    parts.insert(
        "word/document.xml".into(),
        main.replace(r#"w:left="480""#, r#"w:start="480""#)
            .into_bytes(),
    );
    refused(&parts);
}

#[test]
fn active_numbering_requires_explicit_abstract_reference_but_unused_instance_is_ignored() {
    for unsupported in [r#"<w:ind w:leftChars="100"/>"#, r#"<w:ind w:start="720"/>"#] {
        let definition =
            numbering_definition(unsupported).replace(r#"<w:abstractNumId w:val="0"/>"#, "");
        let mut active = package(&para(list_props(), "Active"));
        numbers(&mut active, &definition);
        refused(&active);
        let mut unused = package(&para(r#"<w:ind w:left="120"/>"#, "Body"));
        numbers(&mut unused, &definition);
        assert_eq!(attrs(&imported(&unused))["indentLeftTwips"], 120);
    }
    // An unused malformed instance also stays harmless beside a valid active
    // instance, even when the parser would silently point both at abstract 0.
    let mut parts = package(&para(list_props(), "ValidList"));
    numbers(
        &mut parts,
        &format!(
            r#"{}<w:num w:numId="2"/>"#,
            numbering_definition(r#"<w:ind w:left="720" w:hanging="360"/>"#)
        ),
    );
    assert_eq!(imported(&parts)["content"][0]["type"], "orderedList");
}

#[test]
fn active_numbering_zero_fallback_identities_are_audited_before_parser_defaults() {
    let base = numbering_definition(r#"<w:ind w:leftChars="100"/>"#);
    for declaration in [
        "",
        r#"w:ilvl="bad""#,
        r#"w:ilvl="256""#,
        r#"w:ilvl="999999999999999999999999""#,
    ] {
        let definition = base.replacen(r#"w:ilvl="0""#, declaration, 1);
        let mut parts = package(&para(list_props(), "LevelZero"));
        numbers(&mut parts, &definition);
        refused(&parts);
    }
    for declaration in [
        r#"w:abstractNumId="bad""#,
        r#"w:abstractNumId="4294967296""#,
        r#"w:abstractNumId="-1""#,
    ] {
        let definition = base.replacen(r#"w:abstractNumId="0""#, declaration, 1);
        let mut parts = package(&para(list_props(), "AbstractZero"));
        numbers(&mut parts, &definition);
        refused(&parts);
    }
    // Missing abstractNum@abstractNumId and missing/invalid lvlOverride@ilvl
    // are genuinely ignored by the pinned reader. Do not invent zero aliases
    // for those elements, or reject unrelated malformed num instances (ID0).
    let valid = numbering_definition(r#"<w:ind w:left="720" w:hanging="360"/>"#);
    let unused = format!(
        r#"{valid}<w:abstractNum><w:lvl><w:pPr><w:ind w:leftChars="100"/></w:pPr></w:lvl></w:abstractNum><w:num w:numId="bad"><w:abstractNumId w:val="0"/></w:num>"#
    );
    let unused = unused.replacen("</w:num>",r#"<w:lvlOverride><w:lvl w:ilvl="0"><w:pPr><w:ind w:start="720"/></w:pPr></w:lvl></w:lvlOverride><w:lvlOverride w:ilvl="bad"><w:lvl w:ilvl="0"><w:pPr><w:ind w:leftChars="100"/></w:pPr></w:lvl></w:lvlOverride></w:num>"#,1);
    let mut parts = package(&para(list_props(), "Ordinary"));
    numbers(&mut parts, &unused);
    assert_eq!(imported(&parts)["content"][0]["type"], "orderedList");
}

#[test]
fn unresolved_styles_refuse_new_indentation_but_keep_legacy_unindented_route() {
    let unknown = r#"<w:pStyle w:val="Unknown"/>"#;
    for props in [
        r#"<w:ind w:left="120"/>"#,
        r#"<w:ind w:left="0"/>"#,
        r#"<w:ind/>"#,
    ] {
        refused(&package(&para(&format!("{unknown}{props}"), "Direct")));
        let mut defaulted = package(&para(unknown, "Default"));
        styles(
            &mut defaulted,
            &format!(
                r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:pPr>{props}</w:pPr></w:style>"#
            ),
        );
        refused(&defaulted);
        let mut inherited = package(&para(r#"<w:pStyle w:val="Child"/>"#, "BasedOn"));
        styles(&mut inherited, &style("Child", props, Some("MissingBase")));
        refused(&inherited);
    }
    let mut defaults = package(&para(unknown, "DocDefaults"));
    styles(
        &mut defaults,
        r#"<w:docDefaults><w:pPrDefault><w:pPr><w:ind w:left="120"/></w:pPr></w:pPrDefault></w:docDefaults>"#,
    );
    refused(&defaults);
    let mut fallback_chain = package(&para(unknown, "FallbackAncestor"));
    styles(
        &mut fallback_chain,
        &format!(
            r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:basedOn w:val="Base"/></w:style>{}"#,
            style("Base", r#"<w:ind w:left="120"/>"#, None)
        ),
    );
    refused(&fallback_chain);
    assert!(
        attrs(&imported(&package(&para(unknown, "LegacyUnknown"))))["indentLeftTwips"].is_null()
    );
    let mut legacy = package(&para(r#"<w:pStyle w:val="Child"/>"#, "LegacyMissingBase"));
    styles(
        &mut legacy,
        &format!(
            "{}{}",
            style("Child", "", Some("MissingBase")),
            style("Unused", r#"<w:ind w:left="120"/>"#, Some("AlsoMissing"))
        ),
    );
    assert!(attrs(&imported(&legacy))["indentLeftTwips"].is_null());
    // An unrelated unused missing reference does not prevent a proven direct
    // indent whose own style chain is complete.
    let mut unrelated = package(&para(r#"<w:ind w:left="120"/>"#, "Proven"));
    styles(
        &mut unrelated,
        &style("Unused", r#"<w:ind w:left="240"/>"#, Some("Missing")),
    );
    assert_eq!(attrs(&imported(&unrelated))["indentLeftTwips"], 120);
}
#[test]
fn unresolved_table_style_does_not_guess_indented_cell_margins() {
    for props in [r#"<w:ind w:left="120"/>"#, r#"<w:ind w:left="0"/>"#] {
        refused(&package(&table(
            r#"<w:tblStyle w:val="MissingTable"/>"#,
            &para(props, "Cell"),
            "",
        )));
        let mut inherited = package(&table(
            r#"<w:tblStyle w:val="Derived"/>"#,
            &para(props, "Cell"),
            "",
        ));
        styles(
            &mut inherited,
            r#"<w:style w:type="table" w:styleId="Derived"><w:basedOn w:val="MissingBase"/></w:style>"#,
        );
        refused(&inherited);
    }
    // Existing table geometry checks may independently refuse unresolved
    // table styles. Without cell indents this audit adds no new refusal.
    with_source(
        &package(&table(
            r#"<w:tblStyle w:val="MissingTable"/>"#,
            &para("", "OrdinaryCell"),
            "",
        )),
        |path| {
            assert!(
                !compatibility::audit_docx(path)
                    .unwrap()
                    .risks
                    .contains(BLOCK)
            );
        },
    );
}

#[test]
fn numbering_level_paragraph_style_association_is_valid_but_ambiguous_links_refuse() {
    let base = numbering_definition(
        r#"<w:tabs><w:tab w:val="num" w:pos="360"/></w:tabs><w:ind w:left="360" w:hanging="360"/>"#,
    );
    let linked = base.replace(
        r#"<w:start w:val="1"/>"#,
        r#"<w:pStyle w:val="ListNumber"/><w:start w:val="1"/>"#,
    );
    let mut parts = package(&para(list_props(), "Associated"));
    numbers(&mut parts, &linked);
    assert_eq!(imported(&parts)["content"][0]["type"], "orderedList");
    for invalid in [
        linked.replace(
            r#"<w:pStyle w:val="ListNumber"/>"#,
            r#"<w:pStyle w:val="ListNumber"/><w:pStyle w:val="Other"/>"#,
        ),
        linked.replace(
            r#"<w:pStyle w:val="ListNumber"/>"#,
            r#"<w:pStyle same:val="ListNumber"/>"#,
        ),
        linked.replace(
            r#"<w:pStyle w:val="ListNumber"/>"#,
            r#"<w:wrap><w:pStyle w:val="ListNumber"/></w:wrap>"#,
        ),
    ] {
        let mut parts = package(&para(list_props(), "Associated"));
        numbers(&mut parts, &invalid);
        refused(&parts);
    }
}
