//! Raw DOCX table geometry preflight. The vendor IR intentionally drops layout
//! declarations, so a grid by itself must never become a fixed-width promise.
use anyhow::{Result, ensure};
use quick_xml::{NsReader, events::Event, name::ResolveResult};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const TABLE_GEOMETRY_RISK: &str =
    "Unsupported table geometry cannot be preserved in an editable copy";
pub(crate) const AUTOFIT_WIDTHS_RISK: &str =
    "Autofit table widths normalized; fixed column widths were not inferred";
pub(crate) const INHERITED_POSITION_RISK: &str =
    "Inherited table indentation normalized; automatic table geometry not preserved";
pub(crate) const TABLE_FORMATTING_RISK: &str = "Table borders and cell formatting normalized";

#[derive(Default)]
pub(crate) struct TableAudit {
    pub tables: Vec<TableGeometry>,
    pub risks: BTreeSet<String>,
}

pub(crate) struct TableGeometry {
    pub widths: Option<Vec<u64>>,
    pub row_cells: Vec<usize>,
    pub alignment: Option<String>,
    pub indent: Option<u64>,
}

#[derive(Default)]
struct Xml {
    tag: String,
    wml: bool,
    attrs: BTreeMap<String, (String, bool)>,
    children: Vec<Xml>,
}
fn wml(ns: ResolveResult<'_>) -> bool {
    matches!(ns, ResolveResult::Bound(ns) if matches!(ns.as_ref(),
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main" |
        "http://purl.oclc.org/ooxml/wordprocessingml/main"))
}
impl Xml {
    // Attribute identity matches the downstream reader; reject same-local-name
    // collisions instead of allowing an unrelated prefix to shadow w:*.
    fn attr(&self, name: &str) -> Result<Option<&str>> {
        let matching: Vec<_> = self
            .attrs
            .iter()
            .filter(|(key, _)| key.rsplit(':').next() == Some(name))
            .collect();
        ensure!(matching.len() <= 1, "Conflicting table {name} attributes");
        if let Some((key, (value, bound))) = matching.first() {
            ensure!(
                **key == format!("w:{name}") && *bound,
                "Unsupported table attribute namespace: {name}"
            );
            Ok(Some(value.as_str()))
        } else {
            Ok(None)
        }
    }
    fn literal(&self, name: &str) -> Option<&str> {
        self.attrs
            .get(&format!("w:{name}"))
            .map(|(s, _)| s.as_str())
    }
    fn one(&self, tag: &str) -> Result<Option<&Xml>> {
        let nodes: Vec<_> = self.children.iter().filter(|n| n.tag == tag).collect();
        ensure!(nodes.len() <= 1, "Repeated table property: {tag}");
        if let Some(node) = nodes.first() {
            ensure!(node.wml, "Unsupported table element namespace: {tag}");
        }
        Ok(nodes.first().copied())
    }
    fn descendants<'a>(&'a self, tag: &str, out: &mut Vec<&'a Xml>) {
        for child in &self.children {
            if child.tag == tag {
                out.push(child);
            }
            child.descendants(tag, out);
        }
    }
}

// Text is not retained. Limits bound metadata memory and recursive traversals,
// independently of the package's compressed/expanded size checks.
fn xml(data: &[u8]) -> Result<Xml> {
    let mut reader = NsReader::from_reader(data);
    let mut stack = vec![Xml::default()];
    let mut count = 0usize;
    loop {
        let (ns, event) = reader.read_resolved_event()?;
        let is_wml = wml(ns);
        let start = matches!(&event, Event::Start(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                count += 1;
                ensure!(
                    count <= 100_000 && stack.len() <= 128,
                    "Table metadata exceeds limits"
                );
                let mut node = Xml {
                    tag: e.local_name().as_ref().to_owned(),
                    wml: is_wml,
                    ..Default::default()
                };
                for attr in e.attributes() {
                    let attr = attr?;
                    // Namespace bindings are not attributes in the table schema.
                    if attr.key.as_ref() == "xmlns" || attr.key.as_ref().starts_with("xmlns:") {
                        continue;
                    }
                    let (ns, _) = reader.resolver().resolve_attribute(attr.key);
                    let value = attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)?;
                    ensure!(
                        node.attrs
                            .insert(attr.key.as_ref().to_owned(), (value.into_owned(), wml(ns)))
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
                ensure!(stack.len() > 1, "Unbalanced table metadata XML");
                let node = stack.pop().expect("element");
                stack.last_mut().expect("root").children.push(node);
            }
            Event::DocType(_) => anyhow::bail!("DTD is not supported in DOCX XML"),
            Event::Eof => {
                ensure!(stack.len() == 1, "Incomplete table metadata XML");
                return Ok(stack.pop().expect("root"));
            }
            _ => {}
        }
    }
}

fn number(value: Option<&str>, zero: bool) -> Result<u64> {
    let value = value.ok_or_else(|| anyhow::anyhow!("Missing table width"))?;
    ensure!(
        !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()),
        "Table width must be integer twips"
    );
    let value: u64 = value.parse()?;
    ensure!(
        (if zero { 0 } else { 1 }..=crate::document::MAX_TABLE_WIDTH_TWIPS).contains(&value),
        "Table width is outside supported bounds"
    );
    Ok(value)
}
fn width(node: &Xml, allow_auto: bool) -> Result<Option<u64>> {
    ensure!(
        node.wml && node.children.is_empty(),
        "Unsupported table width declaration"
    );
    match node.attr("type")? {
        Some("dxa") => Ok(Some(number(node.attr("w")?, false)?)),
        Some("auto") if allow_auto && node.attr("w")? == Some("0") => Ok(None),
        _ => anyhow::bail!(
            "Table width must use dxa{}",
            if allow_auto { " or auto zero" } else { "" }
        ),
    }
}
fn neutral_indent(node: &Xml) -> Result<()> {
    ensure!(
        node.wml
            && node.children.is_empty()
            && node.attrs.len() == 2
            && node.attr("type")? == Some("dxa")
            && number(node.attr("w")?, true)? == 0,
        "Table indentation is unsupported"
    );
    Ok(())
}
fn forbidden(node: &Xml) -> bool {
    matches!(
        node.tag.as_str(),
        "tblpPr"
            | "tblOverlap"
            | "gridBefore"
            | "gridAfter"
            | "wBefore"
            | "wAfter"
            | "gridSpan"
            | "vMerge"
            | "hMerge"
            | "tblPrEx"
            | "tblPrChange"
            | "tblGridChange"
            | "trPrChange"
            | "tcPrChange"
            | "cellIns"
            | "cellDel"
            | "cellMerge"
            | "bidiVisual"
    )
}
fn check_tree(node: &Xml) -> Result<()> {
    ensure!(
        !forbidden(node),
        "Unsupported table declaration: {}",
        node.tag
    );
    if node.tag == "tblCellSpacing" {
        ensure!(
            node.wml && node.attr("type")? == Some("dxa") && number(node.attr("w")?, true)? == 0,
            "Table cell spacing is unsupported"
        );
    }
    node.one("tblInd")?;
    node.one("tblCellMar")?;
    node.one("tcMar")?;
    node.one("tblPr")?;
    for child in &node.children {
        ensure!(
            child.tag != "tblInd" || node.tag == "tblPr",
            "Non-direct table indentation"
        );
        ensure!(
            child.tag != "tblPr" || matches!(node.tag.as_str(), "tbl" | "style" | "tblStylePr"),
            "Non-direct table properties"
        );
        ensure!(
            child.tag != "jc" || matches!(node.tag.as_str(), "pPr" | "tblPr"),
            "Non-direct alignment declaration"
        );
        check_tree(child)?;
    }
    Ok(())
}

fn check_styles<'a>(table: &Xml, styles: &'a Xml) -> Result<Vec<&'a Xml>> {
    let mut all = Vec::new();
    styles.descendants("style", &mut all);
    let explicit = table
        .one("tblPr")?
        .map(|pr| pr.one("tblStyle"))
        .transpose()?
        .flatten();
    let mut pending = Vec::new();
    if let Some(style) = explicit {
        let id = style
            .attr("val")?
            .ok_or_else(|| anyhow::anyhow!("Missing table style identifier"))?;
        pending.push(id.to_owned());
    } else {
        for style in &all {
            let table_type = style.attrs.iter().any(|(key, (value, _))| {
                key.rsplit(':').next() == Some("type") && value == "table"
            });
            let default = style.attrs.iter().any(|(key, (value, _))| {
                key.rsplit(':').next() == Some("default")
                    && matches!(value.as_str(), "1" | "true" | "on")
            });
            if table_type && default {
                // An alternate-prefix/default collision must not hide the
                // implicit style from the literal-attribute vendor parser.
                style.attr("type")?;
                style.attr("default")?;
                pending.push(
                    style
                        .attr("styleId")?
                        .ok_or_else(|| anyhow::anyhow!("Default table style lacks an identifier"))?
                        .to_owned(),
                );
            }
        }
        ensure!(pending.len() <= 1, "Ambiguous default table styles");
    }
    let mut visited = BTreeSet::new();
    let mut active = Vec::new();
    while let Some(id) = pending.pop() {
        ensure!(visited.insert(id.clone()), "Cyclic table style inheritance");
        let matching: Vec<_> = all
            .iter()
            .copied()
            .filter(|s| s.literal("styleId") == Some(&id))
            .collect();
        ensure!(matching.len() <= 1, "Duplicate table style identifier");
        let style = matching
            .first()
            .ok_or_else(|| anyhow::anyhow!("Unresolved table style or base style"))?;
        ensure!(
            style.wml
                && style.attr("styleId")? == Some(&id)
                && style.attr("type")? == Some("table"),
            "Unsupported table style identity"
        );
        style.attr("default")?;
        active.push(*style);
        check_tree(style)?;
        let mut indents = Vec::new();
        style.descendants("tblInd", &mut indents);
        for indent in indents {
            neutral_indent(indent)?;
        }
        let mut properties = Vec::new();
        style.descendants("tblPr", &mut properties);
        for properties in properties {
            if let Some(alignment) = properties.one("jc")? {
                ensure!(
                    alignment.children.is_empty()
                        && alignment.attrs.len() == 1
                        && matches!(alignment.attr("val")?, Some("left" | "start")),
                    "Inherited table alignment is unsupported"
                );
            }
        }
        // Do not infer inherited/conditional layout. Even direct overrides of
        // these declarations are intentionally outside this first subset.
        for tag in ["tblLayout", "tblW", "tcW", "tblGrid", "gridCol"] {
            let mut declarations = Vec::new();
            style.descendants(tag, &mut declarations);
            ensure!(
                declarations.is_empty(),
                "Inherited or conditional table geometry is unsupported"
            );
        }
        if let Some(base) = style.one("basedOn")? {
            pending.push(
                base.attr("val")?
                    .ok_or_else(|| anyhow::anyhow!("Missing table base style"))?
                    .to_owned(),
            );
        }
    }
    Ok(active)
}

fn indent_compatibility(settings: Option<&[u8]>) -> Result<()> {
    let Some(data) = settings else {
        return Ok(());
    };
    let settings = xml(data)?;
    ensure!(
        settings.children.len() == 1
            && settings.children[0].tag == "settings"
            && settings.children[0].wml,
        "Unsupported settings identity"
    );
    let root = &settings.children[0];
    let compat = root.one("compat")?;
    let mut containers = Vec::new();
    root.descendants("compat", &mut containers);
    ensure!(
        containers.len() == usize::from(compat.is_some()),
        "Non-direct compatibility container"
    );
    let mut modes = Vec::new();
    root.descendants("compatSetting", &mut modes);
    let direct_modes: Vec<_> = compat
        .into_iter()
        .flat_map(|c| c.children.iter())
        .filter(|c| c.tag == "compatSetting")
        .collect();
    ensure!(
        modes.len() == direct_modes.len(),
        "Non-direct compatibility setting"
    );
    let mut count = 0;
    for mode in direct_modes {
        if mode.attr("name")? == Some("compatibilityMode") {
            count += 1;
            ensure!(
                mode.wml
                    && mode.children.is_empty()
                    && mode.attrs.len() == 3
                    && mode.attr("uri")? == Some("http://schemas.microsoft.com/office/word")
                    && matches!(mode.attr("val")?, Some("11" | "12" | "14")),
                "Table indentation requires supported legacy compatibility semantics"
            );
        }
    }
    ensure!(count <= 1, "Ambiguous compatibility mode");
    Ok(())
}

fn indent_margins(node: &Xml) -> Result<()> {
    // Indentation measures the leading text edge in our compatibility baseline.
    // Horizontal margins alter the border position, so unlike general cell
    // formatting these cannot be normalized silently on this path.
    fn paths(node: &Xml) -> Result<()> {
        for child in &node.children {
            ensure!(
                child.tag != "tblCellMar" || node.tag == "tblPr",
                "Non-direct table margins"
            );
            ensure!(
                child.tag != "tcMar" || node.tag == "tcPr",
                "Non-direct cell margins"
            );
            paths(child)?;
        }
        Ok(())
    }
    paths(node)?;
    let mut margins = Vec::new();
    for tag in ["tblCellMar", "tcMar"] {
        node.descendants(tag, &mut margins);
    }
    for margin in margins {
        ensure!(
            margin.wml && margin.attrs.is_empty(),
            "Unsupported table margin namespace or attributes"
        );
        ensure!(
            margin.children.iter().all(|child| child.wml
                && child.children.is_empty()
                && matches!(
                    child.tag.as_str(),
                    "left" | "right" | "start" | "end" | "top" | "bottom"
                )),
            "Unsupported or non-direct table margin declaration"
        );
        for side in ["left", "right", "start", "end"] {
            if let Some(value) = margin.one(side)? {
                ensure!(
                    value.attrs.len() == 2 && width(value, false)? == Some(108),
                    "Indented tables require 108-twip horizontal cell margins"
                );
            }
        }
        ensure!(
            !(margin.one("left")?.is_some() && margin.one("start")?.is_some()
                || margin.one("right")?.is_some() && margin.one("end")?.is_some()),
            "Ambiguous horizontal table margins"
        );
    }
    Ok(())
}

fn position(
    table: &Xml,
    styles: &Xml,
    settings: Option<&[u8]>,
    fixed: bool,
) -> Result<(Option<String>, Option<u64>, bool)> {
    let properties = table.one("tblPr")?;
    let alignment_node = properties.map(|p| p.one("jc")).transpose()?.flatten();
    let alignment = alignment_node
        .map(|node| {
            ensure!(
                node.children.is_empty() && node.attrs.len() == 1,
                "Unsupported table alignment declaration"
            );
            // bidiVisual is rejected in both the table and its active style chain.
            // With that LTR proof, start/end have physical left/right semantics.
            match node.attr("val")? {
                Some("left" | "start") => Ok("left".to_owned()),
                Some("center") => Ok("center".to_owned()),
                Some("right" | "end") => Ok("right".to_owned()),
                _ => anyhow::bail!("Unsupported table alignment"),
            }
        })
        .transpose()?;
    let indent_node = properties.map(|p| p.one("tblInd")).transpose()?.flatten();
    let mut indent = indent_node
        .map(|node| {
            indent_compatibility(settings)?;
            indent_margins(table)?;
            for style in check_styles(table, styles)? {
                indent_margins(style)?;
            }
            ensure!(
                node.wml
                    && node.children.is_empty()
                    && node.attrs.len() == 2
                    && node.attr("type")? == Some("dxa"),
                "Table indent requires direct dxa units"
            );
            number(node.attr("w")?, true)
        })
        .transpose()?;
    ensure!(
        indent.unwrap_or(0) == 0 || alignment.as_deref().is_none_or(|a| a == "left"),
        "Non-left table indentation differs between consumers"
    );
    let mut declarations = Vec::new();
    table.descendants("tblInd", &mut declarations);
    ensure!(
        declarations.len() == usize::from(indent_node.is_some()),
        "Non-direct table indentation"
    );
    let active = check_styles(table, styles)?;
    // Conditional indentation is not an unconditional table property. Do not
    // promote it merely because its numerical value happens to be zero.
    for style in &active {
        let mut conditional = Vec::new();
        style.descendants("tblStylePr", &mut conditional);
        for condition in conditional {
            let mut indents = Vec::new();
            condition.descendants("tblInd", &mut indents);
            ensure!(
                indents.is_empty(),
                "Conditional table indentation is unsupported"
            );
        }
    }
    let selected_style = properties
        .map(|p| p.one("tblStyle"))
        .transpose()?
        .flatten()
        .is_some();
    let inherited_zero = selected_style
        && active.iter().any(|style| {
            style
                .children
                .iter()
                .filter(|n| n.tag == "tblPr")
                .any(|p| p.children.iter().any(|n| n.tag == "tblInd"))
        });
    let mut normalized = false;
    if indent.is_none() && inherited_zero {
        let proof = (|| -> Result<()> {
            indent_compatibility(settings)?;
            indent_margins(table)?;
            for style in active {
                indent_margins(style)?;
            }
            Ok(())
        })();
        if proof.is_ok() {
            indent = Some(0);
        } else if !fixed {
            normalized = true;
        } else {
            proof?;
        }
    }
    // An implicit default-style zero differs from explicit selected-style zero
    // in the independently checked legacy consumer; keep omission in that case.
    Ok((alignment, indent, normalized))
}

fn table_widths(table: &Xml, styles: &Xml) -> Result<Option<Vec<u64>>> {
    ensure!(table.wml, "Unsupported table namespace");
    check_tree(table)?;
    let mut nested = Vec::new();
    table.descendants("tbl", &mut nested);
    ensure!(nested.is_empty(), "Nested table geometry is unsupported");
    check_styles(table, styles)?;
    let properties = table.one("tblPr")?;
    let layout = properties
        .map(|p| p.one("tblLayout"))
        .transpose()?
        .flatten();
    if let Some(layout) = layout {
        ensure!(
            layout.children.is_empty(),
            "Non-leaf table layout declaration"
        );
    }
    let fixed = match layout.map(|l| l.attr("type")).transpose()?.flatten() {
        Some("fixed") => true,
        None if layout.is_none() => false,
        Some("autofit") => false,
        _ => anyhow::bail!("Unsupported table layout"),
    };
    let table_width = properties
        .map(|p| p.one("tblW"))
        .transpose()?
        .flatten()
        .map(|w| width(w, true))
        .transpose()?
        .flatten();
    let grid = table.one("tblGrid")?;
    let mut widths = Vec::new();
    if let Some(grid) = grid {
        for col in &grid.children {
            ensure!(
                col.tag == "gridCol" && col.wml,
                "Unsupported table grid structure"
            );
            ensure!(
                col.children.is_empty() && col.attr("type")?.is_none(),
                "Unsupported grid column declaration"
            );
            widths.push(number(col.attr("w")?, false)?);
        }
    }
    let rows: Vec<_> = table.children.iter().filter(|r| r.tag == "tr").collect();
    ensure!(!rows.is_empty(), "Table has no rows");
    let mut columns = None;
    let mut cell_properties = 0;
    let mut cell_widths = 0;
    let mut row_properties = 0;
    for row in &rows {
        ensure!(row.wml, "Unsupported table row namespace");
        row_properties += usize::from(row.one("trPr")?.is_some());
        let cells: Vec<_> = row.children.iter().filter(|c| c.tag == "tc").collect();
        ensure!(!cells.is_empty(), "Table row has no cells");
        ensure!(
            columns
                .replace(cells.len())
                .is_none_or(|n| n == cells.len()),
            "Ragged table rows"
        );
        for (index, cell) in cells.iter().enumerate() {
            ensure!(cell.wml, "Unsupported table cell namespace");
            let cell_pr = cell.one("tcPr")?;
            cell_properties += usize::from(cell_pr.is_some());
            let width_node = cell_pr.map(|p| p.one("tcW")).transpose()?.flatten();
            cell_widths += usize::from(width_node.is_some());
            let cell_width = width_node.map(|w| width(w, !fixed)).transpose()?.flatten();
            if fixed {
                ensure!(
                    cell_width.is_some() && cell_width == widths.get(index).copied(),
                    "Fixed cell widths must match the table grid"
                );
            }
        }
    }
    // Relevant declarations must occur exactly where the converter consumes
    // them. Reject a hidden second layout, grid, row or cell under a wrapper.
    for (tag, expected) in [
        ("tblLayout", usize::from(layout.is_some())),
        ("tblGrid", usize::from(grid.is_some())),
        ("tblPr", usize::from(properties.is_some())),
        (
            "tblStyle",
            usize::from(
                properties
                    .map(|p| p.one("tblStyle"))
                    .transpose()?
                    .flatten()
                    .is_some(),
            ),
        ),
        (
            "tblW",
            usize::from(
                properties
                    .map(|p| p.one("tblW"))
                    .transpose()?
                    .flatten()
                    .is_some(),
            ),
        ),
        ("gridCol", widths.len()),
        ("tcPr", cell_properties),
        ("tcW", cell_widths),
        ("trPr", row_properties),
        ("tr", rows.len()),
        ("tc", rows.len() * columns.unwrap_or(0)),
    ] {
        let mut found = Vec::new();
        table.descendants(tag, &mut found);
        ensure!(
            found.len() == expected,
            "Non-direct or conflicting table declaration: {tag}"
        );
    }
    if !fixed {
        return Ok(None);
    }
    ensure!(
        !widths.is_empty() && Some(widths.len()) == columns,
        "Fixed table grid must match every row"
    );
    let total: u64 = widths.iter().sum();
    ensure!(
        total <= crate::document::MAX_TABLE_WIDTH_TWIPS,
        "Fixed table total width exceeds supported bounds"
    );
    ensure!(
        table_width.is_none_or(|width| width == total),
        "Fixed table total conflicts with grid"
    );
    Ok(Some(widths))
}

pub(crate) fn analyze(main: &[u8], styles: &[u8], settings: Option<&[u8]>) -> Result<TableAudit> {
    let main = xml(main)?;
    let mut tables = Vec::new();
    main.descendants("tbl", &mut tables);
    let mut audit = TableAudit::default();
    if tables.is_empty() {
        return Ok(audit);
    }
    let styles = xml(styles)?;
    for table in tables {
        let row_cells = table
            .children
            .iter()
            .filter(|r| r.tag == "tr")
            .map(|r| r.children.iter().filter(|c| c.tag == "tc").count())
            .collect();
        match table_widths(table, &styles) {
            Ok(widths) => {
                let positioning = position(table, &styles, settings, widths.is_some()).and_then(
                    |(alignment, indent, normalized)| {
                        let total = widths.as_ref().map_or(9000, |values| values.iter().sum());
                        ensure!(
                            total + indent.unwrap_or(0) <= crate::document::MAX_TABLE_WIDTH_TWIPS,
                            "Table width and indentation exceed limits"
                        );
                        Ok((alignment, indent, normalized))
                    },
                );
                let (alignment, indent, normalized) = match positioning {
                    Ok(position) => position,
                    Err(_) => {
                        audit.risks.insert(TABLE_GEOMETRY_RISK.into());
                        (None, None, false)
                    }
                };
                if normalized {
                    audit.risks.insert(INHERITED_POSITION_RISK.into());
                }
                if widths.is_none() {
                    audit.risks.insert(AUTOFIT_WIDTHS_RISK.into());
                }
                audit.tables.push(TableGeometry {
                    widths,
                    row_cells,
                    alignment,
                    indent,
                });
            }
            Err(_) => {
                audit.risks.insert(TABLE_GEOMETRY_RISK.into());
                audit.tables.push(TableGeometry {
                    widths: None,
                    alignment: None,
                    indent: None,
                    row_cells,
                });
            }
        }
    }
    Ok(audit)
}
