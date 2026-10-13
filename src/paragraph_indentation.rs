//! Conservative source proof for physical paragraph indentation. This audit
//! runs on the same decoded OPC parts as the parser, before it can discard
//! declarations, and independently resolves the supported style cascade.
use anyhow::{Result, ensure};
use quick_xml::{NsReader, events::Event, name::ResolveResult};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
};

pub(crate) const PARAGRAPH_INDENT_RISK: &str =
    "Unsupported paragraph indentation cannot be preserved in an editable copy";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ParagraphIndent {
    pub left: Option<i32>,
    pub right: Option<i32>,
    pub first_line: Option<i32>,
    // Text is an identity cross-check, not the source of imported content.
    text: String,
}
impl ParagraphIndent {
    fn overlay(&mut self, other: &Self) {
        if other.left.is_some() {
            self.left = other.left;
        }
        if other.right.is_some() {
            self.right = other.right;
        }
        // firstLine and hanging are two spellings of one effective slot.
        if other.first_line.is_some() {
            self.first_line = other.first_line;
        }
    }
    fn present(&self) -> bool {
        self.left.is_some() || self.right.is_some() || self.first_line.is_some()
    }
    fn nonzero(&self) -> bool {
        [self.left, self.right, self.first_line]
            .into_iter()
            .flatten()
            .any(|v| v != 0)
    }
}

#[derive(Default)]
pub(crate) struct ParagraphIndentAudit {
    /// Only the supported paragraphs with a declared effective component.
    /// Ordinary list geometry keeps the previous explicit normalization path.
    pub paragraphs: Vec<ParagraphIndent>,
}

#[derive(Default)]
pub(crate) struct Xml {
    pub(crate) tag: String,
    pub(crate) raw: String,
    pub(crate) wml: bool,
    pub(crate) attrs: BTreeMap<String, (String, bool)>,
    pub(crate) children: Vec<Xml>,
    pub(crate) text: String,
    pub(crate) size: usize,
}
fn wml(ns: ResolveResult<'_>) -> bool {
    matches!(ns, ResolveResult::Bound(ns) if matches!(ns.as_ref(),
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main" |
        "http://purl.oclc.org/ooxml/wordprocessingml/main"))
}
impl Xml {
    pub(crate) fn identity(&self) -> Result<()> {
        ensure!(
            self.wml && self.raw == format!("w:{}", self.tag),
            "Unsupported paragraph indentation element identity: {}",
            self.raw
        );
        Ok(())
    }
    pub(crate) fn attr(&self, name: &str) -> Result<Option<&str>> {
        let mut attrs = self
            .attrs
            .iter()
            .filter(|(key, _)| key.rsplit(':').next() == Some(name));
        let first = attrs.next();
        ensure!(
            attrs.next().is_none(),
            "Conflicting paragraph indentation attribute: {name}"
        );
        match first {
            Some((key, (value, bound))) => {
                ensure!(
                    *key == format!("w:{name}") && *bound,
                    "Unsupported paragraph indentation attribute identity: {name}"
                );
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }
    // Candidate discovery must include same-local-name foreign attributes:
    // otherwise a malformed default/style/numbering identity could be hidden.
    pub(crate) fn has_number(&self, name: &str, value: u32) -> bool {
        self.attrs.iter().any(|(key, (v, _))| {
            key.rsplit(':').next() == Some(name) && v.parse::<u32>().ok() == Some(value)
        })
    }
    pub(crate) fn has_value(&self, name: &str, value: &str) -> bool {
        self.attrs
            .iter()
            .any(|(key, (v, _))| key.rsplit(':').next() == Some(name) && v == value)
    }
    pub(crate) fn one(&self, tag: &str) -> Result<Option<&Xml>> {
        let mut found = self.children.iter().filter(|n| n.tag == tag);
        let first = found.next();
        ensure!(
            found.next().is_none(),
            "Repeated paragraph indentation declaration: {tag}"
        );
        if let Some(node) = first {
            node.identity()?;
        }
        Ok(first)
    }
    pub(crate) fn schema(&self, allowed: &[&str], leaf: bool) -> Result<()> {
        self.identity()?;
        ensure!(
            self.text.trim().is_empty() && (!leaf || self.children.is_empty()),
            "Unsupported paragraph indentation declaration: {}",
            self.tag
        );
        for key in self.attrs.keys() {
            let name = key.rsplit(':').next().expect("attribute");
            ensure!(
                allowed.contains(&name),
                "Unsupported paragraph indentation attribute: {key}"
            );
            self.attr(name)?;
        }
        Ok(())
    }
    fn descendants<'a>(&'a self, tag: &str, found: &mut Vec<&'a Xml>) {
        for child in &self.children {
            if child.tag == tag {
                found.push(child);
            }
            child.descendants(tag, found);
        }
    }
    pub(crate) fn all(&self, tag: &str) -> Vec<&Xml> {
        let mut found = Vec::new();
        self.descendants(tag, &mut found);
        found
    }
}
pub(crate) fn xml(data: &[u8]) -> Result<Xml> {
    let mut reader = NsReader::from_reader(data);
    let mut stack = vec![Xml::default()];
    let mut count = 0;
    loop {
        let (namespace, event) = reader.read_resolved_event()?;
        let is_wml = wml(namespace);
        let start = matches!(&event, Event::Start(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                count += 1;
                ensure!(
                    count <= 100_000 && stack.len() <= 128,
                    "Paragraph indentation metadata exceeds limits"
                );
                let mut node = Xml {
                    tag: e.local_name().as_ref().to_owned(),
                    raw: e.name().as_ref().to_owned(),
                    wml: is_wml,
                    size: 1,
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
                                    wml(ns)
                                )
                            )
                            .is_none(),
                        "Duplicate XML attribute"
                    );
                }
                if start {
                    stack.push(node);
                } else {
                    let parent = stack.last_mut().expect("root");
                    parent.size += node.size;
                    parent.children.push(node);
                }
            }
            Event::End(_) => {
                ensure!(stack.len() > 1, "Unbalanced paragraph indentation XML");
                let node = stack.pop().expect("element");
                let parent = stack.last_mut().expect("root");
                parent.size += node.size;
                parent.children.push(node);
            }
            Event::Text(value) => stack
                .last_mut()
                .expect("root")
                .text
                .push_str(&value.xml_content(quick_xml::XmlVersion::Implicit1_0)),
            Event::CData(value) => stack
                .last_mut()
                .expect("root")
                .text
                .push_str(&value.xml_content(quick_xml::XmlVersion::Implicit1_0)),
            Event::GeneralRef(value) => {
                let current = &mut stack.last_mut().expect("root").text;
                if let Some(ch) = value.resolve_char_ref()? {
                    current.push(ch);
                } else {
                    current.push_str(
                        quick_xml::escape::resolve_predefined_entity(&value)
                            .ok_or_else(|| anyhow::anyhow!("Unsupported XML entity"))?,
                    );
                }
            }
            Event::DocType(_) => anyhow::bail!("DTD is not supported in DOCX XML"),
            Event::Eof => {
                ensure!(stack.len() == 1, "Incomplete paragraph indentation XML");
                return Ok(stack.pop().expect("root"));
            }
            _ => {}
        }
    }
}
pub(crate) fn root<'a>(tree: &'a Xml, tag: &str) -> Result<&'a Xml> {
    ensure!(
        tree.children.len() == 1 && tree.children[0].tag == tag && tree.text.trim().is_empty(),
        "Unsupported paragraph indentation part identity"
    );
    let root = &tree.children[0];
    root.identity()?;
    Ok(root)
}
fn integer(value: &str, signed: bool, max: i32) -> Result<i32> {
    let digits = if signed {
        value.strip_prefix('-').unwrap_or(value)
    } else {
        value
    };
    ensure!(
        !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()),
        "Paragraph indentation requires integer twips"
    );
    let n: i32 = value.parse()?;
    ensure!(
        (-max..=max).contains(&n) && (signed || n >= 0),
        "Paragraph indentation outside supported bounds"
    );
    Ok(n)
}
fn boolean(value: &str) -> Result<bool> {
    match value {
        "1" | "true" | "on" => Ok(true),
        "0" | "false" | "off" => Ok(false),
        _ => anyhow::bail!("Unknown paragraph indentation toggle"),
    }
}
fn disabled(node: &Xml) -> Result<()> {
    node.schema(&["val"], true)?;
    ensure!(
        !node.attr("val")?.map(boolean).transpose()?.unwrap_or(true),
        "Unsupported active indentation setting: {}",
        node.tag
    );
    Ok(())
}
fn indent(node: &Xml) -> Result<ParagraphIndent> {
    node.schema(&["left", "right", "firstLine", "hanging"], true)?;
    ensure!(
        !(node.attr("firstLine")?.is_some() && node.attr("hanging")?.is_some()),
        "Conflicting firstLine and hanging indentation"
    );
    let number = |name| {
        node.attr(name)?
            .map(|v| integer(v, matches!(name, "left" | "right"), 31680))
            .transpose()
    };
    Ok(ParagraphIndent {
        left: number("left")?,
        right: number("right")?,
        first_line: match number("hanging")? {
            Some(value) => Some(-value),
            None => number("firstLine")?,
        },
        ..Default::default()
    })
}

#[derive(Clone, Default)]
struct Properties {
    indent: ParagraphIndent,
    numbering: Option<(u32, u8)>,
    style: Option<String>,
    custom_tabs: bool,
    indent_declared: bool,
    // Keep negative source declarations visible even when overridden. Their
    // newly supported parsing must not relax image, table or list contracts.
    negative_sides_declared: bool,
}
impl Properties {
    fn overlay(&mut self, other: &Self) {
        self.indent.overlay(&other.indent);
        self.custom_tabs |= other.custom_tabs;
        self.indent_declared |= other.indent_declared;
        self.negative_sides_declared |= other.negative_sides_declared;
        if other.numbering.is_some() {
            self.numbering = other.numbering;
        }
    }
}
fn value(node: &Xml) -> Result<&str> {
    node.schema(&["val"], true)?;
    let value = node
        .attr("val")?
        .ok_or_else(|| anyhow::anyhow!("Missing paragraph property value"))?;
    ensure!(!value.is_empty(), "Empty paragraph property value");
    Ok(value)
}
fn properties(node: Option<&Xml>) -> Result<Properties> {
    let Some(node) = node else {
        return Ok(Properties::default());
    };
    node.schema(&[], false)?;
    check_locations(node, None)?;
    let mut props = Properties::default();
    if let Some(tabs) = node.one("tabs")? {
        tabs.schema(&[], false)?;
        props.custom_tabs = !tabs.children.is_empty();
    }
    props.custom_tabs |= node.all("tabs").len() > usize::from(node.one("tabs")?.is_some());
    if let Some(ind) = node.one("ind")? {
        props.indent_declared = true;
        props.indent = indent(ind)?;
        props.negative_sides_declared = ["left", "right"].into_iter().any(|name| {
            ind.attrs
                .get(&format!("w:{name}"))
                .is_some_and(|(value, _)| value.starts_with('-'))
        });
    }
    if let Some(style) = node.one("pStyle")? {
        props.style = Some(value(style)?.to_owned());
    }
    for tag in ["bidi", "mirrorIndents", "adjustRightInd"] {
        if let Some(toggle) = node.one(tag)? {
            disabled(toggle)?;
        }
    }
    if let Some(direction) = node.one("textDirection")? {
        ensure!(
            value(direction)? == "lrTb",
            "Unsupported paragraph text direction"
        );
    }
    if let Some(numbering) = node.one("numPr")? {
        numbering.schema(&[], false)?;
        ensure!(
            numbering
                .children
                .iter()
                .all(|child| matches!(child.tag.as_str(), "numId" | "ilvl")),
            "Unsupported paragraph numbering declaration"
        );
        let id = numbering
            .one("numId")?
            .map(value)
            .transpose()?
            .map(|v| integer(v, false, i32::MAX))
            .transpose()?
            .unwrap_or(0);
        let level = numbering
            .one("ilvl")?
            .map(value)
            .transpose()?
            .map(|v| integer(v, false, 8))
            .transpose()?
            .unwrap_or(0);
        props.numbering = Some((id as u32, level as u8));
    }
    Ok(props)
}

// Relevant declarations must occur where the pinned reader consumes them.
// This also prevents historical pPrChange data or a wrapper from becoming an
// apparent current indent, or hiding an unsupported current declaration.
fn check_locations(node: &Xml, parent: Option<&str>) -> Result<()> {
    match node.tag.as_str() {
        "pPrChange" | "numPrChange" => {
            anyhow::bail!("Revised paragraph properties are unsupported")
        }
        "ind" => {
            ensure!(parent == Some("pPr"), "Non-direct paragraph indentation");
            indent(node)?;
        }
        "pPr" => {
            node.identity()?;
            if let Some(parent) = parent {
                ensure!(
                    matches!(parent, "p" | "style" | "pPrDefault" | "tblStylePr" | "lvl"),
                    "Non-direct paragraph properties"
                );
            }
            node.one("ind")?;
            node.one("pStyle")?;
            node.one("numPr")?;
        }
        "pStyle" => {
            // A level's pStyle is a valid association to a paragraph style,
            // distinct from the paragraph's pPr/pStyle inheritance selector.
            ensure!(
                matches!(parent, Some("pPr" | "lvl")),
                "Non-direct paragraph style reference"
            );
            value(node)?;
        }
        "numPr" | "mirrorIndents" | "adjustRightInd" => {
            node.identity()?;
            ensure!(parent == Some("pPr"), "Non-direct paragraph property");
        }
        "bidi" | "textDirection" if parent != Some("sectPr") && parent != Some("settings") => {
            node.identity()?;
            ensure!(parent == Some("pPr"), "Non-direct paragraph direction");
            if node.tag == "bidi" {
                disabled(node)?;
            } else {
                ensure!(
                    value(node)? == "lrTb",
                    "Unsupported paragraph text direction"
                );
            }
        }
        "rtl" => {
            disabled(node)?;
        }
        "basedOn" => {
            ensure!(parent == Some("style"), "Non-direct style inheritance");
            value(node)?;
        }
        "numId" | "ilvl" => {
            node.identity()?;
            ensure!(parent == Some("numPr"), "Non-direct paragraph numbering");
        }
        _ => {}
    }
    for child in &node.children {
        check_locations(child, Some(&node.tag))?;
    }
    Ok(())
}

struct Styles<'a> {
    defaults: Properties,
    paragraph_defaults: Vec<&'a Xml>,
    table_defaults: Vec<&'a Xml>,
    index: BTreeMap<String, Vec<&'a Xml>>,
    direct_styles: BTreeSet<usize>,
    chain_cache: RefCell<BTreeMap<(String, String), Vec<&'a Xml>>>,
    properties_cache: RefCell<BTreeMap<usize, Properties>>,
    cell_geometry_checked: RefCell<BTreeSet<usize>>,
    margin_proofs: RefCell<std::vec::IntoIter<bool>>,
    unresolved_chains: RefCell<BTreeSet<(String, String)>>,
    fallback_indent: Cell<Option<bool>>,
}
impl<'a> Styles<'a> {
    fn new(root: &'a Xml, margin_proofs: Vec<bool>) -> Result<Self> {
        let mut sheet = Self {
            defaults: Properties::default(),
            paragraph_defaults: Vec::new(),
            table_defaults: Vec::new(),
            index: BTreeMap::new(),
            direct_styles: root
                .children
                .iter()
                .filter(|n| n.tag == "style")
                .map(|n| n as *const Xml as usize)
                .collect(),
            chain_cache: RefCell::default(),
            properties_cache: RefCell::default(),
            cell_geometry_checked: RefCell::default(),
            margin_proofs: RefCell::new(margin_proofs.into_iter()),
            unresolved_chains: RefCell::default(),
            fallback_indent: Cell::new(None),
        };
        let defaults = root.one("docDefaults")?;
        ensure!(
            root.all("docDefaults").len() == usize::from(defaults.is_some()),
            "Non-direct document defaults"
        );
        if let Some(defaults) = defaults {
            let pp = defaults.one("pPrDefault")?;
            ensure!(
                defaults.all("pPr").len()
                    == usize::from(pp.map(|n| n.one("pPr")).transpose()?.flatten().is_some()),
                "Non-direct default paragraph properties"
            );
            for rtl in defaults.all("rtl") {
                disabled(rtl)?;
            }
            sheet.defaults = properties(pp.map(|n| n.one("pPr")).transpose()?.flatten())?;
        }
        for style in root.all("style") {
            for (key, (id, _)) in &style.attrs {
                if key.rsplit(':').next() == Some("styleId") {
                    sheet.index.entry(id.clone()).or_default().push(style);
                }
            }
            let on = ["1", "true", "on"]
                .iter()
                .any(|v| style.has_value("default", v));
            if !on {
                continue;
            }
            if style.has_value("type", "table") {
                sheet.table_defaults.push(style);
            }
            if style.has_value("type", "paragraph")
                || !style
                    .attrs
                    .keys()
                    .any(|key| key.rsplit(':').next() == Some("type"))
            {
                sheet.paragraph_defaults.push(style);
            }
        }
        Ok(sheet)
    }
    fn default_id(&self, kind: &str) -> Result<Option<&str>> {
        let candidates = if kind == "paragraph" {
            &self.paragraph_defaults
        } else {
            &self.table_defaults
        };
        ensure!(candidates.len() <= 1, "Ambiguous active default style");
        let Some(style) = candidates.first().copied() else {
            return Ok(None);
        };
        style.identity()?;
        ensure!(
            self.direct_styles.contains(&(style as *const Xml as usize)),
            "Non-direct default style"
        );
        ensure!(
            style.attr("type")?.unwrap_or("paragraph") == kind
                && style.attr("default")?.map(boolean).transpose()? == Some(true),
            "Ambiguous default style identity"
        );
        Ok(Some(
            style
                .attr("styleId")?
                .filter(|id| !id.is_empty())
                .ok_or_else(|| anyhow::anyhow!("Missing default style identifier"))?,
        ))
    }
    fn style_properties(&self, style: &Xml) -> Result<Properties> {
        let key = style as *const Xml as usize;
        if let Some(properties) = self.properties_cache.borrow().get(&key) {
            return Ok(properties.clone());
        }
        check_locations(style, None)?;
        if style.attr("type")? == Some("table") {
            for conditional in style.all("tblStylePr") {
                ensure!(
                    conditional.all("ind").is_empty(),
                    "Conditional table paragraph indentation is unsupported"
                );
            }
        }
        let result = properties(style.one("pPr")?)?;
        self.properties_cache
            .borrow_mut()
            .insert(key, result.clone());
        Ok(result)
    }
    fn cell_geometry(&self, style: &Xml) -> Result<()> {
        let key = style as *const Xml as usize;
        if self.cell_geometry_checked.borrow().contains(&key) {
            return Ok(());
        }
        ensure!(
            style.all("tblStylePr").is_empty(),
            "Conditional table styles are unsupported for indented cell paragraphs"
        );
        self.cell_geometry_checked.borrow_mut().insert(key);
        Ok(())
    }
    fn unresolved(&self, id: Option<&str>, kind: &str) -> bool {
        id.is_some_and(|id| {
            self.unresolved_chains
                .borrow()
                .contains(&(id.to_owned(), kind.to_owned()))
        })
    }
    fn fallback_declares_indent(&self) -> Result<bool> {
        if let Some(present) = self.fallback_indent.get() {
            return Ok(present);
        }
        // Only discover whether a potential default chain contains indentation.
        // Do not turn a legacy unknown-style paragraph with no indentation into
        // an audit of unrelated default font, direction, or spacing features.
        let mut pending = self.paragraph_defaults.clone();
        let mut seen = BTreeSet::new();
        let mut work = 0usize;
        while let Some(style) = pending.pop() {
            if !seen.insert(style as *const Xml as usize) {
                continue;
            }
            work += style.size;
            ensure!(
                work <= 5_000_000,
                "Fallback style indentation audit exceeds limits"
            );
            if !style.all("ind").is_empty() {
                self.fallback_indent.set(Some(true));
                return Ok(true);
            }
            for base in style.all("basedOn") {
                for (key, (id, _)) in &base.attrs {
                    if key.rsplit(':').next() == Some("val")
                        && let Some(candidates) = self.index.get(id)
                    {
                        pending.extend(candidates);
                    }
                }
            }
        }
        self.fallback_indent.set(Some(false));
        Ok(false)
    }
    fn chain(&self, id: Option<&str>, kind: &str) -> Result<Vec<&'a Xml>> {
        let Some(id) = id else {
            return Ok(Vec::new());
        };
        let key = (id.to_owned(), kind.to_owned());
        if let Some(chain) = self.chain_cache.borrow().get(&key) {
            return Ok(chain.clone());
        }
        let mut chain = Vec::new();
        let mut seen = BTreeSet::new();
        let mut next = Some(id.to_owned());
        while let Some(id) = next {
            ensure!(
                seen.insert(id.clone()) && seen.len() <= 20,
                "Cyclic or overlong active style chain"
            );
            let candidates = self.index.get(&id).map(Vec::as_slice).unwrap_or(&[]);
            ensure!(candidates.len() <= 1, "Ambiguous active style identifier");
            let Some(style) = candidates.first().copied() else {
                // The parser stops here, but consumers may apply a default
                // style. Record the gap so the new preservation path never
                // turns agreement with that parser behavior into proof.
                self.unresolved_chains.borrow_mut().insert(key.clone());
                break;
            };
            style.identity()?;
            ensure!(
                self.direct_styles.contains(&(style as *const Xml as usize)),
                "Non-direct active style"
            );
            ensure!(
                style.attr("styleId")? == Some(id.as_str()),
                "Unsupported active style identity"
            );
            let actual_kind = style.attr("type")?.unwrap_or("paragraph");
            ensure!(actual_kind == kind, "Active style has unsupported type");
            if let Some(default) = style.attr("default")? {
                boolean(default)?;
            }
            self.style_properties(style)?;
            next = style
                .one("basedOn")?
                .map(value)
                .transpose()?
                .map(str::to_owned);
            chain.push(style);
        }
        chain.reverse();
        self.chain_cache.borrow_mut().insert(key, chain.clone());
        Ok(chain)
    }
}

fn settings(data: Option<&[u8]>) -> Result<()> {
    let Some(data) = data else {
        return Ok(());
    };
    let tree = xml(data)?;
    let settings = root(&tree, "settings")?;
    for tag in [
        "mirrorMargins",
        "mirrorIndents",
        "bidi",
        "rtlGutter",
        "bookFoldPrinting",
        "bookFoldRevPrinting",
    ] {
        let setting = settings.one(tag)?;
        ensure!(
            settings.all(tag).len() == usize::from(setting.is_some()),
            "Non-direct indentation setting"
        );
        if let Some(setting) = setting {
            disabled(setting)?;
        }
    }
    Ok(())
}

fn numbering(data: Option<&Xml>, used: &BTreeSet<(u32, u8)>) -> Result<()> {
    if used.is_empty() {
        return Ok(());
    }
    let Some(tree) = data else {
        return Ok(());
    };
    let root = root(tree, "numbering")?;
    let work = Cell::new(0usize);
    fn select<'a>(
        parent: &'a Xml,
        tag: &str,
        attr: &str,
        number: u32,
        work: &Cell<usize>,
    ) -> Result<Option<&'a Xml>> {
        let total = work
            .get()
            .checked_add(parent.size)
            .ok_or_else(|| anyhow::anyhow!("Numbering audit exceeds limits"))?;
        ensure!(
            total <= 5_000_000,
            "Numbering indentation audit work exceeds limits"
        );
        work.set(total);
        let found: Vec<_> = parent
            .all(tag)
            .into_iter()
            .filter(|n| {
                if n.has_number(attr, number) {
                    return true;
                }
                // Candidate discovery mirrors only the pinned reader's
                // element-specific fallbacks. Validation below still rejects
                // a selected malformed identity rather than trusting zero.
                let literal = n.attrs.get(&format!("w:{attr}")).map(|(v, _)| v.as_str());
                match tag {
                    "lvl" if parent.tag == "abstractNum" => {
                        u32::from(literal.and_then(|v| v.parse::<u8>().ok()).unwrap_or(0)) == number
                    }
                    "abstractNum" | "num" => {
                        literal.is_some_and(|v| v.parse::<u32>().unwrap_or(0) == number)
                    }
                    // Missing/malformed lvlOverride indices are ignored, not
                    // defaulted to zero, by the reader.
                    _ => false,
                }
            })
            .collect();
        ensure!(found.len() <= 1, "Ambiguous active numbering identity");
        if let Some(node) = found.first() {
            node.identity()?;
            let raw = node
                .attr(attr)?
                .ok_or_else(|| anyhow::anyhow!("Missing numbering identifier"))?;
            ensure!(
                integer(raw, false, i32::MAX)? as u32 == number
                    && parent.children.iter().any(|n| std::ptr::eq(n, *node)),
                "Unsupported active numbering identity"
            );
        }
        Ok(found.first().copied())
    }
    for (id, level) in used {
        let Some(instance) = select(root, "num", "numId", *id, &work)? else {
            continue;
        };
        ensure!(
            instance.all("abstractNumId").len()
                == usize::from(instance.one("abstractNumId")?.is_some()),
            "Non-direct numbering abstract reference"
        );
        // The vendor supplies abstract ID zero when this mandatory reference
        // is absent. Never treat that parser default as proof of an inactive
        // definition; reject only the actually used malformed instance.
        let abs = instance.one("abstractNumId")?.ok_or_else(|| {
            anyhow::anyhow!("Active numbering instance has no abstract reference")
        })?;
        let abs = integer(value(abs)?, false, i32::MAX)? as u32;
        let abstract_num = select(root, "abstractNum", "abstractNumId", abs, &work)?;
        let override_level = if let Some(over) =
            select(instance, "lvlOverride", "ilvl", u32::from(*level), &work)?
        {
            ensure!(
                over.all("lvl").len() == usize::from(over.one("lvl")?.is_some()),
                "Non-direct numbering override level"
            );
            over.one("lvl")?
        } else {
            None
        };
        if let Some(abstract_num) = abstract_num {
            ensure!(
                abstract_num.all("numStyleLink").is_empty(),
                "Linked numbering styles are unsupported"
            );
        }
        let base_level = abstract_num
            .map(|n| select(n, "lvl", "ilvl", u32::from(*level), &work))
            .transpose()?
            .flatten();
        for source in override_level.into_iter().chain(base_level) {
            source.identity()?;
            let ilvl = source
                .attr("ilvl")?
                .ok_or_else(|| anyhow::anyhow!("Missing numbering level index"))?;
            ensure!(
                integer(ilvl, false, 8)? == i32::from(*level),
                "Unsupported numbering level identity"
            );
            check_locations(source, None)?;
            source.one("pStyle")?.map(value).transpose()?;
            // Physical numbering indents keep the prior list-normalization
            // contract. Logical, character, RTL, and malformed declarations
            // are not ordinary supported numbering geometry.
            ensure!(
                !properties(source.one("pPr")?)?.negative_sides_declared,
                "Negative numbering indentation is unsupported"
            );
        }
    }
    Ok(())
}

fn paragraph_text(node: &Xml, text: &mut String) -> Result<()> {
    match node.tag.as_str() {
        "pPr" => {}
        "t" => {
            node.identity()?;
            ensure!(
                !node.text.contains('\t'),
                "Literal tabs in indented paragraphs are unsupported"
            );
            text.push_str(&node.text);
        }
        "tab" => anyhow::bail!("Tabs in indented paragraphs are unsupported"),
        "br" => {
            node.identity()?;
            match node.attr("type")? {
                None | Some("textWrapping") => text.push('\n'),
                Some("page" | "column") => anyhow::bail!(
                    "Indented paragraphs with hard page or column breaks are unsupported"
                ),
                _ => anyhow::bail!("Unsupported paragraph break"),
            }
        }
        "drawing" | "pict" | "object" => {
            anyhow::bail!("Image paragraph indentation is unsupported")
        }
        _ => {
            for child in &node.children {
                paragraph_text(child, text)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn analyze(
    main: &[u8],
    styles: &[u8],
    numbers: Option<&[u8]>,
    config: Option<&[u8]>,
    page: Option<&crate::document::PageGeometry>,
) -> Result<ParagraphIndentAudit> {
    let margin_proofs = crate::table_widths::paragraph_margin_proofs(main, styles)?;
    let main = xml(main)?;
    let document = root(&main, "document")?;
    let body = document
        .one("body")?
        .ok_or_else(|| anyhow::anyhow!("Missing DOCX body"))?;
    check_locations(body, None)?;
    let styles_xml = xml(styles)?;
    let sheet = Styles::new(root(&styles_xml, "styles")?, margin_proofs)?;
    settings(config)?;
    let numbers_xml = numbers.map(xml).transpose()?;
    let mut used = BTreeSet::new();
    let mut audit = ParagraphIndentAudit::default();
    fn walk(
        node: &Xml,
        sheet: &Styles<'_>,
        table_layer: Option<&Properties>,
        table_depth: usize,
        used: &mut BTreeSet<(u32, u8)>,
        audit: &mut ParagraphIndentAudit,
        body_page: Option<&crate::document::PageGeometry>,
    ) -> Result<()> {
        match node.tag.as_str() {
            "p" => {
                node.identity()?;
                let own = properties(node.one("pPr")?)?;
                let mut effective = sheet.defaults.clone();
                if let Some(layer) = table_layer {
                    effective.overlay(layer);
                }
                let id = if let Some(id) = own.style.as_deref() {
                    Some(id)
                } else {
                    sheet.default_id("paragraph")?
                };
                for style in sheet.chain(id, "paragraph")? {
                    effective.overlay(&sheet.style_properties(style)?);
                }
                effective.overlay(&own);
                if effective.negative_sides_declared
                    || (effective.indent.left.unwrap_or(0)
                        + effective.indent.first_line.unwrap_or(0)
                        < 0
                        && node.all("drawing").is_empty())
                {
                    let page = body_page.ok_or_else(|| {
                        anyhow::anyhow!(
                            "Negative paragraph indentation requires a proved top-level body page"
                        )
                    })?;
                    ensure!(
                        table_depth == 0
                            && effective.numbering.is_none_or(|(id, _)| id == 0)
                            && node.all("drawing").is_empty(),
                        "Negative paragraph indentation is unsupported in lists, tables or images"
                    );
                    crate::document::validate_paragraph_indent_frame(
                        [
                            effective.indent.left.unwrap_or(0) as i64,
                            effective.indent.right.unwrap_or(0) as i64,
                            effective.indent.first_line.unwrap_or(0) as i64,
                        ],
                        page.width_twips - page.margin_left_twips - page.margin_right_twips,
                        Some((page.margin_left_twips, page.margin_right_twips)),
                    )?;
                }
                if let Some(reference) = effective.numbering.filter(|(id, _)| *id != 0) {
                    used.insert(reference);
                    ensure!(
                        !effective.indent.nonzero(),
                        "Custom paragraph indentation on lists is unsupported"
                    );
                    // Explicit zero in an ordinary list is normalized with its
                    // existing marker/container formatting, not misrepresented
                    // as a supported paragraph indentation feature.
                    return Ok(());
                }
                // The separate image audit must prove image-only geometry;
                // its verified empty placeholder is removed before this
                // audit's text targets are compared. Keep the pre-existing
                // warned normalization of image-only physical indentation.
                // Raw declaration/namespace/direction checks above still run.
                if !node.all("drawing").is_empty() {
                    return Ok(());
                }
                if sheet.unresolved(id, "paragraph") {
                    ensure!(
                        !effective.indent_declared && !sheet.fallback_declares_indent()?,
                        "Unresolved paragraph style prevents indentation preservation"
                    );
                }
                if effective.indent.present() {
                    ensure!(
                        !effective.custom_tabs,
                        "Custom tab stops in indented paragraphs are unsupported"
                    );
                    ensure!(
                        table_depth <= 1,
                        "Nested-table paragraph indentation is unsupported"
                    );
                    paragraph_text(node, &mut effective.indent.text)?;
                    audit.paragraphs.push(effective.indent);
                }
                return Ok(());
            }
            "tbl" => {
                let margin_proved = sheet
                    .margin_proofs
                    .borrow_mut()
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("Table margin proof order mismatch"))?;
                node.identity()?;
                let props = node.one("tblPr")?;
                let explicit = props.map(|p| p.one("tblStyle")).transpose()?.flatten();
                let id = if let Some(explicit) = explicit {
                    Some(value(explicit)?)
                } else {
                    sheet.default_id("table")?
                };
                let chain = sheet.chain(id, "table")?;
                let mut layer = Properties::default();
                for style in &chain {
                    let declared = sheet.style_properties(style)?;
                    if explicit.is_none() {
                        ensure!(
                            !declared.indent.present() && declared.numbering.is_none(),
                            "Implicit table style paragraph indentation is unsupported"
                        );
                    } else {
                        layer.overlay(&declared);
                    }
                }
                let before = audit.paragraphs.len();
                for child in &node.children {
                    walk(
                        child,
                        sheet,
                        Some(&layer),
                        table_depth + 1,
                        used,
                        audit,
                        None,
                    )?;
                }
                if audit.paragraphs.len() > before {
                    ensure!(
                        !sheet.unresolved(id, "table"),
                        "Unresolved table style prevents cell indentation preservation"
                    );
                    ensure!(
                        margin_proved,
                        "Unproved horizontal margins in indented cell paragraphs"
                    );
                    for style in chain {
                        sheet.cell_geometry(style)?;
                    }
                }
                return Ok(());
            }
            _ => {}
        }
        for child in &node.children {
            walk(
                child,
                sheet,
                table_layer,
                table_depth,
                used,
                audit,
                if node.tag == "body" { body_page } else { None },
            )?;
        }
        Ok(())
    }
    walk(body, &sheet, None, 0, &mut used, &mut audit, page)?;
    ensure!(
        sheet.margin_proofs.borrow_mut().next().is_none(),
        "Source table margin proof was not consumed"
    );
    numbering(numbers_xml.as_ref(), &used)?;
    Ok(audit)
}

/// Check the converted model after separately verified image-only placeholders
/// have been removed. Only text indent targets participate: ordinary page
/// breaks may legitimately split paragraphs, and ordinary list offsets follow
/// the old explicit normalization path rather than entering this feature.
pub(crate) fn verify_converted(
    audit: &ParagraphIndentAudit,
    document: &crate::document::Node,
) -> Result<()> {
    fn text(node: &crate::document::Node, out: &mut String) {
        if let Some(value) = &node.text {
            out.push_str(value);
        }
        if node.kind == "hardBreak" {
            out.push('\n');
        }
        for child in &node.content {
            text(child, out);
        }
    }
    fn walk(
        node: &crate::document::Node,
        expected: &mut std::slice::Iter<'_, ParagraphIndent>,
    ) -> Result<()> {
        if matches!(node.kind.as_str(), "paragraph" | "heading") {
            let get = |key| -> Result<Option<i32>> {
                node.attrs
                    .get(key)
                    .map(|value| {
                        value
                            .as_i64()
                            .and_then(|v| i32::try_from(v).ok())
                            .ok_or_else(|| {
                                anyhow::anyhow!("Malformed converted paragraph indentation")
                            })
                    })
                    .transpose()
            };
            let mut actual = ParagraphIndent {
                left: get("indentLeftTwips")?,
                right: get("indentRightTwips")?,
                first_line: get("firstLineIndentTwips")?,
                ..Default::default()
            };
            if actual.present() {
                text(node, &mut actual.text);
                let source = expected.next().ok_or_else(|| {
                    anyhow::anyhow!("Unaudited paragraph indentation appeared during conversion")
                })?;
                ensure!(
                    &actual == source,
                    "Converted paragraph indentation or identity differs from audited source"
                );
            }
        }
        for child in &node.content {
            walk(child, expected)?;
        }
        Ok(())
    }
    let mut expected = audit.paragraphs.iter();
    walk(document, &mut expected)?;
    ensure!(
        expected.next().is_none(),
        "Audited paragraph indentation was discarded during conversion"
    );
    Ok(())
}

/// Verify the raw OPC roles that determine the indentation source snapshot.
pub(crate) fn validate_relationships(data: &[u8]) -> Result<()> {
    let mut reader = NsReader::from_reader(data);
    let mut stack = Vec::new();
    let mut identities = BTreeSet::new();
    let mut kinds = BTreeSet::new();
    let mut all_ids = BTreeMap::<String, usize>::new();
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
                    "Indentation relationships exceed limits"
                );
                let tag = e.local_name().as_ref().to_owned();
                if stack.is_empty() {
                    roots += 1;
                    ensure!(
                        roots == 1 && tag == "Relationships" && package_ns,
                        "Invalid indentation relationship root"
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
                    for (key, value) in &attrs {
                        if key.rsplit(':').next() == Some("Id") {
                            *all_ids.entry(value.clone()).or_default() += 1;
                        }
                    }
                    let relevant = attrs.iter().any(|(key, value)| {
                        key.rsplit(':').next() == Some("Type")
                            && ["/settings", "/styles", "/numbering"]
                                .iter()
                                .any(|suffix| value.ends_with(suffix))
                    });
                    if relevant {
                        ensure!(
                            package_ns && stack == ["Relationships"],
                            "Wrapped or foreign indentation relationship"
                        );
                        ensure!(
                            attrs.keys().all(|key| matches!(
                                key.as_str(),
                                "Id" | "Type" | "Target" | "TargetMode"
                            )),
                            "Unknown indentation relationship attribute"
                        );
                        let id = attrs
                            .get("Id")
                            .ok_or_else(|| anyhow::anyhow!("Missing relationship ID"))?;
                        let kind = attrs
                            .get("Type")
                            .ok_or_else(|| anyhow::anyhow!("Missing relationship type"))?;
                        ensure!([
                            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles",
                            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering",
                            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings",
                            "http://purl.oclc.org/ooxml/officeDocument/relationships/styles",
                            "http://purl.oclc.org/ooxml/officeDocument/relationships/numbering",
                            "http://purl.oclc.org/ooxml/officeDocument/relationships/settings",
                        ].contains(&kind.as_str()), "Unsupported indentation relationship type");
                        let kind = kind.rsplit('/').next().expect("type");
                        ensure!(
                            identities.insert(id.clone()) && kinds.insert(kind.to_owned()),
                            "Duplicate indentation relationship"
                        );
                        ensure!(
                            attrs.get("Target").is_some_and(|value| !value.is_empty()),
                            "Missing indentation relationship target"
                        );
                        ensure!(
                            attrs
                                .get("TargetMode")
                                .is_none_or(|value| value == "Internal"),
                            "External or unknown indentation relationship mode"
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
                ensure!(
                    identities.iter().all(|id| all_ids.get(id) == Some(&1)),
                    "Ambiguous indentation relationship identifier"
                );
                return Ok(());
            }
            _ => {}
        }
    }
}
