//! Direct solid cell fill and independently proved automatic foreground.
//! Unproved legacy input keeps its previous normalization with an explicit
//! warning. Neither a lossy IR background nor a missing text mark proves auto.
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

pub(crate) const BINDING_NORMALIZED: &str = "Table cell color identity could not be bound to converted content; original color normalization retained";
pub(crate) const NORMALIZED: &str = "Some table cell fills are unproved and remain normalized";
pub(crate) const AUTO_NORMALIZED: &str = "Some filled-cell automatic text colors are unproved; profile or explicit text colors remain in use";
pub(crate) const THEME_FIXED: &str =
    "Direct table cell theme fills resolved to fixed RGB; dynamic theme linkage is not preserved";
pub(crate) const AUTO_PRESERVED: &str = "Filled-cell automatic text color is preserved locally; consumers choose contrast and editor preview is an approximation";
#[derive(Default)]
pub(crate) struct Audit {
    tables: Vec<Option<Table>>,
    pub warnings: BTreeSet<String>,
}
struct Table {
    rows: Vec<Vec<Cell>>,
}
type RunColors = Vec<(String, Option<String>)>;
struct Cell {
    fill: Option<String>,
    paragraphs: Vec<String>,
    colors: Option<Vec<RunColors>>,
}

fn rgb(value: &str) -> Result<String> {
    ensure!(
        value.len() == 6 && value.bytes().all(|c| c.is_ascii_hexdigit()),
        "Expected six RGB digits"
    );
    Ok(value.to_ascii_uppercase())
}
struct Styles<'a> {
    root: &'a Xml,
    index: BTreeMap<String, Vec<&'a Xml>>,
    work: usize,
    defaults: BTreeMap<String, Option<String>>,
    foreground: Option<std::result::Result<Option<String>, String>>,
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
            foreground: None,
            cache: BTreeMap::new(),
        }
    }
    fn foreground(&mut self) -> Result<Option<String>> {
        if self.foreground.is_none() {
            self.foreground = Some(default_color(self.root).map_err(|e| e.to_string()));
        }
        self.foreground
            .as_ref()
            .expect("resolved")
            .clone()
            .map_err(anyhow::Error::msg)
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
// Only a direct, untransformed RGB or auto declaration is a foreground proof.
fn color(properties: Option<&Xml>) -> Result<Option<Option<String>>> {
    let Some(properties) = properties else {
        return Ok(None);
    };
    properties.identity()?;
    ensure!(
        properties.all("shd").is_empty() && properties.all("highlight").is_empty(),
        "Foreground with shading or highlight is unproved"
    );
    let value = properties.one("color")?;
    ensure!(
        properties.all("color").len() == usize::from(value.is_some()),
        "Wrapped foreground color"
    );
    value
        .map(|value| {
            value.schema(&["val"], true)?;
            let value = value.attr("val")?.context("Missing foreground color")?;
            Ok(if value == "auto" {
                None
            } else {
                Some(rgb(value)?)
            })
        })
        .transpose()
}
fn style_color(style: &Xml) -> Result<Option<Option<String>>> {
    ensure!(
        style.all("shd").is_empty() && style.all("highlight").is_empty(),
        "Styled foreground background is unproved"
    );
    let pr = style.one("rPr")?;
    ensure!(
        style.all("color").len() == pr.map(|p| p.all("color").len()).unwrap_or(0),
        "Conditional or wrapped style foreground"
    );
    color(pr)
}
fn default_color(styles: &Xml) -> Result<Option<String>> {
    let defaults = styles.one("docDefaults")?;
    ensure!(
        defaults.is_none_or(|d| d.all("shd").is_empty() && d.all("highlight").is_empty()),
        "Default foreground background is unproved"
    );
    let properties = defaults
        .map(|d| d.one("rPrDefault"))
        .transpose()?
        .flatten()
        .map(|r| r.one("rPr"))
        .transpose()?
        .flatten();
    ensure!(
        styles
            .children
            .iter()
            .filter(|n| n.tag != "style")
            .map(|n| n.all("color").len())
            .sum::<usize>()
            == properties.map(|p| p.all("color").len()).unwrap_or(0),
        "Unscoped default foreground color"
    );
    Ok(color(properties)?.flatten())
}
fn paragraph_text(paragraph: &Xml) -> Result<String> {
    fn visit(n: &Xml, value: &mut String) -> Result<()> {
        match n.tag.as_str() {
            "pPr" | "rPr" => return Ok(()),
            "p" | "r" | "hyperlink" => n.identity()?,
            "t" => {
                n.identity()?;
                ensure!(n.children.is_empty(), "Nested table text");
                value.push_str(&n.text);
                return Ok(());
            }
            "tab" => {
                n.schema(&[], true)?;
                value.push('\t');
                return Ok(());
            }
            "br" => {
                n.schema(&["type"], true)?;
                ensure!(
                    n.attr("type")?.is_none_or(|v| v == "textWrapping"),
                    "Non-text cell break"
                );
                value.push('\n');
                return Ok(());
            }
            "cr" => {
                n.schema(&[], true)?;
                value.push('\n');
                return Ok(());
            }
            "bookmarkStart" | "bookmarkEnd" | "proofErr" | "lastRenderedPageBreak" => {
                n.identity()?;
                ensure!(n.children.is_empty(), "Wrapped text marker");
                return Ok(());
            }
            _ => anyhow::bail!("Complex table cell content"),
        }
        ensure!(n.text.trim().is_empty(), "Uncontained cell text");
        for c in &n.children {
            visit(c, value)?;
        }
        Ok(())
    }
    let mut value = String::new();
    visit(paragraph, &mut value)?;
    ensure!(
        value.len() <= 2 * 1024 * 1024,
        "Cell color text identity exceeds limits"
    );
    Ok(value)
}
fn paragraph_colors(
    paragraph: &Xml,
    styles: &mut Styles<'_>,
    table_styles: &[&Xml],
) -> Result<RunColors> {
    ensure!(
        table_styles.iter().all(|s| s.all("color").is_empty()
            && s.all("shd").is_empty()
            && s.all("highlight").is_empty()),
        "Table-style foreground is unproved"
    );
    ensure!(
        paragraph.all("shd").is_empty() && paragraph.all("highlight").is_empty(),
        "Direct foreground background is unproved"
    );
    let mut inherited = styles.foreground()?;
    for style in styles.chain("paragraph", reference(paragraph, "pPr", "pStyle")?)? {
        if let Some(value) = style_color(style)? {
            inherited = value;
        }
    }
    // Default character styles with color are not silently assigned a cascade
    // order. Explicit character style chains are resolved for their own runs.
    ensure!(
        styles
            .chain("character", None)?
            .iter()
            .all(|s| s.all("color").is_empty()),
        "Default character foreground is unproved"
    );
    ensure!(
        paragraph.all("numPr").is_empty(),
        "Numbered cell foreground is unproved"
    );
    let mut result = Vec::new();
    for run in &paragraph.children {
        if matches!(
            run.tag.as_str(),
            "pPr" | "bookmarkStart" | "bookmarkEnd" | "proofErr"
        ) {
            continue;
        }
        ensure!(
            run.tag == "r",
            "Only direct plain runs support cell automatic color"
        );
        let mut effective = inherited.clone();
        if let Some(reference) = reference(run, "rPr", "rStyle")? {
            for style in styles.chain("character", Some(reference))? {
                if let Some(value) = style_color(style)? {
                    effective = value;
                }
            }
        }
        if let Some(value) = color(run.one("rPr")?)? {
            effective = value;
        }
        let text = paragraph_text(run)?;
        if !text.is_empty() {
            result.push((text, effective));
        }
    }
    Ok(result)
}

// Namespace-aware theme resolution uses the actual related theme, never the
// conventional file name or the fallback alone. Initially only accent1–6 and
// one untransformed srgbClr are supported; no theme mapping is guessed.
fn theme_rgb(data: &[u8], slot: &str) -> Result<String> {
    use quick_xml::{NsReader, events::Event, name::ResolveResult};
    ensure!(
        matches!(
            slot,
            "accent1" | "accent2" | "accent3" | "accent4" | "accent5" | "accent6"
        ),
        "Unsupported fill theme slot"
    );
    let mut reader = NsReader::from_reader(data);
    let mut path = Vec::<String>::new();
    let mut colors = Vec::new();
    let mut slots = 0;
    let mut roots = 0;
    let mut schemes = 0;
    let mut work = 0;
    loop {
        let (ns, event) = reader.read_resolved_event()?;
        let drawing = matches!(ns,ResolveResult::Bound(n) if matches!(n.as_ref(),"http://schemas.openxmlformats.org/drawingml/2006/main"|"http://purl.oclc.org/ooxml/drawingml/main"));
        let start = matches!(event, Event::Start(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                work += 1;
                ensure!(
                    work <= 100_000 && path.len() < 128,
                    "Theme metadata exceeds limits"
                );
                let name = e.local_name().as_ref().to_owned();
                if path.is_empty() {
                    ensure!(name == "theme" && drawing, "Invalid actual theme root");
                    roots += 1;
                }
                if name == "themeElements" {
                    ensure!(drawing && path == ["theme"], "Invalid theme elements path");
                }
                if name == "clrScheme" {
                    schemes += 1;
                    ensure!(
                        drawing && path == ["theme", "themeElements"],
                        "Ambiguous theme color scheme"
                    );
                }
                if name == slot {
                    ensure!(
                        drawing && path == ["theme", "themeElements", "clrScheme"],
                        "Ambiguous theme slot path"
                    );
                    ensure!(
                        e.attributes().count() == 0,
                        "Unsupported theme slot attributes"
                    );
                    slots += 1;
                }
                if path.iter().any(|p| p == slot) {
                    ensure!(
                        drawing
                            && path == ["theme", "themeElements", "clrScheme", slot]
                            && name == "srgbClr",
                        "Transformed or unsupported theme fill"
                    );
                    let mut value = None;
                    for attr in e.attributes() {
                        let attr = attr?;
                        ensure!(attr.key.as_ref() == "val", "Unknown theme RGB attribute");
                        ensure!(value.is_none(), "Duplicate theme RGB");
                        value = Some(rgb(
                            &attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)?
                        )?);
                    }
                    colors.push(value.context("Missing theme RGB")?);
                }
                if start {
                    path.push(name);
                }
            }
            Event::End(_) => {
                path.pop();
            }
            Event::Text(e) => {
                if path.iter().any(|p| p == slot) {
                    ensure!(
                        e.xml_content(quick_xml::XmlVersion::Implicit1_0)
                            .trim()
                            .is_empty(),
                        "Theme fill text"
                    );
                }
            }
            Event::DocType(_) => anyhow::bail!("Theme DTD is unsupported"),
            Event::Eof => break,
            _ => {}
        }
    }
    ensure!(
        roots == 1 && schemes == 1 && slots == 1 && colors.len() == 1,
        "Missing or ambiguous actual theme slot"
    );
    Ok(colors.remove(0))
}
fn fill(cell: &Xml, theme: Option<&BTreeMap<String, String>>) -> Result<Option<(String, bool)>> {
    let properties = cell.one("tcPr")?;
    let shd = properties.map(|p| p.one("shd")).transpose()?.flatten();
    ensure!(
        cell.all("shd").len() == usize::from(shd.is_some()),
        "Non-direct cell shading"
    );
    let Some(shd) = shd else { return Ok(None) };
    shd.schema(&["val", "color", "fill", "themeFill"], true)?;
    ensure!(
        shd.attr("val")? == Some("clear"),
        "Only direct clear solid cell fill is supported"
    );
    ensure!(
        shd.attr("color")?
            .is_none_or(|v| v == "auto" || rgb(v).is_ok()),
        "Invalid unused shading foreground"
    );
    let fill = rgb(shd.attr("fill")?.context("Missing cell RGB fill")?)?;
    let themed = if let Some(slot) = shd.attr("themeFill")? {
        ensure!(
            theme
                .context("Missing actual theme relationship")?
                .get(slot)
                == Some(&fill),
            "Theme fill differs from fallback RGB"
        );
        true
    } else {
        false
    };
    Ok(Some((fill, themed)))
}
fn table(
    table: &Xml,
    body: &Xml,
    styles: &mut Styles<'_>,
    theme: Option<&BTreeMap<String, String>>,
    warnings: &mut BTreeSet<String>,
) -> Result<Table> {
    ensure!(
        body.children.iter().any(|n| std::ptr::eq(n, table)),
        "Cell colors require a direct body table"
    );
    table.identity()?;
    ensure!(
        table
            .children
            .iter()
            .all(|n| matches!(n.tag.as_str(), "tblPr" | "tblGrid" | "tr")),
        "Wrapped color table rows"
    );
    for forbidden in [
        "tbl",
        "gridSpan",
        "hMerge",
        "vMerge",
        "tblPrChange",
        "tcPrChange",
        "trPrChange",
        "ins",
        "del",
        "moveFrom",
        "moveTo",
    ] {
        ensure!(
            table.all(forbidden).is_empty(),
            "Complex table color structure: {forbidden}"
        );
    }
    let chain = styles.chain("table", reference(table, "tblPr", "tblStyle")?)?;
    ensure!(
        chain.iter().all(|s| s.all("shd").is_empty()),
        "Inherited or conditional table shading is unproved"
    );
    let direct_shading = table
        .children
        .iter()
        .filter(|n| n.tag == "tr")
        .flat_map(|r| r.children.iter().filter(|n| n.tag == "tc"))
        .map(|c| {
            c.one("tcPr").map(|pr| {
                pr.map(|p| p.children.iter().filter(|n| n.tag == "shd").count())
                    .unwrap_or(0)
            })
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .sum::<usize>();
    ensure!(
        table.all("shd").len() == direct_shading,
        "Non-direct table or row shading"
    );
    let mut rows = Vec::new();
    let mut columns = None;
    for row in table.children.iter().filter(|n| n.tag == "tr") {
        row.identity()?;
        ensure!(
            row.children
                .iter()
                .all(|n| matches!(n.tag.as_str(), "trPr" | "tc")),
            "Wrapped color table cells"
        );
        let mut cells = Vec::new();
        for cell in row.children.iter().filter(|n| n.tag == "tc") {
            cell.identity()?;
            ensure!(
                cell.children
                    .iter()
                    .all(|n| matches!(n.tag.as_str(), "tcPr" | "p")),
                "Complex colored cell blocks"
            );
            let paragraphs = cell
                .children
                .iter()
                .filter(|n| n.tag == "p")
                .map(paragraph_text)
                .collect::<Result<Vec<_>>>()?;
            ensure!(!paragraphs.is_empty(), "Missing colored cell paragraph");
            let fill = match fill(cell, theme) {
                Ok(Some((value, themed))) => {
                    if themed {
                        warnings.insert(THEME_FIXED.into());
                    }
                    Some(value)
                }
                Ok(None) => None,
                Err(_) => {
                    warnings.insert(NORMALIZED.into());
                    None
                }
            };
            let colors = if fill.is_some() {
                match cell
                    .children
                    .iter()
                    .filter(|n| n.tag == "p")
                    .map(|p| paragraph_colors(p, styles, &chain))
                    .collect::<Result<Vec<_>>>()
                {
                    Ok(values) => Some(values),
                    Err(_) => {
                        warnings.insert(AUTO_NORMALIZED.into());
                        None
                    }
                }
            } else {
                None
            };
            cells.push(Cell {
                fill,
                paragraphs,
                colors,
            });
        }
        ensure!(
            !cells.is_empty()
                && columns
                    .replace(cells.len())
                    .is_none_or(|n| n == cells.len()),
            "Ragged color table"
        );
        rows.push(cells);
    }
    ensure!(!rows.is_empty(), "Empty color table");
    Ok(Table { rows })
}
pub(crate) fn analyze(bytes: &[u8]) -> Audit {
    fn inner(bytes: &[u8]) -> Result<Audit> {
        let mut package = OpcReader::new(Cursor::new(bytes))?;
        let main = package.main_document_part()?;
        ensure!(
            main.as_str() == "/word/document.xml",
            "Unsupported color main part"
        );
        let data = package.read_part(&main)?;
        let tree = xml(&data)?;
        let tables = tree.all("tbl");
        if !tables.iter().any(|t| !t.all("shd").is_empty()) {
            return Ok(Audit::default());
        }
        let rels = package.read_rels_for(&main)?;
        ensure!(
            rels.duplicate_ids().is_empty(),
            "Ambiguous color relationships"
        );
        let mut read = |kind| -> Result<Option<Vec<u8>>> {
            let found = rels.get_by_type(kind);
            ensure!(found.len() <= 1, "Ambiguous actual color part");
            found
                .first()
                .map(|r| {
                    ensure!(r.target_mode == TargetMode::Internal, "External color part");
                    Ok(package.read_part(&main.resolve_relative(&r.target)?)?)
                })
                .transpose()
        };
        let style_data=read(rel_types::STYLES)?.unwrap_or_else(||br#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#.to_vec());
        let theme = read(rel_types::THEME)?.map(|bytes| {
            (1..=6)
                .filter_map(|n| {
                    let slot = format!("accent{n}");
                    theme_rgb(&bytes, &slot).ok().map(|rgb| (slot, rgb))
                })
                .collect::<BTreeMap<_, _>>()
        });
        let style_tree = xml(&style_data)?;
        let styles_root = root(&style_tree, "styles")?;
        let mut styles = Styles::new(styles_root);
        let body = root(&tree, "document")?
            .one("body")?
            .context("Missing body")?;
        let mut audit = Audit::default();
        let mut work = 0usize;
        for source in tables {
            if source.all("shd").is_empty() {
                audit.tables.push(None);
                continue;
            }
            work = work.saturating_add(source.size);
            ensure!(work <= 2_000_000, "Cell color work exceeds limits");
            let mut table_warnings = BTreeSet::new();
            match table(
                source,
                body,
                &mut styles,
                theme.as_ref(),
                &mut table_warnings,
            ) {
                Ok(table) => {
                    audit.tables.push(Some(table));
                    audit.warnings.extend(table_warnings);
                }
                Err(_) => {
                    audit.warnings.insert(NORMALIZED.into());
                    audit.tables.push(None);
                }
            }
        }
        Ok(audit)
    }
    match inner(bytes) {
        Ok(audit) => audit,
        Err(_) => Audit {
            warnings: BTreeSet::from([NORMALIZED.into()]),
            ..Audit::default()
        },
    }
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
        "Cell color table identity changed"
    );
    for (proof, table) in audit.tables.iter().zip(tables) {
        let Some(proof) = proof else { continue };
        ensure!(
            proof.rows.len() == table.rows.len(),
            "Cell color row identity changed"
        );
        for (proof, row) in proof.rows.iter().zip(&table.rows) {
            ensure!(proof.len() == row.cells.len(), "Cell color shape changed");
            for (proof, cell) in proof.iter().zip(&row.cells) {
                ensure!(
                    cell.col_span <= 1 && cell.row_span <= 1,
                    "Cell color merge changed"
                );
                let text = cell
                    .content
                    .iter()
                    .map(|e| match e {
                        Element::Paragraph(p) => Ok(inline_to_text(&p.content)),
                        Element::Heading(h) => Ok(inline_to_text(&h.content)),
                        _ => anyhow::bail!("Cell color content changed"),
                    })
                    .collect::<Result<Vec<_>>>()?;
                ensure!(text == proof.paragraphs, "Cell color text identity changed");
                if let Some(fill) = &proof.fill {
                    ensure!(
                        cell.background_color
                            .map(|[r, g, b]| format!("{r:02X}{g:02X}{b:02X}"))
                            .as_ref()
                            == Some(fill),
                        "Cell fill differs from independently proved RGB"
                    );
                }
            }
        }
    }
    Ok(())
}
pub(crate) fn attach(audit: &Audit, doc: &mut Node, warnings: &mut BTreeSet<String>) -> Result<()> {
    if audit.tables.is_empty() {
        return Ok(());
    }
    fn colors(p: &Node) -> Result<RunColors> {
        ensure!(
            p.kind == "paragraph",
            "Only plain paragraphs support source automatic foreground proof"
        );
        p.content
            .iter()
            .map(|n| {
                ensure!(
                    n.kind == "text" || n.kind == "hardBreak",
                    "Complex converted cell text"
                );
                ensure!(
                    !n.marks.iter().any(|m| m.kind == "link"),
                    "Converted cell hyperlink"
                );
                let color = n
                    .marks
                    .iter()
                    .find(|m| m.kind == "textColor")
                    .map(|m| {
                        m.attrs["rgb"]
                            .as_str()
                            .context("Invalid converted color")
                            .map(str::to_owned)
                    })
                    .transpose()?;
                Ok((
                    if n.kind == "hardBreak" {
                        "\n".into()
                    } else {
                        n.text.clone().unwrap_or_default()
                    },
                    color,
                ))
            })
            .collect()
    }
    fn normalized(runs: &[(String, Option<String>)]) -> impl Iterator<Item = (char, Option<&str>)> {
        runs.iter()
            .flat_map(|(text, color)| text.chars().map(move |c| (c, color.as_deref())))
    }
    fn visit(
        node: &mut Node,
        proofs: &mut std::slice::Iter<'_, Option<Table>>,
        warnings: &mut BTreeSet<String>,
    ) -> Result<()> {
        if node.kind == "table"
            && let Some(proof) = proofs.next().context("Missing color table proof")?
        {
            ensure!(
                node.content.len() == proof.rows.len(),
                "Converted color rows changed"
            );
            for (row, proof) in node.content.iter_mut().zip(&proof.rows) {
                ensure!(
                    row.content.len() == proof.len(),
                    "Converted color cells changed"
                );
                for (cell, proof) in row.content.iter_mut().zip(proof) {
                    ensure!(
                        cell.content.len() == proof.paragraphs.len(),
                        "Converted color paragraphs changed"
                    );
                    for (p, text) in cell.content.iter().zip(&proof.paragraphs) {
                        let actual = p
                            .content
                            .iter()
                            .map(|n| {
                                if n.kind == "hardBreak" {
                                    "\n"
                                } else {
                                    n.text.as_deref().unwrap_or("")
                                }
                            })
                            .collect::<String>();
                        ensure!(actual == *text, "Converted color text changed");
                    }
                    if let Some(fill) = &proof.fill {
                        cell.attrs.insert("cellFillRgb".into(), fill.clone().into());
                    }
                    if let Some(expected) = &proof.colors {
                        let matches = cell.content.iter().zip(expected).all(|(p, runs)| {
                            colors(p).is_ok_and(|actual| normalized(&actual).eq(normalized(runs)))
                        });
                        if matches {
                            if expected
                                .iter()
                                .flatten()
                                .any(|(text, color)| !text.is_empty() && color.is_none())
                            {
                                cell.attrs.insert("cellTextColorMode".into(), "auto".into());
                                warnings.insert(AUTO_PRESERVED.into());
                            }
                        } else {
                            warnings.insert(AUTO_NORMALIZED.into());
                        }
                    }
                }
            }
        }
        for child in &mut node.content {
            visit(child, proofs, warnings)?;
        }
        Ok(())
    }
    // A late conversion mismatch must not leak earlier cells' new metadata.
    let mut candidate = doc.clone();
    let mut candidate_warnings = warnings.clone();
    let mut proofs = audit.tables.iter();
    visit(&mut candidate, &mut proofs, &mut candidate_warnings)?;
    ensure!(proofs.next().is_none(), "Source color table discarded");
    *doc = candidate;
    *warnings = candidate_warnings;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn theme(slot: &str) -> Vec<u8> {
        format!(r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements><a:clrScheme name="Test">{slot}</a:clrScheme></a:themeElements></a:theme>"#).into_bytes()
    }
    #[test]
    fn theme_requires_unique_actual_untransformed_rgb_slot() {
        assert_eq!(
            theme_rgb(
                &theme(r#"<a:accent2><a:srgbClr val="e97132"/></a:accent2>"#),
                "accent2"
            )
            .unwrap(),
            "E97132"
        );
        for slot in [
            r#"<a:accent2><a:srgbClr val="E97132"><a:tint val="50000"/></a:srgbClr></a:accent2>"#,
            r#"<a:accent2><a:sysClr val="windowText" lastClr="E97132"/></a:accent2>"#,
            r#"<a:accent2><a:srgbClr val="E97132"/></a:accent2><a:accent2><a:srgbClr val="E97132"/></a:accent2>"#,
            r#"<a:accent2><a:srgbClr val="E97132"/><a:srgbClr val="E97132"/></a:accent2>"#,
            r#"<a:accent2 custom="bad"><a:srgbClr val="E97132"/></a:accent2>"#,
        ] {
            assert!(theme_rgb(&theme(slot), "accent2").is_err(), "{slot}");
        }
        let bytes = theme(r#"<a:accent2><a:srgbClr val="E97132"/></a:accent2>"#);
        assert!(
            theme_rgb(
                &String::from_utf8(bytes.clone())
                    .unwrap()
                    .replace("<a:theme ", "<a:theme xmlns:x=\"urn:foreign\" ")
                    .replace("<a:themeElements>", "<x:themeElements>")
                    .replace("</a:themeElements>", "</x:themeElements>")
                    .into_bytes(),
                "accent2"
            )
            .is_err()
        );
        assert!(theme_rgb(&bytes, "bg1").is_err());
    }
    #[test]
    fn literal_fill_proof_rejects_pattern_and_theme_fallback_mismatch() {
        let theme = BTreeMap::from([("accent2".into(), "E97132".into())]);
        for (attrs, valid) in [
            (
                r#"w:val="clear" w:fill="E97132" w:themeFill="accent2""#,
                true,
            ),
            (
                r#"w:val="clear" w:fill="123456" w:themeFill="accent2""#,
                false,
            ),
            (r#"w:val="solid" w:fill="E97132""#, false),
            (
                r#"w:val="clear" w:fill="E97132" w:themeFillShade="80""#,
                false,
            ),
        ] {
            let data = format!(
                r#"<w:tc xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:tcPr><w:shd {attrs}/></w:tcPr><w:p/></w:tc>"#
            );
            let tree = xml(data.as_bytes()).unwrap();
            assert_eq!(
                fill(root(&tree, "tc").unwrap(), Some(&theme)).is_ok(),
                valid
            );
        }
    }
    #[test]
    fn late_attachment_mismatch_is_atomic_for_model_and_warnings() {
        let mut doc:Node=serde_json::from_value(json!({"type":"doc","content":[{"type":"table","content":[{"type":"tableRow","content":[{"type":"tableCell","content":[{"type":"paragraph","content":[{"type":"text","text":"First"}]}]},{"type":"tableCell","content":[{"type":"paragraph","content":[{"type":"text","text":"Second"}]}]}]}]}]})).unwrap();
        let proof = |text: &str| Cell {
            fill: Some("E97132".into()),
            paragraphs: vec![text.into()],
            colors: Some(vec![vec![(text.into(), None)]]),
        };
        let audit = Audit {
            tables: vec![Some(Table {
                rows: vec![vec![proof("First"), proof("Wrong")]],
            })],
            ..Audit::default()
        };
        let before = serde_json::to_value(&doc).unwrap();
        let mut warnings = BTreeSet::from(["old warning".to_owned()]);
        let old = warnings.clone();
        assert!(attach(&audit, &mut doc, &mut warnings).is_err());
        assert_eq!(serde_json::to_value(doc).unwrap(), before);
        assert_eq!(warnings, old);
    }
    #[test]
    fn same_text_with_different_foreground_keeps_fill_but_not_automatic() {
        let mut doc:Node=serde_json::from_value(json!({"type":"doc","content":[{"type":"table","content":[{"type":"tableRow","content":[{"type":"tableCell","content":[{"type":"paragraph","content":[{"type":"text","text":"Cell"}]}]}]}]}]})).unwrap();
        let audit = Audit {
            tables: vec![Some(Table {
                rows: vec![vec![Cell {
                    fill: Some("E97132".into()),
                    paragraphs: vec!["Cell".into()],
                    colors: Some(vec![vec![("Cell".into(), Some("000000".into()))]]),
                }]],
            })],
            ..Audit::default()
        };
        let mut warnings = BTreeSet::new();
        attach(&audit, &mut doc, &mut warnings).unwrap();
        let attrs = &doc.content[0].content[0].content[0].attrs;
        assert_eq!(attrs["cellFillRgb"], "E97132");
        assert!(!attrs.contains_key("cellTextColorMode"));
        assert!(warnings.contains(AUTO_NORMALIZED));
    }
}
