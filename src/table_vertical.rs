//! A narrow source proof for minimum table row heights and cell vertical alignment.
//! The converter defaults unknown enum values and does not resolve table-style
//! inheritance for these properties. Bind raw paths, values and cell text to IR
//! before attaching anything to the editable model.
use crate::paragraph_indentation::{Xml, root, xml};
use anyhow::{Result, ensure};
use office_oxide::ir::{CellVerticalAlign, DocumentIR, Element, RowHeightRule, inline_to_text};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(crate) struct VerticalAudit {
    pub tables: Vec<Option<VerticalTable>>,
}
pub(crate) struct VerticalTable {
    pub rows: Vec<VerticalRow>,
}
pub(crate) struct VerticalRow {
    pub height: Option<u64>,
    rule: Option<RowHeightRule>,
    pub cells: Vec<VerticalCell>,
}
pub(crate) struct VerticalCell {
    pub alignment: Option<String>,
    paragraphs: Vec<String>,
}
fn properties(node: &Xml) -> bool {
    !node.all("trHeight").is_empty() || !node.all("vAlign").is_empty()
}
pub(crate) struct Styles<'a> {
    direct: BTreeSet<usize>,
    index: BTreeMap<String, Vec<&'a Xml>>,
    default_candidates: Vec<&'a Xml>,
    cache: BTreeMap<String, Vec<&'a Xml>>,
    default_id: Option<Option<String>>,
    work: usize,
}
impl<'a> Styles<'a> {
    pub(crate) fn new(root: &'a Xml) -> Result<Self> {
        let mut index: BTreeMap<String, Vec<&Xml>> = BTreeMap::new();
        let mut defaults = Vec::new();
        for style in root.all("style") {
            let mut ids = BTreeSet::new();
            for (key, (value, _)) in &style.attrs {
                if key.rsplit(':').next() == Some("styleId") {
                    ids.insert(value.clone());
                }
            }
            for id in ids {
                index.entry(id).or_default().push(style);
            }
            // Discover possible defaults without validating unused templates.
            // Only an unstyled table makes these candidates active.
            if style
                .attrs
                .iter()
                .any(|(key, (value, _))| key.rsplit(':').next() == Some("type") && value == "table")
                && style
                    .attrs
                    .keys()
                    .any(|key| key.rsplit(':').next() == Some("default"))
            {
                defaults.push(style);
            }
        }
        Ok(Self {
            direct: root
                .children
                .iter()
                .map(|node| node as *const Xml as usize)
                .collect(),
            index,
            default_candidates: defaults,
            cache: BTreeMap::new(),
            default_id: None,
            work: 0,
        })
    }
    pub(crate) fn chain(&mut self, table: &Xml) -> Result<Vec<&'a Xml>> {
        let explicit = table
            .one("tblPr")?
            .map(|p| p.one("tblStyle"))
            .transpose()?
            .flatten();
        let id = if let Some(reference) = explicit {
            reference.schema(&["val"], true)?;
            Some(
                reference
                    .attr("val")?
                    .ok_or_else(|| anyhow::anyhow!("Missing table style"))?
                    .to_owned(),
            )
        } else if let Some(id) = &self.default_id {
            id.clone()
        } else {
            let mut defaults = Vec::new();
            for style in &self.default_candidates {
                let flag = style
                    .attr("default")?
                    .ok_or_else(|| anyhow::anyhow!("Missing default table style flag"))?;
                ensure!(
                    matches!(flag, "0" | "false" | "off" | "1" | "true" | "on"),
                    "Invalid default table style flag"
                );
                if matches!(flag, "1" | "true" | "on") {
                    ensure!(
                        style.attr("type")? == Some("table"),
                        "Ambiguous default table style type"
                    );
                    defaults.push(
                        style
                            .attr("styleId")?
                            .ok_or_else(|| anyhow::anyhow!("Missing default table style"))?
                            .to_owned(),
                    );
                }
            }
            ensure!(defaults.len() <= 1, "Ambiguous default table style");
            let id = defaults.into_iter().next();
            self.default_id = Some(id.clone());
            id
        };
        let Some(id) = id else {
            return Ok(Vec::new());
        };
        if let Some(chain) = self.cache.get(&id) {
            return Ok(chain.clone());
        }
        let mut pending = vec![id.clone()];
        let mut chain = Vec::new();
        let mut seen = BTreeSet::new();
        while let Some(id) = pending.pop() {
            ensure!(
                seen.insert(id.clone()) && seen.len() <= 64,
                "Cyclic or excessive table style chain"
            );
            let matching = self
                .index
                .get(&id)
                .ok_or_else(|| anyhow::anyhow!("Unknown table style"))?;
            ensure!(matching.len() == 1, "Ambiguous table style identity");
            let style = matching[0];
            self.work = self.work.saturating_add(style.size);
            ensure!(
                self.work <= 2_000_000,
                "Table style resolution work exceeds limits"
            );
            ensure!(
                self.direct.contains(&(style as *const Xml as usize)),
                "Wrapped active table style"
            );
            style.identity()?;
            ensure!(
                style.attr("styleId")? == Some(&id) && style.attr("type")? == Some("table"),
                "Invalid table style identity"
            );
            let base = style.one("basedOn")?;
            ensure!(
                style.all("basedOn").len() == usize::from(base.is_some()),
                "Wrapped table base style"
            );
            chain.push(style);
            if let Some(base) = base {
                base.schema(&["val"], true)?;
                pending.push(
                    base.attr("val")?
                        .ok_or_else(|| anyhow::anyhow!("Missing table base style"))?
                        .to_owned(),
                );
            }
        }
        self.cache.insert(id, chain.clone());
        Ok(chain)
    }
}
fn scan_locations(node: &Xml, path: &mut Vec<String>) -> Result<()> {
    if node.tag == "trHeight" {
        ensure!(
            path.iter()
                .map(String::as_str)
                .eq(["document", "body", "tbl", "tr", "trPr"]),
            "Non-direct row height"
        );
    }
    if node.tag == "vAlign" {
        ensure!(
            path.iter()
                .map(String::as_str)
                .eq(["document", "body", "tbl", "tr", "tc", "tcPr"])
                || path
                    .iter()
                    .map(String::as_str)
                    .eq(["document", "body", "sectPr"]),
            "Non-direct vertical alignment"
        );
    }
    if !node.tag.is_empty() {
        path.push(node.tag.clone());
    }
    for child in &node.children {
        scan_locations(child, path)?;
    }
    if !node.tag.is_empty() {
        path.pop();
    }
    Ok(())
}
fn text(node: &Xml, out: &mut String) -> Result<()> {
    match node.tag.as_str() {
        "pPr" | "rPr" => return Ok(()),
        "t" => {
            node.identity()?;
            out.push_str(&node.text);
            return Ok(());
        }
        "tab" => {
            node.schema(&[], true)?;
            out.push('\t');
        }
        "br" => {
            node.schema(&["type"], true)?;
            ensure!(
                matches!(node.attr("type")?, None | Some("textWrapping")),
                "Cell page/column break is unsupported"
            );
            out.push('\n');
        }
        "cr" => {
            node.schema(&[], true)?;
            out.push('\n');
        }
        _ => {}
    }
    for child in &node.children {
        text(child, out)?;
    }
    Ok(())
}
fn vertical_margins(node: &Xml, parent: Option<&str>) -> Result<()> {
    if matches!(node.tag.as_str(), "tblCellMar" | "tcMar") {
        ensure!(
            parent
                == Some(if node.tag == "tblCellMar" {
                    "tblPr"
                } else {
                    "tcPr"
                }),
            "Wrapped vertical cell margins"
        );
        node.schema(&[], false)?;
        ensure!(
            node.children.iter().all(|child| matches!(
                child.tag.as_str(),
                "left" | "right" | "start" | "end" | "top" | "bottom"
            )),
            "Unknown cell margin edge"
        );
        for side in ["top", "bottom"] {
            if let Some(value) = node.one(side)? {
                value.schema(&["w", "type"], true)?;
                let width = value
                    .attr("w")?
                    .ok_or_else(|| anyhow::anyhow!("Missing vertical cell margin"))?;
                ensure!(
                    value.attr("type")? == Some("dxa")
                        && !width.is_empty()
                        && width.bytes().all(|byte| byte.is_ascii_digit())
                        && width.parse::<u64>()? == 0,
                    "Nonzero or ambiguous vertical cell margins are unsupported"
                );
            }
        }
    }
    for child in &node.children {
        // A margin-side declaration hidden below a wrapper must not disappear
        // from the default-zero proof.
        vertical_margins(child, Some(&node.tag))?;
    }
    Ok(())
}
fn analyze_table(table: &Xml, chain: &[&Xml], top_level: bool) -> Result<Option<VerticalTable>> {
    ensure!(
        chain.iter().all(|style| !properties(style)),
        "Inherited or conditional table vertical properties are unsupported"
    );
    if !properties(table) {
        return Ok(None);
    }
    ensure!(
        top_level,
        "Vertical table geometry requires a direct body table"
    );
    for node in std::iter::once(table).chain(chain.iter().copied()) {
        vertical_margins(node, None)?;
        for forbidden in [
            "tblHeader",
            "cantSplit",
            "textDirection",
            "hideMark",
            "hidden",
            "vanish",
            "webHidden",
            "specVanish",
            "trPrChange",
            "tcPrChange",
            "tblPrChange",
            "cellIns",
            "cellDel",
            "cellMerge",
            "gridSpan",
            "vMerge",
            "hMerge",
            "ins",
            "del",
            "moveFrom",
            "moveTo",
        ] {
            ensure!(
                node.all(forbidden).is_empty(),
                "Unsupported vertical table declaration: {forbidden}"
            );
        }
    }
    ensure!(table.all("tbl").is_empty(), "Nested vertical table");
    table.identity()?;
    ensure!(
        table
            .children
            .iter()
            .all(|n| matches!(n.tag.as_str(), "tblPr" | "tblGrid" | "tr")),
        "Wrapped table rows"
    );
    let mut rows = Vec::new();
    let mut columns = None;
    for row in table.children.iter().filter(|n| n.tag == "tr") {
        row.identity()?;
        ensure!(
            row.children
                .iter()
                .all(|n| matches!(n.tag.as_str(), "trPr" | "tc")),
            "Wrapped table cells"
        );
        let properties = row.one("trPr")?;
        if let Some(pr) = properties {
            pr.schema(&[], false)?;
        }
        let height = properties.map(|p| p.one("trHeight")).transpose()?.flatten();
        let (height, rule) = if let Some(height) = height {
            height.schema(&["val", "hRule"], true)?;
            let value = height
                .attr("val")?
                .ok_or_else(|| anyhow::anyhow!("Missing minimum row height"))?;
            ensure!(
                !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()),
                "Row height must be integer twips"
            );
            let value: u64 = value.parse()?;
            ensure!(
                value <= crate::document::MAX_ROW_HEIGHT_TWIPS,
                "Row minimum exceeds limits"
            );
            let rule = match height.attr("hRule")? {
                None => None,
                Some("atLeast") => Some(RowHeightRule::AtLeast),
                _ => anyhow::bail!("Only minimum row heights are supported"),
            };
            (Some(value), rule)
        } else {
            (None, None)
        };
        let mut cells = Vec::new();
        for cell in row.children.iter().filter(|n| n.tag == "tc") {
            cell.identity()?;
            ensure!(
                cell.children
                    .iter()
                    .all(|n| matches!(n.tag.as_str(), "tcPr" | "p")),
                "Only direct cell paragraphs are supported with vertical geometry"
            );
            let properties = cell.one("tcPr")?;
            if let Some(pr) = properties {
                pr.schema(&[], false)?;
            }
            let alignment = properties
                .map(|p| p.one("vAlign"))
                .transpose()?
                .flatten()
                .map(|alignment| {
                    alignment.schema(&["val"], true)?;
                    let value = alignment.attr("val")?;
                    ensure!(
                        matches!(value, Some("top" | "center" | "bottom")),
                        "Unsupported cell vertical alignment"
                    );
                    Ok::<_, anyhow::Error>(value.expect("checked").to_owned())
                })
                .transpose()?;
            let paragraphs = cell
                .children
                .iter()
                .filter(|n| n.tag == "p")
                .map(|p| {
                    p.identity()?;
                    ensure!(
                        p.all("numPr").is_empty(),
                        "List paragraphs in vertical tables are unsupported"
                    );
                    let mut value = String::new();
                    text(p, &mut value)?;
                    Ok(value)
                })
                .collect::<Result<Vec<_>>>()?;
            ensure!(
                !paragraphs.is_empty(),
                "Vertical cell must contain a paragraph"
            );
            cells.push(VerticalCell {
                alignment,
                paragraphs,
            });
        }
        ensure!(
            !cells.is_empty()
                && columns
                    .replace(cells.len())
                    .is_none_or(|count| count == cells.len()),
            "Ragged vertical table"
        );
        rows.push(VerticalRow {
            height,
            rule,
            cells,
        });
    }
    ensure!(!rows.is_empty(), "Empty vertical table");
    Ok(Some(VerticalTable { rows }))
}
pub(crate) fn analyze(main: &[u8], styles: &[u8]) -> Result<VerticalAudit> {
    let main = xml(main)?;
    scan_locations(&main, &mut Vec::new())?;
    let tables = main.all("tbl");
    if tables.is_empty() {
        return Ok(VerticalAudit::default());
    }
    let styles_tree = xml(styles)?;
    let styles = root(&styles_tree, "styles")?;
    // Properties outside named styles (for example malformed docDefaults) must
    // not be silently inherited or dropped.
    for node in styles.children.iter().filter(|n| n.tag != "style") {
        ensure!(
            !properties(node),
            "Unscoped table vertical style declaration"
        );
    }
    let body = root(&main, "document")?
        .one("body")?
        .ok_or_else(|| anyhow::anyhow!("Missing body"))?;
    body.identity()?;
    let mut audit = VerticalAudit::default();
    let mut styles_index = Styles::new(styles)?;
    let mut work = 0usize;
    for table in tables {
        let chain = styles_index.chain(table)?;
        work = work
            .saturating_add(table.size)
            .saturating_add(chain.iter().map(|style| style.size).sum::<usize>());
        ensure!(
            work <= 2_000_000,
            "Table vertical audit work exceeds limits"
        );
        let top_level = body.children.iter().any(|child| std::ptr::eq(child, table));
        audit.tables.push(analyze_table(table, &chain, top_level)?);
    }
    Ok(audit)
}

pub(crate) fn verify_ir(audit: &VerticalAudit, ir: &DocumentIR) -> Result<()> {
    fn visit<'a>(elements: &'a [Element], tables: &mut Vec<&'a office_oxide::ir::Table>) {
        for element in elements {
            match element {
                Element::Table(table) => {
                    tables.push(table);
                    for row in &table.rows {
                        for cell in &row.cells {
                            visit(&cell.content, tables);
                        }
                    }
                }
                Element::List(list) => {
                    for item in &list.items {
                        visit(&item.content, tables);
                    }
                }
                _ => {}
            }
        }
    }
    let mut tables = Vec::new();
    for section in &ir.sections {
        visit(&section.elements, &mut tables);
    }
    ensure!(
        tables.len() == audit.tables.len(),
        "Vertical metadata table identity mismatch"
    );
    for (proof, table) in audit.tables.iter().zip(tables) {
        let Some(proof) = proof else {
            continue;
        };
        ensure!(
            proof.rows.len() == table.rows.len(),
            "Vertical metadata row identity mismatch"
        );
        for (proof, row) in proof.rows.iter().zip(&table.rows) {
            ensure!(
                proof.height == row.height_twips.map(u64::from)
                    && proof.rule == row.height_rule
                    && row.allow_break
                    && !row.is_header
                    && !row.repeat_as_header
                    && proof.cells.len() == row.cells.len(),
                "Vertical row values changed during conversion"
            );
            for (proof, cell) in proof.cells.iter().zip(&row.cells) {
                let alignment = cell
                    .vertical_align
                    .as_ref()
                    .map(|alignment| match alignment {
                        CellVerticalAlign::Top => "top",
                        CellVerticalAlign::Center => "center",
                        CellVerticalAlign::Bottom => "bottom",
                    });
                ensure!(
                    proof.alignment.as_deref() == alignment
                        && cell.col_span <= 1
                        && cell.row_span <= 1,
                    "Vertical cell values changed during conversion"
                );
                let paragraphs = cell
                    .content
                    .iter()
                    .map(|element| match element {
                        Element::Paragraph(p) => Ok(inline_to_text(&p.content)),
                        Element::Heading(h) => Ok(inline_to_text(&h.content)),
                        _ => anyhow::bail!("Cell content identity changed during conversion"),
                    })
                    .collect::<Result<Vec<_>>>()?;
                ensure!(
                    proof.paragraphs == paragraphs,
                    "Vertical cell text/order changed during conversion"
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn check_height_limit(
    audit: &VerticalAudit,
    page: Option<&crate::document::PageGeometry>,
) -> Result<()> {
    let maximum = page.map_or(13958, |page| {
        page.height_twips - page.margin_top_twips - page.margin_bottom_twips
    });
    ensure!(
        audit
            .tables
            .iter()
            .flatten()
            .flat_map(|table| &table.rows)
            .all(|row| row.height.is_none_or(|height| height <= maximum)),
        "Row minimum height exceeds page body height"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use office_oxide::ir::{
        InlineContent, Paragraph, Section, Table, TableCell, TableRow, TextSpan,
    };
    fn fixture() -> (VerticalAudit, DocumentIR) {
        let cell = |text: &str| TableCell {
            content: vec![Element::Paragraph(Paragraph {
                content: vec![InlineContent::Text(TextSpan::plain(text))],
                ..Default::default()
            })],
            vertical_align: Some(CellVerticalAlign::Center),
            ..Default::default()
        };
        let audit = VerticalAudit {
            tables: vec![Some(VerticalTable {
                rows: vec![VerticalRow {
                    height: Some(0),
                    rule: Some(RowHeightRule::AtLeast),
                    cells: ["first", "second"]
                        .map(|text| VerticalCell {
                            alignment: Some("center".into()),
                            paragraphs: vec![text.into()],
                        })
                        .into(),
                }],
            })],
        };
        let ir = DocumentIR {
            sections: vec![Section {
                elements: vec![Element::Table(Table {
                    rows: vec![TableRow {
                        height_twips: Some(0),
                        height_rule: Some(RowHeightRule::AtLeast),
                        cells: vec![cell("first"), cell("second")],
                        ..Default::default()
                    }],
                    ..Default::default()
                })],
                ..Default::default()
            }],
            ..Default::default()
        };
        (audit, ir)
    }
    fn row(ir: &mut DocumentIR) -> &mut TableRow {
        let Element::Table(table) = &mut ir.sections[0].elements[0] else {
            unreachable!()
        };
        &mut table.rows[0]
    }
    #[test]
    fn binding_checks_values_rules_omission_and_pagination_flags() {
        let (proof, ir) = fixture();
        verify_ir(&proof, &ir).unwrap();
        for index in 0..7 {
            let mut changed = ir.clone();
            let row = row(&mut changed);
            match index {
                0 => row.height_twips = None,
                1 => row.height_twips = Some(1),
                2 => row.height_rule = None,
                3 => row.height_rule = Some(RowHeightRule::Exact),
                4 => row.is_header = true,
                5 => row.repeat_as_header = true,
                _ => row.allow_break = false,
            };
            assert!(verify_ir(&proof, &changed).is_err());
        }
    }
    #[test]
    fn binding_checks_table_cell_and_paragraph_text_identities() {
        let (proof, ir) = fixture();
        for index in 0..7 {
            let mut changed = ir.clone();
            match index {
                0 => changed.sections[0].elements.clear(),
                1 => row(&mut changed).cells.swap(0, 1),
                2 => {
                    row(&mut changed).cells.pop();
                }
                3 => row(&mut changed).cells[0].vertical_align = None,
                4 => row(&mut changed).cells[0].vertical_align = Some(CellVerticalAlign::Top),
                5 => row(&mut changed).cells[0].content.clear(),
                _ => row(&mut changed).cells[0]
                    .content
                    .push(Element::Paragraph(Paragraph::default())),
            };
            assert!(verify_ir(&proof, &changed).is_err());
        }
    }
    #[test]
    fn bounded_style_chain_rejects_wrapped_default_alias_and_excessive_depth() {
        const NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
        let main = format!(
            r#"<w:document xmlns:w="{NS}"><w:body><w:tbl><w:tblPr><w:tblStyle w:val="s0"/></w:tblPr><w:tr><w:trPr><w:trHeight w:val="0"/></w:trPr><w:tc><w:p/></w:tc></w:tr></w:tbl></w:body></w:document>"#
        );
        let chain: String = (0..65)
            .map(|i| {
                format!(
                    r#"<w:style w:type="table" w:styleId="s{i}">{}</w:style>"#,
                    if i < 64 {
                        format!(r#"<w:basedOn w:val="s{}"/>"#, i + 1)
                    } else {
                        String::new()
                    }
                )
            })
            .collect();
        assert!(
            analyze(
                main.as_bytes(),
                format!(r#"<w:styles xmlns:w="{NS}">{chain}</w:styles>"#).as_bytes()
            )
            .err()
            .unwrap()
            .to_string()
            .contains("excessive")
        );
        let bad = format!(
            r#"<w:styles xmlns:w="{NS}" xmlns:q="{NS}"><w:style w:type="table" w:styleId="s0" w:default="0" q:default="1"/></w:styles>"#
        );
        assert!(
            analyze(
                main.replace(r#"<w:tblStyle w:val="s0"/>"#, "").as_bytes(),
                bad.as_bytes()
            )
            .is_err()
        );
    }
    #[test]
    fn explicitly_selected_style_ignores_inactive_default_and_type_noise() {
        const NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
        let main = format!(
            r#"<w:document xmlns:w="{NS}"><w:body><w:tbl><w:tblPr><w:tblStyle w:val="Active"/></w:tblPr><w:tr><w:trPr><w:trHeight w:val="0"/></w:trPr><w:tc><w:p/></w:tc></w:tr></w:tbl></w:body></w:document>"#
        );
        for unused in [
            r#"<w:style w:type="table" q:type="paragraph" w:styleId="Unused"/>"#,
            r#"<w:style w:type="table" w:default="unknown" w:styleId="Unused"/>"#,
            r#"<w:style w:type="table" w:default="1" w:styleId="Unused1"/><w:style w:type="table" w:default="1" w:styleId="Unused2"/>"#,
        ] {
            let styles = format!(
                r#"<w:styles xmlns:w="{NS}" xmlns:q="{NS}"><w:style w:type="table" w:styleId="Active"/>{unused}</w:styles>"#
            );
            assert!(analyze(main.as_bytes(), styles.as_bytes()).is_ok());
        }
    }
}
