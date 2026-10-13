//! Conservative preflight for structural DOCX import. This is not a proof of
//! fidelity: it inventories features that the current authoring model lacks.
use anyhow::{Result, ensure};
use quick_xml::{Reader, events::Event};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, io::Read, path::Path};

pub(crate) const HIDDEN_TEXT_RISK: &str = "Hidden text cannot be preserved in an editable copy";

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
        normalized_main = Some(main_xml);
        let relationships = package.read_rels_for(&main)?;
        if let Some(styles) = relationships.first_by_type(rel_types::STYLES) {
            let target = main.resolve_relative(&styles.target)?;
            if has_hidden_text_declaration(&package.read_part(&target)?)? {
                report.risks.insert(HIDDEN_TEXT_RISK.into());
            }
        }
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
                        match tag {
                            "p" => report.paragraphs += 1,
                            "tbl" => report.tables += 1,
                            "drawing" | "pict" => report.drawings += 1,
                            _ => {}
                        }
                    }
                    let risk = match tag {
                        "ins" | "del" | "moveFrom" | "moveTo" => Some("Tracked changes"),
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
                        "sectPr" | "pgSz" | "pgMar" | "cols" => Some("Section and page geometry"),
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
    Ok(report)
}
