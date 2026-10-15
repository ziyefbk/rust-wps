//! Invocation-local source proof for minimum paragraph line height. This module
//! only compares identities; it never reconstructs content or edits formatting.
use crate::{
    document::Node,
    paragraph_indentation::{Xml, root, xml},
};
use anyhow::{Context, Result, ensure};
use office_oxide::{
    core::{
        opc::{OpcReader, PartName},
        relationships::{Relationships, TargetMode, rel_types},
    },
    ir::{DocumentIR, Element, InlineContent, LineSpacing},
};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
};

pub(crate) const WARNING: &str = "Minimum paragraph line height preserved; actual line layout and pagination remain consumer-dependent";
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const MAX_WORK: usize = 5_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Line {
    Auto(u32),
    Minimum(u32),
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    Paragraph,
    Heading(u8),
}
// A literal newline in w:t is deliberately distinct from a w:br. Text runs
// may merge during conversion, so adjacent text spans form a single token.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Inline {
    Text(String),
    Break,
}
fn append_text(out: &mut Vec<Inline>, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(Inline::Text(previous)) = out.last_mut() {
        previous.push_str(text);
    } else {
        out.push(Inline::Text(text.to_owned()));
    }
}
#[derive(Debug)]
struct Paragraph {
    kind: Kind,
    content: Vec<Inline>,
    before: Option<u32>,
    after: Option<u32>,
    line: Option<Line>,
}
#[derive(Debug)]
struct Source {
    // Owned provenance, not a public/reusable boolean or serialized capability.
    // The caller keeps these immutable bytes for parsing in this invocation.
    sha256: [u8; 32],
    bytes: usize,
    main_part: String,
    styles_part: Option<String>,
    paragraphs: Vec<Paragraph>,
}
#[derive(Debug, Default)]
pub(crate) struct MinimumLineHeightAudit {
    source: Option<Source>,
}
impl MinimumLineHeightAudit {
    pub(crate) fn proved(&self) -> bool {
        self.source.is_some()
    }
    pub(crate) fn has_minimum(&self) -> bool {
        self.source.as_ref().is_some_and(|source| {
            source
                .paragraphs
                .iter()
                .any(|p| matches!(p.line, Some(Line::Minimum(_))))
        })
    }
}

fn number(value: &str, maximum: u32) -> Result<u32> {
    ensure!(
        !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()),
        "Minimum line height requires integer twips"
    );
    let value: u32 = value.parse()?;
    ensure!(
        value <= maximum,
        "Minimum line height number exceeds bounds"
    );
    Ok(value)
}
fn boolean(value: &str) -> Result<bool> {
    match value {
        "1" | "true" | "on" => Ok(true),
        "0" | "false" | "off" => Ok(false),
        _ => anyhow::bail!("Unknown minimum line height toggle"),
    }
}
fn value(node: &Xml) -> Result<&str> {
    node.schema(&["val"], true)?;
    node.attr("val")?
        .filter(|v| !v.is_empty())
        .context("Missing paragraph property value")
}
fn disabled(node: &Xml) -> Result<()> {
    node.schema(&["val"], true)?;
    ensure!(
        !node.attr("val")?.map(boolean).transpose()?.unwrap_or(true),
        "Unsupported active minimum line height dependency: {}",
        node.tag
    );
    Ok(())
}
#[derive(Clone, Default)]
struct Properties {
    before: Option<u32>,
    after: Option<u32>,
    line: Option<Line>,
    style: Option<String>,
    outline: Option<u8>,
    tabs: bool,
    nonzero_indent: bool,
}
impl Properties {
    fn overlay(&mut self, other: &Self) {
        if other.before.is_some() {
            self.before = other.before;
        }
        if other.after.is_some() {
            self.after = other.after;
        }
        // The rule and value are one inheritance slot, never independent.
        if other.line.is_some() {
            self.line = other.line;
        }
        if other.outline.is_some() {
            self.outline = other.outline;
        }
        self.tabs |= other.tabs;
        self.nonzero_indent |= other.nonzero_indent;
    }
}
fn spacing(node: &Xml, out: &mut Properties) -> Result<()> {
    node.schema(
        &[
            "before",
            "after",
            "line",
            "lineRule",
            "beforeAutospacing",
            "afterAutospacing",
        ],
        true,
    )?;
    out.before = node.attr("before")?.map(|v| number(v, 31680)).transpose()?;
    out.after = node.attr("after")?.map(|v| number(v, 31680)).transpose()?;
    for name in ["beforeAutospacing", "afterAutospacing"] {
        ensure!(
            !node.attr(name)?.map(boolean).transpose()?.unwrap_or(false),
            "Automatic paragraph gaps are unsupported"
        );
    }
    let line = node.attr("line")?;
    let rule = node.attr("lineRule")?;
    ensure!(
        line.is_some() || rule.is_none(),
        "Line rule without a line value"
    );
    if let Some(line) = line {
        out.line = Some(match rule.unwrap_or("auto") {
            "auto" => {
                let line = number(line, 2400)?;
                ensure!(line >= 120, "Auto line spacing is below supported bounds");
                Line::Auto(line)
            }
            "atLeast" => {
                let line = number(line, 31680)?;
                ensure!(line > 0, "Minimum line height must be positive");
                Line::Minimum(line)
            }
            _ => anyhow::bail!("Unknown or unsupported paragraph line rule"),
        });
    }
    Ok(())
}
fn tabs(node: &Xml) -> Result<bool> {
    node.schema(&[], false)?;
    let mut positions = BTreeSet::new();
    for stop in &node.children {
        ensure!(stop.tag == "tab", "Unsupported tab-stop child");
        stop.schema(&["val", "pos", "leader"], true)?;
        ensure!(
            stop.attr("val")? == Some("left"),
            "Only inactive simple left tab stops are supported"
        );
        ensure!(
            stop.attr("leader")?.is_none_or(|v| v == "none"),
            "Tab leaders are unsupported"
        );
        let position = number(stop.attr("pos")?.context("Missing tab position")?, 31680)?;
        ensure!(positions.insert(position), "Duplicate tab-stop position");
    }
    Ok(!node.children.is_empty())
}
fn indentation(node: &Xml) -> Result<bool> {
    node.schema(&["left", "right", "firstLine", "hanging"], true)?;
    ensure!(
        !(node.attr("firstLine")?.is_some() && node.attr("hanging")?.is_some()),
        "Conflicting paragraph indentation"
    );
    let mut active = false;
    for key in ["left", "right", "firstLine", "hanging"] {
        if let Some(v) = node.attr(key)? {
            let digits = if matches!(key, "left" | "right") {
                v.strip_prefix('-').unwrap_or(v)
            } else {
                v
            };
            active |= number(digits, 31680)? != 0;
        }
    }
    Ok(active)
}
fn properties(node: Option<&Xml>) -> Result<Properties> {
    let Some(node) = node else {
        return Ok(Properties::default());
    };
    node.schema(&[], false)?;
    let mut result = Properties::default();
    if let Some(s) = node.one("spacing")? {
        spacing(s, &mut result)?;
    }
    if let Some(t) = node.one("tabs")? {
        result.tabs = tabs(t)?;
    }
    if let Some(i) = node.one("ind")? {
        result.nonzero_indent = indentation(i)?;
    }
    result.style = node
        .one("pStyle")?
        .map(value)
        .transpose()?
        .map(str::to_owned);
    result.outline = node
        .one("outlineLvl")?
        .map(value)
        .transpose()?
        .map(|v| number(v, 9).map(|v| v as u8))
        .transpose()?;
    // Source numbering disqualifies even if heading promotion would conceal it,
    // or a later layer would switch it off.
    ensure!(
        node.one("numPr")?.is_none(),
        "Numbering is outside minimum line height source proof"
    );
    for key in [
        "contextualSpacing",
        "bidi",
        "mirrorIndents",
        "adjustRightInd",
        "snapToGrid",
    ] {
        if let Some(toggle) = node.one(key)? {
            disabled(toggle)?;
        }
    }
    if let Some(direction) = node.one("textDirection")? {
        ensure!(
            value(direction)? == "lrTb",
            "Unsupported paragraph text direction"
        );
    }
    ensure!(
        node.one("sectPr")?.is_none(),
        "Paragraph section breaks are outside minimum line height source proof"
    );
    Ok(result)
}
fn grid(node: &Xml) -> Result<()> {
    node.schema(&["type", "linePitch", "charSpace"], true)?;
    ensure!(
        node.attr("type")?.is_none_or(|v| v == "default"),
        "Active or unknown document grid is unsupported"
    );
    for key in ["linePitch", "charSpace"] {
        if let Some(v) = node.attr(key)? {
            number(v, 31680)?;
        }
    }
    Ok(())
}
fn locations(node: &Xml, parent: Option<&str>) -> Result<()> {
    match node.tag.as_str() {
        "pPrChange" | "rPrChange" | "sectPrChange" | "numPrChange" | "ins" | "del" | "moveFrom"
        | "moveTo" => anyhow::bail!("Revised minimum line height source is unsupported"),
        "pPr" => {
            ensure!(
                matches!(parent, Some("p" | "style" | "pPrDefault")),
                "Non-direct paragraph properties"
            );
            properties(Some(node))?;
        }
        "spacing" | "contextualSpacing" | "pStyle" | "outlineLvl" | "numPr" | "tabs" | "ind"
        | "snapToGrid" | "mirrorIndents" | "adjustRightInd" => {
            node.identity()?;
            ensure!(
                parent == Some("pPr"),
                "Non-direct paragraph line height dependency"
            );
        }
        "basedOn" => {
            ensure!(
                parent == Some("style"),
                "Non-direct paragraph style inheritance"
            );
            value(node)?;
        }
        "docGrid" => {
            ensure!(parent == Some("sectPr"), "Non-direct document grid");
            grid(node)?;
        }
        _ => {}
    }
    for child in &node.children {
        locations(child, Some(&node.tag))?;
    }
    Ok(())
}
fn local_values<'a>(node: &'a Xml, name: &'a str) -> impl Iterator<Item = &'a str> {
    node.attrs
        .iter()
        .filter(move |(key, _)| key.rsplit(':').next() == Some(name))
        .map(|(_, (value, _))| value.as_str())
}
fn contains_minimum(node: &Xml) -> bool {
    node.all("spacing")
        .into_iter()
        .any(|n| local_values(n, "lineRule").any(|v| v == "atLeast"))
}
struct Styles<'a> {
    root: &'a Xml,
    index: BTreeMap<String, Vec<&'a Xml>>,
    direct: BTreeSet<usize>,
    work: Cell<usize>,
    chains: RefCell<BTreeMap<String, Vec<&'a Xml>>>,
    properties: RefCell<BTreeMap<usize, Properties>>,
}
impl<'a> Styles<'a> {
    fn new(root: &'a Xml) -> Self {
        let mut index = BTreeMap::<String, Vec<&Xml>>::new();
        for style in root.all("style") {
            for id in local_values(style, "styleId") {
                index.entry(id.to_owned()).or_default().push(style);
            }
        }
        Self {
            root,
            index,
            direct: root
                .children
                .iter()
                .filter(|n| n.tag == "style")
                .map(|n| n as *const Xml as usize)
                .collect(),
            work: Cell::new(0),
            chains: RefCell::default(),
            properties: RefCell::default(),
        }
    }
    fn spend(&self, amount: usize) -> Result<()> {
        let work = self
            .work
            .get()
            .checked_add(amount)
            .context("Minimum line height style work exceeds limits")?;
        ensure!(
            work <= MAX_WORK,
            "Minimum line height style work exceeds limits"
        );
        self.work.set(work);
        Ok(())
    }
    // Discovery deliberately uses local names and all possible IDs, before
    // literal namespace/attribute validation, so aliases cannot hide a request.
    fn relevant(&self, main: &Xml) -> Result<bool> {
        if contains_minimum(main)
            || self
                .root
                .all("docDefaults")
                .into_iter()
                .any(contains_minimum)
        {
            return Ok(true);
        }
        let mut pending = Vec::new();
        let mut need_default = false;
        for p in main.all("p") {
            let refs = p.all("pStyle");
            need_default |= refs.is_empty();
            for reference in refs {
                pending.extend(local_values(reference, "val").map(str::to_owned));
            }
        }
        for style in self.root.all("style") {
            let default =
                local_values(style, "default").any(|v| !matches!(v, "0" | "false" | "off"));
            let paragraph = local_values(style, "type").next().is_none()
                || style.has_value("type", "paragraph");
            if need_default && default && paragraph {
                if contains_minimum(style) {
                    return Ok(true);
                }
                pending.extend(local_values(style, "styleId").map(str::to_owned));
            }
        }
        let mut seen = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Some(styles) = self.index.get(&id) {
                for style in styles {
                    self.spend(style.size)?;
                    if contains_minimum(style) {
                        return Ok(true);
                    }
                    for base in style.all("basedOn") {
                        pending.extend(local_values(base, "val").map(str::to_owned));
                    }
                }
            }
        }
        Ok(false)
    }
    fn defaults(&self) -> Result<Properties> {
        let defaults = self.root.one("docDefaults")?;
        ensure!(
            self.root.all("docDefaults").len() == usize::from(defaults.is_some()),
            "Ambiguous document defaults"
        );
        let Some(defaults) = defaults else {
            return Ok(Properties::default());
        };
        defaults.schema(&[], false)?;
        let ppd = defaults.one("pPrDefault")?;
        if let Some(ppd) = ppd {
            ppd.schema(&[], false)?;
        }
        let pp = ppd.map(|p| p.one("pPr")).transpose()?.flatten();
        ensure!(
            defaults.all("pPr").len() == usize::from(pp.is_some())
                && defaults.all("pPrDefault").len() == usize::from(ppd.is_some()),
            "Wrapped paragraph defaults"
        );
        locations(defaults, Some("styles"))?;
        properties(pp)
    }
    fn default_id(&self) -> Result<Option<String>> {
        let defaults: Vec<_> = self
            .root
            .all("style")
            .into_iter()
            .filter(|s| {
                local_values(s, "default").any(|v| !matches!(v, "0" | "false" | "off"))
                    && (local_values(s, "type").next().is_none()
                        || s.has_value("type", "paragraph"))
            })
            .collect();
        ensure!(defaults.len() <= 1, "Ambiguous default paragraph style");
        defaults
            .first()
            .map(|style| {
                style.identity()?;
                ensure!(
                    self.direct.contains(&(*style as *const Xml as usize))
                        && style.attr("type")?.unwrap_or("paragraph") == "paragraph"
                        && style.attr("default")?.map(boolean).transpose()? == Some(true),
                    "Invalid default paragraph style identity"
                );
                Ok(style
                    .attr("styleId")?
                    .filter(|v| !v.is_empty())
                    .context("Missing default paragraph style ID")?
                    .to_owned())
            })
            .transpose()
    }
    fn props(&self, style: &Xml) -> Result<Properties> {
        let key = style as *const Xml as usize;
        if let Some(properties) = self.properties.borrow().get(&key) {
            return Ok(properties.clone());
        }
        self.spend(style.size)?;
        locations(style, Some("styles"))?;
        let pp = style.one("pPr")?;
        ensure!(
            style.all("pPr").len() == usize::from(pp.is_some()),
            "Conditional paragraph style properties are unsupported"
        );
        let properties = properties(pp)?;
        self.properties.borrow_mut().insert(key, properties.clone());
        Ok(properties)
    }
    fn chain(&self, id: Option<&str>) -> Result<Vec<&'a Xml>> {
        let Some(id) = id else {
            return Ok(Vec::new());
        };
        if let Some(chain) = self.chains.borrow().get(id) {
            return Ok(chain.clone());
        }
        let mut next = Some(id.to_owned());
        let mut seen = BTreeSet::new();
        let mut chain = Vec::new();
        while let Some(id) = next {
            ensure!(
                seen.insert(id.clone()) && seen.len() <= 20,
                "Cyclic or overlong paragraph style chain"
            );
            let candidates = self.index.get(&id).map(Vec::as_slice).unwrap_or(&[]);
            ensure!(
                candidates.len() == 1,
                "Missing or ambiguous paragraph style ID"
            );
            let style = candidates[0];
            self.spend(style.size)?;
            style.identity()?;
            ensure!(
                self.direct.contains(&(style as *const Xml as usize))
                    && style.attr("styleId")? == Some(id.as_str())
                    && style.attr("type")?.unwrap_or("paragraph") == "paragraph",
                "Unsupported paragraph style identity or kind"
            );
            if let Some(default) = style.attr("default")? {
                boolean(default)?;
            }
            self.props(style)?;
            next = style
                .one("basedOn")?
                .map(value)
                .transpose()?
                .map(str::to_owned);
            chain.push(style);
        }
        chain.reverse();
        self.chains
            .borrow_mut()
            .insert(id.to_owned(), chain.clone());
        Ok(chain)
    }
}

fn source_inline(node: &Xml, out: &mut Vec<Inline>) -> Result<()> {
    node.identity()?;
    match node.tag.as_str() {
        "pPr" | "rPr" => return Ok(()),
        "t" => {
            ensure!(
                node.children.is_empty() && !node.text.contains('\t'),
                "Nested text or literal tab in minimum line height source"
            );
            for (name, (value, _)) in &node.attrs {
                ensure!(
                    name == "xml:space"
                        && node.attribute_namespaces.get(name).map(String::as_str)
                            == Some("http://www.w3.org/XML/1998/namespace")
                        && matches!(value.as_str(), "default" | "preserve"),
                    "Unsupported text attribute identity"
                );
            }
            append_text(out, &node.text);
            return Ok(());
        }
        "br" => {
            node.schema(&["type", "clear"], true)?;
            ensure!(
                node.attr("type")?.is_none_or(|v| v == "textWrapping")
                    && node.attr("clear")?.is_none_or(|v| v == "none"),
                "Page, column or clearing breaks are unsupported"
            );
            out.push(Inline::Break);
            return Ok(());
        }
        "bookmarkStart" | "bookmarkEnd" | "proofErr" => {
            ensure!(
                node.children.is_empty() && node.text.trim().is_empty(),
                "Unsupported paragraph marker"
            );
            return Ok(());
        }
        "p" | "r" | "hyperlink" => {
            ensure!(node.text.trim().is_empty(), "Uncontained paragraph text");
            let property = if node.tag == "p" { "pPr" } else { "rPr" };
            if node.tag != "hyperlink" {
                node.one(property)?;
            }
            for child in &node.children {
                let allowed = match node.tag.as_str() {
                    "p" => matches!(
                        child.tag.as_str(),
                        "pPr" | "r" | "hyperlink" | "bookmarkStart" | "bookmarkEnd" | "proofErr"
                    ),
                    "r" => matches!(child.tag.as_str(), "rPr" | "t" | "br"),
                    _ => matches!(
                        child.tag.as_str(),
                        "r" | "bookmarkStart" | "bookmarkEnd" | "proofErr"
                    ),
                };
                ensure!(
                    allowed,
                    "Unsupported minimum line height inline content: {}",
                    child.tag
                );
                source_inline(child, out)?;
            }
        }
        _ => anyhow::bail!(
            "Unsupported minimum line height paragraph content: {}",
            node.tag
        ),
    }
    Ok(())
}
fn heading_name(name: &str) -> Option<u8> {
    let lower = name.trim().to_ascii_lowercase();
    let level: u8 = lower.strip_prefix("heading")?.trim().parse().ok()?;
    (1..=9).contains(&level).then_some(level)
}
fn paragraph_kind(own: &Properties, chain: &[&Xml], sheet: &Styles<'_>) -> Result<Kind> {
    // Keep convert_docx::resolve_heading_level's exact direct/style policy.
    // An implicit default style's name or outline never promotes a paragraph.
    let mut style_level = None;
    for style in chain {
        if let Some(outline) = sheet.props(style)?.outline {
            style_level = Some(outline + 1);
        }
    }
    let level = if let Some(outline) = own.outline {
        Some(outline + 1)
    } else if let Some(id) = own.style.as_deref() {
        let name = chain
            .last()
            .map(|s| s.one("name"))
            .transpose()?
            .flatten()
            .map(value)
            .transpose()?;
        style_level
            .or_else(|| heading_name(id))
            .or_else(|| name.and_then(heading_name))
    } else {
        None
    };
    if let Some(level) = level {
        ensure!(
            (1..=3).contains(&level),
            "Only heading levels 1–3 support minimum line height"
        );
        Ok(Kind::Heading(level))
    } else {
        Ok(Kind::Paragraph)
    }
}

fn canonical_target(base: &PartName, target: &str) -> Result<PartName> {
    ensure!(
        !target.is_empty()
            && !target.contains(['%', '\\', '?', '#'])
            && target.split('/').all(|part| part != "." && part != "..")
            && !target.contains("//"),
        "Noncanonical minimum line height relationship target"
    );
    let resolved = base.resolve_relative(target)?;
    let expected = if target.starts_with('/') {
        target.to_owned()
    } else {
        format!("{}{target}", base.directory())
    };
    ensure!(
        resolved.as_str() == expected,
        "Aliased minimum line height relationship target"
    );
    Ok(resolved)
}
fn related_part(rels: &Relationships, main: &PartName, role: &str) -> Result<Option<PartName>> {
    let found = rels.get_by_type(role);
    ensure!(
        found.len() <= 1,
        "Ambiguous minimum line height package relationship"
    );
    found
        .first()
        .map(|rel| {
            ensure!(
                rel.target_mode == TargetMode::Internal,
                "External minimum line height source part"
            );
            canonical_target(main, &rel.target)
        })
        .transpose()
}
fn check_content_types(data: &[u8]) -> Result<()> {
    crate::image_dimensions::audit_content_types(data)?;
    let tree = xml(data)?;
    for declaration in tree.all("Override") {
        let name = &declaration
            .attrs
            .get("PartName")
            .context("Missing content type part name")?
            .0;
        ensure!(
            !name.contains('%') && PartName::new(name)?.as_str() == name,
            "Aliased content type part name"
        );
    }
    Ok(())
}
fn check_relationships(data: &[u8]) -> Result<()> {
    crate::image_dimensions::audit_relationships(data)?;
    crate::paragraph_indentation::validate_relationships(data)?;
    let tree = xml(data)?;
    let mut roles = BTreeSet::new();
    for relationship in tree.all("Relationship") {
        let kind = &relationship
            .attrs
            .get("Type")
            .context("Missing relationship type")?
            .0;
        let role = kind.rsplit('/').next().unwrap_or("");
        if matches!(role, "officeDocument" | "styles" | "settings" | "numbering") {
            ensure!(
                kind == &format!(
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/{role}"
                ) || kind
                    == &format!("http://purl.oclc.org/ooxml/officeDocument/relationships/{role}"),
                "Unknown minimum line height relationship type"
            );
            ensure!(
                roles.insert(role),
                "Duplicate minimum line height relationship role"
            );
        }
    }
    Ok(())
}

/// Analyze before any parser normalization. Inactive files have no capability
/// and remain on the historical import path, including inventory-only ZIPs.
pub(crate) fn analyze(bytes: &[u8]) -> Result<MinimumLineHeightAudit> {
    // A failed discovery is not evidence that this document requests the new
    // capability. Leave legacy errors to the existing package/raw scanners.
    // In particular, unrelated text saying "atLeast", oversized inactive XML,
    // and malformed unused declarations never acquire a new spacing risk.
    if !discover(bytes).unwrap_or(false) {
        return Ok(MinimumLineHeightAudit::default());
    }
    analyze_active(bytes)
}
fn discover(bytes: &[u8]) -> Result<bool> {
    // Bound allocation before OPC reads, using the existing expanded-package
    // budget. Limits here only deny a capability; they never add a new risk.
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut expanded = 0u64;
    for i in 0..archive.len() {
        expanded = expanded
            .checked_add(archive.by_index(i)?.size())
            .context("Package size overflow")?;
        ensure!(
            expanded <= 128 * 1024 * 1024,
            "Package exceeds discovery budget"
        );
    }
    drop(archive);
    let mut package = OpcReader::new(Cursor::new(bytes))?;
    let main = package.main_document_part()?;
    let main_xml = xml(&package.read_part(&main)?)?;
    // A direct request can be established even if its styles part is broken.
    if contains_minimum(&main_xml) {
        return Ok(true);
    }
    let rels = package.read_rels_for(&main)?;
    let styles = if let Some(rel) = rels.first_by_type(rel_types::STYLES) {
        let target = main.resolve_relative(&rel.target)?;
        xml(&package.read_part(&target)?)?
    } else {
        xml(format!(r#"<w:styles xmlns:w="{W}"/>"#).as_bytes())?
    };
    Styles::new(&styles).relevant(&main_xml)
}
fn analyze_active(bytes: &[u8]) -> Result<MinimumLineHeightAudit> {
    ensure!(
        bytes.len() <= 64 * 1024 * 1024,
        "Minimum line height package exceeds 64 MiB"
    );
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    ensure!(
        archive.len() <= 4096,
        "Minimum line height package has too many parts"
    );
    let mut expanded = 0u64;
    let mut names = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        expanded = expanded
            .checked_add(entry.size())
            .context("Minimum line height package size overflow")?;
        ensure!(
            expanded <= 128 * 1024 * 1024,
            "Expanded minimum line height package exceeds 128 MiB"
        );
        names.push(entry.name().to_owned());
    }
    drop(archive);
    let mut package = OpcReader::new(Cursor::new(bytes))?;
    let main = package.main_document_part()?;
    let main_data = package.read_part(&main)?;
    let main_xml = xml(&main_data)?;
    let rels = package.read_rels_for(&main)?;
    // Read the actual role before checking the full package identity. Never
    // choose a conventional styles.xml instead of its relationship target.
    let style_target = rels
        .first_by_type(rel_types::STYLES)
        .map(|r| main.resolve_relative(&r.target))
        .transpose()?;
    let style_data = style_target
        .as_ref()
        .map(|target| package.read_part(target))
        .transpose()?;
    let empty_styles = format!(r#"<w:styles xmlns:w="{W}"/>"#);
    let styles_xml = xml(style_data.as_deref().unwrap_or(empty_styles.as_bytes()))?;
    let discovery = Styles::new(&styles_xml);
    if !discovery.relevant(&main_xml)? {
        return Ok(MinimumLineHeightAudit::default());
    }

    let mut canonical_names = BTreeSet::new();
    for name in names {
        ensure!(
            !name.starts_with('/') && !name.contains('%') && !name.contains('\\'),
            "Noncanonical minimum line height ZIP part"
        );
        if name.ends_with('/') {
            continue;
        }
        let part = PartName::new(&format!("/{name}"))?;
        ensure!(
            canonical_names.insert(part.as_str().to_ascii_lowercase()),
            "Duplicate minimum line height ZIP part"
        );
    }
    ensure!(
        main.as_str() == "/word/document.xml",
        "Unsupported minimum line height main part"
    );
    let package_rels = PartName::new("/_rels/.rels")?;
    check_relationships(&package.read_part(&package_rels)?)?;
    let main_roles = package
        .package_rels()
        .get_by_type(rel_types::OFFICE_DOCUMENT);
    ensure!(
        main_roles.len() == 1 && main_roles[0].target_mode == TargetMode::Internal,
        "Ambiguous minimum line height main relationship"
    );
    let root_part = PartName::new("/root.xml")?;
    ensure!(
        canonical_target(&root_part, &main_roles[0].target)?.as_str() == main.as_str(),
        "Main document relationship identity mismatch"
    );
    let main_rels = PartName::new(&main.rels_path())?;
    if package.has_part(&main_rels) {
        check_relationships(&package.read_part(&main_rels)?)?;
    }
    ensure!(
        rels.duplicate_ids().is_empty(),
        "Duplicate minimum line height relationship IDs"
    );
    check_content_types(&package.read_part(&PartName::new("/[Content_Types].xml")?)?)?;
    ensure!(
        package.content_types().resolve(&main)
            == Some(
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"
            ),
        "Unsupported minimum line height main content type"
    );
    let styles_part = related_part(&rels, &main, rel_types::STYLES)?;
    if let Some(target) = &styles_part {
        ensure!(
            package.content_types().resolve(target)
                == Some(
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"
                ),
            "Unsupported minimum line height styles content type"
        );
    }
    for role in [rel_types::SETTINGS, rel_types::NUMBERING] {
        if let Some(target) = related_part(&rels, &main, role)? {
            let data = package.read_part(&target)?;
            let tree = xml(&data)?;
            ensure!(
                !contains_minimum(&tree),
                "Minimum line height in settings or numbering is unsupported"
            );
            if role == rel_types::SETTINGS {
                let settings = root(&tree, "settings")?;
                locations(settings, None)?;
                let compat = settings.one("compat")?;
                let split = compat
                    .map(|c| c.one("splitPageBreakAndParagraphMark"))
                    .transpose()?
                    .flatten();
                ensure!(
                    settings.all("splitPageBreakAndParagraphMark").len()
                        == usize::from(split.is_some()),
                    "Wrapped legacy page-break setting"
                );
                if let Some(split) = split {
                    disabled(split)?;
                }
            }
        }
    }
    let document = root(&main_xml, "document")?;
    let body = document
        .one("body")?
        .context("Missing minimum line height body")?;
    ensure!(
        document.all("body").len() == 1,
        "Ambiguous minimum line height body"
    );
    ensure!(
        document.all("p").len() == body.children.iter().filter(|n| n.tag == "p").count(),
        "Minimum line height paragraph occurs outside the top-level body"
    );
    ensure!(
        document.children.len() == 1 && document.text.trim().is_empty(),
        "Unsupported document wrapper"
    );
    body.schema(&[], false)?;
    locations(document, None)?;
    let styles_root = root(&styles_xml, "styles")?;
    let sheet = Styles::new(styles_root);
    let defaults = sheet.defaults()?;
    let need_default = body
        .children
        .iter()
        .filter(|p| p.tag == "p")
        .any(|p| p.all("pStyle").is_empty());
    let default_id = if need_default {
        sheet.default_id()?
    } else {
        None
    };
    let mut paragraphs = Vec::new();
    let mut any_tabs = false;
    let mut any_indent = false;
    let mut saw_section = false;
    for child in &body.children {
        if child.tag == "sectPr" {
            ensure!(!saw_section, "Repeated section properties");
            child.identity()?;
            saw_section = true;
            child.one("docGrid")?;
            for tag in [
                "p", "r", "t", "tab", "br", "drawing", "pict", "object", "tbl",
            ] {
                ensure!(
                    child.all(tag).is_empty(),
                    "Unsupported content inside section properties"
                );
            }
            continue;
        }
        ensure!(
            child.tag == "p" && !saw_section,
            "Minimum line height requires simple top-level paragraphs"
        );
        child.identity()?;
        let own = properties(child.one("pPr")?)?;
        let id = own.style.as_deref().or(default_id.as_deref());
        let chain = sheet.chain(id)?;
        let kind = paragraph_kind(&own, &chain, &sheet)?;
        let mut effective = defaults.clone();
        for style in &chain {
            effective.overlay(&sheet.props(style)?);
        }
        effective.overlay(&own);
        let mut content = Vec::new();
        source_inline(child, &mut content)?;
        any_tabs |= effective.tabs;
        any_indent |= effective.nonzero_indent;
        paragraphs.push(Paragraph {
            kind,
            content,
            before: effective.before,
            after: effective.after,
            line: effective.line,
        });
        ensure!(
            paragraphs.len() <= 10_000,
            "Minimum line height paragraph count exceeds limits"
        );
    }
    ensure!(
        !paragraphs.is_empty(),
        "Minimum line height has no source paragraphs"
    );
    ensure!(
        !(any_tabs && any_indent),
        "Custom tab stops depend on paragraph indentation"
    );
    ensure!(
        package.crc_mismatched_parts().is_empty(),
        "Minimum line height package CRC mismatch"
    );
    Ok(MinimumLineHeightAudit {
        source: Some(Source {
            sha256: Sha256::digest(bytes).into(),
            bytes: bytes.len(),
            main_part: main.as_str().to_owned(),
            styles_part: styles_part.map(|p| p.as_str().to_owned()),
            paragraphs,
        }),
    })
}

/// Call immediately before parsing the same immutable snapshot. This catches
/// accidental cross-input reuse without exposing a constructor or cache key.
pub(crate) fn verify_source(audit: &MinimumLineHeightAudit, bytes: &[u8]) -> Result<()> {
    let Some(source) = &audit.source else {
        return Ok(());
    };
    let actual: [u8; 32] = Sha256::digest(bytes).into();
    ensure!(
        source.bytes == bytes.len() && source.sha256 == actual,
        "Minimum line height proof belongs to different source bytes"
    );
    ensure!(
        source.main_part == "/word/document.xml",
        "Invalid proved main part identity"
    );
    if let Some(styles) = &source.styles_part {
        ensure!(
            PartName::new(styles)?.as_str() == styles && !styles.contains('%'),
            "Invalid proved styles part identity"
        );
    }
    Ok(())
}

fn ir_inline(content: &[InlineContent]) -> Result<Vec<Inline>> {
    let mut out = Vec::new();
    for inline in content {
        match inline {
            InlineContent::Text(span) => append_text(&mut out, &span.text),
            InlineContent::LineBreak => out.push(Inline::Break),
            _ => anyhow::bail!(
                "Unsupported inline content appeared during minimum line height conversion"
            ),
        }
    }
    Ok(out)
}
fn ir_line(line: Option<&LineSpacing>) -> Result<Option<Line>> {
    line.map(|line| match line {
        LineSpacing::Auto(n) => Ok(Line::Auto(*n)),
        LineSpacing::AtLeast(n) => Ok(Line::Minimum(*n)),
        _ => anyhow::bail!("Unproved paragraph line mode appeared during conversion"),
    })
    .transpose()
}
pub(crate) fn verify_ir(audit: &MinimumLineHeightAudit, document: &DocumentIR) -> Result<()> {
    let Some(source) = &audit.source else {
        return Ok(());
    };
    let mut expected = source.paragraphs.iter();
    for section in &document.sections {
        ensure!(
            section.header.is_none()
                && section.footer.is_none()
                && section.first_page_header.is_none()
                && section.first_page_footer.is_none()
                && section.even_page_header.is_none()
                && section.even_page_footer.is_none()
                && section.speaker_notes.is_none(),
            "Side stories are outside minimum line height proof"
        );
        for element in &section.elements {
            let (kind, content, before, after, line) = match element {
                Element::Paragraph(p) => (
                    Kind::Paragraph,
                    &p.content,
                    p.space_before_twips,
                    p.space_after_twips,
                    p.line_spacing.as_ref(),
                ),
                Element::Heading(h) => (
                    Kind::Heading(h.level),
                    &h.content,
                    h.space_before_twips,
                    h.space_after_twips,
                    h.line_spacing.as_ref(),
                ),
                _ => anyhow::bail!(
                    "Unsupported block appeared during minimum line height conversion"
                ),
            };
            let source = expected
                .next()
                .context("Extra paragraph appeared during minimum line height conversion")?;
            ensure!(
                source.kind == kind
                    && source.content == ir_inline(content)?
                    && source.before == before
                    && source.after == after
                    && source.line == ir_line(line)?,
                "Minimum line height IR identity or spacing differs from source"
            );
        }
    }
    ensure!(
        expected.next().is_none(),
        "Source paragraph disappeared during minimum line height conversion"
    );
    Ok(())
}
pub(crate) fn verify_model(audit: &MinimumLineHeightAudit, document: &Node) -> Result<()> {
    let Some(source) = &audit.source else {
        return Ok(());
    };
    ensure!(
        !document
            .attrs
            .contains_key("splitPageBreakAndParagraphMark"),
        "Legacy page-break metrics are outside minimum line height proof"
    );
    ensure!(
        document.kind == "doc"
            && document.text.is_none()
            && document.content.len() == source.paragraphs.len(),
        "Minimum line height model paragraph count differs from source"
    );
    for (node, source) in document.content.iter().zip(&source.paragraphs) {
        ensure!(node.text.is_none(), "Uncontained converted paragraph text");
        let get = |key: &str| -> Result<Option<u32>> {
            node.attrs
                .get(key)
                .map(|value| {
                    value
                        .as_u64()
                        .and_then(|v| u32::try_from(v).ok())
                        .context("Invalid converted paragraph spacing value")
                })
                .transpose()
        };
        let kind = match node.kind.as_str() {
            "paragraph" => Kind::Paragraph,
            "heading" => Kind::Heading(u8::try_from(
                get("level")?.context("Missing heading level")?,
            )?),
            _ => anyhow::bail!("Minimum line height moved into an unsupported model block"),
        };
        let mut content = Vec::new();
        for inline in &node.content {
            ensure!(
                inline.content.is_empty(),
                "Nested minimum line height inline content"
            );
            match inline.kind.as_str() {
                "text" => append_text(
                    &mut content,
                    inline.text.as_deref().context("Missing converted text")?,
                ),
                "hardBreak" => {
                    ensure!(inline.text.is_none(), "Text on a converted hard break");
                    content.push(Inline::Break);
                }
                _ => anyhow::bail!("Unsupported minimum line height model inline content"),
            }
        }
        let line = match (get("lineSpacing")?, get("minimumLineHeightTwips")?) {
            (Some(line), None) => Line::Auto(line),
            (None, Some(line)) => Line::Minimum(line),
            _ => anyhow::bail!("Missing or conflicting converted line mode"),
        };
        ensure!(
            kind == source.kind
                && content == source.content
                && get("spaceBefore")? == Some(source.before.unwrap_or(0))
                && get("spaceAfter")? == Some(source.after.unwrap_or(0))
                && line == source.line.unwrap_or(Line::Auto(240)),
            "Minimum line height final model identity or spacing differs from source"
        );
        ensure!(
            !node.attrs.contains_key("breakParagraph"),
            "Legacy page-break metrics are outside minimum line height proof"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    const MIN: &str = r#"<w:spacing w:line="276" w:lineRule="atLeast"/>"#;

    fn parts(body: &str, styles: &str) -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([
            ("[Content_Types].xml".into(), br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/active-styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#.to_vec()),
            ("_rels/.rels".into(), format!(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="main" Type="{R}/officeDocument" Target="word/document.xml"/></Relationships>"#).into_bytes()),
            ("word/_rels/document.xml.rels".into(), format!(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="{R}/styles" Target="active-styles.xml"/></Relationships>"#).into_bytes()),
            ("word/document.xml".into(), format!(r#"<w:document xmlns:w="{W}" xmlns:x="urn:foreign"><w:body>{body}</w:body></w:document>"#).into_bytes()),
            ("word/active-styles.xml".into(), format!(r#"<w:styles xmlns:w="{W}" xmlns:x="urn:foreign">{styles}</w:styles>"#).into_bytes()),
        ])
    }
    fn zipped(parts: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in parts {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }
    fn paragraph(properties: &str, text: &str) -> String {
        format!("<w:p><w:pPr>{properties}</w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>")
    }
    fn audited(body: &str, styles: &str) -> MinimumLineHeightAudit {
        analyze(&zipped(&parts(body, styles))).unwrap()
    }
    fn rejected(body: &str, styles: &str) {
        assert!(
            !matches!(analyze(&zipped(&parts(body, styles))), Ok(audit) if audit.proved()),
            "Unexpected source acceptance: {body} {styles}"
        );
    }
    fn parsed(bytes: &[u8]) -> DocumentIR {
        office_oxide::Document::from_reader(
            Cursor::new(bytes.to_vec()),
            office_oxide::DocumentFormat::Docx,
        )
        .unwrap()
        .to_ir()
    }
    fn model() -> Node {
        serde_json::from_value(json!({"type":"doc","content":[
            {"type":"paragraph","attrs":{"spaceBefore":0,"spaceAfter":0,"minimumLineHeightTwips":276},"content":[{"type":"text","text":"A"}]},
            {"type":"heading","attrs":{"level":2,"spaceBefore":0,"spaceAfter":0,"lineSpacing":240},"content":[{"type":"text","text":"B"},{"type":"hardBreak"},{"type":"text","text":"C"}]}
        ]})).unwrap()
    }

    #[test]
    fn minimum_bounds_and_bad_spacing_are_checked_before_overrides() {
        for n in [1, 276, 31680] {
            let own = format!(r#"<w:spacing w:line="{n}" w:lineRule="atLeast"/>"#);
            let audit = audited(&paragraph(&own, "A"), "");
            assert!(audit.proved() && audit.has_minimum());
        }
        for n in [
            "0",
            "31681",
            "-1",
            "+1",
            "1.5",
            "12pt",
            " 276",
            "4294967296",
            "",
        ] {
            let own = format!(r#"<w:spacing w:line="{n}" w:lineRule="atLeast"/>"#);
            rejected(&paragraph(&own, "A"), "");
        }
        for bad in [
            r#"<w:spacing w:line="276" w:lineRule="exact"/>"#,
            r#"<w:spacing w:line="276" w:lineRule="unknown"/>"#,
            r#"<w:spacing w:lineRule="atLeast"/>"#,
            r#"<w:spacing w:line="276" w:lineRule="atLeast" w:beforeLines="100"/>"#,
            r#"<w:spacing w:line="276" w:lineRule="atLeast" w:afterAutospacing="1"/>"#,
            r#"<w:spacing w:line="276" w:lineRule="atLeast" w:after="-1"/>"#,
        ] {
            let styles = format!(
                r#"<w:style w:type="paragraph" w:styleId="Base"><w:pPr>{bad}</w:pPr></w:style>"#
            );
            rejected(
                &paragraph(&format!(r#"<w:pStyle w:val="Base"/>{MIN}"#), "A"),
                &styles,
            );
        }
    }

    #[test]
    fn minimum_default_chain_and_independent_gap_overrides_bind_to_ir() {
        let styles = format!(
            r#"<w:docDefaults><w:pPrDefault><w:pPr><w:spacing w:before="100" w:after="200" w:line="300"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:styleId="Base"><w:pPr>{MIN}</w:pPr></w:style><w:style w:type="paragraph" w:styleId="Default" w:default="1"><w:basedOn w:val="Base"/><w:pPr><w:spacing w:after="400"/></w:pPr></w:style>"#
        );
        let body = format!(
            "{}{}{}",
            paragraph(r#"<w:spacing w:before="0"/>"#, "A"),
            paragraph(r#"<w:spacing w:line="360"/>"#, "B"),
            paragraph(r#"<w:spacing w:after="0"/>"#, "C")
        );
        let bytes = zipped(&parts(&body, &styles));
        let audit = analyze(&bytes).unwrap();
        verify_ir(&audit, &parsed(&bytes)).unwrap();
        let paragraphs = &audit.source.as_ref().unwrap().paragraphs;
        assert_eq!(
            (
                paragraphs[0].before,
                paragraphs[0].after,
                paragraphs[0].line
            ),
            (Some(0), Some(400), Some(Line::Minimum(276)))
        );
        assert_eq!(paragraphs[1].line, Some(Line::Auto(360)));
        assert_eq!(
            (paragraphs[2].before, paragraphs[2].after),
            (Some(100), Some(0))
        );
    }

    #[test]
    fn minimum_overridden_by_auto_has_proof_but_no_warning() {
        let styles = format!(
            r#"<w:style w:type="paragraph" w:styleId="Default" w:default="1"><w:pPr>{MIN}</w:pPr></w:style>"#
        );
        for spacing in [
            r#"<w:spacing w:line="240"/>"#,
            r#"<w:spacing w:line="240" w:lineRule="auto"/>"#,
        ] {
            let bytes = zipped(&parts(&paragraph(spacing, "A"), &styles));
            let audit = analyze(&bytes).unwrap();
            assert!(audit.proved());
            assert!(!audit.has_minimum());
            verify_ir(&audit, &parsed(&bytes)).unwrap();
        }
    }

    #[test]
    fn minimum_does_not_inject_unrelated_default_or_inspect_unused_properties() {
        let styles = format!(
            r#"<w:style w:type="paragraph" w:styleId="Default" w:default="1"><w:pPr>{MIN}</w:pPr></w:style><w:style w:type="paragraph" w:styleId="Explicit"/><w:style w:type="paragraph" w:styleId="Unused"><w:pPr><w:spacing w:line="480" w:lineRule="exact"/></w:pPr></w:style>"#
        );
        let bytes = zipped(&parts(
            &paragraph(r#"<w:pStyle w:val="Explicit"/>"#, "A"),
            &styles,
        ));
        assert!(!analyze(&bytes).unwrap().proved());
        let bytes = zipped(&parts(
            &paragraph(&format!(r#"<w:pStyle w:val="Explicit"/>{MIN}"#), "A"),
            &styles,
        ));
        let audit = analyze(&bytes).unwrap();
        verify_ir(&audit, &parsed(&bytes)).unwrap();
        assert_eq!(audit.source.as_ref().unwrap().paragraphs[0].before, None);
    }

    #[test]
    fn minimum_inactive_historical_inventory_needs_no_opc_proof() {
        let bytes = zipped(&BTreeMap::from([(
            "word/document.xml".into(),
            b"<not-an-opc-package/>".to_vec(),
        )]));
        assert!(!analyze(&bytes).unwrap().proved());
    }

    #[test]
    fn minimum_inactive_large_xml_and_atleast_text_do_not_gain_spacing_risks() {
        let text = format!("atLeast{}", " ".repeat(16 * 1024 * 1024));
        let bytes = zipped(&parts(&paragraph("", &text), ""));
        assert!(!analyze(&bytes).unwrap().proved());
        let mut package = parts(&paragraph("", "atLeast is ordinary text"), "");
        package.insert("word/active-styles.xml".into(), b"<malformed>".to_vec());
        assert!(!analyze(&zipped(&package)).unwrap().proved());
        let unused = format!(
            r#"<w:style w:type="paragraph" w:styleId="Unused"><w:pPr>{MIN}<w:spacing w:line="not-a-number"/></w:pPr></w:style>"#
        );
        assert!(!audited(&paragraph("", "A"), &unused).proved());
    }

    #[test]
    fn minimum_source_binding_refuses_same_length_different_bytes() {
        let bytes = zipped(&parts(&paragraph(MIN, "A"), ""));
        let mut other = bytes.clone();
        *other.last_mut().unwrap() ^= 1;
        let audit = analyze(&bytes).unwrap();
        verify_source(&audit, &bytes).unwrap();
        assert_eq!(bytes.len(), other.len());
        assert!(verify_source(&audit, &other).is_err());
        assert!(verify_source(&audit, &bytes[..bytes.len() - 1]).is_err());
        assert_eq!(
            audit.source.as_ref().unwrap().main_part,
            "/word/document.xml"
        );
    }

    #[test]
    fn minimum_refuses_legacy_page_break_settings_and_final_root_attribute() {
        let original = parts(&paragraph(MIN, "A"), "");
        for toggle in ["", r#" w:val="true""#, r#" w:val="invalid""#] {
            let mut package = original.clone();
            let rels = String::from_utf8(package["word/_rels/document.xml.rels"].clone()).unwrap().replace("</Relationships>", &format!(r#"<Relationship Id="settings" Type="{R}/settings" Target="settings.xml"/></Relationships>"#));
            package.insert("word/_rels/document.xml.rels".into(), rels.into_bytes());
            package.insert("word/settings.xml".into(), format!(r#"<w:settings xmlns:w="{W}"><w:compat><w:splitPageBreakAndParagraphMark{toggle}/></w:compat></w:settings>"#).into_bytes());
            assert!(analyze(&zipped(&package)).is_err());
        }
        let bytes = zipped(&original);
        let audit = analyze(&bytes).unwrap();
        let mut doc: Node = serde_json::from_value(json!({"type":"doc","attrs":{"splitPageBreakAndParagraphMark":true},"content":[
            {"type":"paragraph","attrs":{"spaceBefore":0,"spaceAfter":0,"minimumLineHeightTwips":276},"content":[{"type":"text","text":"A"}]}
        ]})).unwrap();
        assert!(verify_model(&audit, &doc).is_err());
        doc.attrs.remove("splitPageBreakAndParagraphMark");
        verify_model(&audit, &doc).unwrap();
    }

    #[test]
    fn minimum_uses_actual_nonstandard_styles_relationship() {
        let style = format!(
            r#"<w:style w:type="paragraph" w:styleId="Body" w:default="1"><w:pPr>{MIN}</w:pPr></w:style>"#
        );
        let mut package = parts(&paragraph("", "A"), &style);
        package.insert("word/styles.xml".into(), format!(r#"<w:styles xmlns:w="{W}"><w:docDefaults><w:pPrDefault><w:pPr><w:spacing w:line="480" w:lineRule="exact"/></w:pPr></w:pPrDefault></w:docDefaults></w:styles>"#).into_bytes());
        let bytes = zipped(&package);
        let audit = analyze(&bytes).unwrap();
        assert_eq!(
            audit.source.as_ref().unwrap().styles_part.as_deref(),
            Some("/word/active-styles.xml")
        );
        verify_ir(&audit, &parsed(&bytes)).unwrap();
    }

    #[test]
    fn minimum_rejects_ambiguous_styles_cycles_aliases_and_depth() {
        let body = paragraph(&format!(r#"<w:pStyle w:val="A"/>{MIN}"#), "A");
        for styles in [
            String::new(),
            r#"<w:style w:type="character" w:styleId="A"/>"#.into(),
            r#"<w:style w:type="paragraph" w:styleId="A"/><w:style w:type="paragraph" w:styleId="A"/>"#.into(),
            r#"<w:style w:type="paragraph" w:styleId="A"><w:basedOn w:val="A"/></w:style>"#.into(),
            r#"<w:style w:type="paragraph" w:styleId="A"><w:basedOn w:val="Missing"/></w:style>"#.into(),
            r#"<w:style w:type="paragraph" x:styleId="A"/>"#.into(),
            r#"<w:style w:type="paragraph" w:styleId="A" x:styleId="A"/>"#.into(),
            r#"<x:style w:type="paragraph" w:styleId="A"/>"#.into(),
        ] { rejected(&body, &styles); }
        let defaults = r#"<w:style w:type="paragraph" w:styleId="A" w:default="1"/><w:style w:type="paragraph" w:styleId="B" w:default="true"/>"#;
        rejected(&paragraph(MIN, "A"), defaults);
        let mut chain = String::new();
        for n in 0..21 {
            let base = if n < 20 {
                format!(r#"<w:basedOn w:val="S{}"/>"#, n + 1)
            } else {
                String::new()
            };
            chain.push_str(&format!(
                r#"<w:style w:type="paragraph" w:styleId="S{n}">{base}</w:style>"#
            ));
        }
        rejected(
            &paragraph(&format!(r#"<w:pStyle w:val="S0"/>{MIN}"#), "A"),
            &chain,
        );
    }

    #[test]
    fn minimum_tabs_are_generic_inactive_left_stops_only() {
        for position in [0, 37, 709, 31680] {
            let props = format!(
                r#"{MIN}<w:tabs><w:tab w:val="left" w:pos="{position}" w:leader="none"/></w:tabs>"#
            );
            assert!(audited(&paragraph(&props, "No tabs"), "").proved());
            rejected(&paragraph(&props, "Literal\ttab"), "");
            rejected(
                &format!("<w:p><w:pPr>{props}</w:pPr><w:r><w:tab/></w:r></w:p>"),
                "",
            );
            rejected(
                &paragraph(&format!(r#"{props}<w:ind w:left="1"/>"#), "A"),
                "",
            );
        }
        for stop in [
            r#"<w:tab w:val="center" w:pos="709"/>"#,
            r#"<w:tab w:val="left" w:pos="-1"/>"#,
            r#"<w:tab w:val="left" w:pos="31681"/>"#,
            r#"<w:tab w:val="left" w:pos="709pt"/>"#,
            r#"<w:tab w:val="left" w:pos="709" w:leader="dot"/>"#,
            r#"<w:tab w:val="left" x:pos="709"/>"#,
            r#"<w:tab w:val="left" w:pos="709"/><w:tab w:val="left" w:pos="709"/>"#,
        ] {
            rejected(
                &paragraph(&format!("{MIN}<w:tabs>{stop}</w:tabs>"), "A"),
                "",
            );
        }
    }

    #[test]
    fn minimum_rejects_dependencies_and_unsupported_contexts() {
        for dependency in [
            "<w:contextualSpacing/>",
            "<w:snapToGrid/>",
            "<w:pPrChange/>",
            r#"<w:numPr><w:numId w:val="1"/></w:numPr>"#,
            r#"<w:numPr><w:numId w:val="0"/></w:numPr>"#,
        ] {
            rejected(&paragraph(&format!("{MIN}{dependency}"), "A"), "");
        }
        assert!(
            audited(
                &paragraph(
                    &format!(r#"{MIN}<w:contextualSpacing w:val="false"/>"#),
                    "A"
                ),
                ""
            )
            .proved()
        );
        let p = paragraph(MIN, "A");
        rejected(&format!("<w:tbl><w:tr><w:tc>{p}</w:tc></w:tr></w:tbl>"), "");
        rejected(&format!("{p}<w:p><w:r><w:drawing/></w:r></w:p>"), "");
        rejected(
            &format!(r#"{p}<w:sectPr><w:docGrid w:type="lines"/></w:sectPr>"#),
            "",
        );
        assert!(audited(&format!(r#"{p}<w:sectPr><w:docGrid/></w:sectPr>"#), "").proved());
        rejected(
            &format!(r#"<w:p><w:pPr>{MIN}</w:pPr><w:r><w:br w:type="page"/></w:r></w:p>"#),
            "",
        );
        rejected(&format!("{p}<w:p><w:r><w:fldChar/></w:r></w:p>"), "");
        rejected(
            &paragraph(
                &format!(
                    r#"{MIN}<w:outlineLvl w:val="0"/><w:numPr><w:numId w:val="1"/></w:numPr>"#
                ),
                "A",
            ),
            "",
        );
    }

    #[test]
    fn minimum_heading_policy_does_not_promote_implicit_defaults() {
        let styles = format!(
            r#"<w:style w:type="paragraph" w:styleId="Heading2" w:default="1"><w:name w:val="heading 2"/><w:pPr>{MIN}</w:pPr></w:style>"#
        );
        for (props, kind) in [
            ("", Kind::Paragraph),
            (r#"<w:pStyle w:val="Heading2"/>"#, Kind::Heading(2)),
            (r#"<w:outlineLvl w:val="0"/>"#, Kind::Heading(1)),
        ] {
            let bytes = zipped(&parts(&paragraph(props, "A"), &styles));
            let audit = analyze(&bytes).unwrap();
            assert_eq!(audit.source.as_ref().unwrap().paragraphs[0].kind, kind);
            verify_ir(&audit, &parsed(&bytes)).unwrap();
        }
        rejected(
            &paragraph(&format!(r#"{MIN}<w:outlineLvl w:val="3"/>"#), "A"),
            "",
        );
    }

    #[test]
    fn minimum_rejects_literal_identity_duplicates_and_malformed_package_roles() {
        for own in [
            r#"<w:spacing w:line="276" x:lineRule="atLeast"/>"#,
            r#"<w:spacing w:line="276" w:lineRule="atLeast" x:line="276"/>"#,
            r#"<x:spacing w:line="276" w:lineRule="atLeast"/>"#,
            r#"<w:spacing w:line="276" w:line="276" w:lineRule="atLeast"/>"#,
        ] {
            rejected(&paragraph(own, "A"), "");
        }
        rejected(&paragraph(&format!("{MIN}{MIN}"), "A"), "");
        let base = parts(&paragraph(MIN, "A"), "");
        for replacement in [
            format!(
                r#"<Relationship Id="a" Type="{R}/styles" Target="active-styles.xml"/><Relationship Id="b" Type="{R}/styles" Target="active-styles.xml"/>"#
            ),
            format!(r#"<Relationship Id="a" Type="{R}/styles" Target="./active-styles.xml"/>"#),
            format!(
                r#"<Relationship Id="a" Type="{R}/styles" Target="active-styles.xml" TargetMode="External"/>"#
            ),
        ] {
            let mut package = base.clone();
            package.insert("word/_rels/document.xml.rels".into(), format!(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{replacement}</Relationships>"#).into_bytes());
            assert!(analyze(&zipped(&package)).is_err());
        }
        let mut package = base.clone();
        package.insert(
            "WORD/document.xml".into(),
            package["word/document.xml"].clone(),
        );
        let bytes = zipped(&package);
        assert!(!analyze(&bytes).unwrap().proved());
        // Discovery cannot grant a capability when the preexisting OPC reader
        // rejects canonical duplicates. Verify the public safety boundary too.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("duplicate-canonical-part.docx");
        std::fs::write(&path, &bytes).unwrap();
        assert!(crate::compatibility::audit_docx(&path).is_err());
        let mut package = base;
        let types = String::from_utf8(package["[Content_Types].xml"].clone())
            .unwrap()
            .replace(
                "wordprocessingml.styles+xml",
                "wordprocessingml.numbering+xml",
            );
        package.insert("[Content_Types].xml".into(), types.into_bytes());
        assert!(analyze(&zipped(&package)).is_err());
    }

    #[test]
    fn minimum_full_order_text_kind_gaps_and_typed_line_are_bound() {
        let body = format!(
            r#"{}<w:p><w:pPr><w:outlineLvl w:val="1"/></w:pPr><w:r><w:t>B</w:t><w:br/><w:t>C</w:t></w:r></w:p>"#,
            paragraph(MIN, "A")
        );
        let bytes = zipped(&parts(&body, ""));
        let audit = analyze(&bytes).unwrap();
        let ir = parsed(&bytes);
        verify_ir(&audit, &ir).unwrap();
        verify_model(&audit, &model()).unwrap();
        for edit in 0..7 {
            let mut changed = ir.clone();
            match edit {
                0 => {
                    changed.sections[0].elements.swap(0, 1);
                }
                1 => {
                    changed.sections[0].elements.pop();
                }
                2 => {
                    let Element::Paragraph(p) = &mut changed.sections[0].elements[0] else {
                        panic!()
                    };
                    p.content = Vec::new();
                }
                3 => {
                    let Element::Heading(h) = &mut changed.sections[0].elements[1] else {
                        panic!()
                    };
                    h.level = 1;
                }
                4 => {
                    let Element::Paragraph(p) = &mut changed.sections[0].elements[0] else {
                        panic!()
                    };
                    p.space_before_twips = Some(0);
                }
                5 => {
                    let Element::Paragraph(p) = &mut changed.sections[0].elements[0] else {
                        panic!()
                    };
                    p.line_spacing = Some(LineSpacing::Auto(276));
                }
                _ => {
                    let Element::Heading(h) = &mut changed.sections[0].elements[1] else {
                        panic!()
                    };
                    h.content
                        .retain(|inline| !matches!(inline, InlineContent::LineBreak));
                }
            }
            assert!(verify_ir(&audit, &changed).is_err());
        }
        for edit in 0..8 {
            let mut changed = model();
            match edit {
                0 => changed.content.swap(0, 1),
                1 => {
                    changed.content.pop();
                }
                2 => changed.content[0].content[0].text = Some("wrong".into()),
                3 => {
                    changed.content[1].attrs.insert("level".into(), 1.into());
                }
                4 => {
                    changed.content[0]
                        .attrs
                        .insert("spaceBefore".into(), 1.into());
                }
                5 => {
                    changed.content[0]
                        .attrs
                        .insert("lineSpacing".into(), 276.into());
                }
                6 => {
                    changed.content[1].content[1].kind = "text".into();
                    changed.content[1].content[1].text = Some("\n".into());
                }
                _ => changed.content[0].kind = "listItem".into(),
            }
            assert!(verify_model(&audit, &changed).is_err());
        }
    }

    #[test]
    fn minimum_verification_is_read_only_and_keeps_unrelated_metadata() {
        let audit = audited(&paragraph(MIN, "A"), "");
        let doc: Node = serde_json::from_value(json!({"type":"doc","attrs":{"pageGeometry":{"widthTwips":12240}},"content":[
            {"type":"paragraph","attrs":{"spaceBefore":0,"spaceAfter":0,"minimumLineHeightTwips":276,"keepWithNext":true,"widowControl":true},"content":[
                {"type":"text","text":"A","marks":[{"type":"bold"},{"type":"textStyle","attrs":{"fontFamily":"Calibri","color":"#123456"}},{"type":"link","attrs":{"href":"https://example.com"}}]}
            ]}
        ]})).unwrap();
        let before = serde_json::to_value(&doc).unwrap();
        verify_model(&audit, &doc).unwrap();
        assert_eq!(serde_json::to_value(&doc).unwrap(), before);
    }
}
