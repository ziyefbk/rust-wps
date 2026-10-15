//! Independently proved absent cell shading. This staged group never reuses
//! the legacy RGB audit's ambiguous missing/invalid fill result.
use crate::{
    document::Node,
    paragraph_indentation::{Xml, root, xml},
};
use anyhow::{Context, Result, ensure};
use office_oxide::{
    core::{
        opc::OpcReader,
        relationships::{TargetMode, rel_types},
    },
    ir::{DocumentIR, Element, inline_to_text},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
};
pub(crate) const NORMALIZED: &str =
    "Some unshaded header-table fills are unproved and remain normalized";
pub(crate) const BINDING_NORMALIZED: &str = "Table no-fill identity could not be bound to converted content; original fill normalization retained";
const MAX_WORK: usize = 2_000_000;
#[derive(Default)]
pub(crate) struct Audit {
    tables: Vec<Option<Table>>,
    pub warning: Option<&'static str>,
}
struct Table {
    rows: Vec<Row>,
}
struct Row {
    header: bool,
    paragraphs: Vec<Vec<String>>,
}
fn failed() -> Audit {
    Audit {
        warning: Some(NORMALIZED),
        ..Audit::default()
    }
}
fn direct<'a>(node: &'a Xml, tag: &str) -> Result<Option<&'a Xml>> {
    let value = node.one(tag)?;
    ensure!(
        node.all(tag).len() == usize::from(value.is_some()),
        "Non-direct row property: {tag}"
    );
    Ok(value)
}
fn flag(node: Option<&Xml>) -> Result<Option<bool>> {
    node.map(|node| {
        node.schema(&["val"], true)?;
        match node.attr("val")? {
            None | Some("1" | "true" | "on") => Ok(true),
            Some("0" | "false" | "off") => Ok(false),
            _ => anyhow::bail!("Unsupported row on/off spelling"),
        }
    })
    .transpose()
}
#[derive(Default)]
struct Markers {
    active: BTreeMap<u32, String>,
    seen: BTreeSet<u32>,
    names: BTreeSet<String>,
    visited: usize,
}
impl Markers {
    fn bookmark(&mut self, node: &Xml) -> Result<()> {
        let start = node.tag == "bookmarkStart";
        node.schema(if start { &["id", "name"] } else { &["id"] }, true)?;
        let value = node.attr("id")?.context("Missing bookmark identifier")?;
        ensure!(
            !value.is_empty() && value.len() <= 10 && value.bytes().all(|b| b.is_ascii_digit()),
            "Unsupported bookmark identifier"
        );
        let id: u32 = value.parse()?;
        ensure!(id <= i32::MAX as u32, "Bookmark identifier exceeds subset");
        if start {
            let name = node.attr("name")?.context("Missing bookmark name")?;
            ensure!(
                !name.is_empty()
                    && name.len() <= 256
                    && self.seen.insert(id)
                    && self.names.insert(name.to_owned()),
                "Ambiguous bookmark start"
            );
            self.active.insert(id, name.to_owned());
        } else {
            ensure!(self.active.remove(&id).is_some(), "Unmatched bookmark end");
        }
        self.visited += 1;
        Ok(())
    }
}
fn paragraph(node: &Xml, markers: &mut Markers, text_bytes: &mut usize) -> Result<String> {
    fn inline(
        node: &Xml,
        text: &mut String,
        markers: &mut Markers,
        proof: &mut [bool; 2],
        text_bytes: &mut usize,
    ) -> Result<()> {
        match node.tag.as_str() {
            "pPr" | "rPr" => {
                node.identity()?;
                return Ok(());
            }
            "p" | "r" | "hyperlink" => {
                node.identity()?;
                ensure!(
                    node.children.iter().all(|child| match node.tag.as_str() {
                        "p" => matches!(
                            child.tag.as_str(),
                            "pPr"
                                | "r"
                                | "hyperlink"
                                | "bookmarkStart"
                                | "bookmarkEnd"
                                | "proofErr"
                        ),
                        "r" => matches!(
                            child.tag.as_str(),
                            "rPr" | "t" | "tab" | "br" | "cr" | "lastRenderedPageBreak"
                        ),
                        _ => child.tag == "r",
                    }),
                    "Unsupported row inline ownership"
                );
                if node.tag == "p" {
                    node.one("pPr")?;
                }
                if node.tag == "r" {
                    node.one("rPr")?;
                }
            }
            "t" => {
                node.identity()?;
                ensure!(node.children.is_empty(), "Nested row text");
                for (key, (value, _)) in &node.attrs {
                    ensure!(
                        key == "xml:space"
                            && matches!(value.as_str(), "preserve" | "default")
                            && node.attribute_namespaces.get(key).map(String::as_str)
                                == Some("http://www.w3.org/XML/1998/namespace"),
                        "Unsupported row text attribute"
                    );
                }
                *text_bytes = text_bytes.saturating_add(node.text.len());
                ensure!(
                    *text_bytes <= 2 * 1024 * 1024,
                    "row proof resource limit: text"
                );
                text.push_str(&node.text);
                return Ok(());
            }
            "tab" => {
                node.schema(&[], true)?;
                *text_bytes = text_bytes.saturating_add(1);
                ensure!(
                    *text_bytes <= 2 * 1024 * 1024,
                    "row proof resource limit: text"
                );
                text.push('\t');
                return Ok(());
            }
            "br" => {
                node.schema(&["type", "clear"], true)?;
                ensure!(
                    matches!(node.attr("type")?, None | Some("textWrapping"))
                        && matches!(node.attr("clear")?, None | Some("none")),
                    "Row hard/clearing break unsupported"
                );
                *text_bytes = text_bytes.saturating_add(1);
                ensure!(
                    *text_bytes <= 2 * 1024 * 1024,
                    "row proof resource limit: text"
                );
                text.push('\n');
                return Ok(());
            }
            "cr" => {
                node.schema(&[], true)?;
                *text_bytes = text_bytes.saturating_add(1);
                ensure!(
                    *text_bytes <= 2 * 1024 * 1024,
                    "row proof resource limit: text"
                );
                text.push('\n');
                return Ok(());
            }
            "lastRenderedPageBreak" => {
                node.schema(&[], true)?;
                return Ok(());
            }
            "bookmarkStart" | "bookmarkEnd" => {
                markers.bookmark(node)?;
                return Ok(());
            }
            "proofErr" => {
                node.schema(&["type"], true)?;
                let (index, start) = match node.attr("type")? {
                    Some("spellStart") => (0, true),
                    Some("spellEnd") => (0, false),
                    Some("gramStart") => (1, true),
                    Some("gramEnd") => (1, false),
                    _ => anyhow::bail!("Unsupported row proofing marker"),
                };
                ensure!(proof[index] != start, "Unpaired row proofing marker");
                proof[index] = start;
                markers.visited += 1;
                return Ok(());
            }
            _ => anyhow::bail!("Unsupported row paragraph content"),
        }
        ensure!(node.text.trim().is_empty(), "Uncontained row text");
        for child in &node.children {
            inline(child, text, markers, proof, text_bytes)?;
        }
        Ok(())
    }
    node.identity()?;
    ensure!(
        node.all("numPr").is_empty(),
        "Numbered row paragraph unsupported"
    );
    let mut text = String::new();
    let mut proof = [false; 2];
    inline(node, &mut text, markers, &mut proof, text_bytes)?;
    ensure!(proof == [false; 2], "Unclosed row proofing marker");
    Ok(text)
}
fn structure(
    table: &Xml,
    body: &Xml,
    text_bytes: &mut usize,
    markers: &mut Markers,
) -> Result<Table> {
    ensure!(
        body.children.iter().any(|child| std::ptr::eq(child, table)),
        "Row pagination requires a direct body table"
    );
    table.identity()?;
    ensure!(
        table.text.trim().is_empty() && table.all("tbl").is_empty(),
        "Nested row table"
    );
    for forbidden in [
        "gridSpan",
        "vMerge",
        "hMerge",
        "numPr",
        "numStyleLink",
        "styleLink",
        "pPrChange",
        "rPrChange",
        "sectPrChange",
        "trPrChange",
        "tcPrChange",
        "tblPrChange",
        "tblPrEx",
        "cellIns",
        "cellDel",
        "cellMerge",
        "ins",
        "del",
        "moveFrom",
        "moveTo",
        "hidden",
        "hideMark",
        "vanish",
        "webHidden",
        "specVanish",
        "textDirection",
        "bidiVisual",
        "bidi",
        "rtl",
        "sdt",
        "customXml",
        "drawing",
        "pict",
        "fldChar",
        "instrText",
    ] {
        ensure!(
            table.all(forbidden).is_empty(),
            "Unsupported row table declaration: {forbidden}"
        );
    }
    ensure!(
        table
            .children
            .iter()
            .all(|n| matches!(n.tag.as_str(), "tblPr" | "tblGrid" | "tr" | "bookmarkEnd")),
        "Wrapped table rows"
    );
    table.one("tblPr")?;
    if let Some(grid) = table.one("tblGrid")? {
        grid.schema(&[], false)?;
        for column in &grid.children {
            ensure!(
                column.tag == "gridCol",
                "Unsupported no-fill grid declaration"
            );
            column.schema(&["w"], true)?;
            if let Some(width) = column.attr("w")? {
                ensure!(
                    !width.is_empty()
                        && width.bytes().all(|b| b.is_ascii_digit())
                        && width.parse::<u32>().is_ok(),
                    "Invalid no-fill grid width"
                );
            }
        }
    }
    let marker_start = markers.visited;
    let mut rows = Vec::new();
    let mut columns = None;
    let mut trailing_marker = false;
    for row in &table.children {
        if row.tag == "bookmarkEnd" {
            ensure!(!rows.is_empty(), "Table bookmark end precedes rows");
            trailing_marker = true;
            markers.bookmark(row)?;
            continue;
        }
        if row.tag != "tr" {
            ensure!(
                rows.is_empty() && !trailing_marker,
                "Table properties after rows"
            );
            continue;
        }
        ensure!(!trailing_marker, "Table bookmark end is not trailing");
        row.identity()?;
        ensure!(
            row.text.trim().is_empty()
                && row
                    .children
                    .iter()
                    .all(|n| matches!(n.tag.as_str(), "trPr" | "tc")),
            "Wrapped table cells"
        );
        ensure!(
            row.children
                .iter()
                .position(|n| n.tag == "trPr")
                .is_none_or(|n| n == 0),
            "Row properties after cells"
        );
        let mut paragraphs = Vec::new();
        for cell in row.children.iter().filter(|n| n.tag == "tc") {
            cell.identity()?;
            ensure!(
                cell.text.trim().is_empty()
                    && cell
                        .children
                        .iter()
                        .all(|n| matches!(n.tag.as_str(), "tcPr" | "p")),
                "Complex row cell content"
            );
            cell.one("tcPr")?;
            ensure!(
                cell.children
                    .iter()
                    .position(|n| n.tag == "tcPr")
                    .is_none_or(|n| n == 0),
                "Cell properties after paragraphs"
            );
            let values = cell
                .children
                .iter()
                .filter(|n| n.tag == "p")
                .map(|p| paragraph(p, markers, text_bytes))
                .collect::<Result<Vec<_>>>()?;
            ensure!(!values.is_empty(), "Row cell has no paragraph");
            paragraphs.push(values);
        }
        ensure!(
            !paragraphs.is_empty()
                && paragraphs.len() <= 50
                && columns
                    .replace(paragraphs.len())
                    .is_none_or(|n| n == paragraphs.len()),
            "Ragged or oversized row table"
        );
        rows.push(Row {
            header: false,
            paragraphs,
        });
        ensure!(rows.len() <= 500, "Oversized row table");
    }
    ensure!(
        !rows.is_empty() && markers.active.is_empty(),
        "Empty row table or unclosed bookmarks"
    );
    ensure!(
        markers.visited - marker_start
            == table.all("bookmarkStart").len()
                + table.all("bookmarkEnd").len()
                + table.all("proofErr").len(),
        "Out-of-scope row marker"
    );
    Ok(Table { rows })
}
struct Styles<'a> {
    root: &'a Xml,
    index: BTreeMap<String, Vec<&'a Xml>>,
    work: usize,
    defaults: BTreeMap<String, Option<String>>,
    cache: BTreeMap<(String, String), Vec<&'a Xml>>,
}
impl<'a> Styles<'a> {
    fn new(root: &'a Xml) -> Self {
        let mut index: BTreeMap<String, Vec<&Xml>> = BTreeMap::new();
        for style in root.all("style") {
            for (key, (value, _)) in &style.attrs {
                if key.rsplit(':').next() == Some("styleId") {
                    index.entry(value.clone()).or_default().push(style);
                }
            }
        }
        Self {
            root,
            index,
            work: 0,
            defaults: BTreeMap::new(),
            cache: BTreeMap::new(),
        }
    }
    fn chain(&mut self, kind: &str, reference: Option<&Xml>) -> Result<Vec<&'a Xml>> {
        let id = if let Some(reference) = reference {
            reference.schema(&["val"], true)?;
            Some(
                reference
                    .attr("val")?
                    .context("Missing color style reference")?
                    .to_owned(),
            )
        } else if let Some(id) = self.defaults.get(kind) {
            id.clone()
        } else {
            let mut defaults = Vec::new();
            for style in self.root.all("style") {
                if !style.has_value("type", kind)
                    || !style
                        .attrs
                        .keys()
                        .any(|k| k.rsplit(':').next() == Some("default"))
                {
                    continue;
                }
                let value = style
                    .attr("default")?
                    .context("Missing default style flag")?;
                ensure!(
                    matches!(value, "0" | "false" | "off" | "1" | "true" | "on"),
                    "Ambiguous default color style"
                );
                if matches!(value, "1" | "true" | "on") {
                    defaults.push(
                        style
                            .attr("styleId")?
                            .context("Missing default style identity")?
                            .to_owned(),
                    );
                }
            }
            ensure!(defaults.len() <= 1, "Ambiguous default color style");
            let id = defaults.pop();
            self.defaults.insert(kind.to_owned(), id.clone());
            id
        };
        ensure!(
            id.as_ref().is_none_or(|id| !id.is_empty()),
            "Empty active color style identity"
        );
        let cache_key = (kind.to_owned(), id.clone().unwrap_or_default());
        if let Some(chain) = self.cache.get(&cache_key) {
            self.work = self
                .work
                .saturating_add(chain.iter().map(|s| s.size).sum::<usize>());
            ensure!(
                self.work <= 2_000_000,
                "Cell color style work exceeds limits"
            );
            return Ok(chain.clone());
        }
        let mut id = id;
        let mut seen = BTreeSet::new();
        let mut chain = Vec::new();
        while let Some(current) = id {
            ensure!(
                seen.len() < 64 && seen.insert(current.clone()),
                "Cyclic or excessive color style chain"
            );
            let found = self
                .index
                .get(&current)
                .context("Unknown active color style")?;
            ensure!(found.len() == 1, "Ambiguous active color style identity");
            let style = found[0];
            self.work = self.work.saturating_add(style.size);
            ensure!(
                self.work <= 2_000_000,
                "Cell color style work exceeds limits"
            );
            ensure!(
                self.root.children.iter().any(|n| std::ptr::eq(n, style)),
                "Wrapped active color style"
            );
            style.identity()?;
            ensure!(
                style.attr("type")? == Some(kind)
                    && style.attr("styleId")? == Some(current.as_str()),
                "Wrong active color style identity"
            );
            let base = style.one("basedOn")?;
            ensure!(
                style.all("basedOn").len() == usize::from(base.is_some()),
                "Wrapped color base style"
            );
            id = base
                .map(|base| {
                    base.schema(&["val"], true)?;
                    Ok::<_, anyhow::Error>(
                        base.attr("val")?
                            .context("Missing color base style")?
                            .to_owned(),
                    )
                })
                .transpose()?;
            chain.push(style);
        }
        chain.reverse();
        self.cache.insert(cache_key, chain.clone());
        Ok(chain)
    }
}
fn reference<'a>(node: &'a Xml, properties: &str, name: &str) -> Result<Option<&'a Xml>> {
    let pr = node.one(properties)?;
    ensure!(
        node.all(name).len() == pr.map(|p| p.all(name).len()).unwrap_or(0),
        "Wrapped style reference"
    );
    let found = pr.map(|p| p.one(name)).transpose()?.flatten();
    ensure!(
        pr.map(|p| p.all(name).len()).unwrap_or(0) == usize::from(found.is_some()),
        "Wrapped style reference"
    );
    Ok(found)
}

// Complete raw discovery precedes all new style work. A shd-looking descendant
// excludes FIRST, even when that table also contains malformed header evidence.
fn headers(table: &Xml) -> Result<Vec<bool>> {
    let mut values = Vec::new();
    let mut count = 0;
    for row in table.children.iter().filter(|n| n.tag == "tr") {
        row.identity()?;
        let properties = direct(row, "trPr")?;
        if let Some(properties) = properties {
            properties.schema(&[], false)?;
        }
        let header = properties
            .map(|p| direct(p, "tblHeader"))
            .transpose()?
            .flatten();
        count += usize::from(header.is_some());
        ensure!(
            row.all("tblHeader").len() == usize::from(header.is_some()),
            "Wrapped no-fill header"
        );
        values.push(flag(header)?.unwrap_or(false));
    }
    ensure!(
        table.all("tblHeader").len() == count,
        "Out-of-scope no-fill header"
    );
    Ok(values)
}
fn discover(main: &Xml) -> Result<Vec<Option<Vec<bool>>>> {
    // One complete tree traversal retains all placeholders without repeatedly
    // charging nested excluded subtrees or resolving any irrelevant style.
    fn scan(node: &Xml, flags: &mut BTreeMap<usize, (bool, bool)>) -> (bool, bool) {
        let mut result = (node.tag == "shd", node.tag == "tblHeader");
        for child in &node.children {
            let found = scan(child, flags);
            result.0 |= found.0;
            result.1 |= found.1;
        }
        flags.insert(node as *const Xml as usize, result);
        result
    }
    let mut flags = BTreeMap::new();
    scan(main, &mut flags);
    let tables = main.all("tbl");
    let mut result = Vec::new();
    for table in tables {
        let (shading, header) = flags[&(table as *const Xml as usize)];
        if shading || !header {
            result.push(None);
            continue;
        }
        let body = root(main, "document")?
            .one("body")?
            .context("Missing no-fill body")?;
        ensure!(
            body.children.iter().any(|n| std::ptr::eq(n, table)),
            "Non-direct no-fill header table"
        );
        table.identity()?;
        ensure!(
            table.all("tr").len() == table.children.iter().filter(|n| n.tag == "tr").count(),
            "Wrapped no-fill header rows"
        );
        let headers = headers(table)?;
        result.push(headers.iter().any(|v| *v).then_some(headers));
    }
    Ok(result)
}
fn identities(node: &Xml) -> Result<()> {
    node.identity()?;
    for child in &node.children {
        identities(child)?;
    }
    Ok(())
}
fn property_content(node: &Xml) -> Result<()> {
    ensure!(node.text.trim().is_empty(), "Uncontained property text");
    ensure!(
        !matches!(
            node.tag.as_str(),
            "t" | "p"
                | "r"
                | "tbl"
                | "tr"
                | "tc"
                | "hyperlink"
                | "tab"
                | "br"
                | "cr"
                | "fldSimple"
                | "fldChar"
                | "instrText"
                | "drawing"
                | "pict"
                | "sdt"
                | "customXml"
                | "ins"
                | "del"
                | "moveFrom"
                | "moveTo"
        ),
        "Hidden no-fill property content"
    );
    for child in &node.children {
        property_content(child)?;
    }
    Ok(())
}
fn unshaded(node: &Xml) -> Result<()> {
    ensure!(
        node.all("shd").is_empty() && node.all("highlight").is_empty(),
        "Effective background is not proved absent"
    );
    identities(node)?;
    for tag in ["tblPr", "tblGrid", "trPr", "tcPr", "pPr", "rPr", "tblPrEx"] {
        if node.tag == tag {
            property_content(node)?;
        }
        for property in node.all(tag) {
            property_content(property)?;
        }
    }
    Ok(())
}
fn defaults(styles: &Xml) -> Result<()> {
    styles.identity()?;
    let defaults = styles.one("docDefaults")?;
    ensure!(
        styles.all("docDefaults").len() == usize::from(defaults.is_some()),
        "Wrapped no-fill defaults"
    );
    // All non-style background declarations are relevant; an unexpected wrapper
    // cannot hide a second default or a conditional global background.
    for node in styles.children.iter().filter(|n| n.tag != "style") {
        unshaded(node)?;
        ensure!(
            [
                "pStyle",
                "rStyle",
                "tblStyle",
                "basedOn",
                "numPr",
                "numStyleLink",
                "styleLink"
            ]
            .iter()
            .all(|tag| node.all(tag).is_empty()),
            "Unresolved default style dependency"
        );
    }
    let mut enabled = BTreeSet::new();
    for style in styles.all("style") {
        if style
            .attrs
            .keys()
            .any(|k| k.rsplit(':').next() == Some("default"))
        {
            style.identity()?;
            ensure!(
                matches!(
                    style.attr("type")?,
                    Some("table" | "paragraph" | "character" | "numbering")
                ),
                "Ambiguous no-fill default type"
            );
            if style.attr("type")? == Some("numbering") {
                continue;
            }
            ensure!(
                matches!(
                    style.attr("default")?,
                    Some("0" | "false" | "off" | "1" | "true" | "on")
                ),
                "Ambiguous no-fill default flag"
            );
            if matches!(style.attr("default")?, Some("1" | "true" | "on")) {
                ensure!(
                    styles.children.iter().any(|n| std::ptr::eq(n, style)),
                    "Wrapped no-fill default style"
                );
                let kind = style.attr("type")?.unwrap();
                let id = style
                    .attr("styleId")?
                    .context("Missing no-fill default identity")?;
                ensure!(
                    !id.is_empty() && enabled.insert(kind),
                    "Ambiguous no-fill default identity"
                );
                ensure!(
                    styles
                        .all("style")
                        .iter()
                        .filter(|n| n.has_value("styleId", id))
                        .count()
                        == 1,
                    "Duplicate no-fill default identity"
                );
            }
        }
    }
    Ok(())
}
fn active_unshaded(style: &Xml) -> Result<()> {
    unshaded(style)?;
    ensure!(
        [
            "pStyle",
            "rStyle",
            "tblStyle",
            "numPr",
            "numStyleLink",
            "styleLink"
        ]
        .iter()
        .all(|tag| style.all(tag).is_empty()),
        "Unresolved active style dependency"
    );
    Ok(())
}
fn dependencies(table: &Xml, styles: &mut Styles<'_>) -> Result<()> {
    let mut paragraph_refs = 0usize;
    let mut character_refs = 0usize;
    for style in styles.chain("table", reference(table, "tblPr", "tblStyle")?)? {
        active_unshaded(style)?;
    }
    for style in styles.chain("character", None)? {
        active_unshaded(style)?;
    }
    for paragraph in table.all("p") {
        paragraph_refs += usize::from(reference(paragraph, "pPr", "pStyle")?.is_some());
        for style in styles.chain("paragraph", reference(paragraph, "pPr", "pStyle")?)? {
            active_unshaded(style)?;
        }
        for run in paragraph.all("r") {
            if let Some(reference) = reference(run, "rPr", "rStyle")? {
                character_refs += 1;
                for style in styles.chain("character", Some(reference))? {
                    active_unshaded(style)?;
                }
            }
        }
        // Paragraph-mark run properties may reference a character style too.
        if let Some(pr) = paragraph.one("pPr")? {
            for style_ref in pr.all("rStyle") {
                character_refs += 1;
                let rpr = pr.one("rPr")?.context("Wrapped paragraph-mark style")?;
                ensure!(
                    rpr.one("rStyle")?
                        .is_some_and(|r| std::ptr::eq(r, style_ref)),
                    "Wrapped paragraph-mark style"
                );
                for style in styles.chain("character", Some(style_ref))? {
                    active_unshaded(style)?;
                }
            }
        }
    }
    ensure!(
        table.all("pStyle").len() == paragraph_refs && table.all("rStyle").len() == character_refs,
        "Out-of-scope no-fill style reference"
    );
    Ok(())
}
fn direct_row_subset(table: &Xml) -> Result<()> {
    let mut flags = 0usize;
    for row in table.children.iter().filter(|n| n.tag == "tr") {
        let properties = direct(row, "trPr")?;
        let declaration = properties
            .map(|p| direct(p, "cantSplit"))
            .transpose()?
            .flatten();
        flag(declaration)?;
        flags += usize::from(declaration.is_some());
    }
    ensure!(
        table.all("cantSplit").len() == flags,
        "Out-of-scope no-fill row declaration"
    );
    Ok(())
}
fn prove(main: &Xml, style_tree: &Xml, discovered: Vec<Option<Vec<bool>>>) -> Result<Audit> {
    let tables = main.all("tbl");
    ensure!(
        tables.len() == discovered.len(),
        "No-fill discovery identity changed"
    );
    let body = root(main, "document")?
        .one("body")?
        .context("Missing no-fill body")?;
    body.identity()?;
    let styles = root(style_tree, "styles")?;
    defaults(styles)?;
    let mut styles = Styles::new(styles);
    let mut work = 0usize;
    let mut text_bytes = 0usize;
    let mut proofs = Vec::new();
    let mut markers = Markers::default();
    for (source, header) in tables.into_iter().zip(discovered) {
        let Some(header) = header else {
            proofs.push(None);
            continue;
        };
        work = work.saturating_add(source.size);
        unshaded(source)?;
        direct_row_subset(source)?;
        let mut table = structure(source, body, &mut text_bytes, &mut markers)?;
        ensure!(
            header.len() == table.rows.len(),
            "No-fill header shape changed"
        );
        dependencies(source, &mut styles)?;
        ensure!(
            work.saturating_add(styles.work) <= MAX_WORK,
            "No-fill proof work exceeds limits"
        );
        for (row, header) in table.rows.iter_mut().zip(header) {
            row.header = header;
        }
        proofs.push(Some(table));
    }
    Ok(Audit {
        tables: proofs,
        warning: None,
    })
}
pub(crate) fn analyze(bytes: &[u8]) -> Audit {
    fn inner(bytes: &[u8]) -> Result<Audit> {
        let mut package = OpcReader::new(Cursor::new(bytes))?;
        let main = package.main_document_part()?;
        let tree = xml(&package.read_part(&main)?)?;
        let discovered = discover(&tree)?;
        if discovered.iter().all(Option::is_none) {
            return Ok(Audit::default());
        }
        crate::table_row_pagination::package_identity(&mut package, bytes, &main)?;
        let relationships = package.read_rels_for(&main)?;
        let found = relationships.get_by_type(rel_types::STYLES);
        let data = if let Some(style) = found.first() {
            ensure!(
                style.target_mode == TargetMode::Internal,
                "External no-fill style"
            );
            package.read_part(&main.resolve_relative(&style.target)?)?
        } else {
            // No package style part means no package defaults. Any explicit
            // reference still fails resolution against this empty style tree.
            br#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#
                .to_vec()
        };
        prove(&tree, &xml(&data)?, discovered)
    }
    inner(bytes).unwrap_or_else(|_| failed())
}
fn ir_tables(ir: &DocumentIR) -> Vec<&office_oxide::ir::Table> {
    fn visit<'a>(items: &'a [Element], out: &mut Vec<&'a office_oxide::ir::Table>) {
        for item in items {
            match item {
                Element::Table(t) => {
                    out.push(t);
                    for row in &t.rows {
                        for cell in &row.cells {
                            visit(&cell.content, out);
                        }
                    }
                }
                Element::List(l) => {
                    for i in &l.items {
                        visit(&i.content, out);
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for s in &ir.sections {
        visit(&s.elements, &mut out);
    }
    out
}
pub(crate) fn verify_ir(audit: &Audit, ir: &DocumentIR) -> Result<()> {
    if audit.tables.is_empty() {
        return Ok(());
    }
    let tables = ir_tables(ir);
    ensure!(
        tables.len() == audit.tables.len(),
        "No-fill table identity changed"
    );
    for (proof, table) in audit.tables.iter().zip(tables) {
        let Some(proof) = proof else {
            continue;
        };
        ensure!(
            proof.rows.len() == table.rows.len(),
            "No-fill row identity changed"
        );
        for (proof, row) in proof.rows.iter().zip(&table.rows) {
            ensure!(
                proof.header == row.is_header && proof.header == row.repeat_as_header,
                "No-fill header identity changed"
            );
            ensure!(
                proof.paragraphs.len() == row.cells.len(),
                "No-fill cell identity changed"
            );
            for (proof, cell) in proof.paragraphs.iter().zip(&row.cells) {
                ensure!(
                    cell.col_span == 1 && cell.row_span == 1 && cell.background_color.is_none(),
                    "No-fill background or spans changed"
                );
                let actual = cell
                    .content
                    .iter()
                    .map(|element| match element {
                        Element::Paragraph(p) => Ok(inline_to_text(&p.content)),
                        Element::Heading(h) => Ok(inline_to_text(&h.content)),
                        _ => anyhow::bail!("No-fill cell blocks changed"),
                    })
                    .collect::<Result<Vec<_>>>()?;
                ensure!(actual == *proof, "No-fill paragraph identity changed");
            }
        }
    }
    Ok(())
}
fn model_text(paragraph: &Node) -> Result<String> {
    ensure!(
        paragraph.kind == "paragraph",
        "No-fill model paragraph kind changed"
    );
    paragraph
        .content
        .iter()
        .map(|node| {
            ensure!(node.content.is_empty(), "No-fill nested inline content");
            match node.kind.as_str() {
                "text" => node.text.as_deref().context("Missing no-fill model text"),
                "hardBreak" => Ok("\n"),
                _ => anyhow::bail!("No-fill model inline kind changed"),
            }
        })
        .collect()
}
fn attach_checked(audit: &Audit, doc: &mut Node) -> Result<()> {
    fn visit(
        node: &mut Node,
        direct: bool,
        proofs: &mut std::slice::Iter<'_, Option<Table>>,
    ) -> Result<()> {
        if node.kind == "table"
            && let Some(proof) = proofs.next().context("No-fill model inserted a table")?
        {
            ensure!(
                direct && node.content.len() == proof.rows.len(),
                "No-fill model table position changed"
            );
            for (row, proof) in node.content.iter_mut().zip(&proof.rows) {
                ensure!(
                    row.kind == "tableRow" && row.content.len() == proof.paragraphs.len(),
                    "No-fill model row identity changed"
                );
                for (cell, paragraphs) in row.content.iter_mut().zip(&proof.paragraphs) {
                    ensure!(
                        cell.kind
                            == if proof.header {
                                "tableHeader"
                            } else {
                                "tableCell"
                            },
                        "No-fill model header identity changed"
                    );
                    ensure!(
                        cell.content.len() == paragraphs.len()
                            && !cell.attrs.contains_key("cellFillRgb")
                            && !cell.attrs.contains_key("cellFillMode"),
                        "No-fill model cell identity changed"
                    );
                    for (actual, expected) in cell.content.iter().zip(paragraphs) {
                        ensure!(
                            model_text(actual)? == *expected,
                            "No-fill model paragraph text changed"
                        );
                    }
                    cell.attrs.insert("cellFillMode".into(), "none".into());
                }
            }
        }
        let children_direct = node.kind == "doc";
        for child in &mut node.content {
            visit(child, children_direct, proofs)?;
        }
        Ok(())
    }
    let mut proofs = audit.tables.iter();
    visit(doc, false, &mut proofs)?;
    ensure!(proofs.next().is_none(), "No-fill model dropped a table");
    crate::document::validate(doc)
}
pub(crate) fn attach(
    audit: &Audit,
    ir_matches: bool,
    doc: &mut Node,
    risks: &mut BTreeSet<String>,
) {
    if audit.tables.is_empty() {
        return;
    }
    let mut candidate = doc.clone();
    if ir_matches && attach_checked(audit, &mut candidate).is_ok() {
        *doc = candidate;
    } else {
        risks.insert(BINDING_NORMALIZED.into());
    }
}

/// Independently audit the actual generated package before export can complete.
/// This deliberately does not depend on source candidacy: authored body-only
/// no-fill is legal and needs the same no-inherited-background guarantee.
pub(crate) fn generated_package(bytes: &[u8], model: &Node) -> Result<()> {
    if !has_none(model) {
        return Ok(());
    }
    let mut package = OpcReader::new(Cursor::new(bytes))?;
    let main = package.main_document_part()?;
    crate::table_row_pagination::package_identity(&mut package, bytes, &main)?;
    let tree = xml(&package.read_part(&main)?)?;
    let relationships = package.read_rels_for(&main)?;
    let style = relationships.get_by_type(rel_types::STYLES);
    ensure!(style.len() == 1, "Missing generated no-fill styles");
    let style_tree = xml(&package.read_part(&main.resolve_relative(&style[0].target)?)?)?;
    let styles_root = root(&style_tree, "styles")?;
    defaults(styles_root)?;
    let mut styles = Styles::new(styles_root);
    let tables = tree.all("tbl");
    fn model_tables<'a>(node: &'a Node, tables: &mut Vec<&'a Node>) {
        if node.kind == "table" {
            tables.push(node);
        }
        for child in &node.content {
            model_tables(child, tables);
        }
    }
    let mut models = Vec::new();
    model_tables(model, &mut models);
    ensure!(
        tables.len() == models.len(),
        "Generated no-fill table identity changed"
    );
    let mut work = 0usize;
    for (table, model) in tables.into_iter().zip(models) {
        if !has_none(model) {
            continue;
        }
        work = work.saturating_add(table.size);
        dependencies(table, &mut styles)?;
        ensure!(
            work.saturating_add(styles.work) <= MAX_WORK,
            "Generated no-fill work exceeds limits"
        );
        let rows = table
            .children
            .iter()
            .filter(|n| n.tag == "tr")
            .collect::<Vec<_>>();
        ensure!(
            rows.len() == model.content.len(),
            "Generated no-fill rows changed"
        );
        for (row, model) in rows.into_iter().zip(&model.content) {
            let cells = row
                .children
                .iter()
                .filter(|n| n.tag == "tc")
                .collect::<Vec<_>>();
            ensure!(
                cells.len() == model.content.len(),
                "Generated no-fill cells changed"
            );
            for (cell, model) in cells.into_iter().zip(&model.content) {
                if crate::document::cell_fill_none(model)? {
                    unshaded(cell)?;
                }
            }
        }
        // A generated row/table property background also affects marked cells.
        for tag in ["tblPr", "trPr", "tblPrEx"] {
            for properties in table.all(tag) {
                unshaded(properties)?;
            }
        }
    }
    Ok(())
}
fn has_none(node: &Node) -> bool {
    node.attrs.contains_key("cellFillMode") || node.content.iter().any(has_none)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    fn main(body: &str) -> Xml {
        xml(format!(r#"<w:document xmlns:w="{NS}" xmlns:x="urn:foreign"><w:body>{body}</w:body></w:document>"#).as_bytes()).unwrap()
    }
    fn styles(body: &str) -> Xml {
        xml(
            format!(r#"<w:styles xmlns:w="{NS}" xmlns:x="urn:foreign">{body}</w:styles>"#)
                .as_bytes(),
        )
        .unwrap()
    }
    fn table(header: &str, text: &str) -> String {
        format!(
            "<w:tbl><w:tr><w:trPr>{header}</w:trPr><w:tc><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>"
        )
    }
    fn audit(body: &str, style: &str) -> Result<Audit> {
        let main = main(body);
        let discovered = discover(&main)?;
        if discovered.iter().all(Option::is_none) {
            return Ok(Audit::default());
        }
        prove(&main, &styles(style), discovered)
    }
    fn model(text: &str) -> Node {
        serde_json::from_value(json!({"type":"doc","content":[{"type":"table","content":[{"type":"tableRow","content":[{"type":"tableHeader","content":[{"type":"paragraph","content":[{"type":"text","text":text}]}]}]},{"type":"tableRow","content":[{"type":"tableCell","content":[{"type":"paragraph"}]}]}]}]})).unwrap()
    }
    #[test]
    fn discovery_excludes_shading_before_any_header_validation() {
        let shaded =
            table("<x:tblHeader/>", "Old").replace("<w:tc>", "<w:tc><w:tcPr><x:shd/></w:tcPr>");
        let good = table("<w:tblHeader/>", "New");
        let result = audit(&(shaded + &good), "").unwrap();
        assert_eq!(result.tables.len(), 2);
        assert!(result.tables[0].is_none());
        assert!(result.tables[1].is_some());
    }
    #[test]
    fn no_header_and_valid_false_are_exact_no_candidate_even_with_broken_styles() {
        for header in [
            "",
            "<w:tblHeader w:val=\"0\"/>",
            "<w:tblHeader w:val=\"false\"/>",
            "<w:tblHeader w:val=\"off\"/>",
        ] {
            let a = audit(
                &table(header, "Old"),
                "<x:docDefaults><x:shd/></x:docDefaults>",
            )
            .unwrap();
            assert!(a.tables.is_empty() && a.warning.is_none());
        }
    }
    #[test]
    fn independent_boolean_header_discovery_is_exact_and_atomic() {
        for flag in [
            "<w:tblHeader/>",
            "<w:tblHeader w:val=\"true\"/>",
            "<w:tblHeader w:val=\"1\"/>",
            "<w:tblHeader w:val=\"on\"/>",
        ] {
            let a = audit(&table(flag, "Header"), "").unwrap();
            let t = a.tables[0].as_ref().unwrap();
            assert!(t.rows[0].header && !t.rows[1].header);
        }
        for flag in [
            "<w:tblHeader w:val=\"TRUE\"/>",
            "<w:tblHeader w:val=\"2\"/>",
            "<w:tblHeader extra=\"1\"/>",
            "<w:tblHeader>text</w:tblHeader>",
            "<w:tblHeader><w:b/></w:tblHeader>",
            "<x:tblHeader/>",
            "<w:tblHeader/><w:tblHeader/>",
            "<w:sdt><w:tblHeader/></w:sdt>",
        ] {
            assert!(
                audit(&(table("<w:tblHeader/>", "Good") + &table(flag, "Bad")), "").is_err(),
                "{flag}"
            );
        }
        for wrapper in [
            "<w:sdt>{}</w:sdt>",
            "<w:tbl><w:tr><w:tc>{}</w:tc></w:tr></w:tbl>",
        ] {
            assert!(
                audit(
                    &wrapper.replace("{}", &table("<w:tblHeader w:val=\"false\"/>", "Nested")),
                    ""
                )
                .is_err()
            );
        }
    }
    #[test]
    fn active_chains_defaults_and_paragraph_character_dependencies_must_be_unshaded() {
        let source = table("<w:tblHeader/>", "Header");
        let unused = "<w:style w:type=\"table\" w:styleId=\"Unused\"><w:tblPr><w:shd w:fill=\"ABCDEF\"/></w:tblPr></w:style>";
        assert!(audit(&source, unused).is_ok());
        for kind in ["table", "paragraph", "character"] {
            let active = format!(
                "<w:style w:type=\"{kind}\" w:default=\"1\" w:styleId=\"Default\"><w:basedOn w:val=\"Parent\"/></w:style><w:style w:type=\"{kind}\" w:styleId=\"Parent\"><w:rPr><w:shd/></w:rPr></w:style>"
            );
            assert!(audit(&source, &active).is_err(), "{kind}");
            assert!(audit(&source, &active.replace("<w:shd/>", "")).is_ok());
            assert!(
                audit(
                    &source,
                    &active.replace("w:styleId=\"Parent\"", "w:styleId=\"Missing\"")
                )
                .is_err()
            );
        }
        for defaults in [
            "<w:docDefaults><w:rPrDefault><w:rPr><w:shd/></w:rPr></w:rPrDefault></w:docDefaults>",
            "<w:docDefaults/><w:docDefaults/>",
            "<w:sdt><w:docDefaults/></w:sdt>",
            "<w:style w:type=\"table\" w:default=\"maybe\" w:styleId=\"A\"/>",
        ] {
            assert!(audit(&source, defaults).is_err(), "{defaults}");
        }
        for reference in ["tblStyle", "pStyle", "rStyle"] {
            let tagged = match reference {
                "tblStyle" => source.replace(
                    "<w:tbl>",
                    "<w:tbl><w:tblPr><w:tblStyle w:val=\"Active\"/></w:tblPr>",
                ),
                "pStyle" => {
                    source.replace("<w:p>", "<w:p><w:pPr><w:pStyle w:val=\"Active\"/></w:pPr>")
                }
                _ => source.replace("<w:r>", "<w:r><w:rPr><w:rStyle w:val=\"Active\"/></w:rPr>"),
            };
            let kind = match reference {
                "tblStyle" => "table",
                "pStyle" => "paragraph",
                _ => "character",
            };
            let sheet = format!(
                "<w:style w:type=\"{kind}\" w:styleId=\"Active\"><w:rPr><w:shd/></w:rPr></w:style>"
            );
            assert!(audit(&tagged, &sheet).is_err());
            assert!(audit(&tagged, &sheet.replace("<w:shd/>", "")).is_ok());
        }
    }
    #[test]
    fn duplicate_cyclic_wrapped_and_oversized_active_styles_fail() {
        let source = table("<w:tblHeader/>", "Header");
        let base = "<w:style w:type=\"table\" w:default=\"1\" w:styleId=\"A\"/>";
        assert!(audit(&source, &(base.to_owned() + base)).is_err());
        assert!(audit(&source, &format!("<w:sdt>{base}</w:sdt>")).is_err());
        let cyclic = base.replace("/>", "><w:basedOn w:val=\"A\"/></w:style>");
        assert!(audit(&source, &cyclic).is_err());
        // Internal safeguard only; the public importer retains its earlier20-chain gate.
        let chain = (0..65)
            .map(|n| {
                format!(
                    "<w:style w:type=\"table\" w:styleId=\"S{n}\" {}>{}</w:style>",
                    if n == 0 { "w:default=\"1\"" } else { "" },
                    if n == 64 {
                        String::new()
                    } else {
                        format!("<w:basedOn w:val=\"S{}\"/>", n + 1)
                    }
                )
            })
            .collect::<String>();
        assert!(audit(&source, &chain).is_err());
    }
    #[test]
    fn passive_markers_are_complete_paired_and_location_bound() {
        let source = table("<w:tblHeader/>", "Header").replace("<w:p><w:r>", "<w:p><w:bookmarkStart w:id=\"0\" w:name=\"_GoBack\"/><w:proofErr w:type=\"spellStart\"/><w:r>").replace("</w:r></w:p>", "</w:r><w:proofErr w:type=\"spellEnd\"/></w:p>").replace("</w:tbl>", "<w:bookmarkEnd w:id=\"0\"/></w:tbl>");
        assert!(audit(&source, "").is_ok());
        for bad in [
            source.replace("w:id=\"0\"/>", "w:id=\"1\"/>"),
            source.replace(" w:name=\"_GoBack\"", ""),
            source.replace("spellEnd", "gramEnd"),
            source.replace("<w:bookmarkEnd", "<x:bookmarkEnd"),
            source.replace(
                "<w:bookmarkEnd w:id=\"0\"/>",
                "<w:bookmarkEnd w:id=\"0\">x</w:bookmarkEnd>",
            ),
            source.replace(
                "<w:bookmarkEnd w:id=\"0\"/>",
                "<w:bookmarkEnd w:id=\"0\"/><w:tblPr/>",
            ),
            source.replace("<w:trPr>", "<w:trPr><w:bookmarkEnd w:id=\"0\"/>"),
        ] {
            assert!(audit(&bad, "").is_err(), "{bad}");
        }
    }
    #[test]
    fn complete_paragraph_identity_preserves_empty_tabs_breaks_and_shapes() {
        let source = table("<w:tblHeader/>", "A")
            .replace("<w:t>A</w:t>", "<w:t>A</w:t><w:tab/><w:br/><w:t>B</w:t>");
        let a = audit(&source, "").unwrap();
        assert_eq!(
            a.tables[0].as_ref().unwrap().rows[0].paragraphs,
            vec![vec!["A\t\nB".to_owned()]]
        );
        assert_eq!(
            a.tables[0].as_ref().unwrap().rows[1].paragraphs,
            vec![vec![String::new()]]
        );
        for insert in [
            "<w:gridSpan w:val=\"2\"/>",
            "<w:vMerge/>",
            "<w:vanish/>",
            "<w:textDirection/>",
            "<w:fldChar/>",
        ] {
            assert!(
                audit(
                    &source.replace("<w:tc>", &format!("<w:tc><w:tcPr>{insert}</w:tcPr>")),
                    ""
                )
                .is_err()
            );
        }
    }
    #[test]
    fn failed_and_absent_audits_do_not_add_binding_risks() {
        for a in [Audit::default(), failed()] {
            let mut doc = model("Header");
            let before = serde_json::to_value(&doc).unwrap();
            let mut risks = BTreeSet::from(["old".into()]);
            attach(&a, false, &mut doc, &mut risks);
            assert_eq!(serde_json::to_value(doc).unwrap(), before);
            assert_eq!(risks, BTreeSet::from(["old".into()]));
        }
    }
    #[test]
    fn late_model_mismatch_rolls_back_every_new_mode_without_touching_old_fields() {
        let a = audit(
            &(table("<w:tblHeader/>", "First") + &table("<w:tblHeader/>", "Second")),
            "",
        )
        .unwrap();
        let mut doc = model("First");
        doc.content.push(model("Wrong").content.remove(0));
        doc.content[0].content[1]
            .attrs
            .insert("rowCantSplit".into(), true.into());
        let old = serde_json::to_value(&doc).unwrap();
        let mut risks = BTreeSet::from(["baseline".into()]);
        attach(&a, true, &mut doc, &mut risks);
        assert_eq!(serde_json::to_value(doc).unwrap(), old);
        assert_eq!(
            risks,
            BTreeSet::from(["baseline".into(), BINDING_NORMALIZED.into()])
        );
    }
    #[test]
    fn successful_model_binding_stamps_all_cells_and_preserves_text_mode() {
        let a = audit(&table("<w:tblHeader/>", "Header"), "").unwrap();
        let mut doc = model("Header");
        doc.content[0].content[0].content[0]
            .attrs
            .insert("cellTextColorMode".into(), "auto".into());
        let mut risks = BTreeSet::new();
        attach(&a, true, &mut doc, &mut risks);
        assert!(risks.is_empty());
        for row in &doc.content[0].content {
            assert_eq!(row.content[0].attrs["cellFillMode"], "none");
        }
        assert_eq!(
            doc.content[0].content[0].content[0].attrs["cellTextColorMode"],
            "auto"
        );
    }
    #[test]
    fn model_paragraph_split_kind_fill_and_header_mismatches_never_attach() {
        let a = audit(&table("<w:tblHeader/>", "Header"), "").unwrap();
        for case in 0..7 {
            let mut doc = model("Header");
            let cell = &mut doc.content[0].content[0].content[0];
            match case {
                0 => cell.kind = "tableCell".into(),
                1 => cell
                    .content
                    .push(serde_json::from_value(json!({"type":"paragraph"})).unwrap()),
                2 => cell.content[0].kind = "heading".into(),
                3 => {
                    cell.attrs.insert("cellFillRgb".into(), "FFFFFF".into());
                }
                4 => {
                    cell.attrs.insert("cellFillMode".into(), "none".into());
                }
                5 => cell.content[0].content[0].text = Some("Wrong".into()),
                _ => {
                    cell.attrs.insert("colspan".into(), 2.into());
                }
            }
            let old = serde_json::to_value(&doc).unwrap();
            let mut risks = BTreeSet::new();
            attach(&a, true, &mut doc, &mut risks);
            assert_eq!(serde_json::to_value(doc).unwrap(), old);
            assert!(risks.contains(BINDING_NORMALIZED));
        }
    }
    #[test]
    fn cumulative_text_and_structural_limits_fail_atomically() {
        let half = "x".repeat(1024 * 1024 + 1);
        assert!(
            audit(
                &(table("<w:tblHeader/>", &half) + &table("<w:tblHeader/>", &half)),
                ""
            )
            .is_err()
        );
        let rows = "<w:tr><w:tc><w:p/></w:tc></w:tr>".repeat(500);
        assert!(
            audit(
                &table("<w:tblHeader/>", "A").replace("</w:tbl>", &(rows + "</w:tbl>")),
                ""
            )
            .is_err()
        );
        let cells = "<w:tc><w:p/></w:tc>".repeat(51);
        assert!(
            audit(
                &format!("<w:tbl><w:tr><w:trPr><w:tblHeader/></w:trPr>{cells}</w:tr></w:tbl>"),
                ""
            )
            .is_err()
        );
    }

    #[test]
    fn hidden_property_text_and_unresolved_default_dependencies_are_not_absence_proofs() {
        let source = table("<w:tblHeader/>", "Header");
        for properties in [
            "<w:pPr><w:r><w:t>Hidden</w:t></w:r></w:pPr>",
            "<w:pPr>Hidden</w:pPr>",
            "<w:pPr><w:rPr><w:t>Hidden</w:t></w:rPr></w:pPr>",
        ] {
            assert!(audit(&source.replace("<w:p>", &format!("<w:p>{properties}")), "").is_err());
        }
        assert!(
            audit(
                &source.replace("<w:tbl>", "<w:tbl><w:tblGrid><w:t>Hidden</w:t></w:tblGrid>"),
                ""
            )
            .is_err()
        );
        assert!(
            audit(
                &source.replace(
                    "<w:tc>",
                    "<w:tc><w:tcPr><w:rStyle w:val=\"Hidden\"/></w:tcPr>"
                ),
                ""
            )
            .is_err()
        );
        for reference in [
            "pStyle",
            "rStyle",
            "tblStyle",
            "numPr",
            "numStyleLink",
            "styleLink",
        ] {
            let defaults = format!(
                "<w:docDefaults><w:pPrDefault><w:pPr><w:{reference} w:val=\"Hidden\"/></w:pPr></w:pPrDefault></w:docDefaults>"
            );
            assert!(audit(&source, &defaults).is_err());
        }
        let explicit = source.replace(
            "<w:tbl>",
            "<w:tbl><w:tblPr><w:tblStyle w:val=\"Explicit\"/></w:tblPr>",
        );
        let ambiguous = "<w:style w:type=\"table\" w:styleId=\"Explicit\"/><w:style w:type=\"table\" w:default=\"1\" w:styleId=\"A\"/><w:style w:type=\"table\" w:default=\"1\" w:styleId=\"B\"/>";
        assert!(audit(&explicit, ambiguous).is_err());
    }
    fn ir() -> DocumentIR {
        use office_oxide::ir::{InlineContent, Paragraph, Section, TableCell, TableRow, TextSpan};
        DocumentIR {
            sections: vec![Section {
                elements: vec![Element::Table(office_oxide::ir::Table {
                    rows: vec![
                        TableRow {
                            is_header: true,
                            repeat_as_header: true,
                            cells: vec![TableCell {
                                col_span: 1,
                                row_span: 1,
                                content: vec![Element::Paragraph(Paragraph {
                                    content: vec![InlineContent::Text(TextSpan::plain("Header"))],
                                    ..Default::default()
                                })],
                                ..Default::default()
                            }],
                            ..Default::default()
                        },
                        TableRow {
                            cells: vec![TableCell {
                                col_span: 1,
                                row_span: 1,
                                content: vec![Element::Paragraph(Paragraph::default())],
                                ..Default::default()
                            }],
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                })],
                ..Default::default()
            }],
            ..Default::default()
        }
    }
    #[test]
    fn raw_ir_binding_independently_checks_both_header_flags_background_and_full_shape() {
        let a = audit(&table("<w:tblHeader/>", "Header"), "").unwrap();
        assert!(verify_ir(&a, &ir()).is_ok());
        for case in 0..11 {
            let mut ir = ir();
            let Element::Table(table) = &mut ir.sections[0].elements[0] else {
                unreachable!()
            };
            match case {
                0 => table.rows[0].is_header = false,
                1 => table.rows[0].repeat_as_header = false,
                2 => table.rows[1].is_header = true,
                3 => table.rows[1].repeat_as_header = true,
                4 => table.rows[0].cells[0].background_color = Some([255, 255, 255]),
                5 => table.rows[0].cells[0].col_span = 2,
                6 => table.rows[1].cells[0].content.clear(),
                7 => table.rows.swap(0, 1),
                8 => {
                    table.rows.pop();
                }
                9 => table.rows[0].cells[0].col_span = 0,
                _ => table.rows[0].cells[0].row_span = 0,
            }
            assert!(verify_ir(&a, &ir).is_err(), "{case}");
        }
        let source = table("<w:tblHeader/>", "Header");
        let both = audit(&(source.clone() + &source), "").unwrap();
        for column_zero in [true, false] {
            let mut actual = ir();
            actual.sections[0]
                .elements
                .push(ir().sections[0].elements.remove(0));
            let Element::Table(second) = &mut actual.sections[0].elements[1] else {
                unreachable!()
            };
            if column_zero {
                second.rows[0].cells[0].col_span = 0;
            } else {
                second.rows[0].cells[0].row_span = 0;
            }
            let matches = verify_ir(&both, &actual).is_ok();
            assert!(!matches);
            let mut doc = model("Header");
            doc.content.push(model("Header").content.remove(0));
            let baseline = serde_json::to_value(&doc).unwrap();
            let mut risks = BTreeSet::from(["old risk".into()]);
            attach(&both, matches, &mut doc, &mut risks);
            assert_eq!(serde_json::to_value(doc).unwrap(), baseline);
            assert_eq!(
                risks,
                BTreeSet::from(["old risk".into(), BINDING_NORMALIZED.into()])
            );
        }
        assert!(verify_ir(&Audit::default(), &ir()).is_ok());
        assert!(verify_ir(&failed(), &ir()).is_ok());
    }

    #[test]
    fn marker_id_and_name_reuse_in_a_late_candidate_rolls_back_the_group() {
        let source = table("<w:tblHeader/>", "Header")
            .replace(
                "<w:p><w:r>",
                "<w:p><w:bookmarkStart w:id=\"0\" w:name=\"Mark\"/><w:r>",
            )
            .replace("</w:tbl>", "<w:bookmarkEnd w:id=\"0\"/></w:tbl>");
        assert!(audit(&source, "").is_ok());
        assert!(audit(&(source.clone() + &source), "").is_err());
        assert!(
            audit(
                &(source.clone() + &source.replace("w:id=\"0\"", "w:id=\"1\"")),
                ""
            )
            .is_err()
        );
        assert!(
            audit(
                &(source.clone()
                    + &source
                        .replace("w:id=\"0\"", "w:id=\"1\"")
                        .replace("w:name=\"Mark\"", "w:name=\"Other\"")),
                ""
            )
            .is_ok()
        );
    }

    fn package(body: &str, style_body: &str, target: &str, relationships_extra: &str) -> Vec<u8> {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let main = format!(r#"<w:document xmlns:w="{NS}"><w:body>{body}</w:body></w:document>"#);
        let styles = format!(r#"<w:styles xmlns:w="{NS}">{style_body}</w:styles>"#);
        let content_types = format!(
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/{target}" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#
        );
        let root_rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="main" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let rels = format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="{target}"/>{relationships_extra}</Relationships>"#
        );
        for (name, data) in [
            ("[Content_Types].xml".to_owned(), content_types),
            ("_rels/.rels".to_owned(), root_rels.to_owned()),
            ("word/_rels/document.xml.rels".to_owned(), rels),
            ("word/document.xml".to_owned(), main),
            (format!("word/{target}"), styles),
        ] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(data.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }
    #[test]
    fn actual_opc_styles_relationship_and_content_type_bind_before_absence_proof() {
        let source = table("<w:tblHeader/>", "Header");
        for target in ["styles.xml", "metadata/actual-styles.xml"] {
            let bytes = package(&source, "", target, "");
            let audit = analyze(&bytes);
            assert!(audit.warning.is_none() && audit.tables[0].is_some());
        }
        for extra in [
            r#"<Relationship Id="duplicate" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="missing.xml"/>"#,
            r#"<Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="unused.xml"/>"#,
            r#"<Relationship Id="lookalike" Type="urn:foreign/styles" Target="styles.xml"/>"#,
        ] {
            assert_eq!(
                analyze(&package(&source, "", "styles.xml", extra)).warning,
                Some(NORMALIZED)
            );
        }
        let no_header = table("", "Old");
        assert!(
            analyze(&package(
                &no_header,
                "<w:docDefaults><w:shd/></w:docDefaults>",
                "styles.xml",
                r#"<Relationship Id="styles" Type="urn:duplicate" Target="unused.xml"/>"#
            ))
            .warning
            .is_none()
        );
        // A stylesWithEffects role is independent and cannot replace actual styles.
        let shade = r#"<w:style w:type="table" w:default="1" w:styleId="A"><w:tblPr><w:shd/></w:tblPr></w:style>"#;
        let bytes = package(
            &source,
            shade,
            "actual.xml",
            r#"<Relationship Id="effects" Type="http://schemas.microsoft.com/office/2007/relationships/stylesWithEffects" Target="styles.xml"/>"#,
        );
        assert_eq!(analyze(&bytes).warning, Some(NORMALIZED));
        assert_eq!(analyze(b"not an OPC package").warning, Some(NORMALIZED));
    }
    #[test]
    fn repeated_active_chain_work_and_exact_text_limit_are_bounded() {
        let source = table("<w:tblHeader/>", &"x".repeat(2 * 1024 * 1024));
        assert!(audit(&source, "").is_ok());
        let source = table("<w:tblHeader/>", &"x".repeat(2 * 1024 * 1024 + 1));
        assert!(audit(&source, "").is_err());
        let style = format!(
            "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"A\">{}</w:style>",
            "<w:name w:val=\"Unused metadata\"/>".repeat(1000)
        );
        let paragraphs = "<w:p><w:r><w:t>x</w:t></w:r></w:p>".repeat(2100);
        let source = format!(
            "<w:tbl><w:tr><w:trPr><w:tblHeader/></w:trPr><w:tc>{paragraphs}</w:tc></w:tr></w:tbl>"
        );
        assert!(audit(&source, &style).is_err());
    }

    #[test]
    fn direct_row_declarations_use_the_explicit_conservative_subset() {
        for value in [
            "<w:cantSplit/>",
            "<w:cantSplit w:val=\"1\"/>",
            "<w:cantSplit w:val=\"0\"/>",
            "<w:cantSplit w:val=\"false\"/>",
        ] {
            assert!(audit(&table(&format!("<w:tblHeader/>{value}"), "Header"), "").is_ok());
        }
        for value in [
            "<w:cantSplit w:val=\"bad\"/>",
            "<w:cantSplit/><w:cantSplit/>",
            "<w:cantSplit><w:b/></w:cantSplit>",
            "<x:cantSplit/>",
            "<w:rPr><w:cantSplit/></w:rPr>",
        ] {
            assert!(audit(&table(&format!("<w:tblHeader/>{value}"), "Header"), "").is_err());
        }
        let inherited = "<w:style w:type=\"table\" w:default=\"1\" w:styleId=\"A\"><w:trPr><w:cantSplit/><w:tblHeader/></w:trPr></w:style>";
        assert!(audit(&table("<w:tblHeader/>", "Header"), inherited).is_ok());
    }
    #[test]
    fn inherited_numbering_is_unproved_but_unrelated_numbering_defaults_are_ignored() {
        let source = table("<w:tblHeader/>", "Header");
        for tag in ["numPr", "numStyleLink", "styleLink"] {
            for property in ["tblPr", "trPr", "tcPr", "tblGrid"] {
                let direct = source.replace(
                    "<w:tbl>",
                    &format!("<w:tbl><w:{property}><w:{tag}/></w:{property}>"),
                );
                assert!(audit(&direct, "").is_err());
            }
            let defaults = format!(
                "<w:docDefaults><w:pPrDefault><w:pPr><w:{tag}/></w:pPr></w:pPrDefault></w:docDefaults>"
            );
            assert!(audit(&source, &defaults).is_err());
            let active = format!(
                "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"A\"><w:pPr><w:{tag}/></w:pPr></w:style>"
            );
            assert!(audit(&source, &active).is_err());
        }
        assert!(audit(&source, "<w:style w:type=\"numbering\" w:default=\"malformed\" w:styleId=\"Unused\"><w:rPr><w:shd/></w:rPr></w:style>").is_ok());
    }
}
