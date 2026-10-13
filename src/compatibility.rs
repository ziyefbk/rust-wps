//! Conservative preflight for structural DOCX import. This is not a proof of
//! fidelity: it inventories features that the current authoring model lacks.
use anyhow::{Result, ensure};
use quick_xml::{Reader, events::Event};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, io::Read, path::Path};

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
        let mut reader = Reader::from_reader(data.as_slice());
        let mut in_color_scheme = false;
        loop {
            match reader.read_event()? {
                Event::Start(e) | Event::Empty(e) => {
                    let local = e.local_name();
                    let tag = local.as_ref();
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
