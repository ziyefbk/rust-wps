//! Independent source expectations for the bounded contextual-to-fixed import.
use rust_wps::{compatibility, document, importing};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, io::Write, path::Path};
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const P: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const RISK: &str = "Unsupported paragraph spacing cannot be preserved in an editable copy";
const WARN: &str = "Contextual same-style spacing resolved to fixed paragraph values; named-style and reorder dynamics are not retained";
type Parts = BTreeMap<String, Vec<u8>>;
fn p(props: &str, text: &str) -> String {
    format!("<w:p><w:pPr>{props}</w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>")
}
fn style(id: &str, base: &str, props: &str) -> String {
    format!(
        r#"<w:style w:type="paragraph" w:styleId="{id}">{}<w:pPr>{props}</w:pPr></w:style>"#,
        if base.is_empty() {
            String::new()
        } else {
            format!(r#"<w:basedOn w:val="{base}"/>"#)
        }
    )
}
fn reference(id: &str) -> String {
    format!(r#"<w:pStyle w:val="{id}"/>"#)
}
fn gap(before: u32, after: u32, flag: &str) -> String {
    format!(
        r#"<w:spacing w:before="{before}" w:after="{after}" w:line="276"/><w:contextualSpacing w:val="{flag}"/>"#
    )
}
fn package(body: &str, styles: &str) -> Parts {
    BTreeMap::from([
        ("[Content_Types].xml".into(), br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_vec()),
        ("_rels/.rels".into(),format!(r#"<Relationships xmlns="{P}"><Relationship Id="main" Type="{R}/officeDocument" Target="word/document.xml"/></Relationships>"#).into_bytes()),
        ("word/document.xml".into(),format!(r#"<w:document xmlns:w="{W}" xmlns:x="urn:foreign" xmlns:same="{W}"><w:body>{body}</w:body></w:document>"#).into_bytes()),
        ("word/_rels/document.xml.rels".into(),format!(r#"<Relationships xmlns="{P}"><Relationship Id="styles" Type="{R}/styles" Target="../custom/styles.xml"/></Relationships>"#).into_bytes()),
        ("custom/styles.xml".into(),format!(r#"<w:styles xmlns:w="{W}" xmlns:x="urn:foreign" xmlns:same="{W}">{styles}</w:styles>"#).into_bytes()),
    ])
}
fn numbers(parts: &mut Parts, extra: &str) {
    let rels = String::from_utf8(parts["word/_rels/document.xml.rels"].clone()).unwrap();
    parts.insert("word/_rels/document.xml.rels".into(),rels.replace("</Relationships>",&format!(r#"<Relationship Id="numbers" Type="{R}/numbering" Target="../custom/numbers.xml"/></Relationships>"#)).into_bytes());
    parts.insert("custom/numbers.xml".into(),format!(r#"<w:numbering xmlns:w="{W}"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:pPr>{extra}</w:pPr></w:lvl><w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="•"/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#).into_bytes());
}
fn num(level: u8) -> String {
    format!(r#"<w:numPr><w:numId w:val="1"/><w:ilvl w:val="{level}"/></w:numPr>"#)
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
    let original = fs::read(&path).unwrap();
    let result = f(&path);
    assert_eq!(original, fs::read(&path).unwrap());
    result
}
fn imported(parts: &Parts) -> document::Node {
    with_source(parts, |path| {
        let audit = compatibility::audit_docx(path).unwrap();
        assert!(!audit.risks.contains(RISK), "{:?}", audit.risks);
        assert!(audit.risks.contains(WARN));
        let report = importing::import_docx(path).unwrap();
        assert!(report.compatibility.risks.contains(WARN));
        report.document
    })
}
fn paragraphs(node: &document::Node) -> Vec<&document::Node> {
    let mut values = Vec::new();
    if matches!(node.kind.as_str(), "paragraph" | "heading") {
        values.push(node);
    }
    for child in &node.content {
        values.extend(paragraphs(child));
    }
    values
}
fn margins(node: &document::Node) -> Value {
    json!(
        paragraphs(node)
            .iter()
            .map(|p| vec![
                p.attrs["spaceBefore"].clone(),
                p.attrs["spaceAfter"].clone(),
                p.attrs["lineSpacing"].clone()
            ])
            .collect::<Vec<_>>()
    )
}
fn refused(parts: &Parts) {
    with_source(parts, |path| {
        if let Ok(audit) = compatibility::audit_docx(path) {
            assert!(audit.risks.contains(RISK), "{:?}", audit.risks);
        }
        assert!(importing::import_docx(path).is_err());
    });
}
fn table(cells: &[String]) -> String {
    format!(
        r#"<w:tbl><w:tblPr><w:tblW w:type="auto" w:w="0"/></w:tblPr><w:tblGrid>{}</w:tblGrid><w:tr>{}</w:tr></w:tbl>"#,
        cells
            .iter()
            .map(|_| r#"<w:gridCol w:w="4500"/>"#)
            .collect::<String>(),
        cells
            .iter()
            .map(|s| format!(r#"<w:tc><w:tcPr><w:tcW w:type="auto" w:w="0"/></w:tcPr>{s}</w:tc>"#))
            .collect::<String>()
    )
}
#[test]
fn bilateral_contextual_keeps_exterior_margins_and_uses_original_three_paragraph_edges() {
    let body = p(&gap(80, 200, "true"), "A")
        + &p(&gap(240, 120, "on"), "B")
        + &p(&gap(160, 320, "1"), "C");
    assert_eq!(
        margins(&imported(&package(&body, ""))),
        json!([[80, 0, 276], [0, 0, 276], [0, 320, 276]])
    );
}
#[test]
fn all_four_one_sided_zero_boundaries_are_admitted() {
    for (a, b, flag_a, flag_b, expected_a, expected_b) in [
        (0, 240, "true", "false", 0, 240),
        (200, 0, "true", "false", 0, 0),
        (0, 240, "false", "true", 0, 0),
        (200, 0, "false", "true", 200, 0),
    ] {
        let body = p(&gap(80, a, flag_a), "A") + &p(&gap(b, 320, flag_b), "B");
        assert_eq!(
            margins(&imported(&package(&body, ""))),
            json!([[80, expected_a, 276], [expected_b, 320, 276]])
        );
    }
}
#[test]
fn one_sided_positive_positive_refuses_equal_and_unequal_consumer_divergence() {
    for (a, b) in [(200, 200), (200, 240), (240, 200)] {
        for (fa, fb) in [("true", "false"), ("false", "true")] {
            let parts = package(&(p(&gap(0, a, fa), "A") + &p(&gap(b, 0, fb), "B")), "");
            refused(&parts);
            with_source(&parts, |path| {
                assert!(
                    compatibility::audit_docx(path)
                        .unwrap()
                        .risks
                        .iter()
                        .any(|s| s.contains("One-sided nonzero"))
                )
            });
        }
    }
}
#[test]
fn inherited_partial_values_explicit_zeros_and_false_overrides_resolve_independently() {
    let styles = format!(
        r#"<w:docDefaults><w:pPrDefault><w:pPr><w:spacing w:before="80" w:after="200" w:line="276"/><w:contextualSpacing/></w:pPr></w:pPrDefault></w:docDefaults>{}{}"#,
        style("Base", "", r#"<w:spacing w:before="100"/>"#),
        style("Leaf", "Base", r#"<w:spacing w:after="0"/>"#)
    );
    let body = p(&reference("Leaf"), "A")
        + &p(
            &(reference("Leaf") + r#"<w:contextualSpacing w:val="false"/>"#),
            "B",
        );
    assert_eq!(
        margins(&imported(&package(&body, &styles))),
        json!([[100, 0, 276], [100, 0, 276]])
    );
    let body = p(r#"<w:contextualSpacing w:val="false"/>"#, "A")
        + &p(r#"<w:contextualSpacing w:val="0"/>"#, "B");
    assert_eq!(
        margins(&imported(&package(&body, &styles))),
        json!([[80, 200, 276], [80, 200, 276]])
    );
}
#[test]
fn leaf_style_identity_is_not_inherited_ancestor_or_output_node_kind() {
    let styles = style("Base", "", &gap(80, 200, "true"))
        + &style("A", "Base", "")
        + &style("B", "Base", "");
    let body = p(&reference("A"), "A") + &p(&reference("B"), "B");
    assert_eq!(
        margins(&imported(&package(&body, &styles))),
        json!([[80, 200, 276], [80, 200, 276]])
    );
}
#[test]
fn omitted_style_matches_explicit_actual_default() {
    let styles = format!(
        r#"<w:style w:type="paragraph" w:styleId="ActualDefault" w:default="1"><w:pPr>{}</w:pPr></w:style>"#,
        gap(80, 200, "true")
    );
    let body = p("", "A") + &p(&reference("ActualDefault"), "B");
    assert_eq!(
        margins(&imported(&package(&body, &styles))),
        json!([[80, 0, 276], [0, 200, 276]])
    );
}
#[test]
fn empty_source_paragraphs_survive_and_participate_in_contextual_adjacency() {
    let styles = format!(
        r#"<w:style w:type="paragraph" w:styleId="Normal" w:default="1"><w:pPr>{}</w:pPr></w:style>"#,
        gap(80, 200, "true")
    );
    let body = p("", "A") + "<w:p/>" + &p("", "B");
    let doc = imported(&package(&body, &styles));
    assert_eq!(
        margins(&doc),
        json!([[80, 0, 276], [0, 0, 276], [0, 200, 276]])
    );
    assert!(paragraphs(&doc)[1].content.is_empty());
}
#[test]
fn body_table_and_cell_boundaries_break_adjacency_but_same_cell_does_not() {
    let styled = style("Same", "", &gap(80, 200, "true"));
    let q = |s| p(&reference("Same"), s);
    let body = q("Before") + &table(&[q("C1a") + &q("C1b"), q("C2")]) + &q("After");
    assert_eq!(
        margins(&imported(&package(&body, &styled))),
        json!([
            [80, 200, 276],
            [80, 0, 276],
            [0, 200, 276],
            [80, 200, 276],
            [80, 200, 276]
        ])
    );
}
#[test]
fn list_wrappers_are_transparent_across_numbering_kinds_and_back_to_body() {
    let styles = style("Same", "", &gap(80, 200, "true"));
    let body = p(&(reference("Same") + &num(0)), "A")
        + &p(&(reference("Same") + &num(1)), "")
        + &p(&reference("Same"), "B");
    let mut parts = package(&body, &styles);
    numbers(&mut parts, "");
    let doc = imported(&parts);
    assert_eq!(
        margins(&doc),
        json!([[80, 0, 276], [0, 0, 276], [0, 200, 276]])
    );
    assert_eq!(doc.content[0].kind, "orderedList");
    assert_eq!(doc.content[0].content[0].content[1].kind, "bulletList");
}
#[test]
fn headings_soft_breaks_and_decoded_text_keep_exact_identity() {
    let styles = style("Heading1", "", &gap(80, 200, "true"));
    let body = p(&reference("Heading1"), "A &amp; B")
        + &format!(
            r#"<w:p><w:pPr>{}</w:pPr><w:r><w:t>C</w:t><w:br/><w:t>D</w:t></w:r></w:p>"#,
            reference("Heading1")
        );
    let doc = imported(&package(&body, &styles));
    assert!(paragraphs(&doc).iter().all(|p| p.kind == "heading"));
    assert_eq!(margins(&doc), json!([[80, 0, 276], [0, 200, 276]]));
}
#[test]
fn unknown_cyclic_duplicate_and_foreign_style_identities_are_refused_when_active() {
    for styles in [style("Used","Missing",&gap(0,0,"true")),style("Used","Used",&gap(0,0,"true")),style("Used","",&gap(0,0,"true"))+&style("Used","",""),r#"<w:style w:type="paragraph" w:styleId="Used" x:styleId="Other"><w:pPr><w:contextualSpacing/></w:pPr></w:style>"#.into()] {
        refused(&package(&p(&reference("Used"),"A"),&styles));
    }
    refused(&package(
        &p(&(reference("Unknown") + "<w:contextualSpacing/>"), "A"),
        "",
    ));
    refused(&package(
        &p(r#"<w:pStyle w:val="Used" x:val="Other"/>"#, "A"),
        &style("Used", "", "<w:contextualSpacing/>"),
    ));
}
#[test]
fn malformed_contextual_declarations_and_unproven_content_are_refused() {
    for declaration in [
        r#"<w:contextualSpacing w:val="maybe"/>"#,
        r#"<w:contextualSpacing/><w:contextualSpacing w:val="false"/>"#,
        r#"<same:contextualSpacing/>"#,
        r#"<w:contextualSpacing x:val="false"/>"#,
        r#"<w:contextualSpacing w:val="true" extra="1"/>"#,
    ] {
        refused(&package(&p(declaration, "A"), ""));
    }
    for run in [
        r#"<w:br w:type="page"/>"#,
        r#"<w:br w:type="column"/>"#,
        r#"<w:drawing/>"#,
    ] {
        refused(&package(
            &format!("<w:p><w:pPr><w:contextualSpacing/></w:pPr><w:r>{run}</w:r></w:p>"),
            "",
        ));
    }
}
#[test]
fn numbering_contextual_and_table_conditional_contextual_remain_refused() {
    let mut parts = package(&p(&(num(0) + "<w:contextualSpacing/>"), "A"), "");
    numbers(&mut parts, "<w:contextualSpacing/>");
    refused(&parts);
    let styles = r#"<w:style w:type="table" w:styleId="Grid"><w:tblStylePr w:type="firstRow"><w:pPr><w:contextualSpacing/></w:pPr></w:tblStylePr></w:style>"#;
    let body = table(&[p("<w:contextualSpacing/>", "A")])
        .replace("<w:tblPr>", r#"<w:tblPr><w:tblStyle w:val="Grid"/>"#);
    refused(&package(&body, styles));
}
#[test]
fn unused_contextual_template_is_ignored_but_unknown_active_style_cannot_prove_pagination() {
    let parts = package(
        &p(&reference("Unknown"), "A"),
        &style("Unused", "", "<w:contextualSpacing/>"),
    );
    // A missing active style can no longer prove pagination absence. Keep
    // the old input as a named refusal, not an invented default-false value.
    with_source(&parts, |path| {
        assert!(
            importing::import_docx(path)
                .unwrap_err()
                .to_string()
                .contains("Unsupported or unproved paragraph pagination")
        );
    });
    let complete = package(&p("", "A"), &style("Unused", "", "<w:contextualSpacing/>"));
    with_source(&complete, |path| {
        let report = importing::import_docx(path).unwrap();
        assert!(!report.compatibility.risks.contains(WARN));
        assert_eq!(margins(&report.document), json!([[0, 0, 240]]));
    });
}
#[test]
fn relationship_identity_is_part_of_contextual_proof() {
    let mut parts = package(&p("<w:contextualSpacing/>", "A"), "");
    let rels = String::from_utf8(parts["word/_rels/document.xml.rels"].clone()).unwrap();
    parts.insert("word/_rels/document.xml.rels".into(),rels.replace("</Relationships>",&format!(r#"<Relationship Id="duplicate" Type="{R}/styles" Target="../custom/styles.xml"/></Relationships>"#)).into_bytes());
    refused(&parts);
}
#[test]
fn normalized_values_export_and_trusted_reopen_without_dynamic_metadata() {
    let mut doc = imported(&package(
        &(p(&gap(80, 200, "true"), "A") + &p(&gap(240, 320, "true"), "B")),
        "",
    ));
    doc.content[0].attrs.insert("spaceAfter".into(), json!(140));
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("fixed.docx");
    document::export_docx(&doc, &out).unwrap();
    assert_eq!(
        serde_json::to_value(document::load_authored_docx(&out).unwrap().unwrap()).unwrap(),
        serde_json::to_value(&doc).unwrap()
    );
    assert!(!serde_json::to_string(&doc).unwrap().contains("contextual"));
}

#[test]
fn explicit_false_numbering_and_table_contextual_layers_are_not_silently_ignored() {
    let defaults = r#"<w:docDefaults><w:pPrDefault><w:pPr><w:contextualSpacing/></w:pPr></w:pPrDefault></w:docDefaults>"#;
    let mut parts = package(&(p(&num(0), "A") + &p(&num(0), "B")), defaults);
    numbers(&mut parts, r#"<w:contextualSpacing w:val="false"/>"#);
    refused(&parts);
    let styles = format!(
        r#"{defaults}<w:style w:type="table" w:default="1" w:styleId="TableDefault"><w:pPr><w:contextualSpacing w:val="false"/></w:pPr></w:style>"#
    );
    refused(&package(&table(&[p("", "A") + &p("", "B")]), &styles));
}
#[test]
fn numbering_contextual_cannot_hide_in_wrapper_within_used_level() {
    let mut parts = package(&p(&(num(0) + "<w:contextualSpacing/>"), "A"), "");
    numbers(
        &mut parts,
        r#"<w:wrapper><w:contextualSpacing/></w:wrapper>"#,
    );
    refused(&parts);
}
#[test]
fn unrelated_hard_break_and_image_in_active_document_remain_explicitly_unsupported() {
    for other in [
        r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#,
        r#"<w:p><w:r><w:drawing/></w:r></w:p>"#,
    ] {
        refused(&package(&(p("<w:contextualSpacing/>", "A") + other), ""));
    }
}
#[test]
fn used_distinct_style_ids_with_ambiguous_display_names_are_refused() {
    for names in [("SameName", "SameName"), ("SameName", " samename ")] {
        let styles = format!(
            r#"<w:style w:type="paragraph" w:styleId="A"><w:name w:val="{}"/><w:pPr>{}</w:pPr></w:style><w:style w:type="paragraph" w:styleId="B"><w:name w:val="{}"/><w:pPr>{}</w:pPr></w:style>"#,
            names.0,
            gap(80, 200, "true"),
            names.1,
            gap(80, 200, "true")
        );
        refused(&package(
            &(p(&reference("A"), "A") + &p(&reference("B"), "B")),
            &styles,
        ));
    }
}
#[test]
fn dynamic_style_or_contextual_model_attributes_are_still_invalid_and_export_is_no_clobber() {
    let doc = imported(&package(&p(&gap(0, 0, "true"), "A"), ""));
    for key in ["contextualSpacing", "styleId", "sourceStyle"] {
        for value in [Value::Null, json!(false), json!(true), json!("Used")] {
            for target in ["doc", "paragraph", "heading"] {
                let mut bad = doc.clone();
                if target == "doc" {
                    bad.attrs.insert(key.into(), value.clone());
                } else {
                    let p = &mut bad.content[0];
                    p.kind = target.into();
                    if target == "heading" {
                        p.attrs.insert("level".into(), json!(1));
                    }
                    p.attrs.insert(key.into(), value.clone());
                }
                assert!(document::validate(&bad).is_err(), "{target} {key} {value}");
            }
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fixed.docx");
    document::export_docx(&doc, &path).unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(document::export_docx(&doc, &path).is_err());
    assert_eq!(bytes, fs::read(&path).unwrap());
}

#[test]
fn explicit_display_name_colliding_with_other_unnamed_style_id_is_refused() {
    let styles = style("A", "", &gap(80, 200, "true"))
        + &format!(
            r#"<w:style w:type="paragraph" w:styleId="B"><w:name w:val="A"/><w:pPr>{}</w:pPr></w:style>"#,
            gap(80, 200, "true")
        );
    refused(&package(
        &(p(&reference("A"), "A") + &p(&reference("B"), "B")),
        &styles,
    ));
}

#[test]
fn numbered_headings_validate_contextual_numbering_layers_even_when_not_list_nodes() {
    let styles = style("Heading1", "", &gap(80, 200, "true"));
    let body =
        p(&(reference("Heading1") + &num(0)), "A") + &p(&(reference("Heading1") + &num(0)), "B");
    let mut parts = package(&body, &styles);
    numbers(&mut parts, r#"<w:contextualSpacing w:val="false"/>"#);
    refused(&parts);
}

#[test]
fn undeclared_default_cannot_be_assumed_distinct_from_named_adjacent_style() {
    for (id, name) in [
        ("Normal", "Normal"),
        ("Normal", "Standard"),
        ("ProbeA", "Normal"),
        ("ProbeA", "Standard"),
    ] {
        let styles = format!(
            r#"<w:style w:type="paragraph" w:styleId="{id}"><w:name w:val="{name}"/><w:pPr>{}</w:pPr></w:style>"#,
            gap(80, 200, "true")
        );
        for reverse in [false, true] {
            let implicit = p(&gap(80, 200, "true"), "implicit");
            let explicit = p(&reference(id), "explicit");
            let body = if reverse {
                explicit + &implicit
            } else {
                implicit + &explicit
            };
            let parts = package(&body, &styles);
            refused(&parts);
            with_source(&parts, |path| {
                assert!(
                    compatibility::audit_docx(path)
                        .unwrap()
                        .risks
                        .iter()
                        .any(|s| s.contains("Unproved implicit default"))
                )
            });
        }
    }
}
