//! Conservative, raw-OOXML proof of the display box of an embedded image.
//!
//! The converter's IR drops important drawing declarations. Never infer a box
//! from that IR or from the bitmap's pixel dimensions: audit the complete source
//! shape, then bind the result to the exact bytes consumed downstream.
use anyhow::{Context, Result, bail, ensure};
use office_oxide::core::{
    opc::{OpcReader, PartName},
    relationships::{Relationships, TargetMode, rel_types},
};
use quick_xml::{NsReader, events::Event, name::ResolveResult};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
};

pub(crate) const IMAGE_GEOMETRY_RISK: &str =
    "Unsupported image geometry cannot be preserved in an editable copy";
pub(crate) const IMAGE_FORMATTING_RISK: &str =
    "Image paragraph placement and formatting normalized; original layout not preserved";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImageGeometry {
    pub width_emu: u64,
    pub height_emu: u64,
    pub mime: String,
    pub sha256: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Ns {
    #[default]
    None,
    W,
    A,
    Wp,
    Pic,
    R,
    Relationships,
    ContentTypes,
    A14,
    W14,
    Wp14,
    Mc,
    Other,
}
fn namespace(ns: ResolveResult<'_>) -> Ns {
    match ns {
        ResolveResult::Unbound => Ns::None,
        ResolveResult::Bound(ns) => match ns.as_ref() {
            "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            | "http://purl.oclc.org/ooxml/wordprocessingml/main" => Ns::W,
            "http://schemas.openxmlformats.org/drawingml/2006/main"
            | "http://purl.oclc.org/ooxml/drawingml/main" => Ns::A,
            "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
            | "http://purl.oclc.org/ooxml/drawingml/wordprocessingDrawing" => Ns::Wp,
            "http://schemas.openxmlformats.org/drawingml/2006/picture"
            | "http://purl.oclc.org/ooxml/drawingml/picture" => Ns::Pic,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
            | "http://purl.oclc.org/ooxml/officeDocument/relationships" => Ns::R,
            "http://schemas.openxmlformats.org/package/2006/relationships" => Ns::Relationships,
            "http://schemas.openxmlformats.org/package/2006/content-types" => Ns::ContentTypes,
            "http://schemas.microsoft.com/office/drawing/2010/main" => Ns::A14,
            "http://schemas.microsoft.com/office/word/2010/wordml" => Ns::W14,
            "http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing" => Ns::Wp14,
            "http://schemas.openxmlformats.org/markup-compatibility/2006" => Ns::Mc,
            _ => Ns::Other,
        },
        ResolveResult::Unknown(_) => Ns::Other,
    }
}
#[derive(Default)]
struct Xml {
    ns: Ns,
    tag: String,
    // Keep expanded identity AND literal name. The vendor consumes literal
    // w:* / r:* attributes, so equivalent aliases cannot silently disappear.
    attrs: BTreeMap<String, (Ns, String)>,
    children: Vec<Xml>,
    text: bool,
}
impl Xml {
    fn is(&self, ns: Ns, tag: &str) -> bool {
        self.ns == ns && self.tag == tag
    }
    fn attr(&self, ns: Ns, name: &str) -> Result<Option<&str>> {
        let matching: Vec<_> = self
            .attrs
            .iter()
            .filter(|(key, _)| key.rsplit(':').next() == Some(name))
            .collect();
        ensure!(matching.len() <= 1, "Conflicting image {name} attributes");
        let Some((key, (found_ns, value))) = matching.first() else {
            return Ok(None);
        };
        let expected = match ns {
            Ns::None => name.to_owned(),
            Ns::W => format!("w:{name}"),
            Ns::R => format!("r:{name}"),
            _ => (*key).clone(),
        };
        ensure!(
            *found_ns == ns && **key == expected,
            "Unsupported image attribute namespace: {name}"
        );
        Ok(Some(value))
    }
    fn attributes(&self, allowed: &[(Ns, &str)]) -> Result<()> {
        ensure!(!self.text, "Unexpected text in image {}", self.tag);
        for key in self.attrs.keys() {
            let local = key.rsplit(':').next().expect("name");
            let mut found = false;
            for &(ns, name) in allowed {
                if name == local {
                    self.attr(ns, name)?;
                    found = true;
                    break;
                }
            }
            ensure!(found, "Unsupported image {} attribute: {}", self.tag, key);
        }
        Ok(())
    }
    fn children(&self, allowed: &[(Ns, &str)]) -> Result<()> {
        ensure!(!self.text, "Unexpected text in image {}", self.tag);
        let mut seen = BTreeSet::new();
        for child in &self.children {
            ensure!(
                allowed.iter().any(|&(ns, tag)| child.is(ns, tag)),
                "Unsupported image {} child: {}",
                self.tag,
                child.tag
            );
            ensure!(
                seen.insert(child.tag.as_str()),
                "Repeated image element: {}",
                child.tag
            );
        }
        Ok(())
    }
    fn one(&self, ns: Ns, tag: &str) -> Result<&Xml> {
        let nodes: Vec<_> = self.children.iter().filter(|n| n.tag == tag).collect();
        ensure!(
            nodes.len() == 1 && nodes[0].ns == ns,
            "Missing or ambiguous image {tag}"
        );
        Ok(nodes[0])
    }
    fn optional(&self, ns: Ns, tag: &str) -> Result<Option<&Xml>> {
        if self.children.iter().any(|n| n.tag == tag) {
            Ok(Some(self.one(ns, tag)?))
        } else {
            Ok(None)
        }
    }
    fn empty(&self, allowed: &[(Ns, &str)]) -> Result<()> {
        self.attributes(allowed)?;
        ensure!(
            self.children.is_empty(),
            "Unsupported image {} content",
            self.tag
        );
        Ok(())
    }
    fn count(&self, tag: &str) -> usize {
        usize::from(self.tag == tag) + self.children.iter().map(|c| c.count(tag)).sum::<usize>()
    }
    fn audit_section_direction(&self) -> Result<()> {
        if self.tag == "sectPr" {
            ensure!(
                self.count("textDirection") == 0,
                "Vertical or rotated image section text is unsupported"
            );
        }
        for child in &self.children {
            child.audit_section_direction()?;
        }
        Ok(())
    }
    fn has_shape(&self) -> bool {
        matches!(
            self.tag.as_str(),
            "drawing" | "pict" | "object" | "inline" | "anchor" | "blip" | "pic"
        ) || self.children.iter().any(Self::has_shape)
    }
}

fn xml(data: &[u8]) -> Result<Xml> {
    ensure!(
        data.len() <= 16 * 1024 * 1024,
        "Image metadata exceeds 16 MiB"
    );
    let mut reader = NsReader::from_reader(data);
    let mut stack = vec![Xml::default()];
    let mut count = 0usize;
    loop {
        let (ns, event) = reader.read_resolved_event()?;
        let ns = namespace(ns);
        let start = matches!(&event, Event::Start(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                count += 1;
                ensure!(
                    count <= 100_000 && stack.len() <= 128,
                    "Image metadata exceeds limits"
                );
                let mut node = Xml {
                    ns,
                    tag: e.local_name().as_ref().to_owned(),
                    ..Default::default()
                };
                for attribute in e.attributes() {
                    let attribute = attribute?;
                    if attribute.key.as_ref() == "xmlns"
                        || attribute.key.as_ref().starts_with("xmlns:")
                    {
                        continue;
                    }
                    let (ns, _) = reader.resolver().resolve_attribute(attribute.key);
                    let value = attribute.normalized_value(quick_xml::XmlVersion::Implicit1_0)?;
                    ensure!(
                        value.len() <= 16_384 && node.attrs.len() < 64,
                        "Image attributes exceed limits"
                    );
                    ensure!(
                        node.attrs
                            .insert(
                                attribute.key.as_ref().to_owned(),
                                (namespace(ns), value.into_owned())
                            )
                            .is_none(),
                        "Duplicate image XML attribute"
                    );
                }
                if start {
                    stack.push(node);
                } else {
                    stack.last_mut().expect("root").children.push(node);
                }
            }
            Event::End(_) => {
                ensure!(stack.len() > 1, "Unbalanced image XML");
                let node = stack.pop().expect("element");
                stack.last_mut().expect("root").children.push(node);
            }
            Event::Text(e) => {
                stack.last_mut().expect("root").text |=
                    e.as_ref().bytes().any(|b| !b.is_ascii_whitespace());
            }
            Event::CData(_) | Event::GeneralRef(_) => {
                stack.last_mut().expect("root").text = true;
            }
            Event::DocType(_) => bail!("DTD is not supported in DOCX XML"),
            Event::Eof => {
                ensure!(stack.len() == 1, "Incomplete image XML");
                let root = stack.pop().expect("root");
                ensure!(
                    root.children.len() == 1 && !root.text,
                    "Ambiguous image XML root"
                );
                return Ok(root.children.into_iter().next().expect("root element"));
            }
            _ => {}
        }
    }
}
fn integer(value: Option<&str>, min: u64, max: u64, label: &str) -> Result<u64> {
    let value = value.with_context(|| format!("Missing image {label}"))?;
    ensure!(
        !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()),
        "Image {label} must be an integer"
    );
    let value: u64 = value
        .parse()
        .with_context(|| format!("Invalid image {label}"))?;
    ensure!(
        (min..=max).contains(&value),
        "Image {label} is outside supported bounds"
    );
    Ok(value)
}
fn zero_attrs(node: &Xml, names: &[&str]) -> Result<()> {
    let allowed: Vec<_> = names.iter().map(|&name| (Ns::None, name)).collect();
    node.empty(&allowed)?;
    for name in names {
        if let Some(value) = node.attr(Ns::None, name)? {
            integer(Some(value), 0, 0, name)?;
        }
    }
    Ok(())
}
fn extent(node: &Xml) -> Result<(u64, u64)> {
    node.empty(&[(Ns::None, "cx"), (Ns::None, "cy")])?;
    Ok((
        integer(
            node.attr(Ns::None, "cx")?,
            1,
            crate::document::MAX_IMAGE_WIDTH_EMU,
            "width",
        )?,
        integer(
            node.attr(Ns::None, "cy")?,
            1,
            crate::document::MAX_IMAGE_HEIGHT_EMU,
            "height",
        )?,
    ))
}
fn locks(node: &Xml) -> Result<()> {
    node.empty(&[
        (Ns::None, "noChangeAspect"),
        (Ns::None, "noChangeArrowheads"),
    ])?;
    for (_, value) in node.attrs.values() {
        ensure!(
            matches!(value.as_str(), "0" | "1" | "true" | "false"),
            "Invalid image lock"
        );
    }
    Ok(())
}
fn metadata(node: &Xml) -> Result<()> {
    node.empty(&[
        (Ns::None, "id"),
        (Ns::None, "name"),
        (Ns::None, "descr"),
        (Ns::None, "title"),
    ])?;
    integer(node.attr(Ns::None, "id")?, 0, u32::MAX as u64, "drawing ID")?;
    Ok(())
}

// The allowlist is intentionally complete for the accepted shape. In
// particular, extLst is not a generic escape hatch for effects or new geometry.
fn drawing(node: &Xml) -> Result<(u64, u64, String)> {
    node.attributes(&[])?;
    node.children(&[(Ns::Wp, "inline")])?;
    let inline = node.one(Ns::Wp, "inline")?;
    inline.attributes(&[
        (Ns::None, "distT"),
        (Ns::None, "distB"),
        (Ns::None, "distL"),
        (Ns::None, "distR"),
        (Ns::Wp14, "anchorId"),
        (Ns::Wp14, "editId"),
    ])?;
    for key in ["distT", "distB", "distL", "distR"] {
        if let Some(value) = inline.attr(Ns::None, key)? {
            integer(Some(value), 0, 0, key)?;
        }
    }
    inline.children(&[
        (Ns::Wp, "extent"),
        (Ns::Wp, "effectExtent"),
        (Ns::Wp, "docPr"),
        (Ns::Wp, "cNvGraphicFramePr"),
        (Ns::A, "graphic"),
    ])?;
    let dimensions = extent(inline.one(Ns::Wp, "extent")?)?;
    if let Some(effect) = inline.optional(Ns::Wp, "effectExtent")? {
        zero_attrs(effect, &["l", "t", "r", "b"])?;
    }
    metadata(inline.one(Ns::Wp, "docPr")?)?;
    if let Some(frame) = inline.optional(Ns::Wp, "cNvGraphicFramePr")? {
        frame.attributes(&[])?;
        frame.children(&[(Ns::A, "graphicFrameLocks")])?;
        if let Some(lock) = frame.optional(Ns::A, "graphicFrameLocks")? {
            locks(lock)?;
        }
    }
    let graphic = inline.one(Ns::A, "graphic")?;
    graphic.attributes(&[])?;
    graphic.children(&[(Ns::A, "graphicData")])?;
    let data = graphic.one(Ns::A, "graphicData")?;
    data.attributes(&[(Ns::None, "uri")])?;
    ensure!(
        matches!(
            data.attr(Ns::None, "uri")?,
            Some(
                "http://schemas.openxmlformats.org/drawingml/2006/picture"
                    | "http://purl.oclc.org/ooxml/drawingml/picture"
            )
        ),
        "Unsupported image graphic data"
    );
    data.children(&[(Ns::Pic, "pic")])?;
    let pic = data.one(Ns::Pic, "pic")?;
    pic.attributes(&[])?;
    pic.children(&[
        (Ns::Pic, "nvPicPr"),
        (Ns::Pic, "blipFill"),
        (Ns::Pic, "spPr"),
    ])?;
    let nv = pic.one(Ns::Pic, "nvPicPr")?;
    nv.attributes(&[])?;
    nv.children(&[(Ns::Pic, "cNvPr"), (Ns::Pic, "cNvPicPr")])?;
    metadata(nv.one(Ns::Pic, "cNvPr")?)?;
    let properties = nv.one(Ns::Pic, "cNvPicPr")?;
    properties.attributes(&[(Ns::None, "preferRelativeResize")])?;
    if let Some(value) = properties.attr(Ns::None, "preferRelativeResize")? {
        ensure!(
            matches!(value, "0" | "1" | "false" | "true"),
            "Invalid image resize lock"
        );
    }
    properties.children(&[(Ns::A, "picLocks")])?;
    if let Some(lock) = properties.optional(Ns::A, "picLocks")? {
        locks(lock)?;
    }
    let fill = pic.one(Ns::Pic, "blipFill")?;
    fill.attributes(&[(Ns::None, "dpi"), (Ns::None, "rotWithShape")])?;
    if let Some(value) = fill.attr(Ns::None, "dpi")? {
        integer(Some(value), 0, u32::MAX as u64, "DPI metadata")?;
    }
    if let Some(value) = fill.attr(Ns::None, "rotWithShape")? {
        ensure!(
            matches!(value, "0" | "1" | "false" | "true"),
            "Invalid image rotation flag"
        );
    }
    fill.children(&[(Ns::A, "blip"), (Ns::A, "srcRect"), (Ns::A, "stretch")])?;
    if let Some(crop) = fill.optional(Ns::A, "srcRect")? {
        zero_attrs(crop, &["l", "t", "r", "b"])?;
    }
    let blip = fill.one(Ns::A, "blip")?;
    blip.attributes(&[(Ns::R, "embed"), (Ns::None, "cstate")])?;
    if let Some(state) = blip.attr(Ns::None, "cstate")? {
        ensure!(
            matches!(state, "none" | "email" | "screen" | "print" | "hqprint"),
            "Invalid image compression metadata"
        );
    }
    let relationship = blip
        .attr(Ns::R, "embed")?
        .context("Missing embedded image relationship")?;
    ensure!(
        !relationship.is_empty(),
        "Empty embedded image relationship"
    );
    blip.children(&[(Ns::A, "extLst")])?;
    if let Some(list) = blip.optional(Ns::A, "extLst")? {
        list.attributes(&[])?;
        list.children(&[(Ns::A, "ext")])?;
        let extension = list.one(Ns::A, "ext")?;
        extension.attributes(&[(Ns::None, "uri")])?;
        ensure!(
            extension.attr(Ns::None, "uri")? == Some("{28A0092B-C50C-407E-A947-70E740481C1C}"),
            "Unsupported image extension"
        );
        extension.children(&[(Ns::A14, "useLocalDpi")])?;
        let dpi = extension.one(Ns::A14, "useLocalDpi")?;
        dpi.empty(&[(Ns::None, "val")])?;
        ensure!(
            matches!(
                dpi.attr(Ns::None, "val")?,
                Some("0" | "1" | "false" | "true")
            ),
            "Invalid image DPI metadata"
        );
    }
    let stretch = fill.one(Ns::A, "stretch")?;
    stretch.attributes(&[])?;
    stretch.children(&[(Ns::A, "fillRect")])?;
    zero_attrs(stretch.one(Ns::A, "fillRect")?, &["l", "t", "r", "b"])?;
    let shape = pic.one(Ns::Pic, "spPr")?;
    shape.attributes(&[(Ns::None, "bwMode")])?;
    ensure!(
        shape.attr(Ns::None, "bwMode")?.is_none_or(|v| v == "auto"),
        "Unsupported black-and-white image mode"
    );
    shape.children(&[
        (Ns::A, "xfrm"),
        (Ns::A, "prstGeom"),
        (Ns::A, "noFill"),
        (Ns::A, "ln"),
        (Ns::A, "effectLst"),
    ])?;
    let transform = shape.one(Ns::A, "xfrm")?;
    // Even neutral rotation/flip declarations remain outside this first subset.
    transform.attributes(&[])?;
    transform.children(&[(Ns::A, "off"), (Ns::A, "ext")])?;
    let offset = transform.one(Ns::A, "off")?;
    offset.empty(&[(Ns::None, "x"), (Ns::None, "y")])?;
    integer(offset.attr(Ns::None, "x")?, 0, 0, "x offset")?;
    integer(offset.attr(Ns::None, "y")?, 0, 0, "y offset")?;
    ensure!(
        extent(transform.one(Ns::A, "ext")?)? == dimensions,
        "Conflicting image display extents"
    );
    let geometry = shape.one(Ns::A, "prstGeom")?;
    geometry.attributes(&[(Ns::None, "prst")])?;
    ensure!(
        geometry.attr(Ns::None, "prst")? == Some("rect"),
        "Only rectangular image geometry is supported"
    );
    geometry.children(&[(Ns::A, "avLst")])?;
    if let Some(adjustments) = geometry.optional(Ns::A, "avLst")? {
        adjustments.empty(&[])?;
    }
    if let Some(no_fill) = shape.optional(Ns::A, "noFill")? {
        no_fill.empty(&[])?;
    }
    if let Some(line) = shape.optional(Ns::A, "ln")? {
        line.attributes(&[])?;
        line.children(&[(Ns::A, "noFill")])?;
        line.one(Ns::A, "noFill")?.empty(&[])?;
    }
    if let Some(effects) = shape.optional(Ns::A, "effectLst")? {
        effects.empty(&[])?;
    }
    Ok((dimensions.0, dimensions.1, relationship.to_owned()))
}

// These paragraph/run properties are deliberately NORMALIZED, not preserved.
// Structural properties (numbering, frames, section boundaries, revision
// wrappers, hidden content and unknown extensions) are not normalization cases.
// Text direction, East Asian rotation/combining, character width scaling and
// superscript/subscript are also excluded: their effect on glyph-like inline
// pictures is not proven by the DrawingML extents alone.
fn properties(node: &Xml, paragraph: bool) -> Result<()> {
    ensure!(
        node.is(Ns::W, if paragraph { "pPr" } else { "rPr" }),
        "Unsupported image property namespace"
    );
    node.attributes(&[])?;
    let mut seen = BTreeSet::new();
    for child in &node.children {
        ensure!(
            child.ns == Ns::W && seen.insert(child.tag.as_str()),
            "Ambiguous image paragraph properties"
        );
        if paragraph && child.tag == "rPr" {
            properties(child, false)?;
            continue;
        }
        let permitted = if paragraph {
            matches!(
                child.tag.as_str(),
                "pStyle"
                    | "keepNext"
                    | "keepLines"
                    | "pageBreakBefore"
                    | "widowControl"
                    | "suppressLineNumbers"
                    | "suppressAutoHyphens"
                    | "kinsoku"
                    | "wordWrap"
                    | "overflowPunct"
                    | "topLinePunct"
                    | "autoSpaceDE"
                    | "autoSpaceDN"
                    | "bidi"
                    | "adjustRightInd"
                    | "snapToGrid"
                    | "spacing"
                    | "ind"
                    | "contextualSpacing"
                    | "mirrorIndents"
                    | "suppressOverlap"
                    | "jc"
                    | "textAlignment"
                    | "textboxTightWrap"
                    | "outlineLvl"
                    | "shd"
            )
        } else {
            matches!(
                child.tag.as_str(),
                "rStyle"
                    | "rFonts"
                    | "b"
                    | "bCs"
                    | "i"
                    | "iCs"
                    | "caps"
                    | "smallCaps"
                    | "strike"
                    | "dstrike"
                    | "noProof"
                    | "snapToGrid"
                    | "color"
                    | "spacing"
                    | "kern"
                    | "position"
                    | "sz"
                    | "szCs"
                    | "highlight"
                    | "u"
                    | "rtl"
                    | "cs"
                    | "em"
                    | "lang"
                    | "shd"
            )
        };
        ensure!(
            permitted,
            "Unsupported image paragraph/run property: {}",
            child.tag
        );
        ensure!(
            child.children.is_empty() && !child.text,
            "Unsupported image property content"
        );
        for key in child.attrs.keys() {
            let local = key.rsplit(':').next().expect("name");
            child.attr(Ns::W, local)?;
        }
    }
    Ok(())
}
fn style_id(properties: Option<&Xml>, tag: &str) -> Result<Option<String>> {
    properties
        .map(|p| p.optional(Ns::W, tag))
        .transpose()?
        .flatten()
        .map(|style| {
            Ok(style
                .attr(Ns::W, "val")?
                .context("Missing image style ID")?
                .to_owned())
        })
        .transpose()
}
fn audit_style_chain(styles: &Xml, initial: Option<&str>, kind: &str) -> Result<()> {
    ensure!(
        styles.count("style") == styles.children.iter().filter(|s| s.tag == "style").count(),
        "Non-direct image style definitions"
    );
    let mut indexed = BTreeMap::new();
    let mut default = None;
    for style in &styles.children {
        if style.tag != "style" {
            continue;
        }
        ensure!(style.ns == Ns::W, "Unsupported image style namespace");
        let id = style
            .attr(Ns::W, "styleId")?
            .context("Missing image style ID")?;
        ensure!(
            indexed.insert(id, style).is_none(),
            "Duplicate image style ID"
        );
        if style.attr(Ns::W, "type")?.unwrap_or("paragraph") == kind
            && matches!(style.attr(Ns::W, "default")?, Some("1" | "true" | "on"))
        {
            ensure!(
                default.replace(id).is_none(),
                "Ambiguous default image style"
            );
        }
    }
    let mut current = initial.or(default);
    let mut visited = BTreeSet::new();
    while let Some(id) = current {
        ensure!(
            visited.len() < 64 && visited.insert(id),
            "Cyclic or excessive image style inheritance"
        );
        let style = indexed.get(id).context("Missing inherited image style")?;
        ensure!(
            style.attr(Ns::W, "type")?.unwrap_or("paragraph") == kind,
            "Conflicting image style type"
        );
        for tag in ["pPr", "rPr"] {
            if let Some(props) = style.optional(Ns::W, tag)? {
                properties(props, tag == "pPr")?;
            }
        }
        current = if let Some(base) = style.optional(Ns::W, "basedOn")? {
            base.empty(&[(Ns::W, "val")])?;
            Some(
                base.attr(Ns::W, "val")?
                    .context("Missing inherited image style ID")?,
            )
        } else {
            None
        };
    }
    Ok(())
}
fn audit_styles(styles: &Xml, paragraph: &Xml, run: &Xml) -> Result<()> {
    ensure!(styles.is(Ns::W, "styles"), "Unsupported image styles part");
    ensure!(
        styles.count("docDefaults")
            == styles
                .children
                .iter()
                .filter(|s| s.tag == "docDefaults")
                .count(),
        "Non-direct image document defaults"
    );
    if let Some(defaults) = styles.optional(Ns::W, "docDefaults")? {
        defaults.attributes(&[])?;
        defaults.children(&[(Ns::W, "pPrDefault"), (Ns::W, "rPrDefault")])?;
        for (wrapper, tag) in [("pPrDefault", "pPr"), ("rPrDefault", "rPr")] {
            if let Some(default) = defaults.optional(Ns::W, wrapper)? {
                default.attributes(&[])?;
                default.children(&[(Ns::W, tag)])?;
                if let Some(props) = default.optional(Ns::W, tag)? {
                    properties(props, tag == "pPr")?;
                }
            }
        }
    }
    let paragraph_style = style_id(paragraph.optional(Ns::W, "pPr")?, "pStyle")?;
    audit_style_chain(styles, paragraph_style.as_deref(), "paragraph")?;
    let run_style = style_id(run.optional(Ns::W, "rPr")?, "rStyle")?;
    audit_style_chain(styles, run_style.as_deref(), "character")?;
    Ok(())
}
fn image_paragraph(paragraph: &Xml, styles: &Xml) -> Result<(u64, u64, String)> {
    ensure!(
        paragraph.is(Ns::W, "p"),
        "Image is not in a top-level body paragraph"
    );
    paragraph.attributes(&[
        (Ns::W, "rsidR"),
        (Ns::W, "rsidRDefault"),
        (Ns::W, "rsidP"),
        (Ns::W, "rsidRPr"),
        (Ns::W, "rsidDel"),
        (Ns::W14, "paraId"),
        (Ns::W14, "textId"),
    ])?;
    paragraph.children(&[
        (Ns::W, "pPr"),
        (Ns::W, "r"),
        (Ns::W, "bookmarkStart"),
        (Ns::W, "bookmarkEnd"),
    ])?;
    if let Some(props) = paragraph.optional(Ns::W, "pPr")? {
        properties(props, true)?;
    }
    let bookmark_start = paragraph.optional(Ns::W, "bookmarkStart")?;
    let bookmark_end = paragraph.optional(Ns::W, "bookmarkEnd")?;
    ensure!(
        bookmark_start.is_some() == bookmark_end.is_some(),
        "Unpaired image paragraph bookmark"
    );
    if let (Some(start), Some(end)) = (bookmark_start, bookmark_end) {
        start.empty(&[(Ns::W, "id"), (Ns::W, "name")])?;
        end.empty(&[(Ns::W, "id")])?;
        ensure!(
            start.attr(Ns::W, "id")?.is_some()
                && start.attr(Ns::W, "id")? == end.attr(Ns::W, "id")?,
            "Conflicting image bookmarks"
        );
    }
    let run = paragraph.one(Ns::W, "r")?;
    run.attributes(&[(Ns::W, "rsidR"), (Ns::W, "rsidRPr"), (Ns::W, "rsidDel")])?;
    run.children(&[(Ns::W, "rPr"), (Ns::W, "drawing")])?;
    if let Some(props) = run.optional(Ns::W, "rPr")? {
        properties(props, false)?;
    }
    audit_styles(styles, paragraph, run)?;
    drawing(run.one(Ns::W, "drawing")?)
}
fn audit_relationships(data: &[u8]) -> Result<()> {
    let root = xml(data)?;
    ensure!(
        root.is(Ns::Relationships, "Relationships"),
        "Unsupported image relationships namespace"
    );
    root.attributes(&[])?;
    let mut ids = BTreeSet::new();
    for relationship in &root.children {
        ensure!(
            relationship.is(Ns::Relationships, "Relationship"),
            "Unsupported image relationship element"
        );
        relationship.empty(&[
            (Ns::None, "Id"),
            (Ns::None, "Type"),
            (Ns::None, "Target"),
            (Ns::None, "TargetMode"),
        ])?;
        let id = relationship
            .attr(Ns::None, "Id")?
            .context("Missing image relationship ID")?;
        ensure!(
            !id.is_empty() && ids.insert(id),
            "Duplicate or empty image relationship ID"
        );
        ensure!(
            relationship.attr(Ns::None, "Type")?.is_some()
                && relationship.attr(Ns::None, "Target")?.is_some(),
            "Incomplete image relationship"
        );
        ensure!(
            matches!(
                relationship.attr(Ns::None, "TargetMode")?,
                None | Some("Internal" | "External")
            ),
            "Unsupported image relationship mode"
        );
    }
    Ok(())
}
fn audit_content_types(data: &[u8]) -> Result<()> {
    let root = xml(data)?;
    ensure!(
        root.is(Ns::ContentTypes, "Types"),
        "Unsupported image content types namespace"
    );
    root.attributes(&[])?;
    let mut defaults = BTreeSet::new();
    let mut overrides = BTreeSet::new();
    for child in &root.children {
        ensure!(
            child.ns == Ns::ContentTypes,
            "Unsupported image content type namespace"
        );
        let (key, entries) = match child.tag.as_str() {
            "Default" => ("Extension", &mut defaults),
            "Override" => ("PartName", &mut overrides),
            _ => bail!("Unsupported image content type element"),
        };
        child.empty(&[(Ns::None, key), (Ns::None, "ContentType")])?;
        let value = child
            .attr(Ns::None, key)?
            .context("Incomplete image content type")?;
        let value = if key == "PartName" {
            PartName::new(value)?.as_str().to_ascii_lowercase()
        } else {
            value.to_ascii_lowercase()
        };
        ensure!(
            entries.insert(value),
            "Duplicate image content type declaration"
        );
        ensure!(
            child.attr(Ns::None, "ContentType")?.is_some(),
            "Missing image MIME type"
        );
    }
    Ok(())
}
fn styles_part(
    package: &mut OpcReader<Cursor<&[u8]>>,
    main: &PartName,
    rels: &Relationships,
) -> Result<Xml> {
    let relationships = rels.get_by_type(rel_types::STYLES);
    ensure!(
        relationships.len() <= 1,
        "Ambiguous image styles relationship"
    );
    if let Some(relationship) = relationships.first() {
        ensure!(
            relationship.target_mode == TargetMode::Internal,
            "External image paragraph styles"
        );
        let target = main.resolve_relative(&relationship.target)?;
        xml(&package.read_part(&target)?)
    } else {
        xml(br#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#)
    }
}

/// Analyze the same immutable, bounded DOCX snapshot as the converter. Any
/// unsupported image invalidates the complete proof; partial results must never
/// be attached by sequence to a converter that may have dropped a drawing.
pub(crate) fn analyze(bytes: &[u8]) -> Result<Vec<ImageGeometry>> {
    analyze_inner(bytes).with_context(|| IMAGE_GEOMETRY_RISK)
}
fn analyze_inner(bytes: &[u8]) -> Result<Vec<ImageGeometry>> {
    ensure!(
        bytes.len() <= 64 * 1024 * 1024,
        "Image package exceeds 64 MiB"
    );
    // Bound reads before handing the archive to the vendor. The caller checks
    // CRCs on this same snapshot. Reject ambiguous ZIP identities as well.
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    ensure!(archive.len() <= 4096, "Image package has too many parts");
    let mut expanded = 0u64;
    let mut names = BTreeSet::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        expanded = expanded
            .checked_add(entry.size())
            .context("Image package size overflow")?;
        ensure!(
            expanded <= 128 * 1024 * 1024,
            "Expanded image package exceeds 128 MiB"
        );
        ensure!(
            names.insert(entry.name().replace('\\', "/").to_ascii_lowercase()),
            "Ambiguous image package part"
        );
    }
    // Compatibility inventory historically accepts tiny XML-only ZIPs. They
    // cannot prove an image, but a document with no image needs no such proof.
    if !names.contains("[content_types].xml") {
        let mut main = archive.by_name("word/document.xml")?;
        ensure!(
            main.size() <= 16 * 1024 * 1024,
            "Image metadata exceeds 16 MiB"
        );
        let mut data = Vec::new();
        (&mut main)
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut data)?;
        ensure!(
            !xml(&data)?.has_shape(),
            "Images require an OPC content types part"
        );
        return Ok(Vec::new());
    }
    drop(archive);
    let mut package = OpcReader::new(Cursor::new(bytes))?;
    let main = package.main_document_part()?;
    ensure!(
        main.as_str() == "/word/document.xml",
        "Unsupported image main document part"
    );
    let document = xml(&package.read_part(&main)?)?;
    if !document.has_shape() {
        return Ok(Vec::new());
    }
    ensure!(
        document.is(Ns::W, "document"),
        "Unsupported image document namespace"
    );
    document.attributes(&[(Ns::Mc, "Ignorable"), (Ns::W, "conformance")])?;
    document.audit_section_direction()?;
    let root_rels = PartName::new("/_rels/.rels")?;
    if package.has_part(&root_rels) {
        audit_relationships(&package.read_part(&root_rels)?)?;
    }
    let main_relationships = package
        .package_rels()
        .get_by_type(rel_types::OFFICE_DOCUMENT);
    ensure!(
        main_relationships.len() <= 1
            && main_relationships
                .iter()
                .all(|r| r.target_mode == TargetMode::Internal),
        "Ambiguous or external image main document relationship"
    );
    let body = document.one(Ns::W, "body")?;
    body.attributes(&[])?;
    let relationships = package.read_rels_for(&main)?;
    ensure!(
        relationships.duplicate_ids().is_empty(),
        "Duplicate image relationship ID"
    );
    audit_relationships(&package.read_part(&PartName::new(&main.rels_path())?)?)?;
    audit_content_types(&package.read_part(&PartName::new("/[Content_Types].xml")?)?)?;
    let styles = styles_part(&mut package, &main, &relationships)?;
    let mut result = Vec::new();
    let mut blobs = BTreeMap::new();
    for paragraph in &body.children {
        if !paragraph.has_shape() {
            continue;
        }
        let (width_emu, height_emu, rid) = image_paragraph(paragraph, &styles)?;
        let rel = relationships
            .get_by_id(&rid)
            .context("Missing embedded image relationship")?;
        ensure!(
            rel.rel_type == rel_types::IMAGE && rel.target_mode == TargetMode::Internal,
            "Image must use an internal image relationship"
        );
        let target = main.resolve_relative(&rel.target)?;
        let (mime, sha256) =
            if let Some(identity) = blobs.get(&target.as_str().to_ascii_lowercase()) {
                identity
            } else {
                let mime = match target.extension().map(str::to_ascii_lowercase).as_deref() {
                    Some("png") => "image/png",
                    Some("jpg" | "jpeg") => "image/jpeg",
                    _ => bail!("Only PNG/JPEG image parts are supported"),
                };
                ensure!(
                    package.content_types().resolve(&target) == Some(mime),
                    "Image extension and MIME type conflict"
                );
                let data = package.read_part(&target)?;
                ensure!(
                    !data.is_empty() && data.len() <= 8 * 1024 * 1024,
                    "Image blob exceeds supported bounds"
                );
                crate::document::validate_image_display_media(mime, &data)?;
                let actual = match image::guess_format(&data)? {
                    image::ImageFormat::Png => "image/png",
                    image::ImageFormat::Jpeg => "image/jpeg",
                    _ => bail!("Unsupported image bitmap format"),
                };
                ensure!(actual == mime, "Image bitmap format and MIME type conflict");
                let hash: [u8; 32] = Sha256::digest(&data).into();
                blobs
                    .entry(target.as_str().to_ascii_lowercase())
                    .or_insert((mime.to_owned(), hash))
            };
        result.push(ImageGeometry {
            width_emu,
            height_emu,
            mime: mime.clone(),
            sha256: *sha256,
        });
        ensure!(result.len() <= 4096, "Too many embedded images");
    }
    for tag in ["drawing", "inline", "pic", "blip"] {
        ensure!(
            document.count(tag) == result.len(),
            "Image is outside supported body placement"
        );
    }
    ensure!(
        document.count("anchor") == 0
            && document.count("pict") == 0
            && document.count("object") == 0,
        "Floating, VML, and object images are unsupported"
    );
    ensure!(
        package.crc_mismatched_parts().is_empty(),
        "Image package CRC mismatch"
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::io::Write;
    const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    const DRAWING: &str = r#"<w:drawing><wp:inline><wp:extent cx="1234567" cy="2345678"/><wp:docPr id="1" name="Picture" descr="Original alt"/><wp:cNvGraphicFramePr/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="0" name="source.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="picture"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1234567" cy="2345678"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing>"#;
    fn png() -> Vec<u8> {
        STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==").unwrap()
    }
    fn paragraph(drawing: &str) -> String {
        format!("<w:p><w:r>{drawing}</w:r></w:p>")
    }
    fn document(body: &str) -> String {
        format!(
            r#"<w:document xmlns:w="{W}" xmlns:r="{R}" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" xmlns:wp14="http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing" xmlns:a14="http://schemas.microsoft.com/office/drawing/2010/main" xmlns:x="urn:unknown"><w:body>{body}</w:body></w:document>"#
        )
    }
    fn parts(body: &str) -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([
            ("[Content_Types].xml".into(), br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="png" ContentType="image/png"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_vec()),
            ("_rels/.rels".into(), format!(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="main" Type="{R}/officeDocument" Target="word/document.xml"/></Relationships>"#).into_bytes()),
            ("word/document.xml".into(), document(body).into_bytes()),
            ("word/_rels/document.xml.rels".into(), format!(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="picture" Type="{R}/image" Target="media/original.png"/><Relationship Id="styles" Type="{R}/styles" Target="unusual-styles.xml"/></Relationships>"#).into_bytes()),
            ("word/unusual-styles.xml".into(), format!(r#"<w:styles xmlns:w="{W}"/>"#).into_bytes()),
            ("word/media/original.png".into(), png()),
        ])
    }
    fn zip(parts: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in parts {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }
    fn accepted(body: &str) -> Vec<ImageGeometry> {
        analyze(&zip(&parts(body))).unwrap()
    }
    fn rejected(body: &str) {
        assert!(
            analyze(&zip(&parts(body))).is_err(),
            "Unexpectedly accepted: {body}"
        );
    }
    fn replace(parts: &mut BTreeMap<String, Vec<u8>>, part: &str, from: &str, to: &str) {
        let old = String::from_utf8(parts[part].clone()).unwrap();
        assert!(old.contains(from), "Missing replacement: {from}");
        parts.insert(part.into(), old.replace(from, to).into_bytes());
    }
    fn styles(parts: &mut BTreeMap<String, Vec<u8>>, content: &str) {
        parts.insert(
            "word/unusual-styles.xml".into(),
            format!(r#"<w:styles xmlns:w="{W}">{content}</w:styles>"#).into_bytes(),
        );
    }
    #[test]
    fn canonical_box_is_not_inferred_from_pixels_and_binds_exact_original() {
        let images = accepted(&paragraph(DRAWING));
        assert_eq!(
            images,
            vec![ImageGeometry {
                width_emu: 1_234_567,
                height_emu: 2_345_678,
                mime: "image/png".into(),
                sha256: Sha256::digest(png()).into()
            }]
        );
        let drawing = DRAWING
            .replace("1234567", "1")
            .replace("2345678", "7620000");
        assert_eq!(accepted(&paragraph(&drawing))[0].height_emu, 7_620_000);
        let drawing = DRAWING
            .replace("1234567", "5715000")
            .replace("2345678", "1");
        assert_eq!(accepted(&paragraph(&drawing))[0].width_emu, 5_715_000);
    }
    #[test]
    fn word_neutral_metadata_and_bookmarks_are_normalizable() {
        let drawing = DRAWING
            .replace("<wp:inline>", r#"<wp:inline distT="0" distB="0" distL="0" distR="0" wp14:anchorId="A012DEFF" wp14:editId="A012DEFF">"#)
            .replace("<wp:docPr", r#"<wp:effectExtent l="0" t="0" r="0" b="0"/><wp:docPr"#)
            .replace("<wp:cNvGraphicFramePr/>", r#"<wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect="1"/></wp:cNvGraphicFramePr>"#)
            .replace("<pic:cNvPicPr/>", r#"<pic:cNvPicPr><a:picLocks noChangeAspect="1" noChangeArrowheads="1"/></pic:cNvPicPr>"#)
            .replace(r#"<a:blip r:embed="picture"/>"#, r#"<a:blip r:embed="picture"><a:extLst><a:ext uri="{28A0092B-C50C-407E-A947-70E740481C1C}"><a14:useLocalDpi val="0"/></a:ext></a:extLst></a:blip><a:srcRect/>"#)
            .replace("<pic:spPr>", r#"<pic:spPr bwMode="auto">"#)
            .replace("</pic:spPr>", "<a:noFill/><a:ln><a:noFill/></a:ln><a:effectLst/></pic:spPr>");
        let paragraph = format!(
            r#"<w:p w:rsidR="003872F6"><w:pPr><w:jc w:val="center"/><w:ind w:left="240"/><w:spacing w:after="200"/></w:pPr><w:r><w:rPr><w:noProof/><w:lang w:eastAsia="en-GB"/></w:rPr>{drawing}</w:r><w:bookmarkStart w:id="0" w:name="_GoBack"/><w:bookmarkEnd w:id="0"/></w:p>"#
        );
        assert_eq!(accepted(&paragraph).len(), 1);
    }
    #[test]
    fn strict_namespaces_and_relationship_types_match_the_converter() {
        let mut parts = parts(&paragraph(DRAWING));
        for value in parts.values_mut() {
            if let Ok(s) = std::str::from_utf8(value) {
                *value = s
                    .replace(
                        "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
                        "http://purl.oclc.org/ooxml/wordprocessingml/main",
                    )
                    .replace(
                        "http://schemas.openxmlformats.org/drawingml/2006/",
                        "http://purl.oclc.org/ooxml/drawingml/",
                    )
                    .replace(
                        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
                        "http://purl.oclc.org/ooxml/officeDocument/relationships",
                    )
                    .into_bytes();
            }
        }
        assert_eq!(analyze(&zip(&parts)).unwrap().len(), 1);
    }
    #[test]
    fn source_order_can_repeat_same_blob_with_distinct_boxes() {
        let second = DRAWING
            .replace("1234567", "450000")
            .replace("2345678", "900000");
        let images = accepted(&format!(
            "{}<w:p><w:r><w:t>Between</w:t></w:r></w:p>{}",
            paragraph(DRAWING),
            paragraph(&second)
        ));
        assert_eq!(images.len(), 2);
        assert_eq!(images[0].sha256, images[1].sha256);
        assert_eq!(
            (images[1].width_emu, images[1].height_emu),
            (450000, 900000)
        );
    }
    #[test]
    fn rejects_noncanonical_or_conflicting_extents() {
        for invalid in [
            "0",
            "-1",
            "1.0",
            "+1",
            " 1",
            "1 ",
            "1e3",
            "5715001",
            "18446744073709551616",
        ] {
            rejected(&paragraph(&DRAWING.replace("1234567", invalid)));
        }
        for invalid in ["0", "-1", "7620001"] {
            rejected(&paragraph(&DRAWING.replace("2345678", invalid)));
        }
        for (from, to) in [
            (r#"<wp:extent cx="1234567" cy="2345678"/>"#, ""),
            (r#"<a:ext cx="1234567" cy="2345678"/>"#, ""),
            (
                r#"<a:ext cx="1234567" cy="2345678"/>"#,
                r#"<a:ext cx="1234568" cy="2345678"/>"#,
            ),
            (
                r#"<wp:extent cx="1234567" cy="2345678"/>"#,
                r#"<wp:extent cx="1234567" cy="2345678"/><wp:extent cx="1234567" cy="2345678"/>"#,
            ),
            (
                r#"<a:ext cx="1234567" cy="2345678"/>"#,
                r#"<a:ext cx="1234567" cy="2345678"/><a:ext cx="1234567" cy="2345678"/>"#,
            ),
        ] {
            rejected(&paragraph(&DRAWING.replace(from, to)));
        }
    }
    #[test]
    fn rejects_crop_rotation_flip_offsets_effects_relative_and_unknown_geometry() {
        for (from, to) in [
            ("wp:inline", "wp:anchor"),
            ("<a:xfrm>", r#"<a:xfrm rot="0">"#),
            ("<a:xfrm>", r#"<a:xfrm flipH="0">"#),
            ("<a:xfrm>", r#"<a:xfrm flipV="1">"#),
            (r#"<a:off x="0" y="0"/>"#, r#"<a:off x="1" y="0"/>"#),
            (r#"<a:off x="0" y="0"/>"#, ""),
            (r#"prst="rect""#, r#"prst="ellipse""#),
            (
                "<a:avLst/>",
                r#"<a:avLst><a:gd name="adj" fmla="val 1"/></a:avLst>"#,
            ),
            ("<a:stretch>", r#"<a:srcRect l="1"/><a:stretch>"#),
            ("<a:fillRect/>", r#"<a:fillRect r="1"/>"#),
            ("<wp:docPr", r#"<wp:effectExtent l="1"/><wp:docPr"#),
            (
                "</wp:inline>",
                "<wp14:sizeRelH/><wp14:sizeRelV/></wp:inline>",
            ),
            (
                "</pic:spPr>",
                "<a:effectLst><a:outerShdw/></a:effectLst></pic:spPr>",
            ),
            ("</pic:spPr>", "<a:scene3d/></pic:spPr>"),
            ("</pic:spPr>", "<a:solidFill/></pic:spPr>"),
            ("</pic:spPr>", "<a:ln/></pic:spPr>"),
            (
                r#"<a:blip r:embed="picture"/>"#,
                r#"<a:blip r:embed="picture"><a:grayscl/></a:blip>"#,
            ),
            (
                r#"<a:blip r:embed="picture"/>"#,
                r#"<a:blip r:embed="picture"><a:extLst><a:ext uri="unknown"/></a:extLst></a:blip>"#,
            ),
            ("<wp:inline>", r#"<wp:inline distT="1">"#),
        ] {
            rejected(&paragraph(&DRAWING.replace(from, to)));
        }
    }
    #[test]
    fn rejects_namespace_shadowing_and_alias_attributes_vendor_cannot_consume() {
        for (from, to) in [
            ("wp:extent", "x:extent"),
            ("a:ext ", "x:ext "),
            ("pic:pic", "x:pic"),
            (r#"cx="1234567""#, r#"cx="1234567" x:cx="1234567""#),
            (r#"cx="1234567""#, r#"x:cx="1234567""#),
            (
                r#"r:embed="picture""#,
                r#"r:embed="picture" x:embed="other""#,
            ),
            (r#"r:embed="picture""#, r#"x:embed="picture""#),
            (r#"r:embed="picture""#, r#"r:embed="picture" unknown="1""#),
        ] {
            rejected(&paragraph(&DRAWING.replace(from, to)));
        }
        let body = paragraph(DRAWING)
            .replace("<w:p>", &format!(r#"<w:p xmlns:q="{R}">"#))
            .replace("r:embed", "q:embed");
        rejected(&body);
        rejected(&paragraph(DRAWING).replace("<w:p>", r#"<w:p xmlns:w="urn:unknown">"#));
    }
    #[test]
    fn rejects_mixed_and_nonbody_placements() {
        for body in [
            format!(
                "<w:tbl><w:tr><w:tc>{}</w:tc></w:tr></w:tbl>",
                paragraph(DRAWING)
            ),
            format!(
                "<w:sdt><w:sdtContent>{}</w:sdtContent></w:sdt>",
                paragraph(DRAWING)
            ),
            format!("<w:p><w:hyperlink><w:r>{DRAWING}</w:r></w:hyperlink></w:p>"),
            format!("<w:p><w:r>{DRAWING}<w:t>Caption</w:t></w:r></w:p>"),
            format!("<w:p><w:r>{DRAWING}</w:r><w:r><w:t>Caption</w:t></w:r></w:p>"),
            format!("<w:p><w:r>{DRAWING}{DRAWING}</w:r></w:p>"),
            format!("<w:p><w:r>{DRAWING}<w:tab/></w:r></w:p>"),
            format!("<w:p><w:r>{DRAWING}<w:br/></w:r></w:p>"),
            format!("<w:p><w:r>{DRAWING}<w:fldChar/></w:r></w:p>"),
            format!("<w:p><w:pPr><w:numPr/></w:pPr><w:r>{DRAWING}</w:r></w:p>"),
            format!("<w:p><w:pPr><w:framePr/></w:pPr><w:r>{DRAWING}</w:r></w:p>"),
            format!("<w:p><w:pPr><w:sectPr/></w:pPr><w:r>{DRAWING}</w:r></w:p>"),
            paragraph(DRAWING).replace("<w:r>", "<w:r><w:rPr><w:vanish/></w:rPr>"),
        ] {
            rejected(&body);
        }
    }
    #[test]
    fn used_style_chains_reject_numbering_but_unused_list_styles_do_not() {
        let mut parts = parts(&paragraph(DRAWING));
        styles(
            &mut parts,
            r#"<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:asciiTheme="minorHAnsi"/><w:sz w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:line="276" w:after="200"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"/><w:style w:type="paragraph" w:styleId="UnusedList"><w:pPr><w:numPr/></w:pPr></w:style>"#,
        );
        assert_eq!(analyze(&zip(&parts)).unwrap().len(), 1);
        replace(
            &mut parts,
            "word/document.xml",
            "<w:p>",
            r#"<w:p><w:pPr><w:pStyle w:val="UnusedList"/></w:pPr>"#,
        );
        assert!(analyze(&zip(&parts)).is_err());
        for content in [
            r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:pPr><w:numPr/></w:pPr></w:style>"#,
            r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:basedOn w:val="List"/></w:style><w:style w:type="paragraph" w:styleId="List"><w:pPr><w:numPr/></w:pPr></w:style>"#,
            r#"<w:docDefaults><w:pPrDefault><w:pPr><w:numPr/></w:pPr></w:pPrDefault></w:docDefaults>"#,
            r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:basedOn w:val="Normal"/></w:style>"#,
            r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"/><w:style w:type="paragraph" w:default="1" w:styleId="Other"/>"#,
        ] {
            let mut parts = super::tests::parts(&paragraph(DRAWING));
            styles(&mut parts, content);
            assert!(
                analyze(&zip(&parts)).is_err(),
                "Unexpectedly accepted {content}"
            );
        }
    }
    #[test]
    fn relationship_and_bitmap_identity_must_be_unambiguous() {
        for (part, from, to) in [
            (
                "word/_rels/document.xml.rels",
                "relationships/image",
                "relationships/hyperlink",
            ),
            (
                "word/_rels/document.xml.rels",
                r#"Target="media/original.png""#,
                r#"Target="media/original.png" TargetMode="External""#,
            ),
            (
                "word/_rels/document.xml.rels",
                r#"Target="media/original.png""#,
                r#"Target="media/missing.png""#,
            ),
            (
                "word/_rels/document.xml.rels",
                r#"Target="media/original.png""#,
                r#"Target="media/original.png" TargetMode="unknown""#,
            ),
            (
                "word/_rels/document.xml.rels",
                r#"Id="styles""#,
                r#"Id="picture""#,
            ),
            (
                "word/_rels/document.xml.rels",
                "http://schemas.openxmlformats.org/package/2006/relationships",
                "urn:unknown",
            ),
            ("[Content_Types].xml", "image/png", "image/jpeg"),
            (
                "[Content_Types].xml",
                "http://schemas.openxmlformats.org/package/2006/content-types",
                "urn:unknown",
            ),
        ] {
            let mut parts = parts(&paragraph(DRAWING));
            replace(&mut parts, part, from, to);
            assert!(
                analyze(&zip(&parts)).is_err(),
                "Unexpectedly accepted {part}: {to}"
            );
        }
        let mut parts = parts(&paragraph(DRAWING));
        parts.insert("word/media/original.png".into(), b"not an image".to_vec());
        assert!(analyze(&zip(&parts)).is_err());
        let mut parts = super::tests::parts(&paragraph(DRAWING));
        let xml = parts["word/document.xml"].clone();
        parts.insert("WORD/document.xml".into(), xml);
        assert!(analyze(&zip(&parts)).is_err());
    }
    #[test]
    fn rejects_oversized_deep_or_dtd_metadata() {
        assert!(xml(&vec![b' '; 16 * 1024 * 1024 + 1]).is_err());
        assert!(xml(format!("{}{}", "<a>".repeat(130), "</a>".repeat(130)).as_bytes()).is_err());
        assert!(xml(b"<!DOCTYPE a><a/>").is_err());
    }
    #[test]
    fn no_image_inventory_zip_requires_no_opc_proof() {
        let minimal =
            BTreeMap::from([("word/document.xml".into(), document("<w:p/>").into_bytes())]);
        assert!(analyze(&zip(&minimal)).unwrap().is_empty());
        let minimal = BTreeMap::from([(
            "word/document.xml".into(),
            document(&paragraph(DRAWING)).into_bytes(),
        )]);
        assert!(analyze(&zip(&minimal)).is_err());
    }
    #[test]
    fn untyped_default_and_wrapped_styles_cannot_hide_list_inheritance() {
        for content in [
            r#"<w:style w:default="1" w:styleId="Normal"><w:pPr><w:numPr/></w:pPr></w:style>"#,
            r#"<w:unknown><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:pPr><w:numPr/></w:pPr></w:style></w:unknown>"#,
            r#"<w:unknown><w:docDefaults><w:pPrDefault><w:pPr><w:numPr/></w:pPr></w:pPrDefault></w:docDefaults></w:unknown>"#,
        ] {
            let mut parts = parts(&paragraph(DRAWING));
            styles(&mut parts, content);
            assert!(
                analyze(&zip(&parts)).is_err(),
                "Unexpectedly accepted {content}"
            );
        }
    }
    #[test]
    fn jpeg_identity_is_supported_but_orientation_metadata_is_not() {
        let mut parts = parts(&paragraph(DRAWING));
        let mut cursor = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::new(1, 1))
            .write_to(&mut cursor, image::ImageFormat::Jpeg)
            .unwrap();
        let jpeg = cursor.into_inner();
        replace(
            &mut parts,
            "word/_rels/document.xml.rels",
            "original.png",
            "original.jpg",
        );
        replace(
            &mut parts,
            "[Content_Types].xml",
            r#"Extension="png" ContentType="image/png""#,
            r#"Extension="jpg" ContentType="image/jpeg""#,
        );
        parts.insert("word/media/original.jpg".into(), jpeg.clone());
        let images = analyze(&zip(&parts)).unwrap();
        assert_eq!(images[0].mime, "image/jpeg");
        assert_eq!(images[0].sha256, <[u8; 32]>::from(Sha256::digest(&jpeg)));
        let mut oriented = jpeg;
        oriented.splice(2..2, [0xff, 0xe1, 0, 2]);
        parts.insert("word/media/original.jpg".into(), oriented);
        assert!(analyze(&zip(&parts)).is_err());
    }
    #[test]
    fn main_opc_identity_must_not_be_external_ambiguous_or_foreign() {
        for (from, to) in [
            (
                r#"Target="word/document.xml""#,
                r#"Target="word/document.xml" TargetMode="External""#,
            ),
            (
                "http://schemas.openxmlformats.org/package/2006/relationships",
                "urn:unknown",
            ),
            (
                "</Relationships>",
                &format!(
                    r#"<Relationship Id="other" Type="{R}/officeDocument" Target="word/document.xml"/></Relationships>"#
                ),
            ),
        ] {
            let mut parts = parts(&paragraph(DRAWING));
            replace(&mut parts, "_rels/.rels", from, to);
            assert!(analyze(&zip(&parts)).is_err());
        }
    }
    #[test]
    fn rejects_direct_text_rotation_scaling_and_script_alignment() {
        for (paragraph_property, property) in [
            (true, r#"<w:textDirection w:val="tbRl"/>"#),
            (true, r#"<w:textDirection w:val="lrTb"/>"#),
            (
                false,
                r#"<w:eastAsianLayout w:vert="1" w:vertCompress="1"/>"#,
            ),
            (false, r#"<w:eastAsianLayout w:vert="0"/>"#),
            (false, r#"<w:w w:val="50"/>"#),
            (false, r#"<w:w w:val="100"/>"#),
            (false, r#"<w:vertAlign w:val="superscript"/>"#),
            (false, r#"<w:vertAlign w:val="baseline"/>"#),
        ] {
            let (from, to) = if paragraph_property {
                ("<w:p>", format!("<w:p><w:pPr>{property}</w:pPr>"))
            } else {
                ("<w:r>", format!("<w:r><w:rPr>{property}</w:rPr>"))
            };
            rejected(&paragraph(DRAWING).replace(from, &to));
        }
        // A different text-only paragraph's script formatting does not rotate
        // or scale this image, so it must not tighten the accepted image scope.
        let body = format!(
            r#"<w:p><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:t>Note</w:t></w:r></w:p>{}"#,
            paragraph(DRAWING)
        );
        assert_eq!(accepted(&body).len(), 1);
    }
    #[test]
    fn rejects_inherited_text_rotation_scaling_and_script_alignment() {
        for (tag, property) in [
            ("pPr", r#"<w:textDirection w:val="tbRl"/>"#),
            ("rPr", r#"<w:eastAsianLayout w:vert="1"/>"#),
            ("rPr", r#"<w:w w:val="100"/>"#),
            ("rPr", r#"<w:vertAlign w:val="baseline"/>"#),
        ] {
            for content in [
                format!(
                    r#"<w:docDefaults><w:{tag}Default><w:{tag}>{property}</w:{tag}></w:{tag}Default></w:docDefaults>"#
                ),
                format!(
                    r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:{tag}>{property}</w:{tag}></w:style>"#
                ),
                format!(
                    r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:basedOn w:val="Base"/></w:style><w:style w:type="paragraph" w:styleId="Base"><w:{tag}>{property}</w:{tag}></w:style>"#
                ),
            ] {
                let mut parts = parts(&paragraph(DRAWING));
                styles(&mut parts, &content);
                assert!(
                    analyze(&zip(&parts)).is_err(),
                    "Unexpectedly accepted {content}"
                );
            }
            if tag == "rPr" {
                let mut parts = parts(&paragraph(DRAWING).replace(
                    "<w:r>",
                    r#"<w:r><w:rPr><w:rStyle w:val="Rotated"/></w:rPr>"#,
                ));
                styles(
                    &mut parts,
                    &format!(
                        r#"<w:style w:type="character" w:styleId="Rotated"><w:rPr>{property}</w:rPr></w:style>"#
                    ),
                );
                assert!(analyze(&zip(&parts)).is_err());
            }
        }
    }
    #[test]
    fn rejects_section_level_text_rotation_without_restricting_page_orientation() {
        for value in ["tbRl", "lrTb"] {
            rejected(&format!(
                r#"{}<w:sectPr><w:textDirection w:val="{value}"/></w:sectPr>"#,
                paragraph(DRAWING)
            ));
        }
        assert_eq!(
            accepted(&format!(
                r#"{}<w:sectPr><w:pgSz w:w="15840" w:h="12240" w:orient="landscape"/></w:sectPr>"#,
                paragraph(DRAWING)
            ))
            .len(),
            1
        );
    }
}
