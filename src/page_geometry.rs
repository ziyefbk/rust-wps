//! A bounded proof of a single, terminal DOCX body's physical page geometry.
//! The vendor reader supplies defaults and skips unknown declarations, so its
//! section values alone are insufficient evidence for preservation.
use anyhow::{Result, ensure};
use quick_xml::{NsReader, events::Event, name::ResolveResult};
use std::collections::{BTreeMap, BTreeSet};

use crate::document::PageGeometry;

pub(crate) const PAGE_GEOMETRY_RISK: &str =
    "Unsupported page geometry cannot be preserved in an editable copy";
pub(crate) const PAGE_GEOMETRY_NORMALIZED_RISK: &str =
    "Source page geometry is incomplete; editable copy uses default A4 margins";
pub(crate) const PAGE_METADATA_NORMALIZED_RISK: &str = "Paper code and header/footer distance metadata normalized; only page size and body margins preserved";

pub(crate) const PAGE_AUTO_WIDTH_RISK: &str = "Unpositioned automatic table widths normalized to the explicit page content area; source widths are not preserved";

#[derive(Default)]
pub(crate) struct PageAudit {
    pub geometry: Option<PageGeometry>,
    pub risks: BTreeSet<String>,
}

#[derive(Default)]
struct Xml {
    tag: String,
    wml: bool,
    attrs: BTreeMap<String, (String, bool)>,
    children: Vec<Xml>,
    text: bool,
}
fn is_wml(ns: ResolveResult<'_>) -> bool {
    matches!(ns, ResolveResult::Bound(ns) if matches!(ns.as_ref(),
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main" |
        "http://purl.oclc.org/ooxml/wordprocessingml/main"))
}
impl Xml {
    fn attr(&self, name: &str) -> Result<Option<&str>> {
        let attrs: Vec<_> = self
            .attrs
            .iter()
            .filter(|(key, _)| key.rsplit(':').next() == Some(name))
            .collect();
        ensure!(attrs.len() <= 1, "Conflicting page {name} attributes");
        match attrs.first() {
            Some((key, (value, bound))) => {
                // These are the literal attributes consumed by the parser.
                ensure!(
                    **key == format!("w:{name}") && *bound,
                    "Unsupported page attribute namespace: {name}"
                );
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }
    fn schema(&self, allowed: &[&str], leaf: bool) -> Result<()> {
        ensure!(
            self.wml && !self.text && (!leaf || self.children.is_empty()),
            "Unsupported page declaration: {}",
            self.tag
        );
        for key in self.attrs.keys() {
            let name = key.rsplit(':').next().expect("attribute");
            ensure!(allowed.contains(&name), "Unknown page attribute: {key}");
            self.attr(name)?;
        }
        Ok(())
    }
    fn descendants<'a>(&'a self, tag: &str, out: &mut Vec<&'a Xml>) {
        for child in &self.children {
            if child.tag == tag {
                out.push(child);
            }
            child.descendants(tag, out);
        }
    }
    fn one(&self, tag: &str) -> Result<Option<&Xml>> {
        let mut nodes = self.children.iter().filter(|child| child.tag == tag);
        let first = nodes.next();
        ensure!(nodes.next().is_none(), "Repeated page declaration: {tag}");
        Ok(first)
    }
}
fn xml(data: &[u8]) -> Result<Xml> {
    let mut reader = NsReader::from_reader(data);
    let mut stack = vec![Xml::default()];
    let mut count = 0;
    loop {
        let (ns, event) = reader.read_resolved_event()?;
        let wml = is_wml(ns);
        let start = matches!(&event, Event::Start(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                count += 1;
                ensure!(
                    count <= 100_000 && stack.len() <= 128,
                    "Page metadata exceeds limits"
                );
                let mut node = Xml {
                    tag: e.local_name().as_ref().to_owned(),
                    wml,
                    ..Default::default()
                };
                for attr in e.attributes() {
                    let attr = attr?;
                    if attr.key.as_ref() == "xmlns" || attr.key.as_ref().starts_with("xmlns:") {
                        continue;
                    }
                    let (ns, _) = reader.resolver().resolve_attribute(attr.key);
                    ensure!(
                        node.attrs
                            .insert(
                                attr.key.as_ref().to_owned(),
                                (
                                    attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)?
                                        .into_owned(),
                                    is_wml(ns)
                                )
                            )
                            .is_none(),
                        "Duplicate XML attribute"
                    );
                }
                if start {
                    stack.push(node);
                } else {
                    stack.last_mut().expect("root").children.push(node);
                }
            }
            Event::End(_) => {
                ensure!(stack.len() > 1, "Unbalanced page metadata XML");
                let node = stack.pop().expect("element");
                stack.last_mut().expect("root").children.push(node);
            }
            Event::Text(text) => {
                if !text.as_ref().bytes().all(|byte| byte.is_ascii_whitespace()) {
                    stack.last_mut().expect("root").text = true;
                }
            }
            Event::CData(_) | Event::GeneralRef(_) => stack.last_mut().expect("root").text = true,
            Event::DocType(_) => anyhow::bail!("DTD is not supported in DOCX XML"),
            Event::Eof => {
                ensure!(stack.len() == 1, "Incomplete page metadata XML");
                return Ok(stack.pop().expect("root"));
            }
            _ => {}
        }
    }
}
fn number(value: Option<&str>, max: u64) -> Result<u64> {
    let value = value.ok_or_else(|| anyhow::anyhow!("Missing page geometry value"))?;
    ensure!(
        !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()),
        "Page values must be integer twips"
    );
    let number = value.parse::<u64>()?;
    ensure!(number <= max, "Page value exceeds supported bounds");
    Ok(number)
}
fn boolean(value: &str) -> Result<bool> {
    match value {
        "1" | "true" | "on" => Ok(true),
        "0" | "false" | "off" => Ok(false),
        _ => anyhow::bail!("Unknown page toggle value"),
    }
}
fn disabled(node: &Xml) -> Result<()> {
    node.schema(&["val"], true)?;
    ensure!(
        !node.attr("val")?.map(boolean).transpose()?.unwrap_or(true),
        "Active page setting is unsupported: {}",
        node.tag
    );
    Ok(())
}
fn value_is(node: &Xml, value: &str) -> Result<()> {
    node.schema(&["val"], true)?;
    ensure!(
        node.attr("val")? == Some(value),
        "Unsupported page {} value",
        node.tag
    );
    Ok(())
}
fn neutral_grid(node: &Xml) -> Result<()> {
    node.schema(&["type", "linePitch", "charSpace"], true)?;
    ensure!(
        matches!(node.attr("type")?, None | Some("default")),
        "Active document grid is unsupported"
    );
    // Pitch has no effect while the grid is disabled; still reject malformed
    // attributes rather than letting a conflicting namespace supply a value.
    if node.attr("linePitch")?.is_some() {
        number(node.attr("linePitch")?, 31680)?;
    }
    if let Some(value) = node.attr("charSpace")? {
        ensure!(
            value.parse::<i32>().is_ok(),
            "Malformed inactive document grid"
        );
    }
    Ok(())
}
fn neutral_columns(node: &Xml) -> Result<()> {
    node.schema(&["num", "space", "sep", "equalWidth"], true)?;
    ensure!(
        node.attr("num")?
            .map(|v| number(Some(v), 1))
            .transpose()?
            .unwrap_or(1)
            == 1,
        "Multiple columns are unsupported"
    );
    if node.attr("space")?.is_some() {
        number(node.attr("space")?, 31680)?;
    }
    ensure!(
        !node.attr("sep")?.map(boolean).transpose()?.unwrap_or(false),
        "Column separator is unsupported"
    );
    ensure!(
        node.attr("equalWidth")?
            .map(boolean)
            .transpose()?
            .unwrap_or(true),
        "Unequal columns are unsupported"
    );
    Ok(())
}
fn section_property(tag: &str) -> bool {
    matches!(
        tag,
        "sectPr"
            | "pgSz"
            | "pgMar"
            | "cols"
            | "col"
            | "docGrid"
            | "headerReference"
            | "footerReference"
            | "titlePg"
            | "rtlGutter"
            | "pgBorders"
            | "vAlign"
            | "paperSrc"
            | "printerSettings"
            | "lnNumType"
            | "sectPrChange"
    )
}
fn page_toggle(tag: &str) -> bool {
    matches!(
        tag,
        "mirrorMargins"
            | "gutterAtTop"
            | "bookFoldPrinting"
            | "bookFoldRevPrinting"
            | "printTwoOnOne"
            | "evenAndOddHeaders"
            | "alignBordersAndEdges"
            | "bordersDoNotSurroundHeader"
            | "bordersDoNotSurroundFooter"
            | "doNotUseMarginsForDrawingGridOrigin"
    )
}
fn check_direction(node: &Xml) -> Result<()> {
    match node.tag.as_str() {
        "bidi" | "rtlGutter" => disabled(node),
        "textDirection" => value_is(node, "lrTb"),
        _ => Ok(()),
    }
}
fn scan_main(node: &Xml, section: Option<&Xml>) -> Result<()> {
    for child in &node.children {
        if section.is_some_and(|section| std::ptr::eq(section, child)) {
            continue;
        }
        ensure!(
            !section_property(&child.tag) && !page_toggle(&child.tag),
            "Page declaration is outside the terminal body section: {}",
            child.tag
        );
        check_direction(child)?;
        scan_main(child, section)?;
    }
    Ok(())
}
fn check_settings(data: Option<&[u8]>) -> Result<()> {
    let Some(data) = data else {
        return Ok(());
    };
    let tree = xml(data)?;
    ensure!(
        tree.children.len() == 1 && tree.children[0].tag == "settings" && tree.children[0].wml,
        "Ambiguous page settings part"
    );
    let root = &tree.children[0];
    fn scan(node: &Xml, root: &Xml, seen: &mut BTreeSet<String>) -> Result<()> {
        for child in &node.children {
            if page_toggle(&child.tag) {
                ensure!(
                    std::ptr::eq(node, root) && seen.insert(child.tag.clone()),
                    "Wrapped or repeated page setting"
                );
                disabled(child)?;
            } else if child.tag == "bookFoldPrintingSheets" {
                ensure!(
                    std::ptr::eq(node, root) && seen.insert(child.tag.clone()),
                    "Wrapped or repeated page setting"
                );
                child.schema(&["val"], true)?;
                ensure!(
                    number(child.attr("val")?, 0)? == 0,
                    "Book-fold sheets are unsupported"
                );
            } else if section_property(&child.tag)
                || matches!(child.tag.as_str(), "bidi" | "textDirection")
            {
                anyhow::bail!("Unexpected page declaration in settings: {}", child.tag);
            }
            scan(child, root, seen)?;
        }
        Ok(())
    }
    scan(root, root, &mut BTreeSet::new())
}
fn check_styles(data: &[u8], used: &BTreeSet<String>) -> Result<()> {
    let tree = xml(data)?;
    fn scan(node: &Xml, used: &BTreeSet<String>) -> Result<()> {
        for child in &node.children {
            if child.tag == "style" {
                // A literal, unused style is not an active source declaration.
                // Ambiguous identities are inspected instead of guessed away.
                if let Ok(Some(id)) = child.attr("styleId")
                    && !used.contains(id)
                {
                    continue;
                }
            }
            ensure!(
                !section_property(&child.tag) && !page_toggle(&child.tag),
                "Page declaration in active style is unsupported: {}",
                child.tag
            );
            check_direction(child)?;
            scan(child, used)?;
        }
        Ok(())
    }
    scan(&tree, used)
}

pub(crate) fn analyze(
    main: &[u8],
    settings: Option<&[u8]>,
    styles: &[u8],
    used: &BTreeSet<String>,
) -> Result<PageAudit> {
    let tree = xml(main)?;
    ensure!(
        tree.children.len() == 1 && tree.children[0].tag == "document" && tree.children[0].wml,
        "Page geometry requires one WML document"
    );
    let document = &tree.children[0];
    let mut bodies = Vec::new();
    tree.descendants("body", &mut bodies);
    ensure!(bodies.len() == 1, "Page geometry requires exactly one body");
    let body = document
        .one("body")?
        .ok_or_else(|| anyhow::anyhow!("Missing document body"))?;
    ensure!(body.wml, "Unsupported body namespace");
    let mut sections = Vec::new();
    tree.descendants("sectPr", &mut sections);
    ensure!(sections.len() <= 1, "Multiple sections are unsupported");
    let section = sections.first().copied();
    if let Some(section) = section {
        ensure!(
            body.children
                .last()
                .is_some_and(|last| std::ptr::eq(last, section)),
            "Only a terminal body section is supported"
        );
    }
    scan_main(&tree, section)?;
    check_settings(settings)?;
    check_styles(styles, used)?;
    let mut audit = PageAudit::default();
    let Some(section) = section else {
        audit.risks.insert(PAGE_GEOMETRY_NORMALIZED_RISK.into());
        return Ok(audit);
    };
    section.schema(&["rsidR", "rsidRPr", "rsidSect", "rsidDel"], false)?;
    let mut seen = BTreeSet::new();
    for child in &section.children {
        ensure!(
            seen.insert(child.tag.as_str()),
            "Repeated section declaration: {}",
            child.tag
        );
        match child.tag.as_str() {
            "pgSz" => {
                child.schema(&["w", "h", "orient", "code"], true)?;
                let width = number(child.attr("w")?, 31680)?;
                let height = number(child.attr("h")?, 31680)?;
                crate::document::validate_page_geometry(&PageGeometry {
                    width_twips: width,
                    height_twips: height,
                    margin_top_twips: 0,
                    margin_right_twips: 0,
                    margin_bottom_twips: 0,
                    margin_left_twips: 0,
                })?;
                let orientation = child.attr("orient")?.unwrap_or("portrait");
                ensure!(
                    (orientation == "portrait" && width < height)
                        || (orientation == "landscape" && width > height),
                    "Conflicting page orientation and dimensions"
                );
                if child.attr("code")?.is_some() {
                    number(child.attr("code")?, u16::MAX.into())?;
                    audit.risks.insert(PAGE_METADATA_NORMALIZED_RISK.into());
                }
            }
            "pgMar" => {
                child.schema(
                    &[
                        "top", "right", "bottom", "left", "header", "footer", "gutter",
                    ],
                    true,
                )?;
                for key in ["top", "right", "bottom", "left"] {
                    number(child.attr(key)?, 4320)?;
                }
                if child.attr("gutter")?.is_some() {
                    number(child.attr("gutter")?, 0)?;
                }
                for key in ["header", "footer"] {
                    if child.attr(key)?.is_some() {
                        number(child.attr(key)?, 31680)?;
                        audit.risks.insert(PAGE_METADATA_NORMALIZED_RISK.into());
                    }
                }
            }
            "type" => value_is(child, "nextPage")?,
            "cols" => neutral_columns(child)?,
            "docGrid" => neutral_grid(child)?,
            "titlePg" | "bidi" | "rtlGutter" | "formProt" => disabled(child)?,
            "textDirection" => value_is(child, "lrTb")?,
            "vAlign" => value_is(child, "top")?,
            // Numbering/note policy and printer settings are outside this
            // bounded model even when the section itself has no visible story.
            _ => anyhow::bail!("Unsupported section declaration: {}", child.tag),
        }
    }
    let (Some(size), Some(margins)) = (section.one("pgSz")?, section.one("pgMar")?) else {
        audit.risks.insert(PAGE_GEOMETRY_NORMALIZED_RISK.into());
        return Ok(audit);
    };
    let geometry = PageGeometry {
        width_twips: number(size.attr("w")?, 31680)?,
        height_twips: number(size.attr("h")?, 31680)?,
        margin_top_twips: number(margins.attr("top")?, 4320)?,
        margin_right_twips: number(margins.attr("right")?, 4320)?,
        margin_bottom_twips: number(margins.attr("bottom")?, 4320)?,
        margin_left_twips: number(margins.attr("left")?, 4320)?,
    };
    crate::document::validate_page_geometry(&geometry)?;
    audit.geometry = Some(geometry);
    Ok(audit)
}

/// Relationship shape is audited before a settings/style identity can prove
/// page geometry. The vendor relationship reader matches local names and may
/// otherwise accept foreign namespaces, wrappers or an unknown TargetMode.
pub(crate) fn validate_relationships(data: &[u8]) -> Result<()> {
    let mut reader = NsReader::from_reader(data);
    let mut stack = Vec::new();
    let mut identities = BTreeSet::new();
    let mut kinds = BTreeSet::new();
    let mut roots = 0;
    let mut count = 0;
    loop {
        let (ns, event) = reader.read_resolved_event()?;
        let package_ns = matches!(ns, ResolveResult::Bound(ns) if
            ns.as_ref() == "http://schemas.openxmlformats.org/package/2006/relationships");
        let start = matches!(&event, Event::Start(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                count += 1;
                ensure!(
                    count <= 100_000 && stack.len() <= 128,
                    "Page relationships exceed limits"
                );
                let tag = e.local_name().as_ref().to_owned();
                if stack.is_empty() {
                    roots += 1;
                    ensure!(
                        roots == 1 && tag == "Relationships" && package_ns,
                        "Invalid page relationship root"
                    );
                    for attr in e.attributes() {
                        let attr = attr?;
                        ensure!(
                            attr.key.as_ref() == "xmlns" || attr.key.as_ref().starts_with("xmlns:"),
                            "Unsupported relationship root attribute"
                        );
                    }
                }
                if tag == "Relationship" {
                    let mut attrs = BTreeMap::new();
                    for attr in e.attributes() {
                        let attr = attr?;
                        if attr.key.as_ref() == "xmlns" || attr.key.as_ref().starts_with("xmlns:") {
                            continue;
                        }
                        ensure!(
                            attrs
                                .insert(
                                    attr.key.as_ref().to_owned(),
                                    attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)?
                                        .into_owned()
                                )
                                .is_none(),
                            "Duplicate relationship attribute"
                        );
                    }
                    let relevant = attrs.iter().any(|(key, value)| {
                        key.rsplit(':').next() == Some("Type")
                            && ["/settings", "/styles"]
                                .iter()
                                .any(|suffix| value.ends_with(suffix))
                    });
                    if relevant {
                        ensure!(
                            package_ns && stack == ["Relationships"],
                            "Wrapped or foreign page relationship"
                        );
                        ensure!(
                            attrs.keys().all(|key| matches!(
                                key.as_str(),
                                "Id" | "Type" | "Target" | "TargetMode"
                            )),
                            "Unknown page relationship attribute"
                        );
                        let id = attrs
                            .get("Id")
                            .ok_or_else(|| anyhow::anyhow!("Missing relationship ID"))?;
                        let kind = attrs
                            .get("Type")
                            .ok_or_else(|| anyhow::anyhow!("Missing relationship type"))?;
                        ensure!([
                            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles",
                            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings",
                            "http://purl.oclc.org/ooxml/officeDocument/relationships/styles",
                            "http://purl.oclc.org/ooxml/officeDocument/relationships/settings",
                        ].contains(&kind.as_str()), "Unsupported page relationship type");
                        let kind = kind.rsplit('/').next().expect("type");
                        ensure!(
                            identities.insert(id.clone()) && kinds.insert(kind.to_owned()),
                            "Duplicate page relationship"
                        );
                        ensure!(
                            attrs.get("Target").is_some_and(|value| !value.is_empty()),
                            "Missing page relationship target"
                        );
                        ensure!(
                            attrs
                                .get("TargetMode")
                                .is_none_or(|value| value == "Internal"),
                            "External or unknown page relationship mode"
                        );
                    }
                }
                if start {
                    stack.push(tag);
                }
            }
            Event::End(_) => {
                ensure!(stack.pop().is_some(), "Unbalanced relationships XML");
            }
            Event::DocType(_) => anyhow::bail!("DTD is not supported in DOCX XML"),
            Event::Eof => {
                ensure!(
                    roots == 1 && stack.is_empty(),
                    "Incomplete relationships XML"
                );
                return Ok(());
            }
            _ => {}
        }
    }
}

/// Cross-check the parser consumed the exact audited attributes, rather than
/// silently defaulting or dropping a declaration. This never supplies values.
pub(crate) fn verify_parsed(
    source: &PageGeometry,
    parsed: &office_oxide::docx::DocxDocument,
) -> Result<()> {
    ensure!(
        parsed.sections.len() == 1,
        "Audited page section was discarded or duplicated during conversion"
    );
    let section = &parsed.sections[0];
    let size = section
        .page_size
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Audited page size was discarded"))?;
    let margins = section
        .margins
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Audited margins were discarded"))?;
    for (actual, expected) in [
        (size.width.0, source.width_twips),
        (size.height.0, source.height_twips),
        (margins.top.0, source.margin_top_twips),
        (margins.right.0, source.margin_right_twips),
        (margins.bottom.0, source.margin_bottom_twips),
        (margins.left.0, source.margin_left_twips),
    ] {
        ensure!(
            u64::try_from(actual).ok() == Some(expected),
            "Parsed page geometry differs from audited source"
        );
    }
    ensure!(
        size.orient == Some(office_oxide::docx::PageOrientation::Landscape)
            || source.width_twips < source.height_twips,
        "Parsed orientation differs from audited source"
    );
    Ok(())
}
