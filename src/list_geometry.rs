//! A narrow source proof and model contract for physical decimal-list frames.
//! This deliberately does not change the parser's generic numbering cascade.
use crate::{
    document::{Node, PageGeometry},
    paragraph_indentation::{Xml, root, xml},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const WARNING: &str = "Proved decimal list geometry and fixed Latin marker font preserved; named styles and dynamic numbering relationships are not retained";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Geometry {
    pub left_twips: u64,
    pub right_twips: u64,
    pub hanging_twips: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MarkerFont {
    pub font_family: String,
    /// Integer half-points, matching the inline fontSize model.
    pub font_size: u64,
}
#[derive(Default)]
pub(crate) struct Audit {
    blocks: Vec<Block>,
    pub body_indices: BTreeSet<usize>,
}
#[derive(Debug)]
enum Block {
    Paragraph(String),
    List {
        id: u32,
        start: u64,
        font: MarkerFont,
        items: Vec<(String, Geometry)>,
    },
}
fn present(node: &Node) -> bool {
    node.attrs.contains_key("listGeometry")
        || node.attrs.contains_key("markerFont")
        || node.content.iter().any(present)
}
pub(crate) fn geometry(node: &Node) -> Result<Option<Geometry>> {
    node.attrs
        .get("listGeometry")
        .map(|v| serde_json::from_value(v.clone()).context("Invalid list geometry"))
        .transpose()
}
pub(crate) fn marker_font(node: &Node) -> Result<Option<MarkerFont>> {
    node.attrs
        .get("markerFont")
        .map(|v| serde_json::from_value(v.clone()).context("Invalid list marker font"))
        .transpose()
}
fn frame(g: &Geometry, page: &PageGeometry) -> Result<()> {
    ensure!(
        g.left_twips <= 31680 && g.right_twips <= 31680 && g.hanging_twips <= g.left_twips,
        "List geometry exceeds supported bounds"
    );
    ensure!(
        g.left_twips + g.right_twips
            < page.width_twips - page.margin_left_twips - page.margin_right_twips,
        "List frame has no positive body width"
    );
    Ok(())
}
fn model_text(p: &Node) -> Result<String> {
    ensure!(p.kind == "paragraph", "Preserved list requires paragraphs");
    let mut out = String::new();
    for c in &p.content {
        match c.kind.as_str() {
            "text" => out.push_str(c.text.as_deref().unwrap_or("")),
            "hardBreak" => out.push('\n'),
            _ => anyhow::bail!("Unsupported preserved-list inline content"),
        }
    }
    ensure!(
        !out.contains(['\t', '\r']),
        "Tabs in preserved lists are unsupported"
    );
    Ok(out)
}
pub(crate) fn validate(doc: &Node) -> Result<()> {
    if !present(doc) {
        return Ok(());
    }
    let page =
        crate::document::page_geometry(doc)?.context("Preserved lists require an explicit page")?;
    ensure!(
        !doc.attrs.contains_key("markerFont") && !doc.attrs.contains_key("listGeometry"),
        "Misplaced list metadata"
    );
    for list in &doc.content {
        if !present(list) {
            continue;
        }
        ensure!(
            list.kind == "orderedList" && !list.attrs.contains_key("listGeometry"),
            "List geometry requires a direct body ordered list"
        );
        let font = marker_font(list)?.context("Preserved list requires marker font")?;
        ensure!(
            crate::document::supported_font_family(&font.font_family)
                && (2..=3276).contains(&font.font_size),
            "Invalid list marker typography"
        );
        ensure!(!list.content.is_empty(), "Preserved list is empty");
        for item in &list.content {
            ensure!(
                item.kind == "listItem"
                    && item.content.len() == 1
                    && !item.attrs.contains_key("markerFont")
                    && !item.attrs.contains_key("listGeometry"),
                "Preserved list requires single-paragraph items"
            );
            let p = &item.content[0];
            model_text(p)?;
            let g = geometry(p)?.context("Preserved list item requires complete geometry")?;
            ensure!(
                !p.attrs.contains_key("markerFont")
                    && !p.attrs.contains_key("emptyParagraphFont")
                    && crate::document::PARAGRAPH_INDENT_FIELDS
                        .iter()
                        .all(|k| !p.attrs.contains_key(*k)),
                "Conflicting list paragraph metadata"
            );
            ensure!(
                p.attrs
                    .get("textAlign")
                    .is_none_or(|v| v.is_null() || v.as_str() == Some("left")),
                "Preserved lists require left paragraph alignment"
            );
            ensure!(!p.content.iter().any(present), "Misplaced list metadata");
            frame(&g, &page)?;
        }
    }
    Ok(())
}
fn number(v: &str, max: u64) -> Result<u64> {
    ensure!(
        !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()),
        "Noninteger list value"
    );
    let n = v.parse()?;
    ensure!(n <= max, "List value exceeds bounds");
    Ok(n)
}
fn value(n: &Xml) -> Result<&str> {
    n.schema(&["val"], true)?;
    n.attr("val")?.context("Missing list value")
}
fn select<'a>(parent: &'a Xml, tag: &str, attr: &str, id: u32) -> Result<&'a Xml> {
    let found: Vec<_> = parent
        .all(tag)
        .into_iter()
        .filter(|n| {
            n.has_number(attr, id)
                || n.attrs
                    .get(&format!("w:{attr}"))
                    .is_some_and(|(v, _)| v.parse::<u32>().unwrap_or(0) == id)
        })
        .collect();
    ensure!(
        found.len() == 1,
        "Missing or ambiguous active list identity"
    );
    let n = found[0];
    n.identity()?;
    ensure!(
        parent.children.iter().any(|c| std::ptr::eq(c, n))
            && number(
                n.attr(attr)?.context("Missing list identity")?,
                u32::MAX as u64
            )? == u64::from(id),
        "Wrapped or malformed list identity"
    );
    Ok(n)
}
fn text(n: &Xml, out: &mut String) -> Result<()> {
    match n.tag.as_str() {
        "pPr" | "rPr" => return Ok(()),
        "p" | "hyperlink" => n.identity()?,
        "r" => {
            n.identity()?;
            ensure!(
                n.children
                    .iter()
                    .any(|c| c.tag == "t" && !c.text.is_empty()),
                "Empty source list runs have unproved baseline metrics"
            );
        }
        "t" => {
            n.identity()?;
            ensure!(
                n.children.is_empty() && !n.text.contains(['\t', '\r', '\n']),
                "Unsupported list text"
            );
            out.push_str(&n.text);
            return Ok(());
        }
        "br" => anyhow::bail!("Source list breaks have unproved baseline typography"),
        "bookmarkStart" | "bookmarkEnd" | "proofErr" | "lastRenderedPageBreak" => {
            n.identity()?;
            ensure!(n.children.is_empty(), "Nested list marker");
            return Ok(());
        }
        _ => anyhow::bail!("Unsupported list paragraph structure: {}", n.tag),
    }
    ensure!(n.text.trim().is_empty(), "Uncontained list text");
    for c in &n.children {
        text(c, out)?;
    }
    Ok(())
}
#[derive(Default)]
struct Props {
    geometry: Option<Geometry>,
    number: Option<(u32, u8)>,
    style: Option<String>,
}
fn props(n: Option<&Xml>) -> Result<Props> {
    let Some(n) = n else {
        return Ok(Props::default());
    };
    n.schema(&[], false)?;
    ensure!(
        n.children.iter().all(|c| matches!(
            c.tag.as_str(),
            "pStyle" | "numPr" | "ind" | "spacing" | "contextualSpacing" | "jc"
        )),
        "Unsupported preserved list paragraph properties"
    );
    let mut p = Props::default();
    if let Some(s) = n.one("pStyle")? {
        p.style = Some(value(s)?.into());
    }
    if let Some(i) = n.one("ind")? {
        i.schema(&["left", "right", "hanging"], true)?;
        p.geometry = Some(Geometry {
            left_twips: number(i.attr("left")?.context("Partial list indentation")?, 31680)?,
            right_twips: number(i.attr("right")?.context("Partial list indentation")?, 31680)?,
            hanging_twips: number(
                i.attr("hanging")?.context("Partial list indentation")?,
                31680,
            )?,
        });
    }
    if let Some(j) = n.one("jc")? {
        ensure!(value(j)? == "left", "Unsupported list alignment");
    }
    if let Some(num) = n.one("numPr")? {
        num.schema(&[], false)?;
        ensure!(
            num.children
                .iter()
                .all(|n| matches!(n.tag.as_str(), "numId" | "ilvl")),
            "Unsupported list numbering property"
        );
        let id = number(
            value(num.one("numId")?.context("Partial list reference")?)?,
            u32::MAX as u64,
        )? as u32;
        let level = num
            .one("ilvl")?
            .map(value)
            .transpose()?
            .map(|v| number(v, 8))
            .transpose()?
            .unwrap_or(0) as u8;
        p.number = Some((id, level));
    }
    Ok(p)
}

fn settings(data: Option<&[u8]>) -> Result<()> {
    let Some(data) = data else {
        return Ok(());
    };
    let tree = xml(data)?;
    let settings = root(&tree, "settings")?;
    let tab = settings.one("defaultTabStop")?;
    ensure!(
        settings.all("defaultTabStop").len() == usize::from(tab.is_some()),
        "Wrapped default list tab stop"
    );
    if let Some(tab) = tab {
        ensure!(
            number(value(tab)?, 31680)? == 720,
            "Nonstandard default list tab stop"
        );
    }
    let compat = settings.one("compat")?;
    ensure!(
        settings.all("compat").len() == usize::from(compat.is_some()),
        "Wrapped list compatibility settings"
    );
    if let Some(compat) = compat {
        compat.schema(&[], false)?;
        let mut seen = BTreeSet::new();
        for entry in &compat.children {
            ensure!(
                entry.tag == "compatSetting",
                "Unproved numbering/tab compatibility flag"
            );
            entry.schema(&["name", "uri", "val"], true)?;
            let name = entry.attr("name")?.context("Missing compatibility name")?;
            ensure!(
                seen.insert(name)
                    && entry.attr("uri")? == Some("http://schemas.microsoft.com/office/word"),
                "Ambiguous compatibility setting"
            );
            let expected = match name {
                "compatibilityMode" => "15",
                "overrideTableStyleFontSizeAndJustification"
                | "enableOpenTypeFeatures"
                | "doNotFlipMirrorIndents"
                | "differentiateMultirowTableHeaders" => "1",
                "useWord2013TrackBottomHyphenation" => "0",
                _ => anyhow::bail!("Unproved list compatibility setting"),
            };
            ensure!(
                entry.attr("val")? == Some(expected),
                "Unproved list compatibility value"
            );
        }
    }
    for tag in ["doNotUseIndentAsNumberingTabStop", "noTabHangInd"] {
        ensure!(
            settings.all(tag).is_empty(),
            "Unproved list tab compatibility flag"
        );
    }
    Ok(())
}

/// Failure here never admits a source. The old indentation guard remains in
/// force unless this entire direct-body identity and geometry proof succeeds.
pub(crate) fn analyze(
    main: &[u8],
    styles: &[u8],
    numbers: Option<&[u8]>,
    theme: Option<&[u8]>,
    config: Option<&[u8]>,
    page: Option<&PageGeometry>,
) -> Result<Audit> {
    let page = page.context("List preservation requires a proved page")?;
    settings(config)?;
    let m = xml(main)?;
    let body = root(&m, "document")?
        .one("body")?
        .context("Missing list body")?;
    let s = xml(styles)?;
    let sheet = root(&s, "styles")?;
    let nums = xml(numbers.context("Missing list numbering")?)?;
    let nums = root(&nums, "numbering")?;
    let mut index = BTreeMap::new();
    let mut default = None;
    ensure!(
        sheet.all("style").len() == sheet.children.iter().filter(|n| n.tag == "style").count(),
        "Wrapped list styles"
    );
    for style in sheet.children.iter().filter(|n| n.tag == "style") {
        style.identity()?;
        let id = style.attr("styleId")?.context("Missing style identity")?;
        ensure!(
            index.insert(id, style).is_none(),
            "Ambiguous style identity"
        );
        if style.attr("type")?.unwrap_or("paragraph") == "paragraph"
            && matches!(style.attr("default")?, Some("1" | "true" | "on"))
        {
            ensure!(default.replace(id).is_none(), "Ambiguous default style");
        }
    }
    let defaults = sheet.one("docDefaults")?;
    ensure!(
        sheet.all("docDefaults").len() == usize::from(defaults.is_some()),
        "Wrapped defaults"
    );
    let dp = defaults
        .map(|n| n.one("pPrDefault"))
        .transpose()?
        .flatten()
        .map(|n| n.one("pPr"))
        .transpose()?
        .flatten();
    let base = props(dp)?;
    ensure!(
        base.number.is_none() && base.style.is_none(),
        "Numbered document defaults unsupported"
    );
    let mut audit = Audit::default();
    let mut closed = BTreeSet::new();
    let mut indices = Vec::new();
    let mut pending = Vec::new();
    let mut work = 0usize;
    for (position, p) in body.children.iter().enumerate() {
        if p.tag == "sectPr" {
            p.identity()?;
            continue;
        }
        ensure!(
            p.tag == "p",
            "Only direct body paragraphs have list identity proof"
        );
        let own = props(p.one("pPr")?)?;
        let mut chain = Vec::new();
        let mut seen = BTreeSet::new();
        let mut next = own.style.as_deref().or(default);
        while let Some(id) = next {
            ensure!(
                seen.insert(id) && seen.len() <= 20,
                "Cyclic list style chain"
            );
            let style = *index.get(id).context("Unresolved list style chain")?;
            ensure!(
                style.attr("type")?.unwrap_or("paragraph") == "paragraph",
                "Unsupported list style kind"
            );
            work += style.size;
            ensure!(work <= 5_000_000, "List source audit exceeds work budget");
            chain.push(style);
            next = style.one("basedOn")?.map(value).transpose()?;
        }
        chain.reverse();
        let mut geometry = base.geometry.clone();
        let mut reference = None;
        for layer in chain
            .iter()
            .map(|n| n.one("pPr"))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .chain([p.one("pPr")?])
        {
            let v = props(layer)?;
            if v.geometry.is_some() {
                geometry = v.geometry;
            }
            if v.number.is_some() {
                reference = v.number;
            }
        }
        let mut content = String::new();
        text(p, &mut content)?;
        if let Some((id, level)) = reference.filter(|(id, _)| *id != 0) {
            ensure!(level == 0, "Only level-zero lists are preserved");
            ensure!(
                !content.trim().is_empty(),
                "Empty source list items have unproved baseline typography"
            );
            let g = geometry.context("Missing complete physical list indentation")?;
            frame(&g, page)?;
            work = work
                .checked_add(3 * nums.size)
                .context("List numbering work overflow")?;
            ensure!(
                work <= 5_000_000,
                "List numbering proof exceeds work budget"
            );
            let instance = select(nums, "num", "numId", id)?;
            instance.schema(&["numId"], false)?;
            ensure!(
                instance.children.iter().all(|n| n.tag == "abstractNumId"),
                "List overrides/restarts unsupported"
            );
            let abs_id = number(
                value(
                    instance
                        .one("abstractNumId")?
                        .context("Missing abstract list reference")?,
                )?,
                u32::MAX as u64,
            )? as u32;
            let abs = select(nums, "abstractNum", "abstractNumId", abs_id)?;
            ensure!(
                abs.children
                    .iter()
                    .all(|n| matches!(n.tag.as_str(), "nsid" | "multiLevelType" | "tmpl" | "lvl")),
                "Linked or extended numbering unsupported"
            );
            for (key, (v, _)) in &abs.attrs {
                ensure!(
                    key == "w:abstractNumId"
                        || key == "w15:restartNumberingAfterBreak"
                            && v == "0"
                            && abs
                                .attribute_namespaces
                                .get(key)
                                .is_some_and(|ns| ns
                                    == "http://schemas.microsoft.com/office/word/2012/wordml"),
                    "Unsupported abstract list attribute"
                );
            }
            let lvl = select(abs, "lvl", "ilvl", 0)?;
            lvl.schema(&["ilvl", "tplc"], false)?;
            ensure!(
                lvl.children.iter().all(|n| matches!(
                    n.tag.as_str(),
                    "start" | "numFmt" | "pStyle" | "lvlText" | "lvlJc" | "suff" | "pPr"
                )),
                "Unsupported active numbering level"
            );
            ensure!(
                value(lvl.one("numFmt")?.context("Missing decimal format")?)? == "decimal"
                    && value(lvl.one("lvlText")?.context("Missing list pattern")?)? == "%1.",
                "Only decimal-dot numbering is preserved"
            );
            ensure!(
                lvl.one("lvlJc")?.map(value).transpose()?.unwrap_or("left") == "left"
                    && lvl.one("suff")?.map(value).transpose()?.unwrap_or("tab") == "tab",
                "Unsupported marker alignment or suffix"
            );
            let association = value(
                lvl.one("pStyle")?
                    .context("Missing numbering style association")?,
            )?;
            ensure!(
                chain
                    .iter()
                    .filter(|n| n.has_value("styleId", association))
                    .count()
                    == 1,
                "Unproved numbering style association"
            );
            let mut associated_number = None;
            for style in &chain {
                if let Some(reference) = props(style.one("pPr")?)?.number {
                    associated_number = Some(reference);
                }
                if style.has_value("styleId", association) {
                    break;
                }
            }
            ensure!(
                associated_number == Some((id, 0)),
                "Associated style does not resolve the active numbering instance"
            );
            let associations: Vec<_> = abs
                .all("pStyle")
                .into_iter()
                .filter(|n| n.has_value("val", association))
                .collect();
            ensure!(
                associations.len() == 1,
                "Ambiguous numbering style association"
            );
            let start = number(
                value(lvl.one("start")?.context("Missing list start")?)?,
                1_000_000,
            )?;
            ensure!(start > 0, "Invalid list start");
            if let Some(pp) = lvl.one("pPr")? {
                pp.schema(&[], false)?;
                ensure!(
                    pp.children.iter().all(|n| n.tag == "ind"),
                    "Unsupported numbering paragraph property"
                );
                if let Some(ind) = pp.one("ind")? {
                    ind.schema(&["left", "right", "hanging"], true)?;
                    for name in ["left", "right", "hanging"] {
                        if let Some(v) = ind.attr(name)? {
                            number(v, 31680)?;
                        }
                    }
                }
            }
            indices.push(position);
            audit.body_indices.insert(position);
            pending.push((position, id, start, content, g));
        } else {
            pending.push((
                position,
                0,
                0,
                content,
                Geometry {
                    left_twips: 0,
                    right_twips: 0,
                    hanging_twips: 0,
                },
            ));
        }
    }
    ensure!(!indices.is_empty(), "No preserved list candidates");
    let fonts = crate::image_dimensions::resolve_list_marker_fonts(main, styles, theme, &indices)?;
    let mut fonts = fonts.into_iter();
    for (_, id, start, text, g) in pending {
        if id == 0 {
            if let Some(Block::List { id, .. }) = audit.blocks.last() {
                closed.insert(*id);
            }
            audit.blocks.push(Block::Paragraph(text));
            continue;
        }
        let font = fonts.next().context("Missing marker font proof")?;
        if let Some(Block::List {
            id: previous,
            font: old,
            items,
            ..
        }) = audit.blocks.last_mut()
            && *previous == id
        {
            ensure!(*old == font, "Mixed list marker typography unsupported");
            items.push((text, g));
        } else {
            if let Some(Block::List { id, .. }) = audit.blocks.last() {
                closed.insert(*id);
            }
            ensure!(
                !closed.contains(&id),
                "Resumed numbering instance unsupported"
            );
            audit.blocks.push(Block::List {
                id,
                start,
                font,
                items: vec![(text, g)],
            });
        }
    }
    Ok(audit)
}

pub(crate) fn attach(audit: &Audit, doc: &mut Node, risks: &mut BTreeSet<String>) -> Result<()> {
    if audit.body_indices.is_empty() {
        return Ok(());
    }
    ensure!(
        doc.content.len() == audit.blocks.len(),
        "Converted list body block count differs"
    );
    for (block, proof) in doc.content.iter_mut().zip(&audit.blocks) {
        match proof {
            Block::Paragraph(text) => ensure!(
                model_text(block)? == *text,
                "Converted body identity differs from list source"
            ),
            Block::List {
                start, font, items, ..
            } => {
                ensure!(
                    block.kind == "orderedList"
                        && block
                            .attrs
                            .get("start")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(1)
                            == *start
                        && block.content.len() == items.len(),
                    "Converted numbering structure differs from list source"
                );
                for (item, (text, g)) in block.content.iter_mut().zip(items) {
                    ensure!(
                        item.kind == "listItem"
                            && item.content.len() == 1
                            && model_text(&item.content[0])? == *text,
                        "Converted list item identity differs from source"
                    );
                    item.content[0]
                        .attrs
                        .insert("listGeometry".into(), serde_json::to_value(g)?);
                }
                block
                    .attrs
                    .insert("markerFont".into(), serde_json::to_value(font)?);
            }
        }
    }
    validate(doc)?;
    risks.remove("List marker appearance normalized to editor defaults");
    risks.remove("List numbering indentation normalized to editor defaults");
    risks.insert(WARNING.into());
    Ok(())
}
