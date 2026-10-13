//! Conservative preflight for structural DOCX import. This is not a proof of
//! fidelity: it inventories features that the current authoring model lacks.
use anyhow::{Result, ensure};
use quick_xml::{Reader, events::Event};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, io::Read, path::Path};

pub(crate) const HIDDEN_TEXT_RISK: &str = "Hidden text cannot be preserved in an editable copy";

pub(crate) const PARAGRAPH_SPACING_RISK: &str =
    "Unsupported paragraph spacing cannot be preserved in an editable copy";

// Audit declarations before conversion can clamp negative values, default an
// unknown line rule, or discard automatic/contextual/grid behavior. Only used
// style chains are inspected, so unrelated template styles do not block editing.
fn has_unsupported_paragraph_spacing(data: &[u8], used: &BTreeSet<String>) -> Result<bool> {
    let mut reader = Reader::from_reader(data);
    let mut stack: Vec<String> = Vec::new();
    let mut unsupported = false;
    let mut unused_style_depth = None;
    loop {
        let event = reader.read_event()?;
        let start = matches!(&event, Event::Start(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let local = e.local_name();
                let tag = local.as_ref();
                if tag == "style" && start {
                    let mut id = None;
                    for attribute in e.attributes() {
                        let attribute = attribute?;
                        if attribute.key.as_ref() == "w:styleId" {
                            id = Some(
                                attribute
                                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)?
                                    .into_owned(),
                            );
                        }
                    }
                    if id.as_ref().is_some_and(|id| !used.contains(id)) {
                        unused_style_depth = Some(stack.len());
                    }
                }
                if unused_style_depth.is_some() {
                    if start {
                        stack.push(tag.to_owned());
                    }
                    continue;
                }
                if tag == "spacing" && stack.last().is_some_and(|tag| tag == "pPr") {
                    let mut has_line = false;
                    let mut has_rule = false;
                    for attribute in e.attributes() {
                        let attribute = attribute?;
                        let local = attribute.key.local_name();
                        let key = local.as_ref();
                        let value =
                            attribute.normalized_value(quick_xml::XmlVersion::Implicit1_0)?;
                        // The vendor reader uses literal w: attributes. Do not
                        // claim preservation of an equivalent unconsumed prefix.
                        if matches!(
                            key,
                            "before"
                                | "after"
                                | "line"
                                | "lineRule"
                                | "beforeAutospacing"
                                | "afterAutospacing"
                                | "beforeLines"
                                | "afterLines"
                        ) && attribute.key.as_ref() != format!("w:{key}")
                        {
                            unsupported = true;
                        }
                        match key {
                            "before" | "after" | "line" => {
                                has_line |= key == "line";
                                let (min, max) = if key == "line" {
                                    (120, 2400)
                                } else {
                                    (0, 31680)
                                };
                                let valid = !value.is_empty()
                                    && value.bytes().all(|byte| byte.is_ascii_digit())
                                    && value
                                        .parse::<u32>()
                                        .is_ok_and(|value| (min..=max).contains(&value));
                                unsupported |= !valid;
                            }
                            "lineRule" => {
                                has_rule = true;
                                unsupported |= value != "auto";
                            }
                            "beforeAutospacing" | "afterAutospacing" => {
                                unsupported |= !matches!(value.as_ref(), "0" | "false" | "off")
                            }
                            "beforeLines" | "afterLines" => unsupported = true,
                            _ => {}
                        }
                    }
                    unsupported |= has_rule && !has_line;
                } else if tag == "contextualSpacing" {
                    unsupported |= may_hide_text(&e)?;
                } else if tag == "docGrid" {
                    let mut explicit_default = true;
                    for attribute in e.attributes() {
                        let attribute = attribute?;
                        if attribute.key.local_name().as_ref() == "type" {
                            explicit_default = attribute.key.as_ref() == "w:type"
                                && attribute
                                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)?
                                    == "default";
                        }
                    }
                    unsupported |= !explicit_default;
                }
                if start {
                    stack.push(tag.to_owned());
                }
            }
            Event::End(_) => {
                stack.pop();
                if unused_style_depth == Some(stack.len()) {
                    unused_style_depth = None;
                }
            }
            Event::DocType(_) => anyhow::bail!("DTD is not supported in DOCX XML"),
            Event::Eof => return Ok(unsupported),
            _ => {}
        }
    }
}

// Use the same parsed style identities and references as the converter. Raw
// spacing is still audited below, before lossy conversion; foreign attributes
// cannot shadow the literal w: keys used by the parser.
fn w_attribute(e: &quick_xml::events::BytesStart<'_>, name: &str) -> Result<Option<String>> {
    for attribute in e.attributes() {
        let attribute = attribute?;
        if attribute.key.as_ref() == format!("w:{name}") {
            return Ok(Some(
                attribute
                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

type NumberingUsage = BTreeSet<(u32, u8)>;
fn spacing_usage(main: &[u8], styles: &[u8]) -> Result<(BTreeSet<String>, NumberingUsage)> {
    use office_oxide::docx::{ParagraphProperties, formatting::NumberingRef};
    let sheet = office_oxide::docx::styles::StyleSheet::parse(styles)?;
    let mut used = BTreeSet::new();
    let mut numbers = BTreeSet::new();
    let mut paragraph: Option<ParagraphProperties> = None;
    let mut stack: Vec<String> = Vec::new();
    let mut tables = Vec::new();
    let mut default_tables = BTreeSet::new();
    let mut reader = quick_xml::NsReader::from_reader(main);
    loop {
        let (namespace, event) = reader.read_resolved_event()?;
        let wml = matches!(namespace, quick_xml::name::ResolveResult::Bound(ns)
            if matches!(ns.as_ref(), "http://schemas.openxmlformats.org/wordprocessingml/2006/main" | "http://purl.oclc.org/ooxml/wordprocessingml/main"));
        let empty = matches!(&event, Event::Empty(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let local = e.local_name();
                ensure!(
                    wml || !matches!(
                        local.as_ref(),
                        "p" | "pPr"
                            | "pStyle"
                            | "tbl"
                            | "tblPr"
                            | "tblStyle"
                            | "numPr"
                            | "numId"
                            | "ilvl"
                    ),
                    "{PARAGRAPH_SPACING_RISK}"
                );
                let tag = if wml { local.as_ref() } else { "" };
                let direct = stack.ends_with(&["p".into(), "pPr".into()]);
                let numbering = stack.ends_with(&["p".into(), "pPr".into(), "numPr".into()]);
                match tag {
                    "p" => {
                        paragraph = Some(ParagraphProperties::default());
                        if empty {
                            if let Some(id) = &sheet.default_paragraph_style_id {
                                used.insert(id.clone());
                            }
                            if let Some(nr) =
                                sheet.effective_paragraph_properties(None).numbering_ref
                            {
                                numbers.insert((nr.num_id, nr.ilvl));
                            }
                            paragraph = None;
                        }
                    }
                    "pStyle" if direct => {
                        if let Some(p) = &mut paragraph {
                            p.style_id = w_attribute(&e, "val")?;
                        }
                    }
                    "tbl" => tables.push(false),
                    "tblStyle" if stack.ends_with(&["tbl".into(), "tblPr".into()]) => {
                        if let Some(id) = w_attribute(&e, "val")? {
                            used.insert(id);
                            if let Some(explicit) = tables.last_mut() {
                                *explicit = true;
                            }
                        }
                    }
                    "numPr" if direct => {
                        if let Some(p) = &mut paragraph {
                            p.numbering_ref = Some(NumberingRef { num_id: 0, ilvl: 0 });
                        }
                    }
                    "numId" if numbering => {
                        if let Some(nr) = paragraph.as_mut().and_then(|p| p.numbering_ref.as_mut())
                        {
                            nr.num_id = w_attribute(&e, "val")?
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0);
                        }
                    }
                    "ilvl" if numbering => {
                        if let Some(nr) = paragraph.as_mut().and_then(|p| p.numbering_ref.as_mut())
                        {
                            nr.ilvl = w_attribute(&e, "val")?
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0);
                        }
                    }
                    _ => {}
                }
                if !empty {
                    stack.push(tag.to_owned());
                }
            }
            Event::End(e) => {
                stack.pop();
                if wml && e.local_name().as_ref() == "tbl" && tables.pop() == Some(false) {
                    for style in sheet.styles.values().filter(|s| {
                        s.is_default && s.style_type == office_oxide::docx::styles::StyleType::Table
                    }) {
                        default_tables.insert(style.style_id.clone());
                        used.insert(style.style_id.clone());
                    }
                }
                if !wml || e.local_name().as_ref() != "p" {
                    continue;
                }
                if let Some(p) = paragraph.take() {
                    if let Some(id) = p
                        .style_id
                        .as_ref()
                        .or(sheet.default_paragraph_style_id.as_ref())
                    {
                        used.insert(id.clone());
                    }
                    if let Some(nr) = sheet.effective_paragraph_properties(Some(&p)).numbering_ref {
                        numbers.insert((nr.num_id, nr.ilvl));
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    // The converter does not select an implicit default table style. Refuse
    // its spacing rather than treating those cell paragraphs as unformatted.
    let mut default_pending: Vec<_> = default_tables.iter().cloned().collect();
    while let Some(id) = default_pending.pop() {
        if let Some(style) = sheet.styles.get(&id) {
            ensure!(
                style
                    .paragraph_properties
                    .iter()
                    .chain(
                        style
                            .table_conditionals
                            .iter()
                            .filter_map(|c| c.paragraph_properties.as_ref())
                    )
                    .all(|p| p.spacing.is_none()),
                "{PARAGRAPH_SPACING_RISK}"
            );
            if let Some(base) = &style.based_on
                && default_tables.insert(base.clone())
            {
                default_pending.push(base.clone());
            }
        }
    }
    let mut pending: Vec<_> = used.iter().cloned().collect();
    while let Some(id) = pending.pop() {
        if let Some(style) = sheet.styles.get(&id) {
            if let Some(base) = &style.based_on
                && used.insert(base.clone())
            {
                pending.push(base.clone());
            }
            // A table layer may supply numbering. Including its declared
            // references is conservative when a paragraph overrides them.
            if style.style_type == office_oxide::docx::styles::StyleType::Table {
                for props in style.paragraph_properties.iter().chain(
                    style
                        .table_conditionals
                        .iter()
                        .filter_map(|c| c.paragraph_properties.as_ref()),
                ) {
                    if let Some(nr) = &props.numbering_ref {
                        numbers.insert((nr.num_id, nr.ilvl));
                    }
                }
            }
        }
    }
    numbers.retain(|(id, _)| *id != 0);
    Ok((used, numbers))
}

fn unsupported_numbering_spacing(data: &[u8], used: &NumberingUsage) -> Result<bool> {
    let definitions = office_oxide::docx::numbering::NumberingDefinitions::parse(data)?;
    let abstracts: BTreeSet<_> = used
        .iter()
        .filter_map(|(id, level)| {
            definitions
                .instances
                .get(id)
                .map(|instance| (instance.abstract_num_id, *level))
        })
        .collect();
    let mut reader = Reader::from_reader(data);
    let mut scope = None;
    let mut level = None;
    let mut in_paragraph_properties = false;
    loop {
        match reader.read_event()? {
            Event::Start(e) | Event::Empty(e) => {
                let local = e.local_name();
                let tag = local.as_ref();
                match tag {
                    "abstractNum" | "num" => {
                        scope = w_attribute(
                            &e,
                            if tag == "num" {
                                "numId"
                            } else {
                                "abstractNumId"
                            },
                        )?
                        .map(|v| v.parse::<u32>().unwrap_or(0))
                        .map(|id| (tag == "num", id))
                    }
                    "lvl" => {
                        level = Some(
                            w_attribute(&e, "ilvl")?
                                .and_then(|v| v.parse::<u8>().ok())
                                .unwrap_or(0),
                        )
                    }
                    "pPr" => in_paragraph_properties = true,
                    "spacing" | "contextualSpacing" if in_paragraph_properties => {
                        let active = scope.zip(level).is_some_and(|((instance, id), level)| {
                            if instance {
                                used.contains(&(id, level))
                            } else {
                                abstracts.contains(&(id, level))
                            }
                        });
                        // Numbering-level paragraph spacing is not retained by
                        // the vendor IR. Never claim those declarations survive.
                        if active && (tag == "spacing" || may_hide_text(&e)?) {
                            return Ok(true);
                        }
                    }
                    _ => {}
                }
            }
            Event::End(e) => match e.local_name().as_ref() {
                "num" | "abstractNum" => scope = None,
                "lvl" => level = None,
                "pPr" => in_paragraph_properties = false,
                _ => {}
            },
            Event::Eof => return Ok(false),
            _ => {}
        }
    }
}

// The current parser recognizes toggle attributes by the literal `w:val`
// spelling. A different prefix (even bound to the same namespace) would be
// treated as missing/true downstream, so it must not bypass this guard.
// Scan declarations, not effective styles: an unused or overridden hidden
// style deliberately remains read-only until the model can retain its text.
fn may_hide_text(element: &quick_xml::events::BytesStart<'_>) -> Result<bool> {
    let mut disabled = false;
    let mut values = 0;
    for attribute in element.attributes() {
        let attribute = attribute?;
        if attribute.key.local_name().as_ref() == "val" {
            values += 1;
            let value = attribute.normalized_value(quick_xml::XmlVersion::Implicit1_0)?;
            disabled = attribute.key.as_ref() == "w:val"
                && matches!(value.as_ref(), "0" | "false" | "off");
        }
    }
    Ok(values != 1 || !disabled)
}

fn has_hidden_text_declaration(data: &[u8]) -> Result<bool> {
    let mut reader = Reader::from_reader(data);
    let mut hidden = false;
    loop {
        match reader.read_event()? {
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == "vanish" => {
                hidden |= may_hide_text(&e)?;
            }
            Event::DocType(_) => anyhow::bail!("DTD is not supported in DOCX XML"),
            Event::Eof => return Ok(hidden),
            _ => {}
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CompatibilityReport {
    pub paragraphs: usize,
    pub tables: usize,
    pub drawings: usize,
    pub risks: BTreeSet<String>,
    pub fidelity_verified: bool,
}

pub fn audit_docx(path: &Path) -> Result<CompatibilityReport> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 64 * 1024 * 1024, "Input exceeds 64 MiB");
    audit_docx_bytes(&bytes)
}

pub(crate) fn audit_docx_bytes(bytes: &[u8]) -> Result<CompatibilityReport> {
    Ok(audit_docx_bytes_with_geometry(bytes)?.compatibility)
}

pub(crate) struct DocxGeometryAudit {
    pub compatibility: CompatibilityReport,
    pub tables: Vec<crate::table_widths::TableGeometry>,
    pub images: Vec<crate::image_dimensions::ImageGeometry>,
    pub page: Option<crate::document::PageGeometry>,
}

pub(crate) fn audit_docx_bytes_with_geometry(bytes: &[u8]) -> Result<DocxGeometryAudit> {
    let parts = crate::editing::parts(bytes)?;
    ensure!(
        parts.contains_key("word/document.xml"),
        "Missing DOCX main document"
    );
    let mut report = CompatibilityReport {
        paragraphs: 0,
        tables: 0,
        drawings: 0,
        risks: BTreeSet::from([
            "Page layout, fonts and pagination require visual verification".into(),
        ]),
        fidelity_verified: false,
    };
    let mut links = std::collections::BTreeMap::new();
    let mut normalized_main = None;
    let mut table_widths = Vec::new();
    let mut page = None;
    // Inspect exactly the main/style parts that the downstream parser will
    // consume, including OPC relationship resolution and ZIP name handling.
    // Minimal inventory-only ZIPs may lack content types; import itself still
    // rejects those through the package reader.
    if parts
        .keys()
        .any(|name| name.eq_ignore_ascii_case("[Content_Types].xml"))
    {
        use office_oxide::core::{opc::OpcReader, relationships::rel_types};
        let mut package = OpcReader::new(std::io::Cursor::new(bytes))?;
        let main = package.main_document_part()?;
        ensure!(
            main.as_str() == "/word/document.xml",
            "Only the standard word/document.xml main part is supported for editing"
        );
        let main_xml = package.read_part(&main)?;
        if has_hidden_text_declaration(&main_xml)? {
            report.risks.insert(HIDDEN_TEXT_RISK.into());
        }
        if has_unsupported_paragraph_spacing(&main_xml, &BTreeSet::new())? {
            report.risks.insert(PARAGRAPH_SPACING_RISK.into());
        }
        normalized_main = Some(main_xml);
        let relationships = package.read_rels_for(&main)?;
        // Side stories are identified by OPC relationship type, never by
        // target filename. The shared reader normalizes Strict type URIs.
        // Even unused, missing or external targets must remain read-only:
        // the editable model cannot retain their declarations or content.
        for relationship in relationships.all() {
            if matches!(
                relationship.rel_type.as_str(),
                rel_types::HEADER
                    | rel_types::FOOTER
                    | rel_types::FOOTNOTES
                    | rel_types::ENDNOTES
                    | rel_types::COMMENTS
            ) {
                report.risks.insert(format!(
                    "Additional document content: relationship {} ({})",
                    relationship.id, relationship.rel_type
                ));
            }
        }
        let mut spacing_parts = std::collections::BTreeMap::new();
        let mut unreadable_settings = false;
        for kind in [rel_types::STYLES, rel_types::NUMBERING, rel_types::SETTINGS] {
            if let Some(relationship) = relationships.first_by_type(kind) {
                if kind == rel_types::SETTINGS {
                    match main
                        .resolve_relative(&relationship.target)
                        .and_then(|target| package.read_part(&target))
                    {
                        Ok(data) => {
                            spacing_parts.insert(kind, data);
                        }
                        Err(_) => {
                            unreadable_settings = true;
                        }
                    }
                } else {
                    let target = main.resolve_relative(&relationship.target)?;
                    spacing_parts.insert(kind, package.read_part(&target)?);
                }
            }
        }
        let empty_styles = br#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#;
        let styles = spacing_parts
            .get(rel_types::STYLES)
            .map(Vec::as_slice)
            .unwrap_or(empty_styles);
        let settings_relationships = relationships
            .all()
            .iter()
            .filter(|r| r.rel_type == rel_types::SETTINGS)
            .count();
        // Settings identity is part of both indentation and page proofs.
        // A declared unreadable target must never be mistaken for absence.
        let ambiguous_settings = b"<ambiguous-settings/>";
        let table_settings = if unreadable_settings
            || settings_relationships > 1
            || relationships.all().iter().any(|r| {
                r.rel_type == rel_types::SETTINGS
                    && r.target_mode == office_oxide::core::relationships::TargetMode::External
            }) {
            Some(ambiguous_settings.as_slice())
        } else {
            spacing_parts.get(rel_types::SETTINGS).map(Vec::as_slice)
        };
        let table_audit = crate::table_widths::analyze(
            normalized_main.as_deref().expect("main read"),
            styles,
            table_settings,
        )?;
        table_widths = table_audit.tables;
        report.risks.extend(table_audit.risks);
        let (used, numbers) =
            spacing_usage(normalized_main.as_deref().expect("main read"), styles)?;
        let rels_part = office_oxide::core::opc::PartName::new(&main.rels_path())?;
        let page_relationships = if package.has_part(&rels_part) {
            crate::page_geometry::validate_relationships(&package.read_part(&rels_part)?)
        } else {
            Ok(())
        };
        let page_audit = page_relationships.and_then(|()| {
            crate::page_geometry::analyze(
                normalized_main.as_deref().expect("main read"),
                table_settings,
                styles,
                &used,
            )
        });
        match page_audit {
            Ok(audit) => {
                page = audit.geometry;
                report.risks.extend(audit.risks);
            }
            Err(_) => {
                report
                    .risks
                    .insert(crate::page_geometry::PAGE_GEOMETRY_RISK.into());
            }
        }
        if has_hidden_text_declaration(styles)? {
            report.risks.insert(HIDDEN_TEXT_RISK.into());
        }
        if has_unsupported_paragraph_spacing(styles, &used)? {
            report.risks.insert(PARAGRAPH_SPACING_RISK.into());
        }
        if let Some(data) = spacing_parts.get(rel_types::NUMBERING)
            && unsupported_numbering_spacing(data, &numbers)?
        {
            report.risks.insert(PARAGRAPH_SPACING_RISK.into());
        }
        if let Some(data) = spacing_parts.get(rel_types::SETTINGS)
            && has_unsupported_paragraph_spacing(data, &BTreeSet::new())?
        {
            report.risks.insert(PARAGRAPH_SPACING_RISK.into());
        }
    }
    if normalized_main.is_none() {
        let empty_styles = br#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#;
        let table_audit = crate::table_widths::analyze(
            &parts["word/document.xml"],
            parts
                .get("word/styles.xml")
                .map(Vec::as_slice)
                .unwrap_or(empty_styles),
            parts.get("word/settings.xml").map(Vec::as_slice),
        )?;
        table_widths = table_audit.tables;
        report.risks.extend(table_audit.risks);
        // Inventory-only archives cannot prove OPC settings/style identities.
        // They never advertise preserved page geometry.
        report
            .risks
            .insert(crate::page_geometry::PAGE_GEOMETRY_NORMALIZED_RISK.into());
    }
    if normalized_main.is_none()
        && has_unsupported_paragraph_spacing(&parts["word/document.xml"], &BTreeSet::new())?
    {
        report.risks.insert(PARAGRAPH_SPACING_RISK.into());
    }
    if let Some(data) = parts.get("word/_rels/document.xml.rels") {
        let mut reader = Reader::from_reader(data.as_slice());
        loop {
            match reader.read_event()? {
                Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == "Relationship" => {
                    let mut attrs = std::collections::BTreeMap::new();
                    for attr in e.attributes() {
                        let attr = attr?;
                        attrs.insert(
                            attr.key.as_ref().to_owned(),
                            attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)?
                                .into_owned(),
                        );
                    }
                    if let Some(id) = attrs.get("Id") {
                        ensure!(!links.contains_key(id), "Duplicate DOCX relationship ID");
                        let supported = attrs.get("Type").is_some_and(|s| s == "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink")
                            && attrs.get("TargetMode").is_some_and(|s| s == "External")
                            && attrs.get("Target").is_some_and(|s| crate::document::supported_link(s));
                        links.insert(id.clone(), supported);
                    }
                }
                Event::DocType(_) => anyhow::bail!("DTD is not supported in DOCX XML"),
                Event::Eof => break,
                _ => {}
            }
        }
    }
    let mut has_theme_color_reference = false;
    let mut unsupported_theme_colors = false;
    for (name, data) in parts {
        if name.starts_with("word/")
            && (name.contains("header")
                || name.contains("footer")
                || name.contains("footnotes")
                || name.contains("endnotes")
                || name.contains("comments"))
        {
            report
                .risks
                .insert(format!("Additional document content: {name}"));
        }
        if name.starts_with("word/embeddings/") || name.ends_with("vbaProject.bin") {
            report
                .risks
                .insert(format!("Embedded object or executable content: {name}"));
        }
        if !name.starts_with("word/") || !name.ends_with(".xml") {
            continue;
        }
        let data = if name == "word/document.xml" {
            normalized_main.as_deref().unwrap_or(&data)
        } else {
            data.as_slice()
        };
        let mut reader = Reader::from_reader(data);
        let mut in_color_scheme = false;
        loop {
            match reader.read_event()? {
                Event::Start(e) | Event::Empty(e) => {
                    let local = e.local_name();
                    let tag = local.as_ref();
                    if tag == "vanish" && may_hide_text(&e)? {
                        report.risks.insert(HIDDEN_TEXT_RISK.into());
                    }
                    if tag == "clrScheme" {
                        in_color_scheme = true;
                    } else if in_color_scheme
                        && !matches!(
                            tag,
                            "dk1"
                                | "dk2"
                                | "lt1"
                                | "lt2"
                                | "accent1"
                                | "accent2"
                                | "accent3"
                                | "accent4"
                                | "accent5"
                                | "accent6"
                                | "hlink"
                                | "folHlink"
                                | "srgbClr"
                                | "sysClr"
                        )
                    {
                        // The theme reader does not evaluate nested DrawingML
                        // transforms or other color sources. Do not pretend the
                        // untransformed base is the document's resolved color.
                        unsupported_theme_colors = true;
                    }
                    if tag == "clrSchemeMapping" {
                        // The converter has no settings mapping context. Permit
                        // standard defaults, but never silently use a different
                        // theme slot for an explicitly remapped document.
                        for attribute in e.attributes() {
                            let attribute = attribute?;
                            let key = attribute.key.local_name();
                            let default = match key.as_ref() {
                                "bg1" => Some("light1"),
                                "t1" => Some("dark1"),
                                "bg2" => Some("light2"),
                                "t2" => Some("dark2"),
                                "accent1" | "accent2" | "accent3" | "accent4" | "accent5"
                                | "accent6" | "hyperlink" | "followedHyperlink" => {
                                    Some(key.as_ref())
                                }
                                _ => None,
                            };
                            if let Some(default) = default
                                && attribute.normalized_value(quick_xml::XmlVersion::Implicit1_0)?
                                    != default
                            {
                                report
                                    .risks
                                    .insert("Custom theme color mapping is unsupported".into());
                            }
                        }
                    }
                    if tag == "color" {
                        for attribute in e.attributes() {
                            let attribute = attribute?;
                            let value =
                                attribute.normalized_value(quick_xml::XmlVersion::Implicit1_0)?;
                            if attribute.key.local_name().as_ref() == "themeColor"
                                && value != "none"
                            {
                                has_theme_color_reference = true;
                            }
                            if attribute.key.local_name().as_ref() == "themeColor"
                                || (attribute.key.local_name().as_ref() == "val" && value == "auto")
                            {
                                report.risks.insert(
                                    "Theme and automatic text colors normalized to fixed RGB or editor defaults".into(),
                                );
                            }
                        }
                    }
                    if tag == "hyperlink" {
                        let mut id = None;
                        let mut anchor = false;
                        for attr in e.attributes() {
                            let attr = attr?;
                            match attr.key.local_name().as_ref() {
                                "id" => {
                                    id = Some(
                                        attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)?
                                            .into_owned(),
                                    )
                                }
                                "anchor" => anchor = true,
                                _ => {}
                            }
                        }
                        if anchor || id.as_ref().is_none_or(|id| links.get(id) != Some(&true)) {
                            report
                                .risks
                                .insert("Unsupported or unresolved hyperlink target".into());
                        }
                    }
                    if name == "word/document.xml" {
                        // References also fail closed when their relationship
                        // is absent, external, malformed or has the wrong type.
                        // The body parser otherwise silently skips some of them.
                        if matches!(
                            tag,
                            "headerReference"
                                | "footerReference"
                                | "footnoteReference"
                                | "endnoteReference"
                                | "commentReference"
                                | "commentRangeStart"
                                | "commentRangeEnd"
                        ) {
                            report.risks.insert(format!(
                                "Additional document content: {tag} in main document"
                            ));
                        }
                        match tag {
                            "p" => report.paragraphs += 1,
                            "tbl" => report.tables += 1,
                            "drawing" | "pict" => report.drawings += 1,
                            _ => {}
                        }
                    }
                    let risk = match tag {
                        "ins" | "del" | "moveFrom" | "moveTo" | "pPrChange" | "rPrChange" => {
                            Some("Tracked changes")
                        }
                        "fldChar" | "fldSimple" | "instrText" => Some("Fields and generated text"),
                        "gridSpan" | "vMerge" | "hMerge" => Some("Merged table cells"),
                        "hyperlink" => Some("Hyperlinks"),
                        "anchor" | "pict" | "txbxContent" => {
                            Some("Floating objects, legacy drawings or text boxes")
                        }
                        "oMath" | "oMathPara" => Some("Mathematical equations"),
                        "sdt" | "altChunk" | "object" => {
                            Some("Content controls or embedded content")
                        }
                        "color" | "sz" | "rFonts" | "shd" | "spacing" | "ind" | "jc" => {
                            Some("Typography, spacing or alignment outside the basic editor model")
                        }
                        _ => None,
                    };
                    if let Some(risk) = risk {
                        report.risks.insert(risk.into());
                    }
                }
                Event::End(e) if e.local_name().as_ref() == "clrScheme" => in_color_scheme = false,
                Event::DocType(_) => anyhow::bail!("DTD is not supported in DOCX XML"),
                Event::Eof => break,
                _ => {}
            }
        }
    }
    if has_theme_color_reference && unsupported_theme_colors {
        report
            .risks
            .insert("Unsupported theme color scheme content".into());
    }
    let images = match crate::image_dimensions::analyze(bytes) {
        Ok(images) => images,
        Err(_) => {
            report
                .risks
                .insert(crate::image_dimensions::IMAGE_GEOMETRY_RISK.into());
            Vec::new()
        }
    };
    Ok(DocxGeometryAudit {
        compatibility: report,
        tables: table_widths,
        images,
        page,
    })
}
