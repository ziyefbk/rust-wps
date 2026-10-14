//! Direct row cantSplit proof, independent of the converter's permissive toggle
//! defaults. All candidate tables form one atomic metadata group. Unproved
//! sources retain their old model with one diagnostic; old hard gates win.
use crate::{
    document::Node,
    paragraph_indentation::{Xml, root, xml},
    table_vertical::Styles,
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

pub(crate) const PRESERVED: &str =
    "Direct table row cantSplit preserved; automatic pagination remains consumer-dependent";
pub(crate) const PACKAGE: &str =
    "Table row cantSplit not preserved: package or active style identity is unproved";
pub(crate) const SOURCE: &str = "Table row cantSplit not preserved: source structure or inherited row pagination is unsupported";
pub(crate) const DECLARATION: &str = "Table row cantSplit not preserved: direct row declarations are invalid or outside the supported subset";
pub(crate) const BINDING: &str =
    "Table row cantSplit not preserved: source and converted table identities differ";
pub(crate) const RESOURCE: &str =
    "Table row cantSplit not preserved: proof resource limits exceeded";
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
    cant_split: Option<bool>,
    header: bool,
    paragraphs: Vec<Vec<String>>,
}
fn failed(warning: &'static str) -> Audit {
    Audit {
        warning: Some(warning),
        ..Audit::default()
    }
}
fn classify(error: anyhow::Error, fallback: &'static str) -> &'static str {
    let message = error.to_string();
    if message.contains("metadata exceeds limits")
        || message.contains("work exceeds limits")
        || message.contains("row proof resource limit")
    {
        RESOURCE
    } else {
        fallback
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
fn flag_locations(node: &Xml, path: &mut Vec<String>) -> Result<()> {
    if node.tag == "cantSplit" {
        ensure!(
            path.iter()
                .map(String::as_str)
                .eq(["document", "body", "tbl", "tr", "trPr"]),
            "Non-direct row cantSplit"
        );
    }
    if !node.tag.is_empty() {
        path.push(node.tag.clone());
    }
    for child in &node.children {
        flag_locations(child, path)?;
    }
    if !node.tag.is_empty() {
        path.pop();
    }
    Ok(())
}

#[derive(Default)]
struct Markers {
    active: BTreeMap<u32, String>,
    seen: BTreeSet<u32>,
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
                !name.is_empty() && name.len() <= 256 && self.seen.insert(id),
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
fn structure(table: &Xml, body: &Xml, text_bytes: &mut usize) -> Result<Table> {
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
        "trHeight",
        "vAlign",
        "gridSpan",
        "vMerge",
        "hMerge",
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
    table.one("tblGrid")?;
    let mut markers = Markers::default();
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
                .map(|p| paragraph(p, &mut markers, text_bytes))
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
            cant_split: None,
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
        markers.visited
            == table.all("bookmarkStart").len()
                + table.all("bookmarkEnd").len()
                + table.all("proofErr").len(),
        "Out-of-scope row marker"
    );
    Ok(Table { rows })
}
fn declarations(source: &Xml, table: &mut Table) -> Result<()> {
    let properties = direct(source, "tblPr")?;
    if let Some(properties) = properties {
        properties.schema(&[], false)?;
    }
    let mut headers = 0usize;
    for (source, row) in source
        .children
        .iter()
        .filter(|n| n.tag == "tr")
        .zip(&mut table.rows)
    {
        let properties = direct(source, "trPr")?;
        if let Some(properties) = properties {
            properties.schema(&[], false)?;
        }
        row.cant_split = flag(
            properties
                .map(|p| direct(p, "cantSplit"))
                .transpose()?
                .flatten(),
        )?;
        let header = properties
            .map(|p| direct(p, "tblHeader"))
            .transpose()?
            .flatten();
        headers += usize::from(header.is_some());
        row.header = flag(header)?.unwrap_or(false);
        ensure!(
            source.all("tblHeader").len()
                == usize::from(
                    properties
                        .map(|p| p.one("tblHeader"))
                        .transpose()?
                        .flatten()
                        .is_some()
                ),
            "Wrapped row header"
        );
    }
    ensure!(
        source.all("tblHeader").len() == headers,
        "Out-of-scope row header"
    );
    Ok(())
}
// Discovery is deliberately more permissive than proof: raw aliases, wrapped
// edges and duplicate IDs may establish a possible candidate, never metadata.
// Reverse reachability avoids resolving an unrelated broken style chain merely
// because an unused template contains cantSplit.
fn raw_values<'a>(node: &'a Xml, name: &str) -> Vec<&'a str> {
    node.attrs
        .iter()
        .filter(|(key, _)| key.rsplit(':').next() == Some(name))
        .map(|(_, (value, _))| value.as_str())
        .collect()
}
fn candidates(
    tables: &[&Xml],
    styles: &Xml,
) -> std::result::Result<(Vec<bool>, bool, usize), &'static str> {
    let all_flags = styles.all("cantSplit");
    let direct = tables
        .iter()
        .map(|t| !t.all("cantSplit").is_empty())
        .collect::<Vec<_>>();
    if all_flags.is_empty() {
        return Ok((direct, false, 0));
    }
    let nodes = styles.all("style");
    let mut ids = Vec::new();
    let mut reverse: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    let mut possible = BTreeSet::new();
    let mut pending = Vec::new();
    let mut scoped = BTreeSet::new();
    let mut work = 0usize;
    for (index, node) in nodes.iter().enumerate() {
        work = work.saturating_add(node.size);
        let names = raw_values(node, "styleId");
        work = work.saturating_add(names.len());
        ids.push(names);
        let flags = node.all("cantSplit");
        for flag in &flags {
            scoped.insert(*flag as *const Xml as usize);
        }
        let bases = node.all("basedOn");
        for base in bases {
            let targets = raw_values(base, "val");
            work = work.saturating_add(targets.len());
            for target in targets {
                reverse.entry(target).or_default().push(index);
            }
        }
        if !flags.is_empty() {
            possible.insert(index);
            pending.push(index);
        }
        if work > MAX_WORK {
            return Err(RESOURCE);
        }
    }
    let mut possible_ids = BTreeSet::new();
    while let Some(index) = pending.pop() {
        for id in &ids[index] {
            if !possible_ids.insert(*id) {
                continue;
            }
            if let Some(derived) = reverse.get(id) {
                work = work.saturating_add(derived.len());
                if work > MAX_WORK {
                    return Err(RESOURCE);
                }
                for derived in derived {
                    if possible.insert(*derived) {
                        pending.push(*derived);
                    }
                }
            }
        }
    }
    let unscoped = !tables.is_empty()
        && all_flags
            .iter()
            .any(|n| !scoped.contains(&(*n as *const Xml as usize)));
    let default_possible = nodes.iter().enumerate().any(|(index, node)| {
        possible.contains(&index)
            && raw_values(node, "type").contains(&"table")
            && raw_values(node, "default")
                .iter()
                .any(|value| !matches!(*value, "0" | "false" | "off"))
    });
    let result = tables
        .iter()
        .zip(direct)
        .map(|(table, direct)| {
            if direct {
                return true;
            }
            let references = table.all("tblStyle");
            let reachable = references.iter().any(|reference| {
                raw_values(reference, "val")
                    .iter()
                    .any(|id| possible_ids.contains(id))
            });
            // An unspecified reference is not an edge to every unused style.
            // Conversely, a wrapped/foreign/malformed reference cannot suppress
            // a potentially active default. This only discovers candidates;
            // the complete strict chain remains mandatory before promotion.
            let explicit = table
                .children
                .iter()
                .filter(|p| p.tag == "tblPr" && p.identity().is_ok())
                .flat_map(|p| &p.children)
                .any(|r| {
                    r.tag == "tblStyle"
                        && r.schema(&["val"], true).is_ok()
                        && matches!(r.attr("val"), Ok(Some(value)) if !value.is_empty())
                });
            reachable || (default_possible && !explicit)
        })
        .collect();
    Ok((result, unscoped, work))
}
fn analyze_trees(main: &Xml, styles_tree: &Xml) -> std::result::Result<Audit, &'static str> {
    let main_flag = !main.all("cantSplit").is_empty();
    let tables = main.all("tbl");
    let (candidate, unscoped, mut work) = candidates(&tables, styles_tree)?;
    if !main_flag && !unscoped && !candidate.iter().any(|v| *v) {
        return Ok(Audit::default());
    }
    let body = root(main, "document")
        .and_then(|n| n.one("body")?.context("Missing row body"))
        .map_err(|_| PACKAGE)?;
    body.identity().map_err(|_| PACKAGE)?;
    let styles = root(styles_tree, "styles").map_err(|_| PACKAGE)?;
    let mut index = Styles::new(styles).map_err(|e| classify(e, PACKAGE))?;
    let mut chains = Vec::new();
    // Only a potentially active candidate needs a strict style proof. A table
    // that cannot reach any declared flag retains its old normalization path.
    for (table, candidate) in tables.iter().zip(&candidate) {
        let chain = if *candidate {
            let explicit = table
                .one("tblPr")
                .and_then(|p| p.map(|p| p.one("tblStyle")).transpose())
                .map_err(|_| PACKAGE)?
                .flatten();
            if table.all("tblStyle").len() != usize::from(explicit.is_some()) {
                return Err(PACKAGE);
            }
            index.chain(table).map_err(|e| classify(e, PACKAGE))?
        } else {
            Vec::new()
        };
        work = work
            .saturating_add(table.size)
            .saturating_add(chain.iter().map(|s| s.size).sum::<usize>());
        if work > MAX_WORK {
            return Err(RESOURCE);
        }
        chains.push(chain);
    }
    if unscoped {
        return Err(SOURCE);
    }
    if !candidate.iter().any(|v| *v) {
        return Err(DECLARATION);
    }
    let mut proofs = Vec::new();
    let mut text_bytes = 0usize;
    for ((table, chain), candidate) in tables.iter().zip(&chains).zip(candidate) {
        if !candidate {
            proofs.push(None);
            continue;
        }
        if chain
            .iter()
            .any(|s| !s.all("cantSplit").is_empty() || !s.all("tblHeader").is_empty())
        {
            return Err(SOURCE);
        }
        proofs.push(Some(
            structure(table, body, &mut text_bytes).map_err(|e| classify(e, SOURCE))?,
        ));
    }
    flag_locations(main, &mut Vec::new()).map_err(|_| DECLARATION)?;
    for (source, proof) in tables.iter().zip(&mut proofs) {
        if let Some(proof) = proof {
            declarations(source, proof).map_err(|_| DECLARATION)?;
        }
    }
    Ok(Audit {
        tables: proofs,
        warning: Some(PRESERVED),
    })
}
#[cfg(test)]
fn analyze_xml(main: &[u8], styles: &[u8]) -> std::result::Result<Audit, &'static str> {
    let main = xml(main).map_err(|e| classify(e, PACKAGE))?;
    let styles = match xml(styles) {
        Ok(styles) => styles,
        Err(error) if !main.all("cantSplit").is_empty() => return Err(classify(error, PACKAGE)),
        Err(_) => return Ok(Audit::default()),
    };
    analyze_trees(&main, &styles)
}
fn safe_target(target: &str) -> Result<()> {
    ensure!(
        !target.is_empty()
            && !target.starts_with("//")
            && !target.contains(['\\', '#', '?', '%', ':'])
            && !target.chars().any(char::is_control),
        "Unsupported row part target"
    );
    Ok(())
}
fn package_identity(
    package: &mut OpcReader<Cursor<&[u8]>>,
    bytes: &[u8],
    main: &office_oxide::core::opc::PartName,
) -> Result<()> {
    use office_oxide::core::{opc::PartName, relationships::Relationships};
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut names = BTreeSet::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        let part = PartName::new(&format!("/{name}"))?;
        ensure!(
            names.insert(part.as_str().to_ascii_lowercase()),
            "Aliased row source parts"
        );
    }
    let raw = package.read_part(&PartName::new("/[Content_Types].xml")?)?;
    crate::image_dimensions::audit_content_types(&raw)?;
    let raw = package.read_part(&PartName::new("/_rels/.rels")?)?;
    crate::image_dimensions::audit_relationships(&raw)?;
    let roots = Relationships::parse(&raw)?;
    let selected = roots.get_by_type(rel_types::OFFICE_DOCUMENT);
    ensure!(
        selected.len() == 1
            && roots.duplicate_ids().is_empty()
            && main.as_str() == "/word/document.xml",
        "Ambiguous row main relationship"
    );
    // A lookalike role URI must not be ignored in favor of the canonical one.
    ensure!(
        roots
            .all()
            .iter()
            .filter(|r| r.rel_type.rsplit('/').next() == Some("officeDocument"))
            .count()
            == 1,
        "Ambiguous row main role"
    );
    let selected = selected[0];
    ensure!(
        selected.target_mode == TargetMode::Internal,
        "External row main relationship"
    );
    safe_target(&selected.target)?;
    let target = PartName::new(&format!("/{}", selected.target.trim_start_matches('/')))?;
    ensure!(target == *main, "Row main target differs");
    let main_rels = PartName::new(&main.rels_path())?;
    if package.has_part(&main_rels) {
        let raw = package.read_part(&main_rels)?;
        crate::image_dimensions::audit_relationships(&raw)?;
        crate::paragraph_indentation::validate_relationships(&raw)?;
    }
    let relationships = package.read_rels_for(main)?;
    let styles = relationships.get_by_type(rel_types::STYLES);
    ensure!(
        relationships.duplicate_ids().is_empty() && styles.len() <= 1,
        "Ambiguous row style relationships"
    );
    ensure!(
        relationships
            .all()
            .iter()
            .filter(|r| r.rel_type.rsplit('/').next() == Some("styles"))
            .count()
            == styles.len(),
        "Unsupported row style role"
    );
    if let Some(style) = styles.first() {
        ensure!(
            style.target_mode == TargetMode::Internal,
            "External row styles"
        );
        safe_target(&style.target)?;
        let target = main.resolve_relative(&style.target)?;
        ensure!(
            package.content_types().resolve(&target)
                == Some(
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"
                ),
            "Unproved row styles content type"
        );
    }
    ensure!(
        package.content_types().resolve(main)
            == Some(
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"
            ),
        "Unproved row main content type"
    );
    Ok(())
}
pub(crate) fn analyze(bytes: &[u8]) -> Audit {
    let Ok(mut package) = OpcReader::new(Cursor::new(bytes)) else {
        return Audit::default();
    };
    let Ok(main) = package.main_document_part() else {
        return Audit::default();
    };
    let Ok(data) = package.read_part(&main) else {
        return Audit::default();
    };
    let Ok(tree) = xml(&data) else {
        return Audit::default();
    };
    let direct = !tree.all("cantSplit").is_empty();
    // A broken/missing style source cannot establish inherited candidacy. Do
    // not invent a row warning for an old no-direct-flag document.
    let styles = (|| -> Result<Vec<u8>> {
        let relationships = package.read_rels_for(&main)?;
        let selected = relationships.get_by_type(rel_types::STYLES);
        ensure!(selected.len() <= 1, "Ambiguous row styles");
        if let Some(style) = selected.first() {
            ensure!(
                style.target_mode == TargetMode::Internal,
                "External row styles"
            );
            Ok(package.read_part(&main.resolve_relative(&style.target)?)?)
        } else {
            Ok(br#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#.to_vec())
        }
    })();
    let styles = match styles.and_then(|data| xml(&data)) {
        Ok(styles) => styles,
        Err(error) => {
            return if direct {
                failed(classify(error, PACKAGE))
            } else {
                Audit::default()
            };
        }
    };
    let audit = analyze_trees(&tree, &styles).unwrap_or_else(failed);
    if audit.warning.is_some()
        && let Err(error) = package_identity(&mut package, bytes, &main)
    {
        return failed(classify(error, PACKAGE));
    }
    audit
}
fn ir_tables<'a>(elements: &'a [Element], out: &mut Vec<&'a office_oxide::ir::Table>) {
    for element in elements {
        match element {
            Element::Table(table) => {
                out.push(table);
                for row in &table.rows {
                    for cell in &row.cells {
                        ir_tables(&cell.content, out);
                    }
                }
            }
            Element::List(list) => {
                for item in &list.items {
                    ir_tables(&item.content, out);
                    if let Some(nested) = &item.nested {
                        ir_nested_list(nested, out);
                    }
                }
            }
            _ => {}
        }
    }
}
fn ir_nested_list<'a>(
    list: &'a office_oxide::ir::List,
    out: &mut Vec<&'a office_oxide::ir::Table>,
) {
    for item in &list.items {
        ir_tables(&item.content, out);
        if let Some(nested) = &item.nested {
            ir_nested_list(nested, out);
        }
    }
}
pub(crate) fn verify_ir(audit: &Audit, ir: &DocumentIR) -> Result<()> {
    if audit.warning != Some(PRESERVED) {
        return Ok(());
    }
    let mut tables = Vec::new();
    for section in &ir.sections {
        ir_tables(&section.elements, &mut tables);
    }
    ensure!(
        tables.len() == audit.tables.len(),
        "Row table identity changed"
    );
    for (table, proof) in tables.iter().zip(&audit.tables) {
        let Some(proof) = proof else {
            continue;
        };
        ensure!(table.rows.len() == proof.rows.len(), "Row count changed");
        for (row, proof) in table.rows.iter().zip(&proof.rows) {
            ensure!(
                row.allow_break != proof.cant_split.unwrap_or(false)
                    && row.is_header == proof.header
                    && row.repeat_as_header == proof.header
                    && row.height_twips.is_none()
                    && row.height_rule.is_none()
                    && row.cells.len() == proof.paragraphs.len(),
                "Row flags or identity changed"
            );
            for (cell, proof) in row.cells.iter().zip(&proof.paragraphs) {
                ensure!(
                    cell.col_span <= 1 && cell.row_span <= 1 && cell.vertical_align.is_none(),
                    "Row cell geometry changed"
                );
                let actual = cell
                    .content
                    .iter()
                    .map(|e| match e {
                        Element::Paragraph(p) => Ok(inline_to_text(&p.content)),
                        Element::Heading(h) => Ok(inline_to_text(&h.content)),
                        _ => anyhow::bail!("Row paragraph kind changed"),
                    })
                    .collect::<Result<Vec<_>>>()?;
                ensure!(actual == *proof, "Row text/order changed");
            }
        }
    }
    Ok(())
}
fn model_tables<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
    if node.kind == "table" {
        out.push(node);
    }
    for child in &node.content {
        model_tables(child, out);
    }
}
fn verify_model(audit: &Audit, document: &Node) -> Result<()> {
    let mut tables = Vec::new();
    model_tables(document, &mut tables);
    ensure!(
        tables.len() == audit.tables.len(),
        "Converted row table count changed"
    );
    for (table, proof) in tables.iter().zip(&audit.tables) {
        let Some(proof) = proof else {
            continue;
        };
        ensure!(
            document
                .content
                .iter()
                .any(|child| std::ptr::eq(child, *table))
                && table.content.len() == proof.rows.len(),
            "Converted row table ownership changed"
        );
        for (row, proof) in table.content.iter().zip(&proof.rows) {
            ensure!(
                row.kind == "tableRow"
                    && !row.attrs.contains_key("rowCantSplit")
                    && row.content.len() == proof.paragraphs.len(),
                "Converted row identity changed"
            );
            for (cell, paragraphs) in row.content.iter().zip(&proof.paragraphs) {
                ensure!(
                    cell.kind
                        == if proof.header {
                            "tableHeader"
                        } else {
                            "tableCell"
                        },
                    "Converted header ownership changed"
                );
                let actual =
                    cell.content
                        .iter()
                        .map(|p| {
                            ensure!(
                                matches!(p.kind.as_str(), "paragraph" | "heading"),
                                "Converted row paragraph kind changed"
                            );
                            let mut text = String::new();
                            for c in &p.content {
                                match c.kind.as_str() {
                                    "text" => text
                                        .push_str(c.text.as_deref().context("Missing row text")?),
                                    "hardBreak" => text.push('\n'),
                                    _ => anyhow::bail!("Converted row inline changed"),
                                }
                            }
                            Ok(text)
                        })
                        .collect::<Result<Vec<_>>>()?;
                ensure!(actual == *paragraphs, "Converted row text/order changed");
            }
        }
    }
    Ok(())
}
pub(crate) fn attach(
    audit: &Audit,
    ir_matches: bool,
    document: &mut Node,
    warnings: &mut BTreeSet<String>,
) {
    if audit.warning != Some(PRESERVED) {
        return;
    }
    if !ir_matches || verify_model(audit, document).is_err() {
        warnings.remove(PRESERVED);
        warnings.insert(BINDING.into());
        return;
    }
    fn visit(node: &mut Node, proofs: &mut std::slice::Iter<'_, Option<Table>>) {
        if node.kind == "table"
            && let Some(proof) = proofs.next().expect("all table destinations prevalidated")
        {
            for (row, proof) in node.content.iter_mut().zip(&proof.rows) {
                if let Some(flag) = proof.cant_split {
                    row.attrs.insert("rowCantSplit".into(), flag.into());
                }
            }
        }
        for child in &mut node.content {
            visit(child, proofs);
        }
    }
    visit(document, &mut audit.tables.iter());
}

#[cfg(test)]
mod tests {
    use super::*;
    use office_oxide::ir::{InlineContent, Paragraph, Section, TableCell, TableRow, TextSpan};
    use serde_json::json;
    const NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    fn table(flag: &str, text: &str) -> String {
        format!(
            r#"<w:tbl><w:tblPr/><w:tr><w:trPr>{flag}</w:trPr><w:tc><w:p><w:r><w:t>{text}</w:t></w:r></w:p><w:p/></w:tc></w:tr></w:tbl>"#
        )
    }
    fn audit(body: &str, styles: &str) -> Audit {
        analyze_xml(format!(r#"<w:document xmlns:w="{NS}" xmlns:q="{NS}" xmlns:x="urn:foreign"><w:body>{body}</w:body></w:document>"#).as_bytes(),
            format!(r#"<w:styles xmlns:w="{NS}" xmlns:q="{NS}">{styles}</w:styles>"#).as_bytes()).unwrap_or_else(failed)
    }
    #[test]
    fn direct_bool_preserves_lexical_presence_and_absence() {
        for (value, expected) in [
            ("", true),
            ("1", true),
            ("true", true),
            ("on", true),
            ("0", false),
            ("false", false),
            ("off", false),
        ] {
            let flag = if value.is_empty() {
                "<w:cantSplit/>".to_owned()
            } else {
                format!(r#"<w:cantSplit w:val="{value}"/>"#)
            };
            let audit = audit(&(table(&flag, "first") + &table("", "unrelated")), "");
            assert_eq!(audit.warning, Some(PRESERVED));
            assert_eq!(
                audit.tables[0].as_ref().unwrap().rows[0].cant_split,
                Some(expected)
            );
            assert!(audit.tables[1].is_none());
        }
        assert!(audit(&table("", "old"), "").warning.is_none());
    }
    #[test]
    fn direct_declarations_reject_invalid_and_supported_namespace_aliases() {
        for declaration in [
            r#"<w:cantSplit w:val="TRUE"/>"#,
            r#"<w:cantSplit w:val=""/>"#,
            r#"<w:cantSplit w:val=" 1"/>"#,
            r#"<w:cantSplit w:val="yes"/>"#,
            r#"<w:cantSplit w:unknown="1"/>"#,
            "<w:cantSplit>x</w:cantSplit>",
            "<w:cantSplit><w:unknown/></w:cantSplit>",
            "<w:cantSplit/><w:cantSplit/>",
            "<w:wrapper><w:cantSplit/></w:wrapper>",
            "<q:cantSplit/>",
            "<x:cantSplit/>",
            r#"<w:cantSplit q:val="1"/>"#,
            r#"<w:cantSplit w:val="1" q:val="0"/>"#,
        ] {
            assert_eq!(
                audit(&table(declaration, "cell"), "").warning,
                Some(DECLARATION),
                "{declaration}"
            );
        }
        let wrapped = table("<w:cantSplit/>", "cell").replace("<w:trPr>", "<w:trPr/><w:trPr>");
        assert_eq!(audit(&wrapped, "").warning, Some(DECLARATION));
        assert_eq!(
            audit("<w:p><w:pPr><w:cantSplit/></w:pPr></w:p>", "").warning,
            Some(DECLARATION)
        );
    }
    #[test]
    fn active_inheritance_rolls_back_group_but_unused_templates_are_noops() {
        let selected = table("<w:cantSplit/>", "cell").replace(
            "<w:tblPr/>",
            r#"<w:tblPr><w:tblStyle w:val="Leaf"/></w:tblPr>"#,
        );
        let unused = r#"<w:style w:type="table" w:styleId="Unused"><w:trPr><w:cantSplit/></w:trPr></w:style>"#;
        assert!(audit(&table("", "old"), unused).warning.is_none());
        assert_eq!(
            audit(&table("<w:cantSplit/>", "cell"), unused).warning,
            Some(PRESERVED)
        );
        for declaration in ["<w:cantSplit/>", r#"<w:cantSplit w:val="0"/>"#] {
            for condition in [false, true] {
                let props = format!("<w:trPr>{declaration}</w:trPr>");
                let props = if condition {
                    format!(r#"<w:tblStylePr w:type="firstRow">{props}</w:tblStylePr>"#)
                } else {
                    props
                };
                let styles = format!(
                    r#"<w:style w:type="table" w:styleId="Leaf"><w:basedOn w:val="Base"/></w:style><w:style w:type="table" w:styleId="Base">{props}</w:style>"#
                );
                let result = audit(&(table("<w:cantSplit/>", "good") + &selected), &styles);
                assert_eq!(result.warning, Some(SOURCE));
                assert!(result.tables.is_empty());
                assert_eq!(
                    audit(&selected.replace("<w:cantSplit/>", ""), &styles).warning,
                    Some(SOURCE)
                );
            }
        }
        for styles in [
            "",
            r#"<w:style w:type="table" w:styleId="Leaf"><w:basedOn w:val="Leaf"/></w:style>"#,
            r#"<w:style w:type="table" w:styleId="Leaf"/><w:style w:type="table" w:styleId="Leaf"/>"#,
        ] {
            assert_eq!(audit(&selected, styles).warning, Some(PACKAGE));
        }
    }
    #[test]
    fn discovery_does_not_activate_unrelated_missing_or_broken_style_chains() {
        let unused = r#"<w:style w:type="table" w:styleId="Unused"><w:trPr><w:cantSplit/></w:trPr></w:style>"#;
        for (id, other) in [
            ("Missing", ""),
            (
                "Leaf",
                r#"<w:style w:type="table" w:styleId="Leaf"><w:basedOn w:val="Missing"/></w:style>"#,
            ),
            (
                "Leaf",
                r#"<w:style w:type="table" w:styleId="Leaf"><w:basedOn w:val="Leaf"/></w:style>"#,
            ),
            (
                "Leaf",
                r#"<w:style w:type="table" w:styleId="Leaf"><w:basedOn/></w:style>"#,
            ),
        ] {
            let selected = table("", "cell").replace(
                "<w:tblPr/>",
                &format!(r#"<w:tblPr><w:tblStyle w:val="{id}"/></w:tblPr>"#),
            );
            for styles in [other.to_owned(), format!("{unused}{other}")] {
                assert!(
                    audit(&selected, &styles).warning.is_none(),
                    "{id}: {styles}"
                );
                assert_eq!(
                    audit(
                        &selected.replace("<w:trPr>", "<w:trPr><w:cantSplit/>"),
                        &styles
                    )
                    .warning,
                    Some(PACKAGE)
                );
            }
        }
        let body = format!(
            r#"<w:document xmlns:w="{NS}"><w:body>{}</w:body></w:document>"#,
            table("", "cell")
        );
        assert!(
            analyze_xml(body.as_bytes(), b"<broken")
                .unwrap()
                .warning
                .is_none()
        );
        assert_eq!(
            analyze_xml(
                body.replace("<w:trPr>", "<w:trPr><w:cantSplit/>")
                    .as_bytes(),
                b"<broken"
            )
            .err(),
            Some(PACKAGE)
        );
        for flag in ["0", "false", "off", "1", "true", "on", "unknown"] {
            let style = format!(
                r#"<w:style w:type="table" w:styleId="Default" w:default="{flag}"><w:trPr><w:cantSplit/></w:trPr></w:style>"#
            );
            let expected = match flag {
                "0" | "false" | "off" => None,
                "unknown" => Some(PACKAGE),
                _ => Some(SOURCE),
            };
            assert_eq!(audit(&table("", "cell"), &style).warning, expected);
            let clean = table("", "cell").replace(
                "<w:tblPr/>",
                r#"<w:tblPr><w:tblStyle w:val="Clean"/></w:tblPr>"#,
            );
            assert!(
                audit(
                    &clean,
                    &(style + r#"<w:style w:type="table" w:styleId="Clean"/>"#)
                )
                .warning
                .is_none()
            );
        }
        let selected = table("", "cell").replace(
            "<w:tblPr/>",
            r#"<w:tblPr><w:wrapper><w:tblStyle w:val="Unused"/></w:wrapper></w:tblPr>"#,
        );
        assert_eq!(audit(&selected, unused).warning, Some(PACKAGE));
        let unvalued = table("", "cell").replace("<w:tblPr/>", "<w:tblPr><w:tblStyle/></w:tblPr>");
        assert!(audit(&unvalued, unused).warning.is_none());
        assert_eq!(
            audit(
                &unvalued.replace("<w:trPr>", "<w:trPr><w:cantSplit/>"),
                unused
            )
            .warning,
            Some(PACKAGE)
        );
        let default = r#"<w:style w:type="table" w:styleId="Default" w:default="true"><w:trPr><w:cantSplit/></w:trPr></w:style><w:style w:type="table" w:styleId="Clean"/>"#;
        assert_eq!(audit(&unvalued, default).warning, Some(PACKAGE));
        let wrapped = table("", "cell").replace(
            "<w:tblPr/>",
            r#"<w:tblPr><w:wrapper><w:tblStyle w:val="Clean"/></w:wrapper></w:tblPr>"#,
        );
        assert_eq!(audit(&wrapped, default).warning, Some(PACKAGE));
    }
    fn package_parts(flag: &str) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("[Content_Types].xml".into(), r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#.into()),
            ("_rels/.rels".into(), r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="main" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.into()),
            ("word/_rels/document.xml.rels".into(), r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#.into()),
            ("word/document.xml".into(), format!(r#"<w:document xmlns:w="{NS}"><w:body>{}</w:body></w:document>"#, table(flag, "cell"))),
            ("word/styles.xml".into(), format!(r#"<w:styles xmlns:w="{NS}"/>"#)),
        ])
    }
    fn package_audit(parts: &BTreeMap<String, String>) -> Audit {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, content) in parts {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        analyze(&zip.finish().unwrap().into_inner())
    }
    #[test]
    fn root_and_content_type_authority_are_candidate_scoped() {
        for flag in ["", "<w:cantSplit/>"] {
            let base = package_parts(flag);
            let expected = if flag.is_empty() { None } else { Some(PACKAGE) };
            assert_eq!(
                package_audit(&base).warning,
                if flag.is_empty() {
                    None
                } else {
                    Some(PRESERVED)
                }
            );
            for (part, change) in [
                ("_rels/.rels", 0),
                ("_rels/.rels", 1),
                ("_rels/.rels", 2),
                ("[Content_Types].xml", 0),
                ("[Content_Types].xml", 1),
                ("[Content_Types].xml", 2),
            ] {
                let mut parts = base.clone();
                let value = &base[part];
                let (element, close) = if part == "_rels/.rels" {
                    ("Relationship", "</Relationships>")
                } else {
                    ("Override", "</Types>")
                };
                let start = value.find(&format!("<{element} ")).unwrap();
                let end = start + value[start..].find("/>").unwrap() + 2;
                let declaration = &value[start..end];
                let value = match change {
                    0 => value.replace(declaration, &format!("<wrapper>{declaration}</wrapper>")),
                    1 => value.replace(
                        declaration,
                        &declaration.replacen(
                            &format!("<{element} "),
                            &format!("<{element} xmlns=\"urn:foreign\" "),
                            1,
                        ),
                    ),
                    _ => value.replace(close, &format!("{declaration}{close}")),
                };
                parts.insert(part.into(), value);
                assert_eq!(
                    package_audit(&parts).warning,
                    expected,
                    "{part}: {change}, {flag}"
                );
            }
            for missing in [false, true] {
                let mut parts = base.clone();
                if missing {
                    parts.remove("word/styles.xml");
                } else {
                    parts.insert("word/styles.xml".into(), "<broken".into());
                }
                assert_eq!(package_audit(&parts).warning, expected);
            }
        }
    }
    #[test]
    fn skipped_properties_cannot_hide_passive_markers_or_row_headers() {
        let base = table("<w:cantSplit/>", "cell");
        for marker in [
            r#"<w:proofErr w:type="unknown"/>"#,
            r#"<w:bookmarkStart w:id="8" w:name="hidden"/>"#,
            r#"<w:bookmarkStart w:id="8" w:name="hidden"/><w:bookmarkEnd w:id="8"/>"#,
            r#"<w:proofErr w:type="spellStart"/><w:proofErr w:type="spellEnd"/>"#,
        ] {
            for (old, new) in [
                (
                    "<w:tblPr/>".to_owned(),
                    format!("<w:tblPr>{marker}</w:tblPr>"),
                ),
                (
                    "<w:tc>".to_owned(),
                    format!("<w:tc><w:tcPr>{marker}</w:tcPr>"),
                ),
                ("<w:p>".to_owned(), format!("<w:p><w:pPr>{marker}</w:pPr>")),
                ("<w:r>".to_owned(), format!("<w:r><w:rPr>{marker}</w:rPr>")),
            ] {
                assert_eq!(
                    audit(&base.replace(&old, &new), "").warning,
                    Some(SOURCE),
                    "{new}"
                );
            }
        }
        for header in ["<w:tblHeader/>", r#"<w:tblHeader w:val="0"/>"#] {
            assert_eq!(
                audit(
                    &base.replace("<w:tblPr/>", &format!("<w:tblPr>{header}</w:tblPr>")),
                    ""
                )
                .warning,
                Some(DECLARATION)
            );
        }
    }
    #[test]
    fn source_bookmark_and_proofing_markers_are_independently_accounted() {
        let body = table("<w:cantSplit/>", "Nicolaas").replace("<w:r><w:t>Nicolaas", r#"<w:bookmarkStart w:id="0" w:name="_GoBack"/><w:proofErr w:type="spellStart"/><w:r><w:t>Nicolaas"#)
            .replace("</w:r></w:p>", r#"</w:r><w:proofErr w:type="spellEnd"/></w:p>"#)
            .replace("</w:tbl>", r#"<w:bookmarkEnd w:id="0"/></w:tbl>"#);
        let result = audit(&body, "");
        assert_eq!(result.warning, Some(PRESERVED));
        assert_eq!(
            result.tables[0].as_ref().unwrap().rows[0].paragraphs[0],
            vec!["Nicolaas", ""]
        );
        for bad in [
            body.replace(r#"<w:bookmarkEnd w:id="0"/>"#, ""),
            body.replace(r#"<w:proofErr w:type="spellEnd"/>"#, ""),
            body.replace(r#"w:type="spellStart""#, r#"w:type="unknown""#),
            body.replace("</w:tbl>", "<w:unknown/></w:tbl>"),
            body.replace(
                r#"<w:bookmarkEnd w:id="0"/>"#,
                r#"<w:bookmarkEnd w:id="0"><w:t>lost</w:t></w:bookmarkEnd>"#,
            ),
        ] {
            assert_eq!(audit(&bad, "").warning, Some(SOURCE));
        }
    }
    #[test]
    fn source_shape_text_and_resource_boundaries_do_not_promote_partial_tables() {
        for bad in [
            table("<w:cantSplit/><w:trHeight w:val=\"0\"/>", "cell"),
            table("<w:cantSplit/>", "cell").replace("<w:p/>", "<w:sdt><w:p/></w:sdt>"),
            table("<w:cantSplit/>", "cell")
                .replace("<w:p/>", "<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>"),
            format!("<w:sdt>{}</w:sdt>", table("<w:cantSplit/>", "cell")),
        ] {
            let result = audit(&(table("<w:cantSplit/>", "good") + &bad), "");
            assert_eq!(result.warning, Some(SOURCE));
            assert!(result.tables.is_empty());
        }
        assert_eq!(
            audit(&table("<w:cantSplit/>", &"x".repeat(2 * 1024 * 1024)), "").warning,
            Some(PRESERVED)
        );
        assert_eq!(
            audit(
                &table("<w:cantSplit/>", &"x".repeat(2 * 1024 * 1024 + 1)),
                ""
            )
            .warning,
            Some(RESOURCE)
        );
    }
    #[test]
    fn source_row_and_cell_subset_edges_are_explicit() {
        fn rectangular(rows: usize, columns: usize) -> String {
            let cells = "<w:tc><w:p/></w:tc>".repeat(columns);
            let rows = (0..rows)
                .map(|index| {
                    let properties = if index == 0 {
                        "<w:trPr><w:cantSplit/></w:trPr>"
                    } else {
                        ""
                    };
                    format!("<w:tr>{properties}{cells}</w:tr>")
                })
                .collect::<String>();
            format!("<w:tbl>{rows}</w:tbl>")
        }
        for (rows, cells, warning) in [
            (500, 1, PRESERVED),
            (501, 1, SOURCE),
            (1, 50, PRESERVED),
            (1, 51, SOURCE),
        ] {
            assert_eq!(
                audit(&rectangular(rows, cells), "").warning,
                Some(warning),
                "{rows} x {cells}"
            );
        }
    }
    #[test]
    fn cumulative_work_and_style_depth_have_bounded_positive_controls() {
        let formatting = ["wholeTable", "firstRow", "lastRow", "firstCol", "lastCol", "band1Vert", "band2Vert", "band1Horz", "band2Horz", "neCell", "nwCell", "seCell", "swCell"]
            .iter().map(|kind| {
                let borders = ["top", "left", "bottom", "right"].iter().map(|side| format!(r#"<w:{side} w:val="single" w:sz="4"/>"#)).collect::<String>();
                format!(r#"<w:tblStylePr w:type="{kind}"><w:tcPr><w:tcBorders>{borders}</w:tcBorders></w:tcPr></w:tblStylePr>"#)
            }).collect::<String>();
        let styles = |depth| {
            (0..depth).map(|index| {
            let next = index + 1;
            let base = if next < depth { format!(r#"<w:basedOn w:val="S{next}"/>"#) } else { String::new() };
            format!(r#"<w:style w:type="table" w:styleId="S{index}">{base}{formatting}</w:style>"#)
        }).collect::<String>()
        };
        let table = table("<w:cantSplit/>", "cell").replace(
            "<w:tblPr/>",
            r#"<w:tblPr><w:tblStyle w:val="S0"/></w:tblPr>"#,
        );
        assert_eq!(
            audit(&table.repeat(300), &styles(64)).warning,
            Some(PRESERVED)
        );
        assert_eq!(
            audit(&table.repeat(350), &styles(64)).warning,
            Some(RESOURCE)
        );
        // Shared style authority deliberately keeps cycle/depth failure in its
        // existing package-identity category rather than changing old behavior.
        assert_eq!(audit(&table, &styles(65)).warning, Some(PACKAGE));
    }
    fn ir() -> DocumentIR {
        DocumentIR {
            sections: vec![Section {
                elements: vec![Element::Table(office_oxide::ir::Table {
                    rows: vec![TableRow {
                        allow_break: false,
                        cells: vec![TableCell {
                            content: vec![
                                Element::Paragraph(Paragraph {
                                    content: vec![InlineContent::Text(TextSpan::plain("cell"))],
                                    ..Default::default()
                                }),
                                Element::Paragraph(Paragraph::default()),
                            ],
                            ..Default::default()
                        }],
                        ..Default::default()
                    }],
                    ..Default::default()
                })],
                ..Default::default()
            }],
            ..Default::default()
        }
    }
    fn model() -> Node {
        serde_json::from_value(json!({"type":"doc","content":[{"type":"table","content":[{"type":"tableRow","content":[{"type":"tableCell","content":[{"type":"paragraph","content":[{"type":"text","text":"cell"}]},{"type":"paragraph"}]}]}]}]})).unwrap()
    }
    #[test]
    fn binding_checks_ir_flags_and_complete_model_identity_before_atomic_attach() {
        let proof = audit(&table("<w:cantSplit/>", "cell"), "");
        verify_ir(&proof, &ir()).unwrap();
        for mutation in 0..5 {
            let mut changed = ir();
            let Element::Table(table) = &mut changed.sections[0].elements[0] else {
                unreachable!()
            };
            match mutation {
                0 => table.rows[0].allow_break = true,
                1 => table.rows[0].is_header = true,
                2 => table.rows[0].repeat_as_header = true,
                3 => {
                    table.rows[0].cells[0].content.pop();
                }
                _ => table.rows[0].cells[0].row_span = 2,
            }
            assert!(verify_ir(&proof, &changed).is_err());
        }
        let mut doc = model();
        let mut warnings = BTreeSet::from([PRESERVED.into(), "unrelated".into()]);
        attach(&proof, true, &mut doc, &mut warnings);
        assert_eq!(doc.content[0].content[0].attrs["rowCantSplit"], true);
        for mutation in 0..3 {
            let mut changed = model();
            match mutation {
                0 => changed.content[0].content[0].content[0].content.reverse(),
                1 => changed.content[0].content[0].content[0].kind = "tableHeader".into(),
                _ => {}
            }
            let before = changed.clone();
            let mut warnings = BTreeSet::from([PRESERVED.into(), "unrelated".into()]);
            attach(&proof, mutation != 2, &mut changed, &mut warnings);
            assert_eq!(json!(changed), json!(before));
            assert_eq!(
                warnings,
                BTreeSet::from([BINDING.into(), "unrelated".into()])
            );
        }
        let proof = audit(
            &(table("<w:cantSplit/>", "cell") + &table("<w:cantSplit w:val=\"0\"/>", "cell")),
            "",
        );
        let mut doc = model();
        doc.content.push(model().content.remove(0));
        doc.content[1].content[0].content[0].content[0].content[0].text = Some("changed".into());
        let before = doc.clone();
        let mut warnings = BTreeSet::from([PRESERVED.into()]);
        attach(&proof, true, &mut doc, &mut warnings);
        assert_eq!(json!(doc), json!(before));
        assert_eq!(warnings, BTreeSet::from([BINDING.into()]));
    }
}
