//! Owned fixtures isolate minimum row heights from unrelated source normalization.
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
const NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const RISK: &str = "Unsupported table geometry cannot be preserved in an editable copy";
fn table(height: &str, alignment: &str) -> String {
    format!(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2400"/></w:tblGrid><w:tr><w:trPr>{height}</w:trPr><w:tc><w:tcPr><w:tcW w:type="dxa" w:w="2400"/>{alignment}</w:tcPr><w:p><w:r><w:t>Cell &amp; text</w:t><w:br/><w:t>next</w:t></w:r></w:p><w:p/></w:tc></w:tr></w:tbl>"#
    )
}
fn parts(body: &str, styles: &str) -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
        ("[Content_Types].xml".into(), br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/meta/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#.to_vec()),
        ("_rels/.rels".into(), br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="office" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec()),
        ("word/_rels/document.xml.rels".into(), br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="meta/styles.xml"/></Relationships>"#.to_vec()),
        ("word/document.xml".into(), format!(r#"<w:document xmlns:w="{NS}"><w:body>{body}</w:body></w:document>"#).into_bytes()),
        ("word/meta/styles.xml".into(), format!(r#"<w:styles xmlns:w="{NS}">{styles}</w:styles>"#).into_bytes()),
    ])
}
fn write(path: &Path, parts: &BTreeMap<String, Vec<u8>>) {
    let mut archive = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, value) in parts {
        archive
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(value).unwrap();
    }
    archive.finish().unwrap();
}
fn import(body: &str, styles: &str, refused: bool) -> Option<Node> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.docx");
    write(&path, &parts(body, styles));
    let before = fs::read(&path).unwrap();
    let audit = compatibility::audit_docx(&path);
    let imported = importing::import_docx(&path);
    assert_eq!(before, fs::read(&path).unwrap());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    if refused {
        if let Err(error) = &audit {
            assert!(
                error
                    .to_string()
                    .contains("duplicate DOCX style identifier")
                    || error
                        .to_string()
                        .contains("invalid DOCX style default value"),
                "{error}"
            );
            assert!(imported.is_err());
            return None;
        }
        let audit = audit.unwrap();
        assert!(
            audit.risks.contains(RISK),
            "{body}\n{styles}\n{:?}",
            audit.risks
        );
        assert!(imported.is_err());
        None
    } else {
        audit.unwrap();
        Some(imported.unwrap().document)
    }
}
fn minimum(value: &str, rule: &str) -> String {
    format!(r#"<w:trHeight w:val="{value}"{rule}/>"#)
}
fn alignment(value: &str) -> String {
    format!(r#"<w:vAlign w:val="{value}"/>"#)
}
fn model(height: Option<Value>, align: Option<Value>) -> Node {
    let mut row = json!({"type":"tableRow","attrs":{},"content":[{"type":"tableCell","attrs":{},"content":[{"type":"paragraph","content":[{"type":"text","text":"Cell"}]}]}]});
    if let Some(value) = height {
        row["attrs"]["rowMinHeightTwips"] = value;
    }
    if let Some(value) = align {
        row["content"][0]["attrs"]["cellVerticalAlign"] = value;
    }
    serde_json::from_value(json!({"type":"doc","attrs":{"styleProfile":"standard-v2"},"content":[{"type":"table","content":[row]}]})).unwrap()
}
fn unzip(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|i| {
            let mut entry = archive.by_index(i).unwrap();
            let mut bytes = vec![];
            entry.read_to_end(&mut bytes).unwrap();
            (entry.name().into(), bytes)
        })
        .collect()
}
#[test]
fn source_direct_minimum_and_alignment_preserve_omission_zero_and_explicit_top() {
    for value in ["0", "1", "1340", "13958"] {
        for rule in ["", r#" w:hRule="atLeast""#] {
            for align in ["top", "center", "bottom"] {
                let doc =
                    import(&table(&minimum(value, rule), &alignment(align)), "", false).unwrap();
                assert_eq!(
                    doc.content[0].content[0].attrs["rowMinHeightTwips"],
                    json!(value.parse::<u64>().unwrap())
                );
                assert_eq!(
                    doc.content[0].content[0].content[0].attrs["cellVerticalAlign"],
                    align
                );
            }
        }
    }
    for (height, align) in [
        (minimum("0", ""), "".to_owned()),
        ("".into(), alignment("top")),
        ("".into(), "".into()),
    ] {
        let doc = import(&table(&height, &align), "", false).unwrap();
        let row = &doc.content[0].content[0];
        assert_eq!(
            row.attrs.contains_key("rowMinHeightTwips"),
            !height.is_empty()
        );
        assert_eq!(
            row.content[0].attrs.contains_key("cellVerticalAlign"),
            !align.is_empty()
        );
    }
}
#[test]
fn source_row_rules_values_and_leaf_schema_are_strict() {
    for value in [
        "-1",
        "1.0",
        "1pt",
        "",
        " 1",
        "+1",
        "31681",
        "13959",
        "999999999999999999999999",
    ] {
        import(&table(&minimum(value, ""), ""), "", true);
    }
    for rule in ["exact", "auto", "AtLeast", "unknown", ""] {
        import(
            &table(&minimum("1340", &format!(r#" w:hRule="{rule}""#)), ""),
            "",
            true,
        );
    }
    for height in [
        "<w:trHeight/>",
        r#"<w:trHeight w:val="1" w:unknown="1"/>"#,
        r#"<w:trHeight w:val="1"><w:dummy/></w:trHeight>"#,
        r#"<w:trHeight w:val="1">text</w:trHeight>"#,
    ] {
        import(&table(height, ""), "", true);
    }
}
#[test]
fn source_alignment_values_duplicates_and_wrappers_are_strict() {
    for value in ["both", "Top", "middle", "", "auto"] {
        import(&table("", &alignment(value)), "", true);
    }
    for value in [
        "<w:vAlign/>",
        r#"<w:vAlign w:val="top" w:unknown="1"/>"#,
        r#"<w:wrapper><w:vAlign w:val="top"/></w:wrapper>"#,
    ] {
        import(&table("", value), "", true);
    }
    import(
        &table(&(minimum("1", "") + &minimum("2", "")), ""),
        "",
        true,
    );
    import(
        &table("", &(alignment("top") + &alignment("bottom"))),
        "",
        true,
    );
    import(
        &table(&format!("<w:wrapper>{}</w:wrapper>", minimum("1", "")), ""),
        "",
        true,
    );
}
#[test]
fn source_wrong_namespaces_prefix_aliases_and_conflicts_are_not_defaults() {
    let good = table(&minimum("1", ""), &alignment("top"));
    for bad in [
        good.replace("w:val=\"1\"", r#"xmlns:q="urn:foreign" q:val="1""#),
        good.replace("w:val=\"1\"", &format!(r#"xmlns:q="{NS}" q:val="1""#)),
        good.replace("<w:trHeight", r#"<x:trHeight xmlns:x="urn:foreign""#),
        good.replace(
            "w:val=\"top\"",
            r#"xmlns:q="urn:foreign" w:val="top" q:val="bottom""#,
        ),
    ] {
        import(&bad, "", true);
    }
}
#[test]
fn source_inherited_conditional_and_default_vertical_properties_are_refused() {
    let body = table(&minimum("1", ""), "").replace(
        "<w:tblPr/>",
        r#"<w:tblPr><w:tblStyle w:val="Leaf"/></w:tblPr>"#,
    );
    for property in [
        format!("<w:trPr>{}</w:trPr>", minimum("1", "")),
        format!("<w:tcPr>{}</w:tcPr>", alignment("top")),
    ] {
        for wrapped in [
            property.clone(),
            format!("<w:tblStylePr w:type=\"firstRow\">{property}</w:tblStylePr>"),
        ] {
            let styles = format!(
                r#"<w:style w:type="table" w:styleId="Leaf"><w:basedOn w:val="Base"/></w:style><w:style w:type="table" w:styleId="Base">{wrapped}</w:style>"#
            );
            import(&body, &styles, true);
        }
        let styles = format!(
            r#"<w:style w:type="table" w:default="1" w:styleId="Base">{property}</w:style>"#
        );
        import(&table("", ""), &styles, true);
    }
}
#[test]
fn source_style_identity_missing_duplicate_cyclic_and_wrapped_edges_are_refused() {
    let body = table(&minimum("1", ""), "").replace(
        "<w:tblPr/>",
        r#"<w:tblPr><w:tblStyle w:val="Leaf"/></w:tblPr>"#,
    );
    for styles in ["".to_owned(),r#"<w:style w:type="table" w:styleId="Leaf"><w:basedOn w:val="Leaf"/></w:style>"#.into(),r#"<w:style w:type="table" w:styleId="Leaf"/><w:style w:type="table" w:styleId="Leaf"/>"#.into(),r#"<w:style w:type="table" w:styleId="Leaf"><w:wrapper><w:basedOn w:val="Missing"/></w:wrapper></w:style>"#.into(),r#"<w:style w:type="table" w:styleId="Leaf"><w:basedOn w:val="Base"/><w:basedOn w:val="Base"/></w:style>"#.into(),format!(r#"<w:style w:type="table" w:styleId="Leaf"/><w:style xmlns:q="{NS}" w:type="table" q:styleId="Leaf"/>"#)] {import(&body,&styles,true);}
    for flag in ["unknown", "", "TRUE"] {
        import(
            &table(&minimum("1", ""), ""),
            &format!(r#"<w:style w:type="table" w:default="{flag}" w:styleId="Leaf"/>"#),
            true,
        );
    }
}
#[test]
fn source_unused_vertical_style_and_empty_active_chain_do_not_invent_properties() {
    let styles = r#"<w:style w:type="table" w:styleId="Unused"><w:trPr><w:trHeight w:val="900" w:hRule="exact"/></w:trPr></w:style><w:style w:type="table" w:styleId="Leaf"><w:basedOn w:val="Base"/></w:style><w:style w:type="table" w:styleId="Base"/>"#;
    let body = table(&minimum("1", ""), "").replace(
        "<w:tblPr/>",
        r#"<w:tblPr><w:tblStyle w:val="Leaf"/></w:tblPr>"#,
    );
    import(&body, styles, false);
}
#[test]
fn source_vertical_path_guards_headers_split_hidden_revision_direction_and_merge() {
    for property in [
        "<w:tblHeader/>",
        "<w:cantSplit/>",
        r#"<w:tblHeader w:val="0"/>"#,
        r#"<w:cantSplit w:val="0"/>"#,
        r#"<w:hidden w:val="0"/>"#,
        r#"<w:textDirection w:val="lrTb"/>"#,
        r#"<w:gridSpan w:val="1"/>"#,
        "<w:vMerge/>",
        "<w:trPrChange/>",
        "<w:tcPrChange/>",
    ] {
        import(&table(&(minimum("1", "") + property), ""), "", true);
    }
    let nested = table(&minimum("1", ""), "").replace("<w:p/>", &table("", ""));
    import(&nested, "", true);
    let wrapped = format!("<w:sdt>{}</w:sdt>", table(&minimum("1", ""), ""));
    import(&wrapped, "", true);
}
#[test]
fn source_vertical_properties_bind_multiple_table_and_cell_text_order() {
    let body = format!(
        "{}<w:p/>{}<w:p/>{}",
        table("", ""),
        table(&minimum("240", ""), &alignment("center")),
        table(&minimum("480", ""), &alignment("bottom")).replace("Cell &amp; text", "Other")
    );
    let doc = import(&body, "", false).unwrap();
    let tables: Vec<_> = doc.content.iter().filter(|n| n.kind == "table").collect();
    assert!(!tables[0].content[0].attrs.contains_key("rowMinHeightTwips"));
    assert_eq!(tables[1].content[0].attrs["rowMinHeightTwips"], 240);
    assert_eq!(tables[2].content[0].attrs["rowMinHeightTwips"], 480);
}
#[test]
fn model_null_types_bounds_and_placement_are_strict() {
    for value in [
        Value::Null,
        json!(true),
        json!("1"),
        json!(-1),
        json!(1.0),
        json!(0.5),
        json!(31681),
        json!({}),
        json!([]),
    ] {
        assert!(document::validate(&model(Some(value), None)).is_err());
    }
    for value in [
        Value::Null,
        json!(1),
        json!(true),
        json!("middle"),
        json!("Top"),
        json!([]),
        json!({}),
    ] {
        assert!(document::validate(&model(None, Some(value))).is_err());
    }
    for kind in ["doc", "table", "paragraph", "tableCell"] {
        let mut doc = model(None, None);
        let node = match kind {
            "doc" => &mut doc,
            "table" => &mut doc.content[0],
            "tableCell" => &mut doc.content[0].content[0].content[0],
            _ => &mut doc.content[0].content[0].content[0].content[0],
        };
        node.attrs.insert("rowMinHeightTwips".into(), json!(1));
        assert!(document::validate(&doc).is_err());
    }
    for height in [0, 1, 13958] {
        for align in ["top", "center", "bottom"] {
            document::validate(&model(Some(json!(height)), Some(json!(align)))).unwrap();
        }
    }
}
#[test]
fn model_minimum_height_tracks_page_body_and_prevents_nested_geometry() {
    let mut doc = model(Some(json!(13958)), Some(json!("center")));
    document::validate(&doc).unwrap();
    doc.content[0].content[0]
        .attrs
        .insert("rowMinHeightTwips".into(), json!(13959));
    assert!(document::validate(&doc).is_err());
    doc.attrs.insert("pageGeometry".into(),json!({"widthTwips":12240,"heightTwips":15840,"marginLeftTwips":1440,"marginRightTwips":1440,"marginTopTwips":1440,"marginBottomTwips":1440}));
    for (height, valid) in [(12960, true), (12961, false)] {
        doc.content[0].content[0]
            .attrs
            .insert("rowMinHeightTwips".into(), json!(height));
        assert_eq!(document::validate(&doc).is_ok(), valid);
    }
    let inner = model(Some(json!(1)), None).content.remove(0);
    let mut nested = model(None, None);
    nested.content[0].content[0].content[0].content.push(inner);
    assert!(document::validate(&nested).is_err());
}
#[test]
fn model_export_draft_trusted_reopen_and_default_reset_are_exact() {
    let dir = tempfile::tempdir().unwrap();
    let draft = dir.path().join("draft.json");
    for (i, (height, align)) in [
        (None, None),
        (Some(json!(0)), Some(json!("top"))),
        (Some(json!(1340)), Some(json!("bottom"))),
    ]
    .into_iter()
    .enumerate()
    {
        let doc = model(height, align);
        document::save_draft(&doc, &draft).unwrap();
        assert_eq!(
            serde_json::to_value(document::load_draft(&draft).unwrap().unwrap()).unwrap(),
            serde_json::to_value(&doc).unwrap()
        );
        let path = dir.path().join(format!("{i}.docx"));
        document::export_docx(&doc, &path).unwrap();
        assert_eq!(
            serde_json::to_value(document::load_authored_docx(&path).unwrap().unwrap()).unwrap(),
            serde_json::to_value(&doc).unwrap()
        );
        let bytes = fs::read(&path).unwrap();
        assert!(document::export_docx(&doc, &path).is_err());
        assert_eq!(fs::read(path).unwrap(), bytes);
        let xml = String::from_utf8(unzip(&bytes)["word/document.xml"].clone()).unwrap();
        assert_eq!(xml.contains("<w:trHeight"), i > 0);
        assert_eq!(xml.contains("<w:vAlign"), i > 0);
        if i > 0 {
            assert!(xml.contains("w:hRule=\"atLeast\""));
        }
    }
    let mut reset = model(Some(json!(1340)), Some(json!("center")));
    reset.content[0].content[0].attrs.clear();
    reset.content[0].content[0].content[0].attrs.clear();
    let path = dir.path().join("reset.docx");
    document::export_docx(&reset, &path).unwrap();
    assert_eq!(
        fs::read(path).unwrap(),
        fs::read(dir.path().join("0.docx")).unwrap()
    );
}
#[test]
fn invalid_vertical_candidate_preserves_previous_draft_and_publishes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let draft = dir.path().join("draft.json");
    let output = dir.path().join("invalid.docx");
    let good = model(Some(json!(1340)), Some(json!("bottom")));
    document::save_draft(&good, &draft).unwrap();
    let original = fs::read(&draft).unwrap();
    let bad = model(Some(json!(13959)), None);
    assert!(document::save_draft(&bad, &draft).is_err());
    assert_eq!(fs::read(draft).unwrap(), original);
    assert!(document::export_docx(&bad, &output).is_err());
    assert!(!output.exists());
}
#[test]
fn minimum_or_alignment_xml_tampering_invalidates_trusted_model() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.docx");
    document::export_docx(&model(Some(json!(1340)), Some(json!("bottom"))), &source).unwrap();
    let bytes = fs::read(&source).unwrap();
    for (i, (from, to)) in [
        ("1340", "1341"),
        ("w:val=\"bottom\"", "w:val=\"top\""),
        ("atLeast", "exact"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut parts = unzip(&bytes);
        let main = String::from_utf8(parts["word/document.xml"].clone())
            .unwrap()
            .replace(from, to);
        parts.insert("word/document.xml".into(), main.into_bytes());
        let path = dir.path().join(format!("tamper-{i}.docx"));
        write(&path, &parts);
        assert!(document::load_authored_docx(&path).is_err());
    }
}

#[test]
fn vertical_layout_requires_default_or_proven_zero_physical_vertical_padding() {
    let base = table(&minimum("1340", ""), &alignment("bottom"));
    for (padding, valid) in [
        (
            r#"<w:top w:w="0" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/>"#,
            true,
        ),
        (r#"<w:top w:w="1" w:type="dxa"/>"#, false),
        (r#"<w:bottom w:w="0"/>"#, false),
        (r#"<w:top w:w="0" w:type="pct"/>"#, false),
        (
            r#"<w:top w:w="0" w:type="dxa"/><w:top w:w="0" w:type="dxa"/>"#,
            false,
        ),
        (
            r#"<w:wrapper><w:top w:w="0" w:type="dxa"/></w:wrapper>"#,
            false,
        ),
        (r#"<w:before w:w="0" w:type="dxa"/>"#, false),
    ] {
        let direct = base.replace(
            "<w:tblPr/>",
            &format!("<w:tblPr><w:tblCellMar>{padding}</w:tblCellMar></w:tblPr>"),
        );
        import(&direct, "", !valid);
        let cell = base.replace("<w:tcPr>", &format!("<w:tcPr><w:tcMar>{padding}</w:tcMar>"));
        import(&cell, "", !valid);
        let selected = base.replace(
            "<w:tblPr/>",
            r#"<w:tblPr><w:tblStyle w:val="Leaf"/></w:tblPr>"#,
        );
        for conditional in [false, true] {
            let margin = format!("<w:tblPr><w:tblCellMar>{padding}</w:tblCellMar></w:tblPr>");
            let margin = if conditional {
                format!(r#"<w:tblStylePr w:type="firstRow">{margin}</w:tblStylePr>"#)
            } else {
                margin
            };
            let styles = format!(
                r#"<w:style w:type="table" w:styleId="Leaf"><w:basedOn w:val="Base"/></w:style><w:style w:type="table" w:styleId="Base">{margin}</w:style>"#
            );
            import(&selected, &styles, !valid);
        }
    }
    // A direct zero does not justify dropping an unsupported base declaration.
    let selected=base.replace("<w:tblPr/>",r#"<w:tblPr><w:tblStyle w:val="Base"/><w:tblCellMar><w:top w:w="0" w:type="dxa"/></w:tblCellMar></w:tblPr>"#);
    import(
        &selected,
        r#"<w:style w:type="table" w:styleId="Base"><w:tblPr><w:tblCellMar><w:top w:w="120" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style>"#,
        true,
    );
    // No new vertical declaration keeps the previously warned normalization.
    let old = table("", "").replace(
        "<w:tblPr/>",
        r#"<w:tblPr><w:tblCellMar><w:top w:w="120" w:type="dxa"/></w:tblCellMar></w:tblPr>"#,
    );
    import(&old, "", false);
}

#[test]
fn shared_table_audit_rejects_duplicate_zero_vertical_margin_containers() {
    let base = table(&minimum("1340", ""), &alignment("bottom"));
    let zero = r#"<w:top w:w="0" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/>"#;
    let table_margin = format!("<w:tblCellMar>{zero}</w:tblCellMar>");
    let cell_margin = format!("<w:tcMar>{zero}</w:tcMar>");
    import(
        &base.replace(
            "<w:tblPr/>",
            &format!("<w:tblPr>{table_margin}{table_margin}</w:tblPr>"),
        ),
        "",
        true,
    );
    import(
        &base.replace("<w:tcPr>", &format!("<w:tcPr>{cell_margin}{cell_margin}")),
        "",
        true,
    );
    let selected = base.replace(
        "<w:tblPr/>",
        r#"<w:tblPr><w:tblStyle w:val="Active"/></w:tblPr>"#,
    );
    for properties in [
        format!("<w:tblPr>{table_margin}{table_margin}</w:tblPr>"),
        format!("<w:tcPr>{cell_margin}{cell_margin}</w:tcPr>"),
    ] {
        for conditional in [false, true] {
            let properties = if conditional {
                format!(r#"<w:tblStylePr w:type="firstRow">{properties}</w:tblStylePr>"#)
            } else {
                properties.clone()
            };
            let styles =
                format!(r#"<w:style w:type="table" w:styleId="Active">{properties}</w:style>"#);
            import(&selected, &styles, true);
        }
    }
}
