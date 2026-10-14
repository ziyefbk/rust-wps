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
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
};

pub(crate) const IMAGE_GEOMETRY_RISK: &str =
    "Unsupported image geometry cannot be preserved in an editable copy";
pub(crate) const IMAGE_FORMATTING_RISK: &str = "Image-only paragraph spacing and alignment preserved; other page and paragraph layout may be normalized";
pub(crate) const IMAGE_BASELINE_RISK: &str =
    "Image paragraph baseline font is unspecified; editor defaults may change its line box";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImageGeometry {
    pub width_emu: u64,
    pub height_emu: u64,
    pub mime: String,
    pub sha256: [u8; 32],
    pub paragraph: ImageParagraph,
}

/// An image-only paragraph's supported effective spacing, plus its Latin
/// paragraph-style strut and drawing-run metrics when both are provable.
/// These are source values, never the editable document's default profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ImageParagraph {
    pub space_before: u64,
    pub space_after: u64,
    pub line_spacing: u64,
    pub text_align: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paragraph_font: Option<ImageParagraphFont>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_font: Option<ImageParagraphFont>,
}
impl Default for ImageParagraph {
    fn default() -> Self {
        Self {
            space_before: 0,
            space_after: 0,
            line_spacing: 240,
            text_align: "left".into(),
            paragraph_font: None,
            run_font: None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ImageParagraphFont {
    pub family: String,
    pub half_points: u64,
    pub bold: bool,
    pub italic: bool,
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
#[derive(Clone, Default)]
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
            if let Some(grid) = self.optional(Ns::W, "docGrid")? {
                grid.empty(&[(Ns::W, "type"), (Ns::W, "linePitch"), (Ns::W, "charSpace")])?;
                ensure!(
                    grid.attr(Ns::W, "type")?
                        .is_none_or(|value| value == "default"),
                    "Active section document grids cannot preserve image line spacing"
                );
            }
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

// Spacing/alignment and bounded Latin baseline metrics are resolved below.
// Other allowed properties remain explicitly normalized, not preserved.
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
            style.empty(&[(Ns::W, "val")])?;
            Ok(style
                .attr(Ns::W, "val")?
                .context("Missing image style ID")?
                .to_owned())
        })
        .transpose()
}
fn style_chain<'a>(styles: &'a Xml, initial: Option<&str>, kind: &str) -> Result<Vec<&'a Xml>> {
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
        if let Some(value) = style.attr(Ns::W, "default")? {
            ensure!(
                matches!(value, "1" | "true" | "on" | "0" | "false" | "off"),
                "Invalid image default-style flag"
            );
        }
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
    let mut chain = Vec::new();
    while let Some(id) = current {
        ensure!(
            visited.len() < 64 && visited.insert(id),
            "Cyclic or excessive image style inheritance"
        );
        let style = indexed.get(id).context("Missing inherited image style")?;
        style.attributes(&[
            (Ns::W, "type"),
            (Ns::W, "default"),
            (Ns::W, "customStyle"),
            (Ns::W, "styleId"),
        ])?;
        // Only direct properties participate in this bounded cascade. Do not
        // silently pass wrappers a different consumer may interpret (notably
        // AlternateContent). Check active styles only, leaving unrelated
        // template styles outside this image's proof.
        style.children(&[
            (Ns::W, "name"),
            (Ns::W, "aliases"),
            (Ns::W, "basedOn"),
            (Ns::W, "next"),
            (Ns::W, "link"),
            (Ns::W, "autoRedef"),
            (Ns::W, "hidden"),
            (Ns::W, "uiPriority"),
            (Ns::W, "semiHidden"),
            (Ns::W, "unhideWhenUsed"),
            (Ns::W, "qFormat"),
            (Ns::W, "locked"),
            (Ns::W, "personal"),
            (Ns::W, "personalCompose"),
            (Ns::W, "personalReply"),
            (Ns::W, "rsid"),
            (Ns::W, "pPr"),
            (Ns::W, "rPr"),
        ])?;
        for metadata in style
            .children
            .iter()
            .filter(|c| !matches!(c.tag.as_str(), "pPr" | "rPr"))
        {
            metadata.empty(&[(Ns::W, "val")])?;
        }
        ensure!(
            style.attr(Ns::W, "type")?.unwrap_or("paragraph") == kind,
            "Conflicting image style type"
        );
        for tag in ["pPr", "rPr"] {
            if let Some(props) = style.optional(Ns::W, tag)? {
                properties(props, tag == "pPr")?;
                ensure!(
                    props
                        .optional(Ns::W, if tag == "pPr" { "pStyle" } else { "rStyle" })?
                        .is_none(),
                    "Nested image style references are unsupported"
                );
            }
        }
        chain.push(*style);
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
    chain.reverse();
    Ok(chain)
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
                    ensure!(
                        props
                            .optional(Ns::W, if tag == "pPr" { "pStyle" } else { "rStyle" })?
                            .is_none(),
                        "Image document defaults cannot reference a style"
                    );
                }
            }
        }
    }
    let paragraph_style = style_id(paragraph.optional(Ns::W, "pPr")?, "pStyle")?;
    style_chain(styles, paragraph_style.as_deref(), "paragraph")?;
    let run_style = style_id(run.optional(Ns::W, "rPr")?, "rStyle")?;
    style_chain(styles, run_style.as_deref(), "character")?;
    Ok(())
}
fn on_off(node: &Xml) -> Result<bool> {
    node.empty(&[(Ns::W, "val")])?;
    match node.attr(Ns::W, "val")? {
        None | Some("1" | "true" | "on") => Ok(true),
        Some("0" | "false" | "off") => Ok(false),
        _ => bail!("Invalid image {} boolean", node.tag),
    }
}

fn paragraph_layer(out: &mut ImageParagraph, props: &Xml) -> Result<()> {
    properties(props, true)?;
    if let Some(spacing) = props.optional(Ns::W, "spacing")? {
        spacing.empty(&[
            (Ns::W, "before"),
            (Ns::W, "after"),
            (Ns::W, "line"),
            (Ns::W, "lineRule"),
            (Ns::W, "beforeLines"),
            (Ns::W, "afterLines"),
            (Ns::W, "beforeAutospacing"),
            (Ns::W, "afterAutospacing"),
        ])?;
        for name in ["beforeLines", "afterLines"] {
            // Even zero line-count spacing changes precedence over twip values.
            ensure!(
                spacing.attr(Ns::W, name)?.is_none(),
                "Image paragraph line-count spacing is unsupported"
            );
        }
        for name in ["beforeAutospacing", "afterAutospacing"] {
            if let Some(value) = spacing.attr(Ns::W, name)? {
                ensure!(
                    matches!(value, "0" | "false" | "off"),
                    "Automatic image paragraph spacing is unsupported"
                );
            }
        }
        if let Some(rule) = spacing.attr(Ns::W, "lineRule")? {
            ensure!(
                rule == "auto",
                "Only automatic image line spacing is supported"
            );
        }
        for (name, target, min, max) in [
            ("before", &mut out.space_before, 0, 31680),
            ("after", &mut out.space_after, 0, 31680),
            ("line", &mut out.line_spacing, 120, 2400),
        ] {
            if let Some(value) = spacing.attr(Ns::W, name)? {
                *target = integer(Some(value), min, max, name)?;
            }
        }
    }
    if let Some(jc) = props.optional(Ns::W, "jc")? {
        jc.empty(&[(Ns::W, "val")])?;
        out.text_align = match jc.attr(Ns::W, "val")? {
            Some("left" | "start") => "left",
            Some("right" | "end") => "right",
            Some("center") => "center",
            Some("both") => "justify",
            _ => bail!("Unsupported image paragraph alignment"),
        }
        .into();
    }
    for tag in ["bidi", "contextualSpacing"] {
        if let Some(property) = props.optional(Ns::W, tag)? {
            ensure!(
                !on_off(property)?,
                "Unsupported active image paragraph {tag}"
            );
        }
    }
    if let Some(alignment) = props.optional(Ns::W, "textAlignment")? {
        alignment.empty(&[(Ns::W, "val")])?;
        ensure!(
            matches!(alignment.attr(Ns::W, "val")?, Some("auto" | "baseline")),
            "Unsupported image paragraph baseline alignment"
        );
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum FontChoice {
    Literal(String),
    Theme(String),
}
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct FontState {
    ascii: Option<FontChoice>,
    h_ansi: Option<FontChoice>,
    half_points: Option<u64>,
    bold: bool,
    italic: bool,
}
impl FontState {
    fn overlay(&mut self, props: &Xml, style: bool) -> Result<()> {
        properties(props, false)?;
        if let Some(fonts) = props.optional(Ns::W, "rFonts")? {
            fonts.empty(&[
                (Ns::W, "ascii"),
                (Ns::W, "hAnsi"),
                (Ns::W, "eastAsia"),
                (Ns::W, "cs"),
                (Ns::W, "asciiTheme"),
                (Ns::W, "hAnsiTheme"),
                (Ns::W, "eastAsiaTheme"),
                (Ns::W, "cstheme"),
                (Ns::W, "hint"),
            ])?;
            ensure!(
                fonts.attr(Ns::W, "hint")?.is_none_or(|v| v == "default"),
                "Non-Latin image baseline font hint is unsupported"
            );
            for (literal, theme, target) in [
                ("ascii", "asciiTheme", &mut self.ascii),
                ("hAnsi", "hAnsiTheme", &mut self.h_ansi),
            ] {
                // A theme beats a literal at the SAME cascade level. A more
                // specific literal replaces the inherited theme for its slot.
                if let Some(value) = fonts.attr(Ns::W, theme)? {
                    ensure!(
                        matches!(
                            value,
                            "majorAscii" | "majorHAnsi" | "minorAscii" | "minorHAnsi"
                        ),
                        "Unsupported non-Latin image font theme"
                    );
                    *target = Some(FontChoice::Theme(value.into()));
                } else if let Some(value) = fonts.attr(Ns::W, literal)? {
                    ensure!(
                        crate::document::supported_font_family(value),
                        "Unsupported image baseline font family"
                    );
                    *target = Some(FontChoice::Literal(value.into()));
                }
            }
        }
        if let Some(size) = props.optional(Ns::W, "sz")? {
            size.empty(&[(Ns::W, "val")])?;
            self.half_points = Some(integer(size.attr(Ns::W, "val")?, 2, 3276, "font size")?);
        }
        for (tag, target) in [("b", &mut self.bold), ("i", &mut self.italic)] {
            if let Some(value) = props.optional(Ns::W, tag)? {
                let enabled = on_off(value)?;
                // OOXML style b/i are toggles; a false style declaration leaves
                // the inherited value unchanged. Direct formatting is absolute.
                if style {
                    *target ^= enabled;
                } else {
                    *target = enabled;
                }
            }
        }
        for tag in ["rtl", "cs"] {
            if let Some(property) = props.optional(Ns::W, tag)? {
                ensure!(
                    !on_off(property)?,
                    "Complex-script image baseline is unsupported"
                );
            }
        }
        for tag in ["position", "spacing"] {
            if let Some(property) = props.optional(Ns::W, tag)? {
                property.empty(&[(Ns::W, "val")])?;
                integer(property.attr(Ns::W, "val")?, 0, 0, tag)?;
            }
        }
        Ok(())
    }
    fn resolved(&self, theme: Option<&Xml>) -> Result<Option<ImageParagraphFont>> {
        fn family(choice: &Option<FontChoice>, theme: Option<&Xml>) -> Result<Option<String>> {
            match choice {
                None => Ok(None),
                Some(FontChoice::Literal(value)) => Ok(Some(value.clone())),
                Some(FontChoice::Theme(value)) => {
                    let Some(theme) = theme else { return Ok(None) };
                    ensure!(
                        theme.is(Ns::A, "theme"),
                        "Unsupported image font theme namespace"
                    );
                    ensure!(
                        theme.count("themeElements") == 1 && theme.count("fontScheme") == 1,
                        "Wrapped or ambiguous image font scheme"
                    );
                    let scheme = theme
                        .one(Ns::A, "themeElements")?
                        .one(Ns::A, "fontScheme")?;
                    ensure!(
                        scheme.count("majorFont") == 1 && scheme.count("minorFont") == 1,
                        "Wrapped or ambiguous image theme font collection"
                    );
                    let name = if value.starts_with("major") {
                        "majorFont"
                    } else {
                        "minorFont"
                    };
                    let collection = scheme.one(Ns::A, name)?;
                    ensure!(
                        collection.count("latin") == 1,
                        "Wrapped or ambiguous image theme Latin font"
                    );
                    let font = collection.one(Ns::A, "latin")?;
                    font.empty(&[
                        (Ns::None, "typeface"),
                        (Ns::None, "panose"),
                        (Ns::None, "pitchFamily"),
                        (Ns::None, "charset"),
                    ])?;
                    let family = font
                        .attr(Ns::None, "typeface")?
                        .context("Missing image theme Latin font")?;
                    ensure!(
                        crate::document::supported_font_family(family),
                        "Unsupported image theme Latin font"
                    );
                    Ok(Some(family.into()))
                }
            }
        }
        let ascii = family(&self.ascii, theme)?;
        let h_ansi = family(&self.h_ansi, theme)?;
        if let (Some(a), Some(h)) = (&ascii, &h_ansi) {
            ensure!(a == h, "Image baseline uses conflicting Latin font slots");
        }
        // Do not substitute our editable profile or guess an application's
        // unstated default. Spacing still survives; baseline omission is warned.
        let (Some(family), Some(_), Some(half_points)) = (ascii, h_ansi, self.half_points) else {
            return Ok(None);
        };
        Ok(Some(ImageParagraphFont {
            family,
            half_points,
            bold: self.bold,
            italic: self.italic,
        }))
    }
}

fn image_paragraph_format(
    paragraph: &Xml,
    run: &Xml,
    styles: &Xml,
    theme: Option<&Xml>,
) -> Result<ImageParagraph> {
    let mut out = ImageParagraph::default();
    let mut font = FontState::default();
    let mut marks = Vec::new();
    if let Some(defaults) = styles.optional(Ns::W, "docDefaults")? {
        if let Some(wrapper) = defaults.optional(Ns::W, "pPrDefault")?
            && let Some(props) = wrapper.optional(Ns::W, "pPr")?
        {
            paragraph_layer(&mut out, props)?;
            if let Some(mark) = props.optional(Ns::W, "rPr")? {
                marks.push(mark);
            }
        }
        if let Some(wrapper) = defaults.optional(Ns::W, "rPrDefault")?
            && let Some(props) = wrapper.optional(Ns::W, "rPr")?
        {
            font.overlay(props, false)?;
        }
    }
    let direct_p = paragraph.optional(Ns::W, "pPr")?;
    let p_style = style_id(direct_p, "pStyle")?;
    for layer in style_chain(styles, p_style.as_deref(), "paragraph")? {
        if let Some(props) = layer.optional(Ns::W, "pPr")? {
            paragraph_layer(&mut out, props)?;
            if let Some(mark) = props.optional(Ns::W, "rPr")? {
                marks.push(mark);
            }
        }
        if let Some(props) = layer.optional(Ns::W, "rPr")? {
            font.overlay(props, true)?;
        }
    }
    if let Some(props) = direct_p {
        paragraph_layer(&mut out, props)?;
        if let Some(mark) = props.optional(Ns::W, "rPr")? {
            marks.push(mark);
        }
    }
    let paragraph_font = font.resolved(theme)?;
    let mut mark_font = font.clone();
    for props in marks {
        let style = style_id(Some(props), "rStyle")?;
        if style.is_some() {
            for layer in style_chain(styles, style.as_deref(), "character")? {
                if let Some(props) = layer.optional(Ns::W, "rPr")? {
                    mark_font.overlay(props, true)?;
                }
            }
        }
        mark_font.overlay(props, false)?;
    }
    ensure!(
        mark_font == font
            || (paragraph_font.is_some() && mark_font.resolved(theme)? == paragraph_font),
        "Distinct image paragraph-mark metrics are unsupported"
    );
    let direct_r = run.optional(Ns::W, "rPr")?;
    let r_style = style_id(direct_r, "rStyle")?;
    for layer in style_chain(styles, r_style.as_deref(), "character")? {
        if let Some(props) = layer.optional(Ns::W, "rPr")? {
            font.overlay(props, true)?;
        }
    }
    if let Some(props) = direct_r {
        font.overlay(props, false)?;
    }
    let run_font = font.resolved(theme)?;
    if paragraph_font.is_some() && run_font.is_some() {
        out.paragraph_font = paragraph_font;
        out.run_font = run_font;
    }
    Ok(out)
}

fn image_paragraph(
    paragraph: &Xml,
    styles: &Xml,
    theme: Option<&Xml>,
) -> Result<(u64, u64, String, ImageParagraph)> {
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
    let (width, height, rid) = drawing(run.one(Ns::W, "drawing")?)?;
    let formatting = image_paragraph_format(paragraph, run, styles, theme)?;
    Ok((width, height, rid, formatting))
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
pub(crate) fn audit_list_relationships(data: &[u8]) -> Result<()> {
    audit_relationships(data)?;
    let root = xml(data)?;
    let mut roles = BTreeSet::new();
    for r in &root.children {
        let kind = r
            .attr(Ns::None, "Type")?
            .context("Missing list relationship type")?;
        let role = kind.rsplit('/').next().unwrap_or("");
        if matches!(role, "styles" | "numbering" | "theme" | "settings") {
            ensure!(
                kind == format!(
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/{role}"
                ) || kind
                    == format!("http://purl.oclc.org/ooxml/officeDocument/relationships/{role}"),
                "Unsupported list relationship type"
            );
            ensure!(
                roles.insert(role)
                    && r.attr(Ns::None, "TargetMode")?
                        .is_none_or(|v| v == "Internal"),
                "Ambiguous or external list relationship"
            );
        }
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
    let themes = relationships.get_by_type(rel_types::THEME);
    ensure!(themes.len() <= 1, "Ambiguous image theme relationship");
    let theme = if let Some(relationship) = themes.first() {
        ensure!(
            relationship.target_mode == TargetMode::Internal,
            "External image font theme"
        );
        let target = main.resolve_relative(&relationship.target)?;
        Some(xml(&package.read_part(&target)?)?)
    } else {
        None
    };
    let mut result = Vec::new();
    let mut blobs = BTreeMap::new();
    for paragraph in &body.children {
        if !paragraph.has_shape() {
            continue;
        }
        let (width_emu, height_emu, rid, paragraph) =
            image_paragraph(paragraph, &styles, theme.as_ref())?;
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
            paragraph,
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

/// Resolve only the Latin font of true-empty paragraph marks. Empty text runs
/// are deliberately irrelevant. Unlike drawing lines, a direct pPr/rPr font
/// overrides the style strut for an empty paragraph (independent LO probes).
/// Reuse the bounded font parser, not the image-only paragraph semantics.
pub(crate) fn resolve_empty_paragraph_fonts(
    main: &[u8],
    styles: &[u8],
    theme: Option<&[u8]>,
    body_indices: &[usize],
) -> Result<Vec<Option<ImageParagraphFont>>> {
    let document = xml(main)?;
    ensure!(
        document.is(Ns::W, "document"),
        "Unsupported empty paragraph document"
    );
    document.audit_section_direction()?;
    let body = document.one(Ns::W, "body")?;
    let styles = xml(styles)?;
    ensure!(
        styles.is(Ns::W, "styles"),
        "Unsupported empty paragraph styles"
    );
    let theme = theme.map(xml).transpose()?;
    fn size(n: &Xml) -> usize {
        1 + n.children.iter().map(size).sum::<usize>()
    }
    fn spend(work: &mut usize, amount: usize) -> Result<()> {
        *work = work
            .checked_add(amount)
            .context("Empty font work overflow")?;
        ensure!(
            *work <= 5_000_000,
            "Empty paragraph font audit exceeds work limits"
        );
        Ok(())
    }
    fn overlay(font: &mut FontState, props: &Xml, style: bool) -> Result<()> {
        if let Some(lang) = props.optional(Ns::W, "lang")?
            && let Some(value) = lang.attr(Ns::W, "val")?
        {
            ensure!(
                matches!(
                    value
                        .split('-')
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .as_str(),
                    "en" | "es" | "fr" | "de" | "it" | "pt" | "nl"
                ),
                "Empty paragraph primary language is outside the proved Latin subset"
            );
        }
        // Ligatures cannot affect a paragraph containing no characters. Ignore
        // only this known extension here, never unknown extension metrics.
        let mut filtered = props.clone();
        filtered
            .children
            .retain(|n| !(n.ns == Ns::W14 && n.tag == "ligatures"));
        font.overlay(&filtered, style)
    }
    fn paragraph_metrics(props: &Xml, inherited: bool) -> Result<()> {
        properties(props, true)?;
        ensure!(
            props.optional(Ns::W, "numPr")?.is_none(),
            "Numbered empty paragraph fonts are not yet supported"
        );
        if inherited {
            ensure!(
                props.optional(Ns::W, "rPr")?.is_none(),
                "Inherited paragraph-mark formatting is not yet supported"
            );
        }
        if let Some(bidi) = props.optional(Ns::W, "bidi")? {
            ensure!(
                !on_off(bidi)?,
                "Bidirectional empty paragraph metrics are unsupported"
            );
        }
        if let Some(align) = props.optional(Ns::W, "textAlignment")? {
            align.empty(&[(Ns::W, "val")])?;
            ensure!(
                matches!(align.attr(Ns::W, "val")?, Some("auto" | "baseline")),
                "Unsupported empty paragraph baseline alignment"
            );
        }
        Ok(())
    }
    ensure!(
        styles.count("docDefaults")
            == usize::from(styles.optional(Ns::W, "docDefaults")?.is_some()),
        "Non-direct document defaults"
    );
    let mut defaults_font = FontState::default();
    if let Some(defaults) = styles.optional(Ns::W, "docDefaults")? {
        defaults.attributes(&[])?;
        defaults.children(&[(Ns::W, "pPrDefault"), (Ns::W, "rPrDefault")])?;
        for (name, tag) in [("pPrDefault", "pPr"), ("rPrDefault", "rPr")] {
            if let Some(wrapper) = defaults.optional(Ns::W, name)? {
                wrapper.attributes(&[])?;
                wrapper.children(&[(Ns::W, tag)])?;
                if let Some(props) = wrapper.optional(Ns::W, tag)? {
                    ensure!(
                        props
                            .optional(Ns::W, if tag == "pPr" { "pStyle" } else { "rStyle" })?
                            .is_none(),
                        "Document defaults cannot reference a style"
                    );
                    if tag == "pPr" {
                        paragraph_metrics(props, true)?;
                    } else {
                        overlay(&mut defaults_font, props, false)?;
                    }
                }
            }
        }
    }
    let mut result = Vec::new();
    let mut bases = BTreeMap::<Option<String>, FontState>::new();
    let mut resolved = BTreeMap::<FontState, Option<ImageParagraphFont>>::new();
    let mut work = size(&styles);
    let theme_work = theme.as_ref().map_or(0, size);
    for &index in body_indices {
        let paragraph = body
            .children
            .get(index)
            .context("Empty paragraph identity mismatch")?;
        ensure!(
            paragraph.is(Ns::W, "p"),
            "Empty font target is not a paragraph"
        );
        let direct = paragraph.optional(Ns::W, "pPr")?;
        if let Some(props) = direct {
            paragraph_metrics(props, false)?;
        }
        let style = style_id(direct, "pStyle")?;
        if !bases.contains_key(&style) {
            spend(&mut work, size(&styles))?;
            let mut base = defaults_font.clone();
            for layer in style_chain(&styles, style.as_deref(), "paragraph")? {
                if let Some(props) = layer.optional(Ns::W, "rPr")? {
                    overlay(&mut base, props, true)?;
                }
                if let Some(props) = layer.optional(Ns::W, "pPr")? {
                    paragraph_metrics(props, true)?;
                }
            }
            bases.insert(style.clone(), base);
        }
        let mut font = bases[&style].clone();
        if let Some(props) = direct
            && let Some(mark) = props.optional(Ns::W, "rPr")?
        {
            ensure!(
                mark.optional(Ns::W, "rStyle")?.is_none(),
                "Character-style paragraph-mark fonts are not yet supported"
            );
            overlay(&mut font, mark, false)?;
        }
        if !resolved.contains_key(&font) {
            spend(&mut work, 1 + theme_work * 2)?;
            resolved.insert(font.clone(), font.resolved(theme.as_ref())?);
        }
        result.push(resolved[&font].clone());
    }
    Ok(result)
}

/// Resolve the plain Latin marker base independently from body run fonts.
/// The list source proof separately validates numbering, paragraph geometry,
/// and the full body identity. Mark overrides remain outside this subset.
pub(crate) fn resolve_list_marker_fonts(
    main: &[u8],
    styles: &[u8],
    theme: Option<&[u8]>,
    body_indices: &[usize],
) -> Result<Vec<crate::list_geometry::MarkerFont>> {
    let document = xml(main)?;
    document.audit_section_direction()?;
    let body = document.one(Ns::W, "body")?;
    let styles = xml(styles)?;
    // Numbering and all active paragraph layers are proved by list_geometry.
    // The shared font-only style validator excludes numPr for its older image
    // contract; remove only those already-proved direct references here.
    let mut font_styles = styles.clone();
    for style in &mut font_styles.children {
        if style.is(Ns::W, "style") {
            for props in &mut style.children {
                if props.is(Ns::W, "pPr") {
                    props.children.retain(|n| !n.is(Ns::W, "numPr"));
                }
            }
        }
    }
    let theme = theme.map(xml).transpose()?;
    fn overlay(font: &mut FontState, props: &Xml, style: bool) -> Result<()> {
        props.children(&[
            (Ns::W, "rFonts"),
            (Ns::W, "sz"),
            (Ns::W, "szCs"),
            (Ns::W, "lang"),
            (Ns::W, "b"),
            (Ns::W, "bCs"),
            (Ns::W, "i"),
            (Ns::W, "iCs"),
            (Ns::W, "noProof"),
        ])?;
        if let Some(lang) = props.optional(Ns::W, "lang")? {
            lang.empty(&[(Ns::W, "val"), (Ns::W, "eastAsia"), (Ns::W, "bidi")])?;
            ensure!(
                lang.attr(Ns::W, "val")?.is_none_or(|v| matches!(
                    v.split('-')
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .as_str(),
                    "en" | "es" | "fr" | "de" | "it" | "pt" | "nl"
                )),
                "Unproved marker language"
            );
        }
        for tag in ["bCs", "iCs"] {
            if let Some(n) = props.optional(Ns::W, tag)? {
                ensure!(!on_off(n)?, "Complex marker emphasis unsupported");
            }
        }
        font.overlay(props, style)
    }
    let mut base = FontState::default();
    if let Some(defaults) = styles.optional(Ns::W, "docDefaults")? {
        defaults.attributes(&[])?;
        defaults.children(&[(Ns::W, "pPrDefault"), (Ns::W, "rPrDefault")])?;
        if let Some(w) = defaults.optional(Ns::W, "pPrDefault")? {
            w.attributes(&[])?;
            w.children(&[(Ns::W, "pPr")])?;
            if let Some(p) = w.optional(Ns::W, "pPr")? {
                ensure!(
                    p.optional(Ns::W, "rPr")?.is_none(),
                    "Default marker formatting unsupported"
                );
            }
        }
        if let Some(w) = defaults.optional(Ns::W, "rPrDefault")? {
            w.attributes(&[])?;
            w.children(&[(Ns::W, "rPr")])?;
            if let Some(p) = w.optional(Ns::W, "rPr")? {
                overlay(&mut base, p, false)?;
            }
        }
    }
    fn size(n: &Xml) -> usize {
        1 + n.children.iter().map(size).sum::<usize>()
    }
    let style_work = size(&styles);
    let theme_work = theme.as_ref().map_or(0, size);
    let mut work = style_work;
    let mut result = Vec::new();
    let mut cache = BTreeMap::new();
    for &index in body_indices {
        let p = body
            .children
            .get(index)
            .context("Marker paragraph identity mismatch")?;
        ensure!(p.is(Ns::W, "p"), "Marker target must be paragraph");
        let direct = p.optional(Ns::W, "pPr")?;
        if let Some(p) = direct {
            ensure!(
                p.optional(Ns::W, "rPr")?.is_none(),
                "Paragraph marker overrides unsupported"
            );
        }
        let id = style_id(direct, "pStyle")?;
        if !cache.contains_key(&id) {
            ensure!(cache.len() < 10000, "Marker font proof exceeds bounds");
            work = work
                .checked_add(style_work + 2 * theme_work)
                .context("Marker font work overflow")?;
            ensure!(work <= 5_000_000, "Marker font proof exceeds work budget");
            let mut font = base.clone();
            for layer in style_chain(&font_styles, id.as_deref(), "paragraph")? {
                if let Some(p) = layer.optional(Ns::W, "pPr")? {
                    ensure!(
                        p.optional(Ns::W, "rPr")?.is_none(),
                        "Inherited marker overrides unsupported"
                    );
                }
                if let Some(p) = layer.optional(Ns::W, "rPr")? {
                    overlay(&mut font, p, true)?;
                }
            }
            let font = font
                .resolved(theme.as_ref())?
                .context("Unresolved marker font")?;
            ensure!(
                !font.bold && !font.italic,
                "Emphasized marker font unsupported"
            );
            cache.insert(
                id.clone(),
                crate::list_geometry::MarkerFont {
                    font_family: font.family,
                    font_size: font.half_points,
                },
            );
        }
        result.push(cache[&id].clone());
    }
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
                sha256: Sha256::digest(png()).into(),
                paragraph: ImageParagraph::default(),
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
    const LATIN_DEFAULTS: &str = r#"<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="DejaVu Sans" w:hAnsi="DejaVu Sans"/><w:sz w:val="24"/></w:rPr></w:rPrDefault></w:docDefaults>"#;
    fn formatted_parts(p: &str, r: &str, style: &str) -> BTreeMap<String, Vec<u8>> {
        let mut result = parts(&format!(
            "<w:p><w:pPr>{p}</w:pPr><w:r><w:rPr>{r}</w:rPr>{DRAWING}</w:r></w:p>"
        ));
        styles(&mut result, style);
        result
    }
    fn formatting(p: &str, r: &str, style: &str) -> ImageParagraph {
        analyze(&zip(&formatted_parts(p, r, style)))
            .unwrap()
            .remove(0)
            .paragraph
    }
    fn add_theme(parts: &mut BTreeMap<String, Vec<u8>>) {
        replace(
            parts,
            "word/_rels/document.xml.rels",
            "</Relationships>",
            &format!(
                r#"<Relationship Id="theme" Type="{R}/theme" Target="themes/unusual.xml"/></Relationships>"#
            ),
        );
        parts.insert("word/themes/unusual.xml".into(), br#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements><a:fontScheme><a:majorFont><a:latin typeface="DejaVu Serif"/></a:majorFont><a:minorFont><a:latin typeface="DejaVu Sans"/></a:minorFont></a:fontScheme></a:themeElements></a:theme>"#.to_vec());
    }
    #[test]
    fn image_paragraph_unspecified_baseline_still_preserves_canonical_spacing() {
        let result = formatting("", "", "");
        assert_eq!(result, ImageParagraph::default());
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            serde_json::json!({
                "spaceBefore": 0, "spaceAfter": 0, "lineSpacing": 240, "textAlign": "left"
            })
        );
        for incomplete in [
            r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial"/>"#,
            r#"<w:sz w:val="22"/>"#,
            r#"<w:rFonts w:ascii="Arial"/><w:sz w:val="22"/>"#,
            r#"<w:rFonts w:asciiTheme="minorHAnsi" w:hAnsiTheme="minorHAnsi"/><w:sz w:val="22"/>"#,
        ] {
            let styles = format!(
                "<w:docDefaults><w:rPrDefault><w:rPr>{incomplete}</w:rPr></w:rPrDefault></w:docDefaults>"
            );
            let result = formatting(
                r#"<w:spacing w:after="77"/><w:jc w:val="center"/>"#,
                "",
                &styles,
            );
            assert_eq!(result.space_after, 77);
            assert_eq!(result.text_align, "center");
            assert!(result.paragraph_font.is_none() && result.run_font.is_none());
        }
    }
    #[test]
    fn image_spacing_resolves_defaults_base_chain_and_each_direct_attribute() {
        let styles = r#"<w:docDefaults><w:pPrDefault><w:pPr><w:spacing w:before="10" w:after="20" w:line="276"/><w:jc w:val="left"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:styleId="Base"><w:pPr><w:spacing w:before="30" w:after="40"/><w:jc w:val="right"/></w:pPr></w:style><w:style w:type="paragraph" w:styleId="Normal" w:default="1"><w:basedOn w:val="Base"/><w:pPr><w:spacing w:after="50"/><w:jc w:val="center"/></w:pPr></w:style>"#;
        let inherited = formatting("", "", styles);
        assert_eq!(
            (
                inherited.space_before,
                inherited.space_after,
                inherited.line_spacing
            ),
            (30, 50, 276)
        );
        assert_eq!(inherited.text_align, "center");
        let direct = formatting(
            r#"<w:spacing w:before="0" w:line="360"/><w:jc w:val="both"/>"#,
            "",
            styles,
        );
        assert_eq!(
            (direct.space_before, direct.space_after, direct.line_spacing),
            (0, 50, 360)
        );
        assert_eq!(direct.text_align, "justify");
    }
    #[test]
    fn explicit_style_does_not_accidentally_inherit_the_default_style() {
        let styles = format!(
            r#"{LATIN_DEFAULTS}<w:style w:type="paragraph" w:styleId="Normal" w:default="1"><w:pPr><w:spacing w:after="500"/></w:pPr><w:rPr><w:sz w:val="50"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Other"><w:pPr><w:spacing w:before="200"/></w:pPr></w:style>"#
        );
        let result = formatting(r#"<w:pStyle w:val="Other"/>"#, "", &styles);
        assert_eq!((result.space_before, result.space_after), (200, 0));
        assert_eq!(result.paragraph_font.unwrap().half_points, 24);
    }
    #[test]
    fn image_baseline_separates_paragraph_style_and_character_direct_run_metrics() {
        let styles = format!(
            r#"{LATIN_DEFAULTS}<w:style w:type="paragraph" w:styleId="Normal" w:default="1"><w:rPr><w:b/><w:sz w:val="26"/></w:rPr></w:style><w:style w:type="character" w:styleId="Emphasis"><w:rPr><w:i/><w:sz w:val="28"/></w:rPr></w:style>"#
        );
        let result = formatting(
            "",
            r#"<w:rStyle w:val="Emphasis"/><w:b w:val="0"/><w:sz w:val="30"/>"#,
            &styles,
        );
        assert_eq!(
            result.paragraph_font,
            Some(ImageParagraphFont {
                family: "DejaVu Sans".into(),
                half_points: 26,
                bold: true,
                italic: false,
            })
        );
        assert_eq!(
            result.run_font,
            Some(ImageParagraphFont {
                family: "DejaVu Sans".into(),
                half_points: 30,
                bold: false,
                italic: true,
            })
        );
    }
    #[test]
    fn style_bold_and_italic_are_toggles_but_direct_run_values_are_absolute() {
        let styles = format!(
            r#"{LATIN_DEFAULTS}<w:style w:type="paragraph" w:styleId="Base"><w:rPr><w:b/><w:i/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Normal" w:default="1"><w:basedOn w:val="Base"/><w:rPr><w:b/><w:i w:val="0"/></w:rPr></w:style>"#
        );
        let result = formatting("", r#"<w:b/><w:i w:val="off"/>"#, &styles);
        let paragraph = result.paragraph_font.unwrap();
        let run = result.run_font.unwrap();
        assert!(!paragraph.bold && paragraph.italic);
        assert!(run.bold && !run.italic);
    }
    #[test]
    fn image_theme_uses_actual_relationship_and_supports_major_minor_latin() {
        let styles = r#"<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:asciiTheme="minorHAnsi" w:hAnsiTheme="minorHAnsi"/><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults>"#;
        let mut parts = formatted_parts(
            "",
            r#"<w:rFonts w:asciiTheme="majorAscii" w:hAnsiTheme="majorHAnsi"/>"#,
            styles,
        );
        add_theme(&mut parts);
        let result = analyze(&zip(&parts)).unwrap().remove(0).paragraph;
        assert_eq!(result.paragraph_font.unwrap().family, "DejaVu Sans");
        assert_eq!(result.run_font.unwrap().family, "DejaVu Serif");
    }
    #[test]
    fn more_specific_literal_font_replaces_theme_and_same_level_theme_wins() {
        let styles = r#"<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:asciiTheme="minorHAnsi" w:hAnsiTheme="minorHAnsi"/><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:styleId="Normal" w:default="1"><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/></w:rPr></w:style>"#;
        let mut parts = formatted_parts(
            "",
            r#"<w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:asciiTheme="majorAscii" w:hAnsiTheme="majorHAnsi"/>"#,
            styles,
        );
        add_theme(&mut parts);
        let result = analyze(&zip(&parts)).unwrap().remove(0).paragraph;
        assert_eq!(result.paragraph_font.unwrap().family, "Arial");
        assert_eq!(result.run_font.unwrap().family, "DejaVu Serif");
    }
    #[test]
    fn distinct_paragraph_mark_font_cannot_be_misrepresented_as_style_strut() {
        let same = formatting(
            r#"<w:rPr><w:rFonts w:ascii="DejaVu Sans" w:hAnsi="DejaVu Sans"/><w:sz w:val="24"/><w:b w:val="0"/></w:rPr>"#,
            "",
            LATIN_DEFAULTS,
        );
        assert_eq!(same.paragraph_font, same.run_font);
        for mark in [
            r#"<w:sz w:val="22"/>"#,
            r#"<w:b/>"#,
            r#"<w:i/>"#,
            r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial"/>"#,
        ] {
            let parts = formatted_parts(&format!("<w:rPr>{mark}</w:rPr>"), "", LATIN_DEFAULTS);
            assert!(
                analyze(&zip(&parts)).is_err(),
                "Unexpectedly accepted mark {mark}"
            );
        }
    }
    #[test]
    fn image_spacing_and_alignment_reject_unpreservable_or_malformed_values() {
        for property in [
            r#"<w:spacing w:before="-1"/>"#,
            r#"<w:spacing w:after="31681"/>"#,
            r#"<w:spacing w:line="119"/>"#,
            r#"<w:spacing w:line="2401"/>"#,
            r#"<w:spacing w:line="240.0"/>"#,
            r#"<w:spacing w:line="+240"/>"#,
            r#"<w:spacing w:before=" 0"/>"#,
            r#"<w:spacing w:after="18446744073709551616"/>"#,
            r#"<w:spacing w:lineRule="exact" w:line="240"/>"#,
            r#"<w:spacing w:lineRule="atLeast"/>"#,
            r#"<w:spacing w:lineRule="unknown"/>"#,
            r#"<w:spacing w:beforeLines="0"/>"#,
            r#"<w:spacing w:afterLines="100"/>"#,
            r#"<w:spacing w:beforeAutospacing="true"/>"#,
            r#"<w:spacing w:afterAutospacing="1"/>"#,
            r#"<w:spacing w:unknown="0"/>"#,
            r#"<w:jc w:val="distribute"/>"#,
            r#"<w:jc w:val="justify"/>"#,
            r#"<w:jc/>"#,
            r#"<w:contextualSpacing/>"#,
            r#"<w:bidi/>"#,
            r#"<w:textAlignment w:val="top"/>"#,
            r#"<w:spacing w:after="10"/><w:spacing w:after="20"/>"#,
        ] {
            let direct = formatted_parts(property, "", LATIN_DEFAULTS);
            assert!(
                analyze(&zip(&direct)).is_err(),
                "Unexpectedly accepted direct {property}"
            );
            let style = format!(
                r#"{LATIN_DEFAULTS}<w:style w:type="paragraph" w:styleId="Normal" w:default="1"><w:pPr>{property}</w:pPr></w:style>"#
            );
            assert!(
                analyze(&zip(&formatted_parts("", "", &style))).is_err(),
                "Unexpectedly accepted inherited {property}"
            );
        }
    }
    #[test]
    fn image_spacing_accepts_bounds_explicit_false_and_ltr_start_end() {
        let result = formatting(
            r#"<w:spacing w:before="31680" w:after="0" w:line="120" w:lineRule="auto" w:beforeAutospacing="false" w:afterAutospacing="0"/><w:bidi w:val="off"/><w:contextualSpacing w:val="0"/><w:textAlignment w:val="baseline"/><w:jc w:val="start"/>"#,
            "",
            "",
        );
        assert_eq!(
            (result.space_before, result.space_after, result.line_spacing),
            (31680, 0, 120)
        );
        assert_eq!(result.text_align, "left");
        let result = formatting(r#"<w:spacing w:line="2400"/><w:jc w:val="end"/>"#, "", "");
        assert_eq!(result.line_spacing, 2400);
        assert_eq!(result.text_align, "right");
    }
    #[test]
    fn baseline_refuses_shift_nonlatin_or_ambiguous_active_metrics() {
        for property in [
            r#"<w:position w:val="1"/>"#,
            r#"<w:position w:val="-1"/>"#,
            r#"<w:spacing w:val="1"/>"#,
            r#"<w:rtl/>"#,
            r#"<w:cs/>"#,
            r#"<w:rFonts w:hint="eastAsia"/>"#,
            r#"<w:rFonts w:hint="cs"/>"#,
            r#"<w:rFonts w:asciiTheme="minorEastAsia"/>"#,
            r#"<w:rFonts w:ascii="Arial" w:hAnsi="Calibri"/>"#,
            r#"<w:rFonts w:ascii=" Bad"/>"#,
            r#"<w:rFonts w:ascii=""/>"#,
            r#"<w:sz w:val="1"/>"#,
            r#"<w:sz w:val="3277"/>"#,
            r#"<w:sz w:val="22.0"/>"#,
            r#"<w:sz/>"#,
            r#"<w:b w:val="yes"/>"#,
            r#"<w:i w:val="2"/>"#,
            r#"<w:rFonts w:unknown="Arial"/>"#,
        ] {
            assert!(
                analyze(&zip(&formatted_parts("", property, LATIN_DEFAULTS))).is_err(),
                "Unexpectedly accepted run {property}"
            );
            let style = format!(
                r#"{LATIN_DEFAULTS}<w:style w:type="paragraph" w:styleId="Normal" w:default="1"><w:rPr>{property}</w:rPr></w:style>"#
            );
            assert!(
                analyze(&zip(&formatted_parts("", "", &style))).is_err(),
                "Unexpectedly accepted style {property}"
            );
        }
        assert!(formatting("", r#"<w:position w:val="0"/><w:spacing w:val="0"/><w:rtl w:val="false"/><w:cs w:val="0"/>"#, LATIN_DEFAULTS).run_font.is_some());
    }
    #[test]
    fn source_spacing_namespace_aliases_conflicts_and_shadowing_are_rejected() {
        for property in [
            r#"<w:spacing x:after="0"/>"#,
            r#"<w:spacing w:after="0" x:after="0"/>"#,
            r#"<x:spacing w:after="0"/>"#,
            r#"<w:spacing xmlns:w="urn:other" w:after="0"/>"#,
            r#"<w:spacing xmlns:q="http://schemas.openxmlformats.org/wordprocessingml/2006/main" q:after="0"/>"#,
        ] {
            assert!(
                analyze(&zip(&formatted_parts(property, "", LATIN_DEFAULTS))).is_err(),
                "Unexpectedly accepted {property}"
            );
        }
    }
    #[test]
    fn theme_identity_must_be_internal_unique_and_namespace_correct() {
        let styles = r#"<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:asciiTheme="minorHAnsi" w:hAnsiTheme="minorHAnsi"/><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults>"#;
        let mut original = formatted_parts("", "", styles);
        add_theme(&mut original);
        for (part, from, to) in [
            (
                "word/_rels/document.xml.rels",
                "Target=\"themes/unusual.xml\"",
                "Target=\"themes/unusual.xml\" TargetMode=\"External\"",
            ),
            (
                "word/_rels/document.xml.rels",
                "</Relationships>",
                &format!(
                    r#"<Relationship Id="theme2" Type="{R}/theme" Target="themes/unusual.xml"/></Relationships>"#
                ),
            ),
            (
                "word/themes/unusual.xml",
                "http://schemas.openxmlformats.org/drawingml/2006/main",
                "urn:other",
            ),
            (
                "word/themes/unusual.xml",
                "<a:latin typeface=\"DejaVu Sans\"/>",
                "<a:latin typeface=\"DejaVu Sans\"/><a:latin typeface=\"Arial\"/>",
            ),
            (
                "word/themes/unusual.xml",
                "typeface=\"DejaVu Sans\"",
                "typeface=\"\"",
            ),
        ] {
            let mut parts = original.clone();
            replace(&mut parts, part, from, to);
            assert!(analyze(&zip(&parts)).is_err(), "Unexpectedly accepted {to}");
        }
    }
    #[test]
    fn image_paragraph_metadata_keeps_order_when_identical_blobs_repeat() {
        let body = format!(
            "{}{}",
            paragraph(DRAWING).replace("<w:p>", r#"<w:p><w:pPr><w:spacing w:after="50"/></w:pPr>"#),
            paragraph(DRAWING).replace(
                "<w:p>",
                r#"<w:p><w:pPr><w:spacing w:after="100"/><w:jc w:val="right"/></w:pPr>"#
            )
        );
        let images = accepted(&body);
        assert_eq!(images[0].sha256, images[1].sha256);
        assert_eq!(images[0].paragraph.space_after, 50);
        assert_eq!(images[1].paragraph.space_after, 100);
        assert_eq!(images[1].paragraph.text_align, "right");
    }
    #[test]
    fn active_document_grids_are_outside_the_image_line_spacing_subset() {
        for kind in ["lines", "linesAndChars", "snapToChars", "unknown"] {
            rejected(&format!(
                r#"{}<w:sectPr><w:docGrid w:type="{kind}" w:linePitch="360"/></w:sectPr>"#,
                paragraph(DRAWING)
            ));
        }
        for properties in [
            r#"w:linePitch="360""#,
            r#"w:type="default" w:linePitch="360""#,
        ] {
            assert_eq!(
                accepted(&format!(
                    "{}<w:sectPr><w:docGrid {properties}/></w:sectPr>",
                    paragraph(DRAWING)
                ))
                .len(),
                1
            );
        }
    }
    #[test]
    fn active_style_wrappers_cannot_hide_properties_or_inheritance() {
        for wrapped in [
            r#"<w:unknown><w:rPr><w:sz w:val="80"/></w:rPr></w:unknown>"#,
            r#"<w:unknown><w:pPr><w:spacing w:after="500"/></w:pPr></w:unknown>"#,
            r#"<w:unknown><w:basedOn w:val="Base"/></w:unknown>"#,
            r#"<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Fallback><w:rPr><w:sz w:val="80"/></w:rPr></mc:Fallback></mc:AlternateContent>"#,
            r#"<w:name w:val="Normal"><w:rPr><w:sz w:val="80"/></w:rPr></w:name>"#,
        ] {
            let active = format!(
                r#"{LATIN_DEFAULTS}<w:style w:type="paragraph" w:styleId="Normal" w:default="1">{wrapped}</w:style>"#
            );
            assert!(
                analyze(&zip(&formatted_parts("", "", &active))).is_err(),
                "Unexpectedly accepted active {wrapped}"
            );
            let unused = format!(
                r#"{LATIN_DEFAULTS}<w:style w:type="paragraph" w:styleId="Unused">{wrapped}</w:style>"#
            );
            assert!(
                analyze(&zip(&formatted_parts("", "", &unused))).is_ok(),
                "Unrelated style tightened scope: {wrapped}"
            );
        }
    }
    #[test]
    fn nested_theme_fonts_cannot_override_the_directly_resolved_latin_identity() {
        let styles = r#"<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:asciiTheme="minorHAnsi" w:hAnsiTheme="minorHAnsi"/><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults>"#;
        for (from, to) in [
            (
                "</a:theme>",
                r#"<a:unknown><a:fontScheme/></a:unknown></a:theme>"#,
            ),
            (
                "</a:fontScheme>",
                r#"<a:unknown><a:minorFont/></a:unknown></a:fontScheme>"#,
            ),
            (
                "</a:minorFont>",
                r#"<a:unknown><a:latin typeface="Arial"/></a:unknown></a:minorFont>"#,
            ),
            (
                r#"<a:latin typeface="DejaVu Sans"/>"#,
                r#"<a:unknown><a:latin typeface="DejaVu Sans"/></a:unknown>"#,
            ),
        ] {
            let mut parts = formatted_parts("", "", styles);
            add_theme(&mut parts);
            replace(&mut parts, "word/themes/unusual.xml", from, to);
            assert!(analyze(&zip(&parts)).is_err(), "Unexpectedly accepted {to}");
        }
    }
    #[test]
    fn style_references_are_only_resolved_at_supported_cascade_entry_points() {
        for (tag, reference) in [
            ("pPr", r#"<w:pStyle w:val="Other"/>"#),
            ("rPr", r#"<w:rStyle w:val="Other"/>"#),
        ] {
            for styles in [
                format!(
                    r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:{tag}>{reference}</w:{tag}></w:style>"#
                ),
                format!(
                    "<w:docDefaults><w:{tag}Default><w:{tag}>{reference}</w:{tag}></w:{tag}Default></w:docDefaults>"
                ),
            ] {
                assert!(
                    analyze(&zip(&formatted_parts("", "", &styles))).is_err(),
                    "Unexpectedly accepted {styles}"
                );
            }
        }
    }
    #[test]
    fn empty_fonts_cache_one_active_style_for_thousands_of_blank_paragraphs() {
        let unused = (0..2000)
            .map(|i| format!("<w:style w:type=\"paragraph\" w:styleId=\"Unused{i}\"/>"))
            .collect::<String>();
        let styles = format!(
            r#"<w:styles xmlns:w="{W}"><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:rPr><w:rFonts w:ascii="DejaVu Sans" w:hAnsi="DejaVu Sans"/><w:sz w:val="24"/></w:rPr></w:style>{unused}</w:styles>"#
        );
        let main = document(&"<w:p/>".repeat(5000));
        let fonts = resolve_empty_paragraph_fonts(
            main.as_bytes(),
            styles.as_bytes(),
            None,
            &(0..5000).collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(fonts.len(), 5000);
        assert!(fonts.iter().all(|font| {
            font.as_ref()
                .is_some_and(|font| font.family == "DejaVu Sans" && font.half_points == 24)
        }));
    }
    #[test]
    fn empty_fonts_bound_distinct_style_resolution_work_without_partial_results() {
        let count = 1500;
        let styles = (0..count).map(|i| format!("<w:style w:type=\"paragraph\" w:styleId=\"S{i}\"><w:rPr><w:rFonts w:ascii=\"DejaVu Sans\" w:hAnsi=\"DejaVu Sans\"/><w:sz w:val=\"24\"/></w:rPr></w:style>")).collect::<String>();
        let styles = format!(r#"<w:styles xmlns:w="{W}">{styles}</w:styles>"#);
        let body = (0..count)
            .map(|i| format!("<w:p><w:pPr><w:pStyle w:val=\"S{i}\"/></w:pPr></w:p>"))
            .collect::<String>();
        let main = document(&body);
        let result = resolve_empty_paragraph_fonts(
            main.as_bytes(),
            styles.as_bytes(),
            None,
            &(0..count).collect::<Vec<_>>(),
        );
        assert!(result.unwrap_err().to_string().contains("work limits"));
    }
}
