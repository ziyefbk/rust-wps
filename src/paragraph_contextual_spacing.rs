//! Bounded, source-order proof for importing contextual spacing as fixed gaps.
//! No style identity or dynamic contextual behavior enters the editor model.
use crate::{
    document::Node,
    paragraph_indentation::{Xml, root, xml},
};
use anyhow::{Result, ensure};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
};

pub(crate) const NORMALIZATION_WARNING: &str = "Contextual same-style spacing resolved to fixed paragraph values; named-style and reorder dynamics are not retained";

#[derive(Clone, Debug, Default)]
struct Properties {
    before: Option<u32>,
    after: Option<u32>,
    line: Option<u32>,
    contextual: Option<bool>,
    style: Option<String>,
    numbering: Option<(u32, u8)>,
    outline: Option<u8>,
}
impl Properties {
    fn overlay(&mut self, other: &Self) {
        if other.before.is_some() {
            self.before = other.before;
        }
        if other.after.is_some() {
            self.after = other.after;
        }
        if other.line.is_some() {
            self.line = other.line;
        }
        if other.contextual.is_some() {
            self.contextual = other.contextual;
        }
        if other.numbering.is_some() {
            self.numbering = other.numbering;
        }
        if other.outline.is_some() {
            self.outline = other.outline;
        }
    }
}
fn boolean(v: &str) -> Result<bool> {
    match v {
        "1" | "true" | "on" => Ok(true),
        "0" | "false" | "off" => Ok(false),
        _ => anyhow::bail!("Unknown contextual spacing toggle"),
    }
}
fn number(v: &str, maximum: u32) -> Result<u32> {
    ensure!(
        !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()),
        "Invalid contextual spacing number"
    );
    let value = v.parse::<u32>()?;
    ensure!(value <= maximum, "Contextual spacing number exceeds bounds");
    Ok(value)
}
fn value(n: &Xml) -> Result<&str> {
    n.schema(&["val"], true)?;
    n.attr("val")?
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing contextual spacing property value"))
}
fn properties(n: Option<&Xml>) -> Result<Properties> {
    let Some(n) = n else {
        return Ok(Properties::default());
    };
    n.schema(&[], false)?;
    let mut p = Properties::default();
    if let Some(s) = n.one("spacing")? {
        s.schema(
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
        p.before = s.attr("before")?.map(|v| number(v, 31680)).transpose()?;
        p.after = s.attr("after")?.map(|v| number(v, 31680)).transpose()?;
        p.line = s.attr("line")?.map(|v| number(v, 2400)).transpose()?;
        ensure!(
            p.line.is_none_or(|v| v >= 120),
            "Unsupported contextual line spacing"
        );
        ensure!(
            s.attr("lineRule")?
                .is_none_or(|v| v == "auto" && p.line.is_some()),
            "Unsupported contextual line rule"
        );
        for key in ["beforeAutospacing", "afterAutospacing"] {
            ensure!(
                !s.attr(key)?.map(boolean).transpose()?.unwrap_or(false),
                "Automatic contextual paragraph spacing is unsupported"
            );
        }
    }
    if let Some(c) = n.one("contextualSpacing")? {
        c.schema(&["val"], true)?;
        p.contextual = Some(c.attr("val")?.map(boolean).transpose()?.unwrap_or(true));
    }
    p.style = n.one("pStyle")?.map(value).transpose()?.map(str::to_owned);
    p.outline = n
        .one("outlineLvl")?
        .map(value)
        .transpose()?
        .map(|v| number(v, 9).map(|v| v as u8))
        .transpose()?;
    if let Some(num) = n.one("numPr")? {
        num.schema(&[], false)?;
        ensure!(
            num.children
                .iter()
                .all(|n| matches!(n.tag.as_str(), "numId" | "ilvl")),
            "Unsupported contextual numbering reference"
        );
        let id = num
            .one("numId")?
            .map(value)
            .transpose()?
            .map(|v| number(v, i32::MAX as u32))
            .transpose()?
            .unwrap_or(0);
        let level = num
            .one("ilvl")?
            .map(value)
            .transpose()?
            .map(|v| number(v, 8))
            .transpose()?
            .unwrap_or(0) as u8;
        p.numbering = Some((id, level));
    }
    Ok(p)
}
fn locations(n: &Xml, parent: Option<&str>) -> Result<()> {
    match n.tag.as_str() {
        "pPrChange" | "numPrChange" => {
            anyhow::bail!("Revised contextual paragraph properties are unsupported")
        }
        "contextualSpacing" | "spacing" | "pStyle" | "numPr" | "outlineLvl" => {
            ensure!(
                parent == Some("pPr"),
                "Non-direct contextual paragraph property"
            );
            n.identity()?;
        }
        "pPr" => {
            n.identity()?;
            ensure!(
                matches!(
                    parent,
                    Some("p" | "style" | "pPrDefault" | "tblStylePr" | "lvl")
                ),
                "Non-direct contextual paragraph properties"
            );
            properties(Some(n))?;
        }
        "basedOn" => {
            ensure!(
                parent == Some("style"),
                "Non-direct contextual style inheritance"
            );
            value(n)?;
        }
        _ => {}
    }
    for child in &n.children {
        locations(child, Some(&n.tag))?;
    }
    Ok(())
}
fn potentially_active(n: &Xml) -> bool {
    n.all("contextualSpacing").iter().any(|n| {
        let values: Vec<_> = n.attrs.iter().filter(|(k, _)| k.rsplit(':').next() == Some("val")).collect();
        !matches!(values.as_slice(), [(k, (v, true))] if *k == "w:val" && matches!(v.as_str(), "0" | "false" | "off"))
    })
}
fn local_values<'a>(n: &'a Xml, name: &'a str) -> impl Iterator<Item = &'a str> {
    n.attrs
        .iter()
        .filter(move |(k, _)| k.rsplit(':').next() == Some(name))
        .map(|(_, (v, _))| v.as_str())
}

struct Styles<'a> {
    root: &'a Xml,
    index: BTreeMap<String, Vec<&'a Xml>>,
    defaults: Properties,
    work: Cell<usize>,
    default_cache: RefCell<BTreeMap<String, Option<String>>>,
    chain_cache: RefCell<BTreeMap<(String, String), Vec<&'a Xml>>>,
    properties_cache: RefCell<BTreeMap<usize, Properties>>,
    list_cache: RefCell<BTreeMap<(u32, u8), bool>>,
    names_cache: RefCell<BTreeMap<usize, Option<String>>>,
}
impl<'a> Styles<'a> {
    fn discover(root: &'a Xml) -> Self {
        let mut index = BTreeMap::<String, Vec<&Xml>>::new();
        for s in root.all("style") {
            for id in local_values(s, "styleId") {
                index.entry(id.into()).or_default().push(s);
            }
        }
        Self {
            root,
            index,
            defaults: Properties::default(),
            work: Cell::new(0),
            default_cache: RefCell::default(),
            chain_cache: RefCell::default(),
            properties_cache: RefCell::default(),
            list_cache: RefCell::default(),
            names_cache: RefCell::default(),
        }
    }
    // Discovery deliberately includes foreign/duplicate attribute identities.
    // It does not turn unrelated contextual template styles into active ones.
    fn relevant(&self, main: &Xml) -> Result<bool> {
        if potentially_active(main) {
            return Ok(true);
        }
        for d in self.root.all("docDefaults") {
            if potentially_active(d) {
                return Ok(true);
            }
        }
        let mut pending = Vec::new();
        let mut need_paragraph_default = false;
        for p in main.all("p") {
            let refs = p.all("pStyle");
            if refs.is_empty() {
                need_paragraph_default = true;
            }
            for r in refs {
                for id in local_values(r, "val") {
                    pending.push(id.to_owned());
                    need_paragraph_default |= !self.index.contains_key(id);
                }
            }
        }
        let mut need_table_default = false;
        for t in main.all("tbl") {
            let refs = t.all("tblStyle");
            if refs.is_empty() {
                need_table_default = true;
            }
            for r in refs {
                pending.extend(local_values(r, "val").map(str::to_owned));
            }
        }
        for s in self.root.all("style") {
            let is_default =
                local_values(s, "default").any(|v| !matches!(v, "0" | "false" | "off"));
            let paragraph =
                local_values(s, "type").next().is_none() || s.has_value("type", "paragraph");
            if is_default
                && ((paragraph && need_paragraph_default)
                    || (s.has_value("type", "table") && need_table_default))
            {
                if potentially_active(s) {
                    return Ok(true);
                }
                pending.extend(local_values(s, "styleId").map(str::to_owned));
            }
        }
        let mut seen = BTreeSet::new();
        let mut work = 0usize;
        while let Some(id) = pending.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Some(styles) = self.index.get(&id) {
                for s in styles {
                    work += s.size;
                    ensure!(
                        work <= 5_000_000,
                        "Contextual style discovery exceeds limits"
                    );
                    if potentially_active(s) {
                        return Ok(true);
                    }
                    for base in s.all("basedOn") {
                        pending.extend(local_values(base, "val").map(str::to_owned));
                    }
                }
            }
        }
        Ok(false)
    }
    fn initialize(&mut self) -> Result<()> {
        let d = self.root.one("docDefaults")?;
        ensure!(
            self.root.all("docDefaults").len() == usize::from(d.is_some()),
            "Non-direct contextual document defaults"
        );
        if let Some(d) = d {
            let ppd = d.one("pPrDefault")?;
            let pp = ppd.map(|n| n.one("pPr")).transpose()?.flatten();
            ensure!(
                d.all("pPr").len() == usize::from(pp.is_some()),
                "Ambiguous contextual document defaults"
            );
            locations(d, Some("styles"))?;
            self.defaults = properties(pp)?;
        }
        Ok(())
    }
    fn spend(&self, amount: usize) -> Result<()> {
        let work = self
            .work
            .get()
            .checked_add(amount)
            .ok_or_else(|| anyhow::anyhow!("Contextual style proof work exceeds limits"))?;
        ensure!(
            work <= 5_000_000,
            "Contextual style proof work exceeds limits"
        );
        self.work.set(work);
        Ok(())
    }
    fn default_id(&self, kind: &str) -> Result<Option<String>> {
        if let Some(id) = self.default_cache.borrow().get(kind) {
            return Ok(id.clone());
        }
        self.spend(self.root.size)?;
        let found: Vec<_> = self
            .root
            .all("style")
            .into_iter()
            .filter(|s| {
                local_values(s, "default").any(|v| !matches!(v, "0" | "false" | "off"))
                    && (s.has_value("type", kind)
                        || (kind == "paragraph" && local_values(s, "type").next().is_none()))
            })
            .collect();
        ensure!(found.len() <= 1, "Ambiguous contextual default style");
        let result = found
            .first()
            .map(|s| {
                s.identity()?;
                ensure!(
                    self.root.children.iter().any(|n| std::ptr::eq(n, *s)),
                    "Non-direct contextual default style"
                );
                ensure!(
                    s.attr("type")?.unwrap_or("paragraph") == kind
                        && s.attr("default")?.map(boolean).transpose()? == Some(true),
                    "Invalid contextual default style identity"
                );
                Ok(s.attr("styleId")?
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("Missing contextual default style ID"))?
                    .to_owned())
            })
            .transpose()?;
        self.default_cache
            .borrow_mut()
            .insert(kind.to_owned(), result.clone());
        Ok(result)
    }
    fn style_properties(&self, style: &Xml) -> Result<Properties> {
        let key = style as *const Xml as usize;
        if let Some(p) = self.properties_cache.borrow().get(&key) {
            return Ok(p.clone());
        }
        self.spend(style.size)?;
        let p = properties(style.one("pPr")?)?;
        self.properties_cache.borrow_mut().insert(key, p.clone());
        Ok(p)
    }
    fn style_name(&self, style: &Xml) -> Result<Option<String>> {
        let key = style as *const Xml as usize;
        if let Some(name) = self.names_cache.borrow().get(&key) {
            return Ok(name.clone());
        }
        self.spend(style.size)?;
        let name = style
            .one("name")?
            .map(|n| -> Result<Option<String>> {
                n.schema(&["val"], true)?;
                let name = n.attr("val")?;
                ensure!(
                    name.is_none_or(|s| s.len() <= 1024),
                    "Contextual style name exceeds limits"
                );
                Ok(name.filter(|s| !s.trim().is_empty()).map(str::to_owned))
            })
            .transpose()?
            .flatten();
        self.names_cache.borrow_mut().insert(key, name.clone());
        Ok(name)
    }
    fn list_kind(&self, numbers: Option<&Xml>, id: u32, level: u8) -> Result<bool> {
        if let Some(ordered) = self.list_cache.borrow().get(&(id, level)) {
            return Ok(*ordered);
        }
        self.spend(numbers.map_or(0, |n| n.size).saturating_mul(10))?;
        let ordered = list_kind(numbers, id, level)?;
        self.list_cache.borrow_mut().insert((id, level), ordered);
        Ok(ordered)
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
        let mut next = Some(id.to_owned());
        let mut seen = BTreeSet::new();
        while let Some(id) = next {
            ensure!(
                seen.insert(id.clone()) && seen.len() <= 20,
                "Cyclic or overlong contextual style chain"
            );
            let candidates = self.index.get(&id).map(Vec::as_slice).unwrap_or(&[]);
            ensure!(
                candidates.len() == 1,
                "Unknown or ambiguous contextual style identity"
            );
            let s = candidates[0];
            self.spend(s.size + self.root.children.len())?;
            s.identity()?;
            ensure!(
                self.root.children.iter().any(|n| std::ptr::eq(n, s))
                    && s.attr("styleId")? == Some(id.as_str())
                    && s.attr("type")?.unwrap_or("paragraph") == kind,
                "Unsupported contextual style identity"
            );
            if let Some(v) = s.attr("default")? {
                boolean(v)?;
            }
            locations(s, Some("styles"))?;
            ensure!(
                s.all("pPr").len() == usize::from(s.one("pPr")?.is_some()),
                "Conditional contextual style properties are unsupported"
            );
            next = s.one("basedOn")?.map(value).transpose()?.map(str::to_owned);
            chain.push(s);
        }
        chain.reverse();
        self.chain_cache.borrow_mut().insert(key, chain.clone());
        Ok(chain)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    Paragraph,
    Heading(u8),
}
#[derive(Clone, Debug)]
struct Paragraph {
    text: String,
    style: Option<String>,
    contextual: bool,
    kind: Kind,
    list: Option<bool>,
    before: u32,
    after: u32,
    line: u32,
    fixed_before: u32,
    fixed_after: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Paragraph(usize),
    TableStart,
    TableEnd,
    RowStart,
    RowEnd,
    CellStart,
    CellEnd,
}
#[derive(Default)]
pub(crate) struct ContextualSpacingAudit {
    pub proved: bool,
    tokens: Vec<Token>,
    paragraphs: Vec<Paragraph>,
}

fn text(n: &Xml, out: &mut String) -> Result<()> {
    match n.tag.as_str() {
        "pPr" | "rPr" => return Ok(()),
        "p" | "r" | "hyperlink" => {
            n.identity()?;
        }
        "t" => {
            n.identity()?;
            ensure!(n.children.is_empty(), "Nested contextual text");
            out.push_str(&n.text);
            return Ok(());
        }
        "tab" => {
            n.identity()?;
            out.push('\t');
            return Ok(());
        }
        "br" => {
            n.schema(&["type", "clear"], true)?;
            ensure!(
                n.attr("type")?.is_none_or(|v| v == "textWrapping")
                    && n.attr("clear")?.is_none_or(|v| v == "none"),
                "Contextual paragraphs with hard page/column or clearing breaks are unsupported"
            );
            out.push('\n');
            return Ok(());
        }
        "bookmarkStart" | "bookmarkEnd" | "proofErr" => {
            n.identity()?;
            ensure!(n.children.is_empty(), "Unsupported contextual text marker");
            return Ok(());
        }
        _ => anyhow::bail!("Unsupported contextual paragraph content: {}", n.tag),
    }
    ensure!(
        n.text.trim().is_empty(),
        "Uncontained contextual paragraph text"
    );
    for c in &n.children {
        text(c, out)?;
    }
    Ok(())
}
fn heading_name(s: &str) -> Option<u8> {
    let lower = s.trim().to_ascii_lowercase();
    let n: u8 = lower.strip_prefix("heading")?.trim().parse().ok()?;
    (1..=9).contains(&n).then_some(n)
}
fn list_kind(numbers: Option<&Xml>, id: u32, level: u8) -> Result<bool> {
    let root = numbers.ok_or_else(|| anyhow::anyhow!("Missing contextual numbering part"))?;
    fn select<'a>(parent: &'a Xml, tag: &str, attr: &str, value: u32) -> Result<&'a Xml> {
        let matches: Vec<_> = parent
            .all(tag)
            .into_iter()
            .filter(|n| n.has_number(attr, value))
            .collect();
        ensure!(
            matches.len() == 1,
            "Unknown or ambiguous contextual numbering identity"
        );
        let n = matches[0];
        n.identity()?;
        ensure!(
            parent.children.iter().any(|c| std::ptr::eq(c, n))
                && n.attr(attr)?
                    .map(|v| number(v, i32::MAX as u32))
                    .transpose()?
                    == Some(value),
            "Invalid contextual numbering identity"
        );
        Ok(n)
    }
    let instance = select(root, "num", "numId", id)?;
    let abstract_id = number(
        value(
            instance
                .one("abstractNumId")?
                .ok_or_else(|| anyhow::anyhow!("Missing contextual abstract numbering"))?,
        )?,
        i32::MAX as u32,
    )?;
    let abstract_num = select(root, "abstractNum", "abstractNumId", abstract_id)?;
    ensure!(
        abstract_num.all("numStyleLink").is_empty() && abstract_num.all("styleLink").is_empty(),
        "Linked contextual numbering is unsupported"
    );
    let base = select(abstract_num, "lvl", "ilvl", u32::from(level))?;
    let overrides: Vec<_> = instance
        .all("lvlOverride")
        .into_iter()
        .filter(|n| n.has_number("ilvl", u32::from(level)))
        .collect();
    ensure!(
        overrides.len() <= 1,
        "Ambiguous contextual numbering override"
    );
    let over = if let Some(n) = overrides.first() {
        n.identity()?;
        ensure!(
            instance.children.iter().any(|c| std::ptr::eq(c, *n))
                && n.attr("ilvl")?.map(|v| number(v, 8)).transpose()? == Some(u32::from(level)),
            "Invalid contextual numbering override"
        );
        n.one("lvl")?
    } else {
        None
    };
    for source in Some(base).into_iter().chain(over) {
        ensure!(
            source.all("contextualSpacing").is_empty(),
            "Numbering-level contextual spacing is unsupported"
        );
        ensure!(
            source.all("pPr").len() == usize::from(source.one("pPr")?.is_some()),
            "Non-direct contextual numbering properties"
        );
        if let Some(p) = source.one("pPr")? {
            ensure!(
                p.all("spacing").is_empty() && !potentially_active(p),
                "Numbering-level contextual spacing is unsupported"
            );
        }
    }
    let source = over.unwrap_or(base);
    ensure!(
        source.attr("ilvl")?.map(|v| number(v, 8)).transpose()? == Some(u32::from(level)),
        "Invalid contextual numbering level"
    );
    let format = source
        .one("numFmt")?
        .map(value)
        .transpose()?
        .unwrap_or("decimal");
    Ok(!matches!(format, "bullet" | "none"))
}

pub(crate) fn analyze(
    main: &[u8],
    styles: &[u8],
    numbers: Option<&[u8]>,
) -> Result<ContextualSpacingAudit> {
    let main_xml = xml(main)?;
    let styles_xml = xml(styles)?;
    // Do not impose this feature's strict style/structure proof on the legacy
    // inactive route, including unknown styles and untouched template entries.
    let mut sheet = Styles::discover(&styles_xml);
    if !sheet.relevant(&main_xml)? {
        return Ok(ContextualSpacingAudit::default());
    }
    let styles_root = root(&styles_xml, "styles")?;
    sheet = Styles::discover(styles_root);
    sheet.initialize()?;
    let document = root(&main_xml, "document")?;
    let body = document
        .one("body")?
        .ok_or_else(|| anyhow::anyhow!("Missing contextual DOCX body"))?;
    ensure!(
        document.all("body").len() == 1,
        "Ambiguous contextual DOCX body"
    );
    locations(body, Some("document"))?;
    let numbers_xml = numbers.map(xml).transpose()?;
    let numbers_root = numbers_xml
        .as_ref()
        .map(|n| root(n, "numbering"))
        .transpose()?;
    let mut audit = ContextualSpacingAudit {
        proved: true,
        ..Default::default()
    };
    fn walk(
        n: &Xml,
        sheet: &Styles<'_>,
        table_layer: Option<&Properties>,
        numbers: Option<&Xml>,
        audit: &mut ContextualSpacingAudit,
        depth: usize,
    ) -> Result<()> {
        ensure!(
            depth <= 64 && audit.tokens.len() <= 100_000,
            "Contextual source structure exceeds limits"
        );
        n.identity()?;
        match n.tag.as_str() {
            "p" => {
                let own = properties(n.one("pPr")?)?;
                let id = if own.style.is_some() {
                    own.style.clone()
                } else {
                    sheet.default_id("paragraph")?
                };
                let chain = sheet.chain(id.as_deref(), "paragraph")?;
                let mut effective = sheet.defaults.clone();
                if let Some(layer) = table_layer {
                    effective.overlay(layer);
                }
                let mut style_outline = None;
                for s in &chain {
                    let props = sheet.style_properties(s)?;
                    if props.outline.is_some() {
                        style_outline = props.outline;
                    }
                    effective.overlay(&props);
                }
                effective.overlay(&own);
                let style_name = chain
                    .last()
                    .map(|s| sheet.style_name(s))
                    .transpose()?
                    .flatten();
                let level = own.outline.map(|v| v + 1).or_else(|| {
                    let id = own.style.as_deref()?;
                    style_outline
                        .map(|v| v + 1)
                        .or_else(|| heading_name(id))
                        .or_else(|| style_name.as_deref().and_then(heading_name))
                });
                let kind = if let Some(level) = level {
                    ensure!(
                        (1..=3).contains(&level),
                        "Unsupported contextual heading level"
                    );
                    Kind::Heading(level)
                } else {
                    Kind::Paragraph
                };
                // Numbered headings remain headings, but their active
                // numbering layer still participates in the source proof.
                let numbering_kind = effective
                    .numbering
                    .filter(|(id, _)| *id != 0)
                    .map(|(id, level)| sheet.list_kind(numbers, id, level))
                    .transpose()?;
                let list = if level.is_none() {
                    numbering_kind
                } else {
                    None
                };
                let mut content = String::new();
                text(n, &mut content)?;
                let before = effective.before.unwrap_or(0);
                let after = effective.after.unwrap_or(0);
                audit.tokens.push(Token::Paragraph(audit.paragraphs.len()));
                audit.paragraphs.push(Paragraph {
                    text: content,
                    style: id,
                    contextual: effective.contextual.unwrap_or(false),
                    kind,
                    list,
                    before,
                    after,
                    line: effective.line.unwrap_or(240),
                    fixed_before: before,
                    fixed_after: after,
                });
                return Ok(());
            }
            "tbl" => {
                let props = n.one("tblPr")?;
                let explicit = props
                    .map(|p| p.one("tblStyle"))
                    .transpose()?
                    .flatten()
                    .map(value)
                    .transpose()?;
                let default_id = if explicit.is_none() {
                    sheet.default_id("table")?
                } else {
                    None
                };
                let id = explicit.or(default_id.as_deref());
                let mut layer = Properties::default();
                for s in sheet.chain(id, "table")? {
                    let p = sheet.style_properties(s)?;
                    ensure!(
                        s.all("contextualSpacing").is_empty(),
                        "Table-style contextual spacing is unsupported"
                    );
                    if explicit.is_some() {
                        layer.overlay(&p);
                    } else {
                        ensure!(
                            p.before.is_none()
                                && p.after.is_none()
                                && p.line.is_none()
                                && p.numbering.is_none(),
                            "Implicit table paragraph formatting is unsupported"
                        );
                    }
                }
                audit.tokens.push(Token::TableStart);
                for c in &n.children {
                    match c.tag.as_str() {
                        "tblPr" | "tblGrid" => {}
                        "tr" => walk(c, sheet, Some(&layer), numbers, audit, depth + 1)?,
                        _ => anyhow::bail!("Unsupported contextual table structure"),
                    }
                }
                audit.tokens.push(Token::TableEnd);
                return Ok(());
            }
            "tr" => audit.tokens.push(Token::RowStart),
            "tc" => audit.tokens.push(Token::CellStart),
            "body" => {}
            _ => anyhow::bail!("Unsupported contextual source container"),
        }
        for c in &n.children {
            match (n.tag.as_str(), c.tag.as_str()) {
                ("body", "p" | "tbl") | ("tc", "p" | "tbl") | ("tr", "tc") => {
                    walk(c, sheet, table_layer, numbers, audit, depth + 1)?
                }
                ("body", "sectPr") | ("tr", "trPr") | ("tc", "tcPr") => {}
                _ => anyhow::bail!("Unsupported contextual source order"),
            }
        }
        match n.tag.as_str() {
            "tr" => audit.tokens.push(Token::RowEnd),
            "tc" => audit.tokens.push(Token::CellEnd),
            _ => {}
        }
        Ok(())
    }
    walk(body, &sheet, None, numbers_root, &mut audit, 0)?;
    // Some consumers identify named styles by display name instead of the
    // distinct OOXML style IDs. Do not normalize an ambiguous consumer pair.
    let mut display_names = BTreeMap::<String, String>::new();
    let mut leaf_ids = BTreeSet::new();
    for p in &audit.paragraphs {
        let Some(id) = p.style.as_deref() else {
            continue;
        };
        if !leaf_ids.insert(id) {
            continue;
        }
        let chain = sheet.chain(Some(id), "paragraph")?;
        let leaf = chain.last().expect("resolved paragraph style");
        let name = sheet.style_name(leaf)?;
        let key = name.as_deref().unwrap_or(id).trim().to_lowercase();
        if let Some(other) = display_names.insert(key, id.to_owned()) {
            ensure!(
                other == id,
                "Ambiguous contextual style display names differ between consumers"
            );
        }
    }
    for pair in audit.tokens.windows(2) {
        let [Token::Paragraph(a), Token::Paragraph(b)] = pair else {
            continue;
        };
        let a = *a;
        let b = *b;
        ensure!(
            audit.paragraphs[a].style.is_none() == audit.paragraphs[b].style.is_none()
                || !(audit.paragraphs[a].contextual || audit.paragraphs[b].contextual),
            "Unproved implicit default style identity at contextual boundary"
        );
        if audit.paragraphs[a].style != audit.paragraphs[b].style {
            continue;
        }
        let left = &audit.paragraphs[a];
        let right = &audit.paragraphs[b];
        ensure!(
            left.contextual == right.contextual || left.after == 0 || right.before == 0,
            "One-sided nonzero contextual spacing differs between supported consumers"
        );
        if left.contextual {
            audit.paragraphs[a].fixed_after = 0;
        }
        if audit.paragraphs[b].contextual {
            audit.paragraphs[b].fixed_before = 0;
        }
    }
    Ok(audit)
}

/// Complete immutable comparison first: no spacing is rewritten on a partial
/// count/text/type/container match, including genuine empty paragraphs.
pub(crate) fn apply(audit: &ContextualSpacingAudit, document: &mut Node) -> Result<()> {
    if !audit.proved {
        return Ok(());
    }
    fn content(n: &Node, out: &mut String) -> Result<()> {
        match n.kind.as_str() {
            "paragraph" | "heading" => {}
            "text" => {
                out.push_str(n.text.as_deref().unwrap_or(""));
                return Ok(());
            }
            "hardBreak" => {
                out.push('\n');
                return Ok(());
            }
            _ => anyhow::bail!("Unsupported converted contextual inline type"),
        }
        for c in &n.content {
            content(c, out)?;
        }
        Ok(())
    }
    type ConvertedParagraph = (String, Kind, Option<bool>, u32, u32, u32);
    fn walk(
        n: &Node,
        list: Option<bool>,
        tokens: &mut Vec<Token>,
        paragraphs: &mut Vec<ConvertedParagraph>,
    ) -> Result<()> {
        match n.kind.as_str() {
            "paragraph" | "heading" => {
                let get = |key| {
                    n.attrs
                        .get(key)
                        .and_then(|v| v.as_u64())
                        .and_then(|v| u32::try_from(v).ok())
                        .ok_or_else(|| {
                            anyhow::anyhow!("Missing converted contextual spacing: {key}")
                        })
                };
                let kind = if n.kind == "heading" {
                    Kind::Heading(u8::try_from(get("level")?)?)
                } else {
                    Kind::Paragraph
                };
                let mut text = String::new();
                content(n, &mut text)?;
                tokens.push(Token::Paragraph(paragraphs.len()));
                paragraphs.push((
                    text,
                    kind,
                    list,
                    get("spaceBefore")?,
                    get("spaceAfter")?,
                    get("lineSpacing")?,
                ));
                return Ok(());
            }
            "table" => tokens.push(Token::TableStart),
            "tableRow" => tokens.push(Token::RowStart),
            "tableCell" | "tableHeader" => tokens.push(Token::CellStart),
            "orderedList" | "bulletList" => {
                for child in &n.content {
                    ensure!(
                        child.kind == "listItem",
                        "Invalid converted contextual list"
                    );
                    walk(child, Some(n.kind == "orderedList"), tokens, paragraphs)?;
                }
                return Ok(());
            }
            "doc" | "listItem" => {}
            _ => anyhow::bail!("Unsupported converted contextual block type"),
        }
        for child in &n.content {
            walk(child, list, tokens, paragraphs)?;
        }
        match n.kind.as_str() {
            "table" => tokens.push(Token::TableEnd),
            "tableRow" => tokens.push(Token::RowEnd),
            "tableCell" | "tableHeader" => tokens.push(Token::CellEnd),
            _ => {}
        }
        Ok(())
    }
    let mut tokens = Vec::new();
    let mut paragraphs = Vec::new();
    walk(document, None, &mut tokens, &mut paragraphs)?;
    ensure!(
        tokens == audit.tokens && paragraphs.len() == audit.paragraphs.len(),
        "Converted contextual paragraph count or container order differs from source"
    );
    for (actual, expected) in paragraphs.iter().zip(&audit.paragraphs) {
        ensure!(
            actual.0 == expected.text
                && actual.1 == expected.kind
                && actual.2 == expected.list
                && actual.3 == expected.before
                && actual.4 == expected.after
                && actual.5 == expected.line,
            "Converted contextual paragraph text, type, or spacing differs from source"
        );
    }
    fn write(n: &mut Node, source: &mut std::slice::Iter<'_, Paragraph>) {
        if matches!(n.kind.as_str(), "paragraph" | "heading") {
            let p = source.next().expect("complete contextual source proof");
            n.attrs.insert("spaceBefore".into(), p.fixed_before.into());
            n.attrs.insert("spaceAfter".into(), p.fixed_after.into());
        }
        for c in &mut n.content {
            write(c, source);
        }
    }
    write(document, &mut audit.paragraphs.iter());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    fn audit() -> ContextualSpacingAudit {
        let main = format!(
            r#"<w:document xmlns:w="{W}"><w:body><w:p><w:pPr><w:spacing w:after="200"/><w:contextualSpacing/></w:pPr><w:r><w:t>A</w:t></w:r></w:p><w:p><w:pPr><w:spacing w:before="240"/><w:contextualSpacing/></w:pPr><w:r><w:t>B</w:t></w:r></w:p></w:body></w:document>"#
        );
        analyze(
            main.as_bytes(),
            format!(r#"<w:styles xmlns:w="{W}"/>"#).as_bytes(),
            None,
        )
        .unwrap()
    }
    fn model() -> Node {
        serde_json::from_value(json!({"type":"doc","content":[
            {"type":"paragraph","attrs":{"spaceBefore":0,"spaceAfter":200,"lineSpacing":240},"content":[{"type":"text","text":"A"}]},
            {"type":"paragraph","attrs":{"spaceBefore":240,"spaceAfter":0,"lineSpacing":240},"content":[{"type":"text","text":"B"}]}
        ]})).unwrap()
    }
    #[test]
    fn contextual_all_identity_checks_finish_before_any_spacing_write() {
        let audit = audit();
        for edit in 0..7 {
            let mut doc = model();
            match edit {
                0 => {
                    doc.content[1].content[0].text = Some("wrong".into());
                }
                1 => {
                    doc.content.pop();
                }
                2 => {
                    doc.content[1].kind = "heading".into();
                    doc.content[1].attrs.insert("level".into(), 1.into());
                }
                3 => {
                    doc.content[1]
                        .attrs
                        .insert("spaceBefore".into(), 241.into());
                }
                4 => {
                    doc.content[1]
                        .attrs
                        .insert("lineSpacing".into(), 276.into());
                }
                5 => {
                    let p = doc.content.pop().unwrap();
                    let item = Node {
                        kind: "listItem".into(),
                        content: vec![p],
                        attrs: serde_json::Map::new(),
                        text: None,
                        marks: Vec::new(),
                    };
                    doc.content.push(Node {
                        kind: "bulletList".into(),
                        content: vec![item],
                        attrs: serde_json::Map::new(),
                        text: None,
                        marks: Vec::new(),
                    });
                }
                _ => {
                    let p = doc.content.pop().unwrap();
                    let cell = Node {
                        kind: "tableCell".into(),
                        content: vec![p],
                        attrs: serde_json::Map::new(),
                        text: None,
                        marks: Vec::new(),
                    };
                    let row = Node {
                        kind: "tableRow".into(),
                        content: vec![cell],
                        attrs: serde_json::Map::new(),
                        text: None,
                        marks: Vec::new(),
                    };
                    doc.content.push(Node {
                        kind: "table".into(),
                        content: vec![row],
                        attrs: serde_json::Map::new(),
                        text: None,
                        marks: Vec::new(),
                    });
                }
            }
            let before = serde_json::to_value(&doc).unwrap();
            assert!(apply(&audit, &mut doc).is_err());
            assert_eq!(serde_json::to_value(doc).unwrap(), before);
        }
        let mut doc = model();
        apply(&audit, &mut doc).unwrap();
        assert_eq!(doc.content[0].attrs["spaceAfter"], 0);
        assert_eq!(doc.content[1].attrs["spaceBefore"], 0);
    }
    #[test]
    fn contextual_cached_styles_admit_many_repeated_paragraphs_and_ignore_unused_styles() {
        let mut styles = format!(
            r#"<w:styles xmlns:w="{W}"><w:style w:type="paragraph" w:styleId="Used" w:default="1"><w:pPr><w:contextualSpacing/></w:pPr></w:style>"#
        );
        for i in 0..2000 {
            styles.push_str(&format!(
                r#"<w:style w:type="paragraph" w:styleId="Unused{i}"/>"#
            ));
        }
        styles.push_str("</w:styles>");
        let main = format!(
            r#"<w:document xmlns:w="{W}"><w:body>{}</w:body></w:document>"#,
            "<w:p/>".repeat(4000)
        );
        let proof = analyze(main.as_bytes(), styles.as_bytes(), None).unwrap();
        assert_eq!(proof.paragraphs.len(), 4000);
    }
    #[test]
    fn contextual_distinct_style_work_is_bounded_with_clear_diagnostic() {
        let mut styles = format!(r#"<w:styles xmlns:w="{W}">"#);
        let mut body = String::new();
        for i in 0..2400 {
            styles.push_str(&format!(r#"<w:style w:type="paragraph" w:styleId="S{i}"><w:pPr><w:contextualSpacing/></w:pPr></w:style>"#));
            body.push_str(&format!(
                r#"<w:p><w:pPr><w:pStyle w:val="S{i}"/></w:pPr></w:p>"#
            ));
        }
        styles.push_str("</w:styles>");
        let main = format!(r#"<w:document xmlns:w="{W}"><w:body>{body}</w:body></w:document>"#);
        let error = analyze(main.as_bytes(), styles.as_bytes(), None)
            .err()
            .unwrap();
        assert!(error.to_string().contains("work exceeds limits"), "{error}");
    }
}
