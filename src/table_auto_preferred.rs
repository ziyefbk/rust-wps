//! Narrow source AutoFit preferences. This group never changes old hard gates.
//! Preferences are not physical frames; final model validation is mandatory.
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

pub(crate) const PRESERVED: &str = "Automatic table width preferences preserved; intrinsic layout and pagination remain consumer-dependent";
pub(crate) const NORMALIZED: &str = "Automatic table width preferences not preserved: source or converted identity is outside the supported subset";
const MAX_WORK: usize = 2_000_000;
const ATTRIBUTE: &str = "autoPreferredColumnWidthsTwips";
#[derive(Default)]
pub(crate) struct Audit {
    tables: Vec<Option<Table>>,
    automatic: Vec<bool>,
    pub warning: Option<&'static str>,
}
struct Table {
    rows: Vec<Row>,
    widths: Vec<u64>,
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
#[derive(Default, Clone)]
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
                "Unsupported preferred-width grid declaration"
            );
            column.schema(&["w"], true)?;
            if let Some(width) = column.attr("w")? {
                ensure!(
                    !width.is_empty()
                        && width.bytes().all(|b| b.is_ascii_digit())
                        && width.parse::<u32>().is_ok(),
                    "Invalid preferred-width grid width"
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
    Ok(Table {
        rows,
        widths: Vec::new(),
    })
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
                    .context("Missing preferred style reference")?
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
                    "Ambiguous default preferred style"
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
            ensure!(defaults.len() <= 1, "Ambiguous default preferred style");
            let id = defaults.pop();
            self.defaults.insert(kind.to_owned(), id.clone());
            id
        };
        ensure!(
            id.as_ref().is_none_or(|id| !id.is_empty()),
            "Empty active preferred style identity"
        );
        let cache_key = (kind.to_owned(), id.clone().unwrap_or_default());
        if let Some(chain) = self.cache.get(&cache_key) {
            self.work = self
                .work
                .saturating_add(chain.iter().map(|s| s.size).sum::<usize>());
            ensure!(
                self.work <= 2_000_000,
                "Cell preferred style work exceeds limits"
            );
            return Ok(chain.clone());
        }
        let mut id = id;
        let mut seen = BTreeSet::new();
        let mut chain = Vec::new();
        while let Some(current) = id {
            ensure!(
                seen.len() < 64 && seen.insert(current.clone()),
                "Cyclic or excessive preferred style chain"
            );
            let found = self
                .index
                .get(&current)
                .context("Unknown active preferred style")?;
            ensure!(
                found.len() == 1,
                "Ambiguous active preferred style identity"
            );
            let style = found[0];
            self.work = self.work.saturating_add(style.size);
            ensure!(
                self.work <= 2_000_000,
                "Cell preferred style work exceeds limits"
            );
            ensure!(
                self.root.children.iter().any(|n| std::ptr::eq(n, style)),
                "Wrapped active preferred style"
            );
            style.identity()?;
            ensure!(
                style.attr("type")? == Some(kind)
                    && style.attr("styleId")? == Some(current.as_str()),
                "Wrong active preferred style identity"
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
            "Wrapped preferred-width header"
        );
        values.push(flag(header)?.unwrap_or(false));
    }
    ensure!(
        table.all("tblHeader").len() == count,
        "Out-of-scope preferred-width header"
    );
    Ok(values)
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
        "Hidden preferred-width property content"
    );
    for child in &node.children {
        property_content(child)?;
    }
    Ok(())
}
fn safe_properties(node: &Xml) -> Result<()> {
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
        "Wrapped preferred-width defaults"
    );
    // All non-style background declarations are relevant; an unexpected wrapper
    // cannot hide a second default or a conditional global background.
    for node in styles.children.iter().filter(|n| n.tag != "style") {
        active_safe_properties(node)?;
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
                "Ambiguous preferred-width default type"
            );
            if style.attr("type")? == Some("numbering") {
                continue;
            }
            ensure!(
                matches!(
                    style.attr("default")?,
                    Some("0" | "false" | "off" | "1" | "true" | "on")
                ),
                "Ambiguous preferred-width default flag"
            );
            if matches!(style.attr("default")?, Some("1" | "true" | "on")) {
                ensure!(
                    styles.children.iter().any(|n| std::ptr::eq(n, style)),
                    "Wrapped preferred-width default style"
                );
                let kind = style.attr("type")?.unwrap();
                let id = style
                    .attr("styleId")?
                    .context("Missing preferred-width default identity")?;
                ensure!(
                    !id.is_empty() && enabled.insert(kind),
                    "Ambiguous preferred-width default identity"
                );
                ensure!(
                    styles
                        .all("style")
                        .iter()
                        .filter(|n| n.has_value("styleId", id))
                        .count()
                        == 1,
                    "Duplicate preferred-width default identity"
                );
            }
        }
    }
    Ok(())
}
fn default_table_geometry(owner: &Xml) -> Result<()> {
    let pr = owner.one("tblPr")?;
    ensure!(
        owner.all("tblPr").len() == usize::from(pr.is_some()),
        "Conditional or wrapped preferred table properties"
    );
    for tag in ["tblInd", "tblCellMar"] {
        let value = pr.map(|p| direct(p, tag)).transpose()?.flatten();
        ensure!(
            owner.all(tag).len() == usize::from(value.is_some()),
            "Wrapped preferred default geometry: {tag}"
        );
        if let Some(value) = value {
            if tag == "tblInd" {
                value.schema(&["type", "w"], true)?;
                ensure!(
                    value.attr("type")? == Some("dxa") && value.attr("w")? == Some("0"),
                    "Nonzero inherited preferred indentation"
                );
            } else {
                value.schema(&[], false)?;
                for side in &value.children {
                    ensure!(
                        matches!(side.tag.as_str(), "top" | "left" | "bottom" | "right"),
                        "Unsupported preferred default margin"
                    );
                    value.one(&side.tag)?;
                    side.schema(&["type", "w"], true)?;
                    let expected = if matches!(side.tag.as_str(), "left" | "right") {
                        "108"
                    } else {
                        "0"
                    };
                    ensure!(
                        side.attr("type")? == Some("dxa") && side.attr("w")? == Some(expected),
                        "Nondefault inherited preferred margin"
                    );
                }
            }
        }
    }
    if let Some(pr) = pr
        && let Some(jc) = direct(pr, "jc")?
    {
        jc.schema(&["val"], true)?;
        ensure!(
            matches!(jc.attr("val")?, Some("left" | "start")),
            "Unsupported inherited preferred table alignment"
        );
    }
    ensure!(
        owner.all("tcMar").is_empty(),
        "Inherited cell margins are outside preferred subset"
    );
    Ok(())
}
fn active_safe_properties(style: &Xml) -> Result<()> {
    safe_properties(style)?;
    default_table_geometry(style)?;
    for tag in [
        "tblW",
        "tblLayout",
        "tblGrid",
        "gridCol",
        "tcW",
        "ind",
        "tblHeader",
        "vanish",
        "webHidden",
        "specVanish",
        "bidi",
        "rtl",
        "textDirection",
        "bidiVisual",
        "noWrap",
        "tcFitText",
        "fitText",
    ] {
        ensure!(
            style.all(tag).is_empty(),
            "Unsupported inherited preferred-width dependency: {tag}"
        );
    }
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
        active_safe_properties(style)?;
    }
    for style in styles.chain("character", None)? {
        active_safe_properties(style)?;
    }
    for paragraph in table.all("p") {
        paragraph_refs += usize::from(reference(paragraph, "pPr", "pStyle")?.is_some());
        for style in styles.chain("paragraph", reference(paragraph, "pPr", "pStyle")?)? {
            active_safe_properties(style)?;
        }
        for run in paragraph.all("r") {
            if let Some(reference) = reference(run, "rPr", "rStyle")? {
                character_refs += 1;
                for style in styles.chain("character", Some(reference))? {
                    active_safe_properties(style)?;
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
                    active_safe_properties(style)?;
                }
            }
        }
    }
    ensure!(
        table.all("pStyle").len() == paragraph_refs && table.all("rStyle").len() == character_refs,
        "Out-of-scope preferred-width style reference"
    );
    Ok(())
}

fn number(node: &Xml, name: &str) -> Result<u64> {
    let value = node.attr(name)?.context("Missing preferred width")?;
    ensure!(
        !value.is_empty() && value.len() <= 5 && value.bytes().all(|b| b.is_ascii_digit()),
        "Invalid preferred width"
    );
    let value: u64 = value.parse()?;
    ensure!((1..=31680).contains(&value), "Preferred width out of range");
    Ok(value)
}
fn fixed(table: &Xml) -> Result<bool> {
    let layout = table
        .one("tblPr")?
        .map(|p| direct(p, "tblLayout"))
        .transpose()?
        .flatten();
    Ok(layout.map(|p| p.attr("type")).transpose()?.flatten() == Some("fixed"))
}
fn potential(table: &Xml) -> Result<bool> {
    let Some(pr) = table.one("tblPr")? else {
        return Ok(false);
    };
    let Some(width) = direct(pr, "tblW")? else {
        return Ok(false);
    };
    let layout = direct(pr, "tblLayout")?;
    Ok(width.attr("type")? == Some("auto")
        && width.attr("w")? == Some("0")
        && layout
            .map(|l| l.attr("type"))
            .transpose()?
            .flatten()
            .is_none_or(|v| v == "autofit"))
}
fn geometry(table: &Xml) -> Result<Vec<u64>> {
    ensure!(potential(table)?, "Not automatic zero width");
    let pr = direct(table, "tblPr")?.context("Missing table properties")?;
    pr.schema(&[], false)?;
    let width = direct(pr, "tblW")?.context("Missing preferred automatic width")?;
    width.schema(&["type", "w"], true)?;
    if let Some(layout) = direct(pr, "tblLayout")? {
        layout.schema(&["type"], true)?;
        ensure!(
            layout.attr("type")? == Some("autofit"),
            "Invalid automatic layout"
        );
    }
    ensure!(
        table.all("tblW").len() == 1
            && table.all("tblLayout").len() == usize::from(pr.one("tblLayout")?.is_some()),
        "Out-of-scope preferred layout"
    );
    // The source subset does not adopt table/cell custom-margin or cell-indent
    // dependencies, even if an older normalization would discard them.
    for tag in ["ind", "tcMar", "noWrap", "tcFitText", "fitText"] {
        ensure!(
            table.all(tag).is_empty(),
            "Preferred source geometry dependency: {tag}"
        );
    }
    default_table_geometry(table)?;
    if let Some(align) = direct(pr, "jc")? {
        align.schema(&["val"], true)?;
        ensure!(
            matches!(align.attr("val")?, Some("left" | "start")),
            "Unsupported preferred alignment"
        );
    }
    if let Some(indent) = direct(pr, "tblInd")? {
        indent.schema(&["type", "w"], true)?;
        ensure!(
            indent.attr("type")? == Some("dxa") && indent.attr("w")? == Some("0"),
            "Unsupported preferred indentation"
        );
    }
    let grid = direct(table, "tblGrid")?.context("Missing preferred grid")?;
    grid.schema(&[], false)?;
    let mut widths = Vec::new();
    let mut total = 0u64;
    for col in &grid.children {
        ensure!(col.tag == "gridCol", "Unsupported preferred grid content");
        col.schema(&["w"], true)?;
        let value = number(col, "w")?;
        total += value;
        ensure!(total <= 31680, "Preferred total exceeds limit");
        widths.push(value);
    }
    ensure!(
        !widths.is_empty() && widths.len() <= 50,
        "Preferred column count exceeds limit"
    );
    let rows: Vec<_> = table.children.iter().filter(|n| n.tag == "tr").collect();
    let mut tcw = 0usize;
    for row in &rows {
        let cells: Vec<_> = row.children.iter().filter(|n| n.tag == "tc").collect();
        ensure!(
            cells.len() == widths.len(),
            "Preferred grid does not match cells"
        );
        for (cell, expected) in cells.iter().zip(&widths) {
            let pr = direct(cell, "tcPr")?.context("Missing preferred cell properties")?;
            pr.schema(&[], false)?;
            let width = direct(pr, "tcW")?.context("Missing preferred cell width")?;
            width.schema(&["type", "w"], true)?;
            ensure!(
                width.attr("type")? == Some("dxa") && number(width, "w")? == *expected,
                "Preferred cell width differs from grid"
            );
            tcw += 1;
        }
    }
    ensure!(
        table.all("tcW").len() == tcw
            && table.all("tblGrid").len() == 1
            && table.all("gridCol").len() == widths.len(),
        "Wrapped preferred geometry"
    );
    Ok(widths)
}
fn analyze_trees(tree: &Xml, styles_tree: &Xml) -> Result<Audit> {
    let document = root(tree, "document")?;
    let body = document.one("body")?.context("Missing preferred body")?;
    ensure!(document.all("body").len() == 1, "Ambiguous preferred body");
    let tables = tree.all("tbl");
    let styles_root = root(styles_tree, "styles")?;
    defaults(styles_root)?;
    let mut styles = Styles::new(styles_root);
    let mut audit = Audit::default();
    let mut text_bytes = 0usize;
    let mut work = 0usize;
    let mut markers = Markers::default();
    for table in tables {
        work = work.saturating_add(table.size);
        ensure!(work <= MAX_WORK, "Preferred proof work exceeds limits");
        audit.automatic.push(!fixed(table)?);
        if !potential(table)? {
            audit.tables.push(None);
            continue;
        }
        // Ordinary geometry ineligibility is per table, before any shared
        // style/bookmark proof state. A normalized sibling must not discard a
        // complete eligible preference proof.
        let Ok(widths) = geometry(table) else {
            audit.tables.push(None);
            continue;
        };
        let mut candidate_markers = markers.clone();
        let proof = (|| -> Result<Table> {
            safe_properties(table)?;
            dependencies(table, &mut styles)?;
            let mut proof = structure(table, body, &mut text_bytes, &mut candidate_markers)?;
            let flags = headers(table)?;
            ensure!(
                proof.rows.len() == flags.len(),
                "Preferred header count changed"
            );
            for (row, header) in proof.rows.iter_mut().zip(flags) {
                row.header = header;
            }
            proof.widths = widths;
            Ok(proof)
        })();
        match proof {
            Ok(proof) => {
                markers = candidate_markers;
                audit.tables.push(Some(proof));
            }
            Err(_) => {
                // Shared identity/style/bookmark corruption is fail-closed
                // for the new group; the old hard gates and model still win.
                return Ok(failed());
            }
        }
    }
    Ok(audit)
}
pub(crate) fn analyze(bytes: &[u8]) -> Audit {
    fn inner(bytes: &[u8]) -> Result<Audit> {
        let mut package = OpcReader::new(Cursor::new(bytes))?;
        let main = package.main_document_part()?;
        let tree = xml(&package.read_part(&main)?)?;
        let tables = tree.all("tbl");
        if tables.is_empty()
            || !tables
                .iter()
                .map(|t| potential(t))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .any(|v| v)
        {
            return Ok(Audit::default());
        }
        crate::table_row_pagination::package_identity(&mut package, bytes, &main)?;
        let relationships = package.read_rels_for(&main)?;
        let found = relationships.get_by_type(rel_types::STYLES);
        let data = if let Some(style) = found.first() {
            ensure!(
                style.target_mode == TargetMode::Internal,
                "External preferred styles"
            );
            package.read_part(&main.resolve_relative(&style.target)?)?
        } else {
            br#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#
                .to_vec()
        };
        analyze_trees(&tree, &xml(&data)?)
    }
    inner(bytes).unwrap_or_else(|_| failed())
}
fn ir_tables(ir: &DocumentIR) -> Vec<&office_oxide::ir::Table> {
    fn nested<'a>(list: &'a office_oxide::ir::List, out: &mut Vec<&'a office_oxide::ir::Table>) {
        for item in &list.items {
            visit(&item.content, out);
            if let Some(list) = &item.nested {
                nested(list, out);
            }
        }
    }
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
                    nested(l, out);
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
        "Preferred-width table identity changed"
    );
    for (proof, table) in audit.tables.iter().zip(tables) {
        let Some(proof) = proof else {
            continue;
        };
        ensure!(
            proof.rows.len() == table.rows.len(),
            "Preferred-width row identity changed"
        );
        for (proof, row) in proof.rows.iter().zip(&table.rows) {
            ensure!(
                proof.header == row.is_header && proof.header == row.repeat_as_header,
                "Preferred-width header identity changed"
            );
            ensure!(
                proof.paragraphs.len() == row.cells.len(),
                "Preferred-width cell identity changed"
            );
            for (proof, cell) in proof.paragraphs.iter().zip(&row.cells) {
                ensure!(
                    cell.col_span == 1 && cell.row_span == 1,
                    "Preferred-width background or spans changed"
                );
                let actual = cell
                    .content
                    .iter()
                    .map(|element| match element {
                        Element::Paragraph(p) => Ok(inline_to_text(&p.content)),
                        Element::Heading(h) => Ok(inline_to_text(&h.content)),
                        _ => anyhow::bail!("Preferred-width cell blocks changed"),
                    })
                    .collect::<Result<Vec<_>>>()?;
                ensure!(
                    actual == *proof,
                    "Preferred-width paragraph identity changed"
                );
            }
        }
    }
    Ok(())
}
fn model_text(paragraph: &Node) -> Result<String> {
    ensure!(
        matches!(paragraph.kind.as_str(), "paragraph" | "heading"),
        "Preferred-width model paragraph kind changed"
    );
    paragraph
        .content
        .iter()
        .map(|node| {
            ensure!(
                node.content.is_empty(),
                "Preferred-width nested inline content"
            );
            match node.kind.as_str() {
                "text" => node
                    .text
                    .as_deref()
                    .context("Missing preferred-width model text"),
                "hardBreak" => Ok("\n"),
                _ => anyhow::bail!("Preferred-width model inline kind changed"),
            }
        })
        .collect()
}

fn attach_checked(audit: &Audit, doc: &mut Node) -> Result<(usize, bool)> {
    let page = crate::document::page_geometry(doc)?;
    let budget = page
        .as_ref()
        .map(|p| p.width_twips - p.margin_left_twips - p.margin_right_twips);
    let mut attached = vec![false; audit.tables.len()];
    fn visit(
        node: &mut Node,
        direct: bool,
        audit: &Audit,
        index: &mut usize,
        budget: Option<u64>,
        attached: &mut [bool],
    ) -> Result<()> {
        if node.kind == "table" {
            let ordinal = *index;
            let proof = audit
                .tables
                .get(ordinal)
                .context("Preferred model inserted a table")?;
            *index += 1;
            if let Some(proof) = proof {
                ensure!(
                    direct && node.content.len() == proof.rows.len(),
                    "Preferred table ownership changed"
                );
                for (row, proof) in node.content.iter().zip(&proof.rows) {
                    ensure!(
                        row.kind == "tableRow" && row.content.len() == proof.paragraphs.len(),
                        "Preferred row identity changed"
                    );
                    for (cell, paragraphs) in row.content.iter().zip(&proof.paragraphs) {
                        ensure!(
                            cell.kind
                                == if proof.header {
                                    "tableHeader"
                                } else {
                                    "tableCell"
                                },
                            "Preferred header identity changed"
                        );
                        ensure!(
                            cell.content.len() == paragraphs.len(),
                            "Preferred paragraph count changed"
                        );
                        for (actual, expected) in cell.content.iter().zip(paragraphs) {
                            ensure!(
                                model_text(actual)? == *expected,
                                "Preferred paragraph text/order changed"
                            );
                        }
                    }
                }
                ensure!(
                    !node.attrs.contains_key(ATTRIBUTE),
                    "Unexpected preexisting preferred metadata"
                );
                // New-mode coexistence failures retain the historical model.
                // Existing preserved row/color/etc. attrs are never removed.
                let mut table = node.clone();
                table
                    .attrs
                    .insert(ATTRIBUTE.into(), serde_json::json!(proof.widths));
                if budget.is_some_and(|width| proof.widths.iter().sum::<u64>() <= width)
                    && crate::document::table_auto_preferred_widths(&table).is_ok()
                {
                    *node = table;
                    attached[ordinal] = true;
                }
            }
        }
        let children_direct = node.kind == "doc";
        for child in &mut node.content {
            visit(child, children_direct, audit, index, budget, attached)?;
        }
        Ok(())
    }
    let mut index = 0;
    visit(doc, false, audit, &mut index, budget, &mut attached)?;
    ensure!(
        index == audit.tables.len(),
        "Preferred model dropped a table"
    );
    crate::document::validate(doc)?;
    let count = attached.iter().filter(|v| **v).count();
    let all_auto = audit
        .automatic
        .iter()
        .zip(&attached)
        .all(|(auto, kept)| !auto || *kept);
    Ok((count, all_auto))
}
pub(crate) fn attach(
    audit: &Audit,
    ir_matches: bool,
    doc: &mut Node,
    risks: &mut BTreeSet<String>,
) {
    if audit.warning.is_some() || audit.tables.is_empty() {
        return;
    }
    let mut candidate = doc.clone();
    let result = if ir_matches {
        attach_checked(audit, &mut candidate)
    } else {
        Err(anyhow::anyhow!("Preferred IR identity differs"))
    };
    if let Ok((count, all_auto)) = result {
        if count == 0 {
            return;
        }
        *doc = candidate;
        risks.insert(PRESERVED.into());
        if all_auto {
            risks.remove(crate::table_widths::AUTOFIT_WIDTHS_RISK);
        }
        // This legacy page warning can have been inserted before attachment.
        if let Ok(Some(page)) = crate::document::page_geometry(doc) {
            let remaining = doc
                .content
                .iter()
                .filter(|n| n.kind == "table")
                .any(|table| {
                    !table.attrs.contains_key("columnWidthsTwips")
                        && !table.attrs.contains_key(ATTRIBUTE)
                        && !table.attrs.contains_key("tableAlign")
                        && !table.attrs.contains_key("tableIndentTwips")
                        && crate::document::table_layout_width(table, Some(&page))
                            .is_ok_and(|w| w < 9000)
                });
            if !remaining {
                risks.remove(crate::page_geometry::PAGE_AUTO_WIDTH_RISK);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn source(properties: &str, cell_width: u64, text: &str) -> Vec<u8> {
        format!(r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tblPr>{properties}</w:tblPr><w:tblGrid><w:gridCol w:w="1870"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:type="dxa" w:w="{cell_width}"/></w:tcPr><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#).into_bytes()
    }
    fn audit(data: &[u8]) -> Audit {
        analyze_trees(&xml(data).unwrap(), &xml(br#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#).unwrap()).unwrap()
    }
    #[test]
    fn strict_auto_zero_retains_preferences_and_text() {
        for layout in ["", r#"<w:tblLayout w:type="autofit"/>"#] {
            let result = audit(&source(
                &format!(r#"<w:tblW w:type="auto" w:w="0"/>{layout}"#),
                1870,
                "identity",
            ));
            let table = result.tables[0].as_ref().unwrap();
            assert_eq!(table.widths, [1870]);
            assert_eq!(table.rows[0].paragraphs, vec![vec!["identity".to_owned()]]);
        }
    }
    #[test]
    fn dxa_is_not_promoted_and_mismatched_auto_is_unproved() {
        let result = audit(&source(
            r#"<w:tblW w:type="dxa" w:w="1870"/>"#,
            1870,
            "text",
        ));
        assert!(result.tables[0].is_none());
        let result = audit(&source(r#"<w:tblW w:type="auto" w:w="0"/>"#, 1800, "text"));
        assert!(result.warning.is_none());
        assert!(result.tables[0].is_none());
    }
    #[test]
    fn mixed_inconsistent_auto_retains_eligible_sibling() {
        let good =
            String::from_utf8(source(r#"<w:tblW w:type="auto" w:w="0"/>"#, 1870, "good")).unwrap();
        let bad =
            String::from_utf8(source(r#"<w:tblW w:type="auto" w:w="0"/>"#, 1800, "bad")).unwrap();
        let bad_table = bad
            .split_once("<w:body>")
            .unwrap()
            .1
            .split_once("</w:body>")
            .unwrap()
            .0;
        let mixed = good.replace("</w:body>", &format!("{bad_table}</w:body>"));
        let result = audit(mixed.as_bytes());
        assert!(result.warning.is_none());
        assert_eq!(result.tables.len(), 2);
        assert!(result.tables[0].is_some());
        assert!(result.tables[1].is_none());
        assert_eq!(result.automatic, [true, true]);
    }
    #[test]
    fn genuine_source_has_separate_preferred_proof() {
        let result = analyze(include_bytes!(
            "../tests/fixtures/docx-cell-no-fill/Bug60337.docx"
        ));
        assert!(result.warning.is_none());
        assert_eq!(result.tables.len(), 1);
        assert_eq!(result.tables[0].as_ref().unwrap().widths, [1870; 5]);
    }
    #[test]
    fn intrinsic_constraints_are_not_silently_promoted() {
        for tag in ["noWrap", "tcFitText", "fitText"] {
            let data =
                String::from_utf8(source(r#"<w:tblW w:type="auto" w:w="0"/>"#, 1870, "text"))
                    .unwrap();
            let data = data.replace("</w:tcPr>", &format!("<w:{tag}/></w:tcPr>"));
            let result = audit(data.as_bytes());
            assert!(result.tables[0].is_none());
        }
    }
    #[test]
    fn hidden_geometry_and_foreign_width_attributes_fail_closed() {
        for properties in [
            r#"<w:tblW w:type="auto" w:w="0"/><w:extension><w:tblW w:type="auto" w:w="0"/></w:extension>"#,
            r#"<w:tblW xmlns:x="urn:foreign" w:type="auto" x:w="0"/>"#,
        ] {
            let tree = xml(&source(properties, 1870, "text")).unwrap();
            let styles = xml(br#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#).unwrap();
            assert!(
                analyze_trees(&tree, &styles)
                    .map_or(true, |a| a.tables.iter().all(Option::is_none))
            );
        }
    }
}
