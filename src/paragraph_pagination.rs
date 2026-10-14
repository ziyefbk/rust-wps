//! Bounded paragraph-pagination proof. Named styles are flattened only after
//! strict source identity/cascade checks and a complete body-order binding.
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

pub(crate) const FIELDS: [&str; 3] = ["keepNext", "keepLines", "pageBreakBefore"];
pub(crate) const RISK: &str = "Unsupported or unproved paragraph pagination";
pub(crate) const WARNING: &str = "Top-level paragraph pagination preserved as fixed values; named style inheritance and automatic page layout are not retained";
pub(crate) const NORMALIZED: &str = "Pagination in lists, tables, images and hard-break-split paragraphs is outside the supported subset and may be normalized";

#[derive(Clone, Default, Debug)]
struct Properties {
    flags: [Option<bool>; 3],
    number: Option<u32>,
    outline: Option<u8>,
}
impl Properties {
    fn overlay(&mut self, other: &Self) {
        for (left, right) in self.flags.iter_mut().zip(other.flags) {
            if right.is_some() {
                *left = right;
            }
        }
        if other.number.is_some() {
            self.number = other.number;
        }
        if other.outline.is_some() {
            self.outline = other.outline;
        }
    }
    fn values(&self) -> [bool; 3] {
        self.flags.map(|v| v.unwrap_or(false))
    }
}
fn boolean(value: &str) -> Result<bool> {
    match value {
        "1" | "true" | "on" => Ok(true),
        "0" | "false" | "off" => Ok(false),
        _ => anyhow::bail!("Invalid pagination boolean"),
    }
}
fn value(n: &Xml) -> Result<&str> {
    n.schema(&["val"], true)?;
    n.attr("val")?
        .filter(|v| !v.is_empty())
        .context("Missing pagination property value")
}
fn direct<'a>(n: &'a Xml, name: &str) -> Result<Option<&'a Xml>> {
    let one = n.one(name)?;
    ensure!(
        n.all(name).len() == usize::from(one.is_some()),
        "Non-direct pagination property: {name}"
    );
    Ok(one)
}
fn properties(n: Option<&Xml>) -> Result<Properties> {
    let Some(n) = n else {
        return Ok(Properties::default());
    };
    n.identity()?;
    ensure!(
        n.text.trim().is_empty(),
        "Uncontained paragraph property text"
    );
    let mut out = Properties::default();
    for (i, name) in FIELDS.iter().enumerate() {
        if let Some(flag) = direct(n, name)? {
            flag.schema(&["val"], true)?;
            out.flags[i] = Some(flag.attr("val")?.map(boolean).transpose()?.unwrap_or(true));
        }
    }
    if let Some(number) = direct(n, "numPr")?
        && let Some(id) = direct(number, "numId")?
    {
        let raw = value(id)?;
        ensure!(
            raw.bytes().all(|b| b.is_ascii_digit()),
            "Invalid pagination numbering identity"
        );
        out.number = Some(raw.parse()?);
    }
    if let Some(level) = direct(n, "outlineLvl")? {
        let raw = value(level)?;
        ensure!(
            raw.bytes().all(|b| b.is_ascii_digit()),
            "Invalid pagination outline identity"
        );
        let level: u8 = raw.parse()?;
        ensure!(level <= 9, "Invalid pagination outline level");
        out.outline = Some(level);
    }
    Ok(out)
}
fn enclosing_properties(n: &Xml) -> Result<Properties> {
    let p = direct(n, "pPr")?;
    for name in FIELDS {
        ensure!(
            n.all(name).len() == p.map_or(0, |p| p.all(name).len()),
            "Misplaced active pagination declaration: {name}"
        );
    }
    properties(p)
}
struct Styles<'a> {
    root: &'a Xml,
    index: BTreeMap<String, Vec<&'a Xml>>,
    defaults: Properties,
    chains: BTreeMap<String, Vec<&'a Xml>>,
    default_id: Option<Option<String>>,
    work: usize,
    properties: BTreeMap<usize, Properties>,
}
impl<'a> Styles<'a> {
    fn new(root: &'a Xml) -> Result<Self> {
        let mut index = BTreeMap::<String, Vec<&Xml>>::new();
        for s in root.all("style") {
            let ids: BTreeSet<_> = s
                .attrs
                .iter()
                .filter(|(k, _)| k.rsplit(':').next() == Some("styleId"))
                .map(|(_, (v, _))| v.clone())
                .collect();
            for id in ids {
                index.entry(id).or_default().push(s);
            }
        }
        let mut defaults = Properties::default();
        if let Some(n) = direct(root, "docDefaults")? {
            let paragraph = direct(n, "pPrDefault")?;
            if let Some(p) = paragraph {
                defaults = enclosing_properties(p)?;
            }
            for name in FIELDS {
                ensure!(
                    n.all(name).len() == paragraph.map_or(0, |p| p.all(name).len()),
                    "Misplaced document pagination default"
                );
            }
        }
        Ok(Self {
            root,
            index,
            defaults,
            chains: BTreeMap::new(),
            default_id: None,
            work: 0,
            properties: BTreeMap::new(),
        })
    }
    fn default(&mut self) -> Result<Option<String>> {
        if let Some(id) = &self.default_id {
            return Ok(id.clone());
        }
        let mut found = None;
        for s in self.root.all("style") {
            if !s
                .attrs
                .keys()
                .any(|k| k.rsplit(':').next() == Some("default"))
            {
                continue;
            }
            if !boolean(s.attr("default")?.context("Missing default flag")?)? {
                continue;
            }
            match s.attr("type")? {
                Some("character" | "table" | "numbering") => continue,
                Some("paragraph") => {}
                _ => anyhow::bail!("Ambiguous default pagination style type"),
            }
            s.identity()?;
            ensure!(found.is_none(), "Multiple default paragraph styles");
            found = Some(
                s.attr("styleId")?
                    .filter(|v| !v.is_empty())
                    .context("Missing default paragraph style identity")?
                    .to_owned(),
            );
        }
        self.default_id = Some(found.clone());
        Ok(found)
    }
    fn chain(&mut self, id: Option<&str>) -> Result<Vec<&'a Xml>> {
        let Some(id) = id else {
            return Ok(Vec::new());
        };
        if let Some(chain) = self.chains.get(id) {
            return Ok(chain.clone());
        }
        let mut chain = Vec::new();
        let mut seen = BTreeSet::new();
        let mut next = Some(id.to_owned());
        while let Some(id) = next {
            self.work += 1;
            ensure!(
                self.work <= 100_000 && chain.len() < 20 && seen.insert(id.clone()),
                "Pagination style chain exceeds bounds or is cyclic"
            );
            let found = self
                .index
                .get(&id)
                .context("Missing active paragraph style")?;
            ensure!(
                found.len() == 1,
                "Duplicate active paragraph style identity"
            );
            let s = found[0];
            s.identity()?;
            ensure!(
                self.root.children.iter().any(|n| std::ptr::eq(n, s))
                    && s.attr("styleId")? == Some(id.as_str())
                    && s.attr("type")? == Some("paragraph"),
                "Unsupported active paragraph style identity"
            );
            let key = s as *const Xml as usize;
            if !self.properties.contains_key(&key) {
                self.work += s.size;
                ensure!(
                    self.work <= 100_000,
                    "Pagination style proof exceeds work budget"
                );
                self.properties.insert(key, enclosing_properties(s)?);
            }
            next = direct(s, "basedOn")?
                .map(value)
                .transpose()?
                .map(str::to_owned);
            chain.push(s);
        }
        chain.reverse();
        self.chains.insert(id.to_owned(), chain.clone());
        Ok(chain)
    }
    fn paragraph(&mut self, p: &Xml) -> Result<(Properties, Option<u8>)> {
        p.identity()?;
        let own = enclosing_properties(p)?;
        let ppr = p.one("pPr")?;
        let explicit = ppr
            .map(|n| direct(n, "pStyle"))
            .transpose()?
            .flatten()
            .map(value)
            .transpose()?;
        ensure!(
            p.all("pStyle").len() == usize::from(explicit.is_some()),
            "Misplaced paragraph style reference"
        );
        let id = match explicit {
            Some(id) => Some(id.to_owned()),
            None => self.default()?,
        };
        let chain = self.chain(id.as_deref())?;
        let mut result = self.defaults.clone();
        let mut style_outline = None;
        for style in &chain {
            let props = &self.properties[&(*style as *const Xml as usize)];
            if props.outline.is_some() {
                style_outline = props.outline;
            }
            result.overlay(props);
        }
        result.overlay(&own);
        // This is the existing importer heading classification, independently
        // derived from source identities. It does not promote implicit defaults.
        let mut heading = own.outline;
        if heading.is_none() && explicit.is_some() {
            heading = style_outline;
        }
        if heading.is_none()
            && let Some(id) = explicit
        {
            heading = heading_name(id);
            if heading.is_none()
                && let Some(leaf) = chain.last()
            {
                heading = direct(leaf, "name")?
                    .map(value)
                    .transpose()?
                    .and_then(heading_name);
            }
        }
        Ok((result, heading.map(|n| (n + 1).min(6))))
    }
}
fn heading_name(value: &str) -> Option<u8> {
    let lower = value.trim().to_ascii_lowercase();
    let n: u8 = lower.strip_prefix("heading")?.trim().parse().ok()?;
    (1..=9).contains(&n).then(|| n - 1)
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    Paragraph {
        text: String,
        level: Option<u8>,
        nested: bool,
    },
    PageBreak,
    ColumnBreak,
    Image,
    Table,
}
#[derive(Clone, Debug)]
struct Unit {
    kind: Kind,
    flags: Option<[bool; 3]>,
}
#[derive(Default)]
pub(crate) struct Audit {
    units: Vec<Unit>,
    pub warnings: Vec<String>,
}
fn inline(n: &Xml, text: &mut String, segments: &mut Vec<(String, u8)>) -> Result<()> {
    match n.tag.as_str() {
        "pPr" | "rPr" => return Ok(()),
        "p" | "r" | "hyperlink" => n.identity()?,
        "t" => {
            n.identity()?;
            ensure!(n.children.is_empty(), "Nested pagination text");
            text.push_str(&n.text);
            return Ok(());
        }
        "tab" => {
            n.schema(&[], true)?;
            text.push('\t');
            return Ok(());
        }
        "br" => {
            n.schema(&["type", "clear"], true)?;
            ensure!(
                n.attr("clear")?.is_none_or(|v| v == "none"),
                "Clearing break is unsupported for pagination"
            );
            match n.attr("type")? {
                None | Some("textWrapping") => text.push('\n'),
                Some("page") => segments.push((std::mem::take(text), 1)),
                Some("column") => segments.push((std::mem::take(text), 2)),
                _ => anyhow::bail!("Invalid pagination break type"),
            }
            return Ok(());
        }
        "lastRenderedPageBreak" => {
            n.schema(&[], true)?;
            return Ok(());
        }
        "bookmarkStart" | "bookmarkEnd" | "proofErr" => {
            n.identity()?;
            ensure!(n.children.is_empty(), "Nested paragraph marker");
            return Ok(());
        }
        _ => anyhow::bail!("Unsupported pagination paragraph content: {}", n.tag),
    }
    ensure!(
        n.text.trim().is_empty(),
        "Uncontained pagination paragraph text"
    );
    for c in &n.children {
        inline(c, text, segments)?;
    }
    Ok(())
}
fn analyze_xml(main: &[u8], styles: &[u8]) -> Result<Audit> {
    let main = xml(main)?;
    let body = direct(root(&main, "document")?, "body")?.context("Missing pagination body")?;
    let styles = xml(styles)?;
    let mut styles = Styles::new(root(&styles, "styles")?)?;
    let mut audit = Audit::default();
    let mut excluded = false;
    for block in &body.children {
        block.identity()?;
        match block.tag.as_str() {
            "sectPr" => {}
            "tbl" => {
                // Existing table proof owns geometry. These paragraphs are
                // deliberately not metadata targets, but malformed active
                // paragraph flags cannot escape through their context.
                for p in block.all("p") {
                    styles.paragraph(p)?;
                }
                audit.units.push(Unit {
                    kind: Kind::Table,
                    flags: None,
                });
                excluded = true;
            }
            "p" => {
                let (props, level) = styles.paragraph(block)?;
                let nested = props.number.is_some_and(|n| n != 0) && level.is_none();
                if !block.all("drawing").is_empty() {
                    audit.units.push(Unit {
                        kind: Kind::Image,
                        flags: None,
                    });
                    excluded = true;
                    continue;
                }
                let mut text = String::new();
                let mut segments = Vec::new();
                inline(block, &mut text, &mut segments)?;
                let split = !segments.is_empty();
                ensure!(
                    !split || level.is_none(),
                    "Hard-break headings have no supported pagination identity"
                );
                segments.push((text, 0));
                excluded |= nested || split;
                for (text, page_break) in segments {
                    if !split || !text.is_empty() {
                        audit.units.push(Unit {
                            kind: Kind::Paragraph {
                                text,
                                level,
                                nested,
                            },
                            flags: (!nested && !split).then(|| props.values()),
                        });
                    }
                    if page_break != 0 {
                        audit.units.push(Unit {
                            kind: if page_break == 1 {
                                Kind::PageBreak
                            } else {
                                Kind::ColumnBreak
                            },
                            flags: None,
                        });
                    }
                }
            }
            _ => anyhow::bail!("Unsupported pagination body wrapper: {}", block.tag),
        }
    }
    if audit.units.iter().any(|u| u.flags.is_some()) {
        audit.warnings.push(WARNING.into());
    }
    if excluded {
        audit.warnings.push(NORMALIZED.into());
    }
    Ok(audit)
}
pub(crate) fn analyze(bytes: &[u8]) -> Result<Audit> {
    let mut package = OpcReader::new(Cursor::new(bytes))?;
    let main = package.main_document_part()?;
    ensure!(
        main.as_str() == "/word/document.xml",
        "Unsupported pagination main part"
    );
    let rels_part = office_oxide::core::opc::PartName::new(&main.rels_path())?;
    if package.has_part(&rels_part) {
        crate::paragraph_indentation::validate_relationships(&package.read_part(&rels_part)?)?;
    }
    let relationships = package.read_rels_for(&main)?;
    ensure!(
        relationships.duplicate_ids().is_empty(),
        "Ambiguous pagination relationships"
    );
    let selected = relationships.get_by_type(rel_types::STYLES);
    ensure!(selected.len() <= 1, "Ambiguous pagination styles part");
    let styles = if let Some(r) = selected.first() {
        ensure!(
            r.target_mode == TargetMode::Internal,
            "External pagination styles part"
        );
        package.read_part(&main.resolve_relative(&r.target)?)?
    } else {
        br#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#
            .to_vec()
    };
    analyze_xml(&package.read_part(&main)?, &styles)
}
fn ir_units(
    elements: &[Element],
    nested: bool,
    out: &mut Vec<(Kind, Option<[bool; 3]>)>,
) -> Result<()> {
    for element in elements {
        let unit = match element {
            Element::Paragraph(p) => (
                Kind::Paragraph {
                    text: inline_to_text(&p.content),
                    level: None,
                    nested,
                },
                Some([p.keep_with_next, p.keep_together, p.page_break_before]),
            ),
            Element::Heading(p) => (
                Kind::Paragraph {
                    text: inline_to_text(&p.content),
                    level: Some(p.level),
                    nested,
                },
                Some([p.keep_with_next, p.keep_together, p.page_break_before]),
            ),
            Element::List(l) => {
                for i in &l.items {
                    ir_units(&i.content, true, out)?;
                    if let Some(l) = &i.nested {
                        ir_units(&[Element::List(l.clone())], true, out)?;
                    }
                }
                continue;
            }
            Element::PageBreak => (Kind::PageBreak, None),
            Element::ColumnBreak => (Kind::ColumnBreak, None),
            Element::Image(_) => (Kind::Image, None),
            Element::Table(_) => (Kind::Table, None),
            _ => anyhow::bail!("Unsupported converted pagination block"),
        };
        out.push(unit);
    }
    Ok(())
}
pub(crate) fn verify_ir(audit: &Audit, ir: &DocumentIR) -> Result<()> {
    let mut actual = Vec::new();
    for section in &ir.sections {
        ir_units(&section.elements, false, &mut actual)?;
    }
    let mut at = 0;
    for unit in &audit.units {
        // The existing image conversion emits an empty placeholder before a
        // separately proven image. Only this exact adjacent pair is skipped.
        if unit.kind == Kind::Image
            && matches!(actual.get(at), Some((Kind::Paragraph { text, level: None, nested: false }, _)) if text.is_empty())
            && matches!(actual.get(at + 1), Some((Kind::Image, _)))
        {
            at += 1;
        }
        let (kind, flags) = actual
            .get(at)
            .context("Pagination source block discarded")?;
        ensure!(
            *kind == unit.kind && unit.flags.is_none_or(|expected| *flags == Some(expected)),
            "Pagination source identity or values differ from parser IR"
        );
        at += 1;
    }
    ensure!(
        at == actual.len(),
        "Unaudited pagination block in parser IR"
    );
    Ok(())
}
fn model_units<'a>(nodes: &'a [Node], nested: bool, out: &mut Vec<(Kind, &'a Node)>) -> Result<()> {
    for n in nodes {
        let kind = match n.kind.as_str() {
            "paragraph" | "heading" => {
                let mut text = String::new();
                for c in &n.content {
                    match c.kind.as_str() {
                        "text" => text.push_str(c.text.as_deref().unwrap_or("")),
                        "hardBreak" => text.push('\n'),
                        _ => anyhow::bail!("Unsupported pagination model inline"),
                    }
                }
                Kind::Paragraph {
                    text,
                    level: if n.kind == "heading" {
                        Some(
                            n.attrs
                                .get("level")
                                .and_then(serde_json::Value::as_u64)
                                .context("Missing heading identity")?
                                .try_into()?,
                        )
                    } else {
                        None
                    },
                    nested,
                }
            }
            "orderedList" | "bulletList" => {
                for item in &n.content {
                    model_units(&item.content, true, out)?;
                }
                continue;
            }
            "pageBreak" => Kind::PageBreak,
            "image" => Kind::Image,
            "table" => Kind::Table,
            _ => anyhow::bail!("Unsupported pagination model block"),
        };
        out.push((kind, n));
    }
    Ok(())
}
pub(crate) fn apply(audit: &Audit, doc: &mut Node) -> Result<()> {
    if audit.units.is_empty() {
        ensure!(
            doc.content.len() == 1
                && doc.content[0].kind == "paragraph"
                && doc.content[0].content.is_empty()
                && doc.content[0].attrs.is_empty(),
            "Empty source differs from the single importer placeholder"
        );
        return Ok(());
    }
    let mut actual = Vec::new();
    model_units(&doc.content, false, &mut actual)?;
    ensure!(
        actual.len() == audit.units.len()
            && actual
                .iter()
                .zip(&audit.units)
                .all(|((kind, _), proof)| *kind == proof.kind
                    || (proof.kind == Kind::ColumnBreak
                        && *kind
                            == Kind::Paragraph {
                                text: String::new(),
                                level: None,
                                nested: false
                            })),
        "Pagination model body identity differs from source"
    );
    let targets: BTreeMap<_, _> = actual
        .iter()
        .zip(&audit.units)
        .filter_map(|((_, n), u)| u.flags.map(|flags| (*n as *const Node as usize, flags)))
        .collect();
    // Targets are exclusively direct body blocks; do not attach to same-text
    // nested paragraphs or to image placeholders.
    for n in &mut doc.content {
        if let Some(flags) = targets.get(&(n as *const Node as usize)) {
            for (key, value) in FIELDS.into_iter().zip(flags) {
                n.attrs.insert(key.into(), (*value).into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    fn audit(body: &str, styles: &str) -> Result<Audit> {
        analyze_xml(
            format!("<w:document xmlns:w=\"{W}\"><w:body>{body}</w:body></w:document>").as_bytes(),
            format!("<w:styles xmlns:w=\"{W}\">{styles}</w:styles>").as_bytes(),
        )
    }
    fn p(props: &str) -> String {
        format!("<w:p><w:pPr>{props}</w:pPr><w:r><w:t>same</w:t></w:r></w:p>")
    }
    fn style(id: &str, base: Option<&str>, props: &str) -> String {
        format!(
            "<w:style w:type=\"paragraph\" w:styleId=\"{id}\">{}<w:pPr>{props}</w:pPr></w:style>",
            base.map_or(String::new(), |id| format!("<w:basedOn w:val=\"{id}\"/>"))
        )
    }
    #[test]
    fn pagination_absence_and_all_boolean_spellings_have_precise_values() {
        for (raw, expected) in [
            ("", true),
            ("1", true),
            ("true", true),
            ("on", true),
            ("0", false),
            ("false", false),
            ("off", false),
        ] {
            for key in FIELDS {
                let value = if raw.is_empty() {
                    String::new()
                } else {
                    format!(" w:val=\"{raw}\"")
                };
                let a = audit(&p(&format!("<w:{key}{value}/>")), "").unwrap();
                let mut flags = [false; 3];
                flags[FIELDS.iter().position(|k| *k == key).unwrap()] = expected;
                assert_eq!(a.units[0].flags, Some(flags));
            }
        }
        assert_eq!(audit(&p(""), "").unwrap().units[0].flags, Some([false; 3]));
    }
    #[test]
    fn pagination_cascade_is_fieldwise_and_false_overrides() {
        let styles = format!(
            "<w:docDefaults><w:pPrDefault><w:pPr><w:keepNext/><w:keepLines/><w:pageBreakBefore/></w:pPr></w:pPrDefault></w:docDefaults>{}{}",
            style("Base", None, "<w:keepLines w:val=\"0\"/>"),
            style("Leaf", Some("Base"), "<w:keepNext w:val=\"off\"/>")
        );
        let a = audit(
            &p("<w:pStyle w:val=\"Leaf\"/><w:pageBreakBefore w:val=\"false\"/>"),
            &styles,
        )
        .unwrap();
        assert_eq!(a.units[0].flags, Some([false; 3]));
    }
    #[test]
    fn pagination_explicit_style_does_not_overlay_or_validate_unselected_default() {
        let styles = format!(
            "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"D\"><w:pPr><w:keepLines w:val=\"maybe\"/></w:pPr></w:style>{}",
            style("Own", None, "<w:keepNext/>")
        );
        assert_eq!(
            audit(&p("<w:pStyle w:val=\"Own\"/>"), &styles)
                .unwrap()
                .units[0]
                .flags,
            Some([true, false, false])
        );
        assert!(audit(&p(""), &styles).is_err());
    }
    #[test]
    fn pagination_default_style_with_missing_or_foreign_type_cannot_hide_chain() {
        for kind in ["", " x:type=\"paragraph\" xmlns:x=\"urn:wrong\""] {
            let styles = format!(
                "<w:style{kind} w:default=\"1\" w:styleId=\"A\"><w:basedOn w:val=\"Missing\"/></w:style>"
            );
            assert!(audit(&p(""), &styles).is_err());
        }
    }
    #[test]
    fn pagination_default_style_is_used_only_for_implicit_paragraphs() {
        let styles = format!(
            "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"D\"><w:pPr><w:keepLines/></w:pPr></w:style>{}",
            style("Own", None, "")
        );
        assert_eq!(
            audit(&p(""), &styles).unwrap().units[0].flags,
            Some([false, true, false])
        );
        assert_eq!(
            audit(&p("<w:pStyle w:val=\"Own\"/>"), &styles)
                .unwrap()
                .units[0]
                .flags,
            Some([false; 3])
        );
    }
    #[test]
    fn pagination_active_invalid_toggles_and_attributes_refuse() {
        for raw in ["yes", "TRUE", "False", "", " 1", "-1", "2"] {
            assert!(audit(&p(&format!("<w:keepLines w:val=\"{raw}\"/>")), "").is_err());
        }
        for prop in [
            "<w:keepNext extra=\"x\"/>",
            "<w:keepNext val=\"0\"/>",
            "<w:keepNext><w:x/></w:keepNext>",
            "<w:keepNext>text</w:keepNext>",
            "<w:keepNext/><w:keepNext w:val=\"0\"/>",
        ] {
            assert!(audit(&p(prop), "").is_err(), "{prop}");
        }
    }
    #[test]
    fn pagination_foreign_and_wrapped_active_declarations_refuse() {
        for prop in [
            "<w:keepNext xmlns:w=\"urn:wrong\"/>",
            "<x:keepNext xmlns:x=\"urn:wrong\"/>",
            "<w:x><w:keepLines/></w:x>",
            "<w:rPr><w:keepNext/></w:rPr>",
        ] {
            assert!(audit(&p(prop), "").is_err(), "{prop}");
        }
        assert!(audit("<w:p><w:keepNext/><w:r><w:t>x</w:t></w:r></w:p>", "").is_err());
        assert!(audit(&p(""), "<w:docDefaults><w:rPrDefault><w:rPr><w:keepNext/></w:rPr></w:rPrDefault></w:docDefaults>").is_err());
    }
    #[test]
    fn pagination_missing_cyclic_duplicate_or_wrong_type_active_styles_refuse() {
        let p = p("<w:pStyle w:val=\"A\"/>");
        for styles in [
            String::new(),
            style("A", Some("Missing"), ""),
            style("A", Some("A"), ""),
            style("A", None, "") + &style("A", None, ""),
            style("A", None, "").replace("paragraph", "character"),
            format!("<w:wrapper>{}</w:wrapper>", style("A", None, "")),
        ] {
            assert!(audit(&p, &styles).is_err(), "{styles}");
        }
    }
    #[test]
    fn pagination_chain_depth_is_bounded_and_leaf_precedence_is_stable() {
        let styles: String = (0..21)
            .map(|i| {
                style(
                    &format!("S{i}"),
                    if i == 0 {
                        None
                    } else {
                        Some(format!("S{}", i - 1))
                    }
                    .as_deref(),
                    "",
                )
            })
            .collect();
        assert!(audit(&p("<w:pStyle w:val=\"S20\"/>"), &styles).is_err());
        assert!(audit(&p("<w:pStyle w:val=\"S19\"/>"), &styles).is_ok());
    }
    #[test]
    fn pagination_inactive_malformed_style_properties_are_ignored() {
        let styles = style("Unused", Some("Unused"), "<w:keepNext w:val=\"unknown\"/>")
            + &style("Unused", None, "<w:keepLines/>");
        assert_eq!(
            audit(&p(""), &styles).unwrap().units[0].flags,
            Some([false; 3])
        );
    }
    #[test]
    fn pagination_break_split_exclusion_is_deterministic() {
        let a = audit("<w:p><w:pPr><w:pageBreakBefore/></w:pPr><w:r><w:t>A</w:t><w:br w:type=\"page\"/><w:t>B</w:t></w:r></w:p>", "").unwrap();
        assert_eq!(a.units.len(), 3);
        assert!(a.units.iter().all(|u| u.flags.is_none()));
        assert_eq!(a.units[1].kind, Kind::PageBreak);
        assert!(a.warnings.iter().any(|s| s == NORMALIZED));
    }
    #[test]
    fn pagination_parser_flag_text_type_and_order_binding_are_strict() {
        use office_oxide::ir::{InlineContent, Paragraph, Section, TextSpan};
        let a = audit(&p("<w:keepLines/>"), "").unwrap();
        let p = Paragraph {
            content: vec![InlineContent::Text(TextSpan::plain("same"))],
            keep_together: true,
            ..Default::default()
        };
        let mut ir = DocumentIR {
            sections: vec![Section {
                elements: vec![Element::Paragraph(p.clone())],
                ..Default::default()
            }],
            ..Default::default()
        };
        verify_ir(&a, &ir).unwrap();
        ir.sections[0].elements.push(Element::Paragraph(p.clone()));
        assert!(verify_ir(&a, &ir).is_err());
        ir.sections[0].elements.pop();
        if let Element::Paragraph(p) = &mut ir.sections[0].elements[0] {
            p.keep_together = false;
        }
        assert!(verify_ir(&a, &ir).is_err());
        if let Element::Paragraph(p) = &mut ir.sections[0].elements[0] {
            p.keep_together = true;
            p.content = vec![InlineContent::Text(TextSpan::plain("different"))];
        }
        assert!(verify_ir(&a, &ir).is_err());
    }
    #[test]
    fn pagination_model_binding_rejects_same_text_nested_or_reordered_targets() {
        let a = audit(&p("<w:keepNext/>"), "").unwrap();
        let mut model: Node = serde_json::from_value(serde_json::json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"same"}]}]})).unwrap();
        apply(&a, &mut model).unwrap();
        assert_eq!(model.content[0].attrs["keepNext"], true);
        model.content[0].kind = "heading".into();
        model.content[0].attrs.insert("level".into(), 1.into());
        assert!(apply(&a, &mut model).is_err());
        model = serde_json::from_value(serde_json::json!({"type":"doc","content":[{"type":"bulletList","content":[{"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"same"}]}]}]}]})).unwrap();
        assert!(apply(&a, &mut model).is_err());
    }
}
