//! Additive source proof for external DOCX Roman/Swiss font classifications.
//! This is not a font-table reader for the general converter: unsupported or
//! ambiguous metadata leaves the historical imported model intact. No partial
//! table, font-name inference, replacement-font selection or new hard refusal.
use crate::{
    document::{FontFamilyClass, FontFamilyMetadata, Node},
    empty_paragraph_fonts::{Block, source_body_identity, true_blank, verify_body_identity},
    image_dimensions::ImageParagraphFont,
    paragraph_indentation::{Xml, root, xml},
};
use anyhow::{Context, Result, bail, ensure};
use office_oxide::core::{
    opc::{OpcReader, PartName},
    relationships::{Relationships, TargetMode, rel_types},
};
use quick_xml::{NsReader, events::Event, name::ResolveResult};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
};

pub(crate) const PRESERVED: &str = "Source-declared Roman/Swiss classifications preserved for proved Latin text and empty-paragraph fonts; other font metadata, bare-break defaults and local substitution remain unproved";
const ANCILLARY_LIMIT: usize = 1024 * 1024;
const MAIN_LIMIT: usize = 16 * 1024 * 1024;
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const WS: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
const MIME: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.";

#[derive(Clone, Copy, Debug)]
enum Reason {
    Package,
    Table,
    Body,
    Typography,
    Missing,
    Alias,
    Binding,
    Limit,
    Candidate,
}
impl Reason {
    fn text(self) -> &'static str {
        match self {
            Self::Package => "package identity is unproved",
            Self::Table => "font-table declarations are malformed or ambiguous",
            Self::Body => "body content is outside the supported Latin subset",
            Self::Typography => "effective font or style/theme identity is unproved",
            Self::Missing => "a retained font has no supported source classification",
            Self::Alias => "a retained name has duplicate or ASCII-case-alias definitions",
            Self::Binding => "source and imported font ranges differ",
            Self::Limit => "proof resource limits exceeded",
            Self::Candidate => "candidate metadata failed validation",
        }
    }
    fn message(self) -> String {
        format!(
            "DOCX font-family metadata not preserved: {}; existing imported fonts and layout are unchanged",
            self.text()
        )
    }
}
#[derive(Debug)]
struct Limit;
impl std::fmt::Display for Limit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("font proof resource limit")
    }
}
impl std::error::Error for Limit {}
pub(crate) fn limit(condition: bool) -> Result<()> {
    if condition { Ok(()) } else { Err(Limit.into()) }
}
fn at<T>(result: Result<T>, reason: Reason) -> std::result::Result<T, Reason> {
    result.map_err(|e| {
        if e.downcast_ref::<Limit>().is_some() {
            Reason::Limit
        } else {
            reason
        }
    })
}

#[derive(Default)]
pub(crate) struct Work {
    visits: usize,
    links: usize,
}
impl Work {
    pub(crate) fn spend(&mut self, amount: usize) -> Result<()> {
        self.visits = self.visits.checked_add(amount).ok_or(Limit)?;
        limit(self.visits <= 5_000_000)
    }
    pub(crate) fn spend_product(&mut self, count: usize, size: usize) -> Result<()> {
        self.spend(count.checked_mul(size).ok_or(Limit)?)
    }
    pub(crate) fn links(&mut self, amount: usize) -> Result<()> {
        self.links = self.links.checked_add(amount).ok_or(Limit)?;
        limit(self.links <= 10000)
    }
}

pub(crate) struct Audit(Outcome);
enum Outcome {
    Absent,
    Unavailable(Reason),
    Proved(Proof),
}
struct Proof {
    blocks: Vec<Block>,
    paragraphs: Vec<Paragraph>,
    records: Vec<FontFamilyMetadata>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Range {
    text: String,
    name: String,
}
enum Paragraph {
    Text(Vec<Range>),
    Empty(ImageParagraphFont),
    BareBreak,
}
struct SourceParagraph {
    body_index: usize,
    runs: Vec<(usize, String)>,
    blank: bool,
    bare_break: bool,
}
struct Record {
    name: String,
    family: Option<String>,
    unsupported: bool,
}

/// This cannot replace old parser errors. Its result is consumed only after
/// every historical conversion and validation step has completed successfully.
pub(crate) fn analyze(bytes: &[u8]) -> Audit {
    Audit(match analyze_inner(bytes) {
        Ok(Some(proof)) => Outcome::Proved(proof),
        Ok(None) => Outcome::Absent,
        Err(reason) => Outcome::Unavailable(reason),
    })
}

struct Budget {
    bytes: usize,
    nodes: usize,
}
impl Budget {
    fn charge(&mut self, data: &[u8], main: bool) -> Result<()> {
        if main {
            limit(data.len() <= MAIN_LIMIT)?;
        } else {
            self.bytes = self.bytes.checked_add(data.len()).ok_or(Limit)?;
            limit(self.bytes <= ANCILLARY_LIMIT)?;
        }
        Ok(())
    }
    fn inspect(&mut self, data: &[u8]) -> Result<()> {
        // Charge every XML part before the reusable tree parser can allocate.
        // This scan also applies attribute bounds to the text-preserving tree.
        let mut reader = quick_xml::Reader::from_reader(data);
        let mut depth = 0usize;
        loop {
            let event = reader.read_event()?;
            let start = matches!(&event, Event::Start(_));
            match event {
                Event::Start(e) | Event::Empty(e) => {
                    self.nodes = self.nodes.checked_add(1).ok_or(Limit)?;
                    limit(self.nodes <= 100_000 && depth < 128)?;
                    let mut count = 0;
                    for attr in e.attributes() {
                        let attr = attr?;
                        count += 1;
                        limit(count <= 64 && attr.value.len() <= 16_384)?;
                    }
                    if start {
                        depth += 1;
                    }
                }
                Event::End(_) => {
                    depth = depth.saturating_sub(1);
                }
                Event::DocType(_) => bail!("DTD unsupported"),
                Event::Eof => break,
                _ => {}
            }
        }
        Ok(())
    }
}

fn safe_target(target: &str) -> Result<()> {
    ensure!(
        !target.is_empty()
            && !target.starts_with("//")
            && !target.contains(['\\', ':', '?', '#'])
            && !target.chars().any(char::is_control),
        "Unproved part target"
    );
    let bytes = target.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            ensure!(i + 2 < bytes.len(), "Malformed percent escape");
            let digit = |b: u8| (b as char).to_digit(16);
            let a = digit(bytes[i + 1]).context("Malformed percent escape")?;
            let b = digit(bytes[i + 2]).context("Malformed percent escape")?;
            let value = (a * 16 + b) as u8;
            ensure!(
                !matches!(value, b'/' | b'\\' | b'.' | b':' | b'?' | b'#' | b'%') && value >= 0x20,
                "Encoded target separator"
            );
            decoded.push(value);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    let decoded = std::str::from_utf8(&decoded).context("Invalid percent-encoded UTF-8")?;
    ensure!(
        !decoded.chars().any(char::is_control),
        "Decoded target control"
    );
    Ok(())
}

/// Inventory only: selected payloads are always read through the same OPC
/// reader used by the converter, never through a weaker alternate ZIP path.
fn inventory(bytes: &[u8]) -> Result<BTreeMap<String, u64>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut names = BTreeMap::new();
    limit(archive.len() <= 4096)?;
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let raw = entry.name().replace('\\', "/");
        let part = PartName::new(&format!("/{raw}"))?;
        ensure!(
            names
                .insert(part.as_str().to_ascii_lowercase(), entry.size())
                .is_none(),
            "Aliased physical parts"
        );
    }
    Ok(names)
}
fn read(
    package: &mut OpcReader<Cursor<&[u8]>>,
    names: &BTreeMap<String, u64>,
    part: &PartName,
    budget: &mut Budget,
    main: bool,
) -> Result<Vec<u8>> {
    let size = *names
        .get(&part.as_str().to_ascii_lowercase())
        .context("Missing declared part")?;
    limit(
        size <= if main {
            MAIN_LIMIT as u64
        } else {
            ANCILLARY_LIMIT.saturating_sub(budget.bytes) as u64
        },
    )?;
    let data = package.read_part(part)?;
    budget.charge(&data, main)?;
    Ok(data)
}
fn roles(rels: &Relationships) -> Result<()> {
    let mut seen = BTreeSet::new();
    for rel in rels.all() {
        ensure!(
            !rel.id.is_empty() && !rel.rel_type.is_empty() && !rel.target.is_empty(),
            "Empty relationship attribute"
        );
        let role = rel.rel_type.rsplit('/').next().unwrap_or("");
        if matches!(
            role,
            "officeDocument" | "fontTable" | "styles" | "theme" | "settings"
        ) {
            ensure!(
                rel.rel_type
                    == format!(
                        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/{role}"
                    ),
                "Unproved role URI"
            );
            ensure!(seen.insert(role), "Repeated role");
            ensure!(
                rel.target_mode == TargetMode::Internal,
                "External source role"
            );
            safe_target(&rel.target)?;
        }
    }
    Ok(())
}
fn content_type(package: &OpcReader<Cursor<&[u8]>>, part: &PartName, kind: &str) -> Result<()> {
    let expected = if kind == "theme" {
        "application/vnd.openxmlformats-officedocument.theme+xml".into()
    } else {
        format!("{MIME}{kind}+xml")
    };
    ensure!(
        package
            .content_types()
            .resolve(part)
            .is_some_and(|v| v == expected),
        "Unproved part content type"
    );
    Ok(())
}

fn analyze_inner(bytes: &[u8]) -> std::result::Result<Option<Proof>, Reason> {
    let mut package = at(
        OpcReader::new(Cursor::new(bytes)).map_err(Into::into),
        Reason::Package,
    )?;
    let main = at(
        package.main_document_part().map_err(Into::into),
        Reason::Package,
    )?;
    let rels = at(
        package.read_rels_for(&main).map_err(Into::into),
        Reason::Package,
    )?;
    let names = at(inventory(bytes), Reason::Package)?;
    let mut budget = Budget { bytes: 0, nodes: 0 };
    let root_rels = at(
        PartName::new("/_rels/.rels").map_err(Into::into),
        Reason::Package,
    )?;
    let raw = at(
        read(&mut package, &names, &root_rels, &mut budget, false),
        Reason::Package,
    )?;
    at(budget.inspect(&raw), Reason::Package)?;
    at(
        crate::image_dimensions::audit_relationships(&raw),
        Reason::Package,
    )?;
    let root_relationships = at(
        Relationships::parse(&raw).map_err(Into::into),
        Reason::Package,
    )?;
    at(roles(&root_relationships), Reason::Package)?;
    let office = root_relationships.get_by_type(rel_types::OFFICE_DOCUMENT);
    if office.len() != 1 || main.as_str() != "/word/document.xml" {
        return Err(Reason::Package);
    }
    let main_rels = at(
        PartName::new(&main.rels_path()).map_err(Into::into),
        Reason::Package,
    )?;
    if !names.contains_key(&main_rels.as_str().to_ascii_lowercase()) {
        return Ok(None);
    }
    let raw = at(
        read(&mut package, &names, &main_rels, &mut budget, false),
        Reason::Package,
    )?;
    at(budget.inspect(&raw), Reason::Package)?;
    at(
        crate::image_dimensions::audit_relationships(&raw),
        Reason::Package,
    )?;
    at(roles(&rels), Reason::Package)?;
    // Only a validated authoritative relationship tree can establish absence.
    // An orphan conventional filename supplies no font-table authority.
    if !rels
        .all()
        .iter()
        .any(|r| r.rel_type.rsplit('/').next() == Some("fontTable"))
    {
        return Ok(None);
    }
    at(roles(&rels), Reason::Package)?;
    let raw = at(
        read(
            &mut package,
            &names,
            &PartName::new("/[Content_Types].xml").expect("constant part"),
            &mut budget,
            false,
        ),
        Reason::Package,
    )?;
    at(budget.inspect(&raw), Reason::Package)?;
    at(
        crate::image_dimensions::audit_content_types(&raw),
        Reason::Package,
    )?;
    at(content_type_values(&raw), Reason::Package)?;
    at(
        content_type(&package, &main, "document.main"),
        Reason::Package,
    )?;
    let font = rels.get_by_type(rel_types::FONT_TABLE);
    if font.len() != 1 {
        return Err(Reason::Package);
    }
    let font_part = at(
        main.resolve_relative(&font[0].target).map_err(Into::into),
        Reason::Package,
    )?;
    at(data_part(&font_part), Reason::Package)?;
    at(
        content_type(&package, &font_part, "fontTable"),
        Reason::Package,
    )?;
    if font_part == main || font_part == main_rels || font_part == root_rels {
        return Err(Reason::Package);
    }
    let table = at(
        read(&mut package, &names, &font_part, &mut budget, false),
        Reason::Package,
    )?;
    at(budget.inspect(&table), Reason::Table)?;
    let records = at(font_table(&table), Reason::Table)?;
    if records.is_empty() {
        return Ok(None);
    }
    let font_rels_part = at(
        PartName::new(&font_part.rels_path()).map_err(Into::into),
        Reason::Package,
    )?;
    if names.contains_key(&font_rels_part.as_str().to_ascii_lowercase()) {
        let raw = at(
            read(&mut package, &names, &font_rels_part, &mut budget, false),
            Reason::Package,
        )?;
        at(budget.inspect(&raw), Reason::Package)?;
        at(
            crate::image_dimensions::audit_relationships(&raw),
            Reason::Package,
        )?;
        let font_rels = at(
            Relationships::parse(&raw).map_err(Into::into),
            Reason::Package,
        )?;
        if !font_rels.all().is_empty() {
            return Err(Reason::Table);
        }
    }
    let mut selected = BTreeSet::from([
        main.as_str().to_ascii_lowercase(),
        font_part.as_str().to_ascii_lowercase(),
    ]);
    let mut dependencies = BTreeMap::new();
    for (kind, mime) in [
        (rel_types::STYLES, "styles"),
        (rel_types::THEME, "theme"),
        (rel_types::SETTINGS, "settings"),
    ] {
        if let Some(r) = rels.first_by_type(kind) {
            let part = at(
                main.resolve_relative(&r.target).map_err(Into::into),
                Reason::Package,
            )?;
            at(data_part(&part), Reason::Package)?;
            if !selected.insert(part.as_str().to_ascii_lowercase()) {
                return Err(Reason::Package);
            }
            at(content_type(&package, &part, mime), Reason::Package)?;
            let data = at(
                read(&mut package, &names, &part, &mut budget, false),
                Reason::Package,
            )?;
            dependencies.insert(kind, data);
        }
    }
    let main_xml = at(
        read(&mut package, &names, &main, &mut budget, true),
        Reason::Package,
    )?;
    at(budget.inspect(&main_xml), Reason::Body)?;
    let (blocks, paragraphs) = at(source_body(&main_xml), Reason::Body)?;
    for data in dependencies.values() {
        at(budget.inspect(data), Reason::Typography)?;
    }
    let mut requests: Vec<_> = paragraphs
        .iter()
        .flat_map(|p| p.runs.iter().map(move |(r, _)| (p.body_index, Some(*r))))
        .collect();
    let blank_indices: Vec<_> = paragraphs
        .iter()
        .filter(|p| p.blank)
        .map(|p| p.body_index)
        .collect();
    requests.extend(blank_indices.iter().map(|index| (*index, None)));
    if requests.is_empty() {
        return Ok(None);
    }
    let empty_styles = format!("<w:styles xmlns:w=\"{W}\"/>");
    let styles = dependencies
        .get(rel_types::STYLES)
        .map_or(empty_styles.as_bytes(), Vec::as_slice);
    let theme = dependencies.get(rel_types::THEME).map(Vec::as_slice);
    let settings = dependencies.get(rel_types::SETTINGS).map(Vec::as_slice);
    let mut work = Work::default();
    if at(
        active_numbering(styles, &main_xml, &blank_indices, &mut work),
        Reason::Typography,
    )? {
        return Err(Reason::Body);
    }
    let fonts = at(
        crate::image_dimensions::resolve_body_latin_fonts(
            &main_xml, styles, theme, settings, &requests, &mut work,
        ),
        Reason::Typography,
    )?;
    let blanks = at(
        crate::image_dimensions::resolve_empty_paragraph_fonts(
            &main_xml,
            styles,
            theme,
            &blank_indices,
        ),
        Reason::Typography,
    )?;
    if fonts.len() != requests.len() {
        return Err(Reason::Typography);
    }
    let text_count: usize = paragraphs.iter().map(|p| p.runs.len()).sum();
    let blank_names = fonts[text_count..].to_vec();
    let mut fonts = fonts[..text_count].iter();
    let mut blank_names = blank_names.into_iter();
    let mut blanks = blanks.into_iter();
    let mut targets = Vec::new();
    let mut retained = BTreeSet::new();
    for p in paragraphs {
        if p.bare_break {
            targets.push(Paragraph::BareBreak);
        } else if p.blank {
            let font = blanks.next().flatten().ok_or(Reason::Typography)?;
            if blank_names.next().as_deref() != Some(font.family.as_str()) {
                return Err(Reason::Typography);
            }
            retained.insert(font.family.clone());
            targets.push(Paragraph::Empty(font));
        } else {
            let mut ranges = Vec::new();
            for (_, text) in p.runs {
                let name = fonts.next().ok_or(Reason::Typography)?.clone();
                retained.insert(name.clone());
                append(&mut ranges, text, name);
            }
            targets.push(Paragraph::Text(ranges));
        }
    }
    if fonts.next().is_some() || blanks.next().is_some() {
        return Err(Reason::Typography);
    }
    if retained.is_empty() {
        return Ok(None);
    }
    at(limit(retained.len() <= 256), Reason::Limit)?;
    let mut out = Vec::new();
    let mut folded = BTreeSet::new();
    for name in retained {
        if !folded.insert(name.to_ascii_lowercase()) {
            return Err(Reason::Alias);
        }
        let matching: Vec<_> = records
            .iter()
            .filter(|r| r.name.eq_ignore_ascii_case(&name))
            .collect();
        if matching.is_empty() {
            return Err(Reason::Missing);
        }
        if matching.len() != 1 || matching[0].name != name {
            return Err(Reason::Alias);
        }
        let record = matching[0];
        if record.unsupported {
            return Err(Reason::Typography);
        }
        if !crate::document::supported_font_family(&name) {
            return Err(Reason::Missing);
        }
        let family = match record.family.as_deref() {
            Some("roman") => FontFamilyClass::Roman,
            Some("swiss") => FontFamilyClass::Swiss,
            _ => return Err(Reason::Missing),
        };
        out.push(FontFamilyMetadata { name, family });
    }
    Ok(Some(Proof {
        blocks,
        paragraphs: targets,
        records: out,
    }))
}

fn word_namespace(ns: ResolveResult<'_>) -> bool {
    matches!(ns, ResolveResult::Bound(n) if n.as_ref() == W || n.as_ref() == WS)
}
fn font_table(data: &[u8]) -> Result<Vec<Record>> {
    let mut reader = NsReader::from_reader(data);
    let mut stack: Vec<String> = Vec::new();
    let mut roots = 0;
    let mut records = Vec::new();
    let mut current = None::<Record>;
    let mut fields = BTreeSet::new();
    loop {
        let (ns, event) = reader.read_resolved_event()?;
        let word = word_namespace(ns);
        let start = matches!(&event, Event::Start(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                ensure!(word, "Foreign font table element");
                let tag = e.local_name().as_ref().to_owned();
                let embedded = matches!(
                    tag.as_str(),
                    "embedRegular" | "embedBold" | "embedItalic" | "embedBoldItalic"
                );
                let mut attrs = BTreeMap::new();
                let mut declarations = BTreeMap::new();
                let mut ignorable = false;
                for a in e.attributes() {
                    let a = a?;
                    if a.key.as_ref() == "xmlns" || a.key.as_ref().starts_with("xmlns:") {
                        declarations.insert(
                            a.key
                                .as_ref()
                                .strip_prefix("xmlns:")
                                .unwrap_or("")
                                .to_owned(),
                            a.normalized_value(quick_xml::XmlVersion::Implicit1_0)?
                                .into_owned(),
                        );
                        continue;
                    }
                    let (ns, local) = reader.resolver().resolve_attribute(a.key);
                    let relationship = matches!(&ns, ResolveResult::Bound(n) if n.as_ref() == "http://schemas.openxmlformats.org/officeDocument/2006/relationships" || n.as_ref() == "http://purl.oclc.org/ooxml/officeDocument/relationships");
                    let local = local.as_ref().to_owned();
                    let inert_ignorable = stack.is_empty()
                        && tag == "fonts"
                        && local == "Ignorable"
                        && matches!(&ns, ResolveResult::Bound(n) if n.as_ref() == "http://schemas.openxmlformats.org/markup-compatibility/2006");
                    ignorable |= inert_ignorable;
                    ensure!(
                        inert_ignorable
                            || word_namespace(ns)
                            || embedded && relationship && local == "id",
                        "Foreign font table attribute"
                    );
                    ensure!(
                        attrs
                            .insert(
                                local,
                                a.normalized_value(quick_xml::XmlVersion::Implicit1_0)?
                                    .into_owned()
                            )
                            .is_none(),
                        "Duplicate expanded font attribute"
                    );
                }
                match stack.as_slice() {
                    [] => {
                        roots += 1;
                        ensure!(roots == 1 && tag == "fonts", "Invalid font table root");
                        if !attrs.is_empty() {
                            ensure!(
                                ignorable && attrs.len() == 1,
                                "Unsupported font table root attribute"
                            );
                            let value =
                                attrs.get("Ignorable").context("Missing Ignorable value")?;
                            let prefixes: Vec<_> = value.split_whitespace().collect();
                            ensure!(
                                !prefixes.is_empty()
                                    && prefixes.iter().all(|p| declarations
                                        .get(*p)
                                        .is_some_and(|ns| !ns.is_empty()))
                                    && prefixes.iter().collect::<BTreeSet<_>>().len()
                                        == prefixes.len(),
                                "Unproved Ignorable prefix"
                            );
                        }
                    }
                    [root] if root == "fonts" => {
                        ensure!(
                            tag == "font" && attrs.len() == 1,
                            "Invalid direct font definition"
                        );
                        let name = attrs.remove("name").context("Missing font name")?;
                        ensure!(!name.is_empty(), "Empty font name");
                        limit(records.len() < 256)?;
                        current = Some(Record {
                            name,
                            family: None,
                            unsupported: false,
                        });
                        fields.clear();
                    }
                    [root, font] if root == "fonts" && font == "font" => {
                        ensure!(fields.insert(tag.clone()), "Repeated font property");
                        let record = current.as_mut().context("Missing font definition")?;
                        let val = attrs.get("val").map(String::as_str);
                        let only_val = attrs.keys().all(|k| k == "val");
                        let hex = |value: Option<&str>, len: usize| {
                            value.is_some_and(|s| {
                                s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit())
                            })
                        };
                        match tag.as_str() {
                            "family" => {
                                ensure!(
                                    only_val
                                        && matches!(
                                            val,
                                            Some(
                                                "auto"
                                                    | "decorative"
                                                    | "modern"
                                                    | "roman"
                                                    | "script"
                                                    | "swiss"
                                            )
                                        ),
                                    "Invalid family enum"
                                );
                                record.family = val.map(str::to_owned);
                            }
                            "altName" => {
                                ensure!(
                                    only_val && val.is_some_and(|s| !s.is_empty()),
                                    "Invalid alternate name"
                                );
                                record.unsupported = true;
                            }
                            "charset" => {
                                ensure!(only_val && hex(val, 2), "Invalid font charset");
                                record.unsupported |= val != Some("00");
                            }
                            "panose1" => ensure!(only_val && hex(val, 20), "Invalid PANOSE"),
                            "pitch" => ensure!(
                                only_val && matches!(val, Some("default" | "fixed" | "variable")),
                                "Invalid pitch"
                            ),
                            "notTrueType" => ensure!(
                                only_val
                                    && matches!(
                                        val,
                                        None | Some("1" | "true" | "on" | "0" | "false" | "off")
                                    ),
                                "Invalid font flag"
                            ),
                            "sig" => {
                                ensure!(
                                    attrs.len() == 6
                                        && ["usb0", "usb1", "usb2", "usb3", "csb0", "csb1"]
                                            .iter()
                                            .all(|key| hex(attrs.get(*key).map(String::as_str), 8)),
                                    "Invalid font signature"
                                );
                            }
                            _ if embedded => {
                                ensure!(
                                    attrs.keys().all(|k| matches!(
                                        k.as_str(),
                                        "id" | "fontKey" | "subsetted"
                                    )) && attrs.get("id").is_some_and(|s| !s.is_empty()),
                                    "Invalid font embedding"
                                );
                                record.unsupported = true;
                            }
                            _ => bail!("Unsupported font table content"),
                        }
                    }
                    _ => bail!("Wrapped font table content"),
                }
                if start {
                    stack.push(tag);
                } else if tag == "font" {
                    records.push(current.take().context("Missing empty font")?);
                }
            }
            Event::End(_) => {
                let tag = stack.pop().context("Unbalanced font table")?;
                if tag == "font" {
                    records.push(current.take().context("Missing font")?);
                }
            }
            Event::Text(t) => ensure!(
                t.xml_content(quick_xml::XmlVersion::Implicit1_0)
                    .trim()
                    .is_empty(),
                "Unexpected font table text"
            ),
            Event::CData(_) | Event::GeneralRef(_) | Event::DocType(_) => {
                bail!("Unsupported font table XML content")
            }
            Event::Eof => {
                ensure!(
                    roots == 1 && stack.is_empty() && current.is_none(),
                    "Incomplete font table"
                );
                return Ok(records);
            }
            _ => {}
        }
    }
}
fn latin(ch: char) -> bool {
    (' '..='~').contains(&ch)
        || ('\u{a0}'..='\u{ff}').contains(&ch) && ch != 'µ'
        || "€‚ƒ„…†‡ˆ‰Š‹ŒŽ‘’“”•–—˜™š›œžŸ".contains(ch)
}
fn inert(n: &Xml) -> Result<bool> {
    if !matches!(
        n.tag.as_str(),
        "bookmarkStart" | "bookmarkEnd" | "proofErr" | "lastRenderedPageBreak"
    ) {
        return Ok(false);
    }
    n.identity()?;
    ensure!(
        n.children.is_empty() && n.text.is_empty(),
        "Nested body marker"
    );
    Ok(true)
}
fn source_body(data: &[u8]) -> Result<(Vec<Block>, Vec<SourceParagraph>)> {
    let tree = xml(data)?;
    let doc = root(&tree, "document")?;
    ensure!(doc.children.len() == 1, "Unsupported document wrapper");
    let body = doc.one("body")?.context("Missing document body")?;
    ensure!(body.text.trim().is_empty(), "Uncontained body text");
    let mut paragraphs = Vec::new();
    let mut scalar_count = 0usize;
    let mut run_count = 0usize;
    let mut section = false;
    for (body_index, p) in body.children.iter().enumerate() {
        p.identity()?;
        if p.tag == "sectPr" {
            ensure!(
                !section && body_index + 1 == body.children.len(),
                "Unsupported section position"
            );
            section = true;
            continue;
        }
        ensure!(
            p.tag == "p" && p.text.trim().is_empty(),
            "Unsupported body block"
        );
        limit(paragraphs.len() < 10000)?;
        if let Some(props) = p.one("pPr")? {
            ensure!(
                props.all("numPr").is_empty(),
                "Numbered paragraph outside body subset"
            );
        }
        let mut source = SourceParagraph {
            body_index,
            runs: Vec::new(),
            blank: false,
            bare_break: false,
        };
        if !p.all("br").is_empty() {
            ensure!(p.children.len() == 1, "Styled or mixed page break");
            let r = &p.children[0];
            r.identity()?;
            ensure!(
                r.tag == "r" && r.children.len() == 1 && r.text.is_empty(),
                "Styled or mixed break run"
            );
            let br = &r.children[0];
            ensure!(br.tag == "br", "Unproved break node");
            br.schema(&["type"], true)?;
            ensure!(br.attr("type")? == Some("page"), "Not a bare page break");
            source.bare_break = true;
        } else if true_blank(p)? {
            source.blank = true;
        } else {
            for (run_index, r) in p.children.iter().enumerate() {
                if r.tag == "pPr" {
                    r.identity()?;
                    continue;
                }
                if inert(r)? {
                    continue;
                }
                r.identity()?;
                ensure!(
                    r.tag == "r" && r.text.trim().is_empty(),
                    "Unsupported body inline wrapper"
                );
                r.one("rPr")?;
                let mut text = String::new();
                for t in &r.children {
                    if t.tag == "rPr" {
                        t.identity()?;
                        continue;
                    }
                    if inert(t)? {
                        continue;
                    }
                    t.identity()?;
                    ensure!(
                        t.tag == "t" && t.children.is_empty(),
                        "Unsupported run content"
                    );
                    for (name, (value, _)) in &t.attrs {
                        ensure!(
                            name == "xml:space"
                                && matches!(value.as_str(), "default" | "preserve")
                                && t.attribute_namespaces
                                    .get(name)
                                    .is_some_and(|ns| ns == "http://www.w3.org/XML/1998/namespace"),
                            "Unproved text attribute"
                        );
                    }
                    ensure!(t.text.chars().all(latin), "Outside Latin text subset");
                    scalar_count = scalar_count
                        .checked_add(t.text.chars().count())
                        .ok_or(Limit)?;
                    limit(scalar_count <= 1_000_000)?;
                    text.push_str(&t.text);
                }
                if !text.is_empty() {
                    run_count += 1;
                    limit(run_count <= 10000)?;
                    source.runs.push((run_index, text));
                } else {
                    ensure!(r.children.is_empty(), "Unproved empty run metrics");
                }
            }
            ensure!(!source.runs.is_empty(), "Unproved empty paragraph identity");
        }
        paragraphs.push(source);
    }
    let blocks = source_body_identity(data)?;
    ensure!(
        blocks.len() == paragraphs.len(),
        "Source body identity differs"
    );
    Ok((blocks, paragraphs))
}
fn append(ranges: &mut Vec<Range>, text: String, name: String) {
    if let Some(last) = ranges.last_mut()
        && last.name == name
    {
        last.text.push_str(&text);
    } else {
        ranges.push(Range { text, name });
    }
}
fn bind(proof: &Proof, document: &Node) -> Result<()> {
    verify_body_identity(&proof.blocks, document)?;
    ensure!(
        document.content.len() == proof.paragraphs.len(),
        "Body count differs"
    );
    for (source, actual) in proof.paragraphs.iter().zip(&document.content) {
        match source {
            Paragraph::BareBreak => ensure!(
                actual.kind == "pageBreak"
                    && actual.attrs.is_empty()
                    && actual.content.is_empty()
                    && actual.text.is_none()
                    && actual.marks.is_empty(),
                "Bare page-break identity differs"
            ),
            Paragraph::Empty(font) => ensure!(
                crate::document::empty_paragraph_font(actual)?.as_ref() == Some(font),
                "Retained empty baseline differs"
            ),
            Paragraph::Text(ranges) => {
                ensure!(
                    matches!(actual.kind.as_str(), "paragraph" | "heading"),
                    "Text block differs"
                );
                let mut actual_ranges = Vec::new();
                for text in &actual.content {
                    ensure!(text.kind == "text", "Unexpected imported inline node");
                    let marks: Vec<_> = text
                        .marks
                        .iter()
                        .filter(|m| m.kind == "fontFamily")
                        .collect();
                    ensure!(marks.len() == 1, "Missing or ambiguous imported font");
                    let name = marks[0]
                        .attrs
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .context("Invalid imported font")?;
                    let value = text.text.as_deref().context("Missing imported text")?;
                    ensure!(!value.is_empty(), "Unexpected empty imported range");
                    append(&mut actual_ranges, value.into(), name.into());
                }
                ensure!(actual_ranges == *ranges, "Imported font ranges differ");
            }
        }
    }
    Ok(())
}

pub(crate) fn attach(audit: Audit, document: &mut Node, risks: &mut BTreeSet<String>) {
    let proof = match audit.0 {
        Outcome::Absent => return,
        Outcome::Unavailable(reason) => {
            risks.insert(reason.message());
            return;
        }
        Outcome::Proved(proof) => proof,
    };
    if bind(&proof, document).is_err() {
        risks.insert(Reason::Binding.message());
        return;
    }
    if document.attrs.contains_key("fontFamilyMetadata") {
        risks.insert(Reason::Candidate.message());
        return;
    }
    let mut candidate = document.clone();
    let Ok(records) = serde_json::to_value(&proof.records) else {
        risks.insert(Reason::Candidate.message());
        return;
    };
    candidate.attrs.insert("fontFamilyMetadata".into(), records);
    if crate::document::validate(&candidate).is_err() {
        risks.insert(Reason::Candidate.message());
        return;
    }
    *document = candidate;
    risks.insert(PRESERVED.into());
}

fn content_type_values(data: &[u8]) -> Result<()> {
    let mut reader = quick_xml::Reader::from_reader(data);
    loop {
        match reader.read_event()? {
            Event::Start(e) | Event::Empty(e)
                if matches!(e.local_name().as_ref(), "Default" | "Override") =>
            {
                for attr in e.attributes() {
                    let attr = attr?;
                    let value = attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)?;
                    match attr.key.as_ref() {
                        "ContentType" | "Extension" => {
                            ensure!(!value.is_empty(), "Empty content type attribute")
                        }
                        "PartName" => safe_target(&value)?,
                        _ => {}
                    }
                }
            }
            Event::Eof => return Ok(()),
            _ => {}
        }
    }
}

fn active_numbering(styles: &[u8], main: &[u8], blanks: &[usize], work: &mut Work) -> Result<bool> {
    let sheet = xml(styles)?;
    let sheet = root(&sheet, "styles")?;
    work.spend(2 * sheet.size)?;
    let mut index = BTreeMap::new();
    let mut default = None;
    for style in sheet.children.iter().filter(|s| s.tag == "style") {
        style.identity()?;
        let id = style.attr("styleId")?.context("Missing style identity")?;
        ensure!(
            index.insert(id, style).is_none(),
            "Duplicate style identity"
        );
        if style.attr("type")?.unwrap_or("paragraph") == "paragraph"
            && matches!(style.attr("default")?, Some("1" | "true" | "on"))
        {
            ensure!(default.replace(id).is_none(), "Ambiguous default style");
        }
    }
    limit(index.len() <= 4093)?;
    if let Some(d) = sheet.one("docDefaults")?
        && let Some(r) = d.one("rPrDefault")?
        && let Some(r) = r.one("rPr")?
    {
        reader_properties(r, work)?;
    }
    if let Some(d) = sheet.one("docDefaults")?
        && let Some(p) = d.one("pPrDefault")?
        && let Some(p) = p.one("pPr")?
        && !p.all("numPr").is_empty()
    {
        return Ok(true);
    }
    let doc = xml(main)?;
    let body = root(&doc, "document")?
        .one("body")?
        .context("Missing body")?;
    let mut checked = BTreeSet::new();
    work.spend(2 * body.size)?;
    let blanks: BTreeSet<_> = blanks.iter().copied().collect();
    let mut blank_styles = BTreeSet::new();
    let mut lengths = BTreeMap::new();
    for (body_index, p) in body
        .children
        .iter()
        .enumerate()
        .filter(|(_, p)| p.tag == "p")
    {
        work.spend(3 * p.size)?;
        let direct = p.one("pPr")?;
        if let Some(p) = direct
            && let Some(r) = p.one("rPr")?
        {
            reader_properties(r, work)?;
        }
        let explicit = direct.map(|p| p.one("pStyle")).transpose()?.flatten();
        let raw_style = explicit
            .map(|s| s.attr("val")?.context("Missing paragraph style"))
            .transpose()?;
        let mut next = raw_style.or(default);
        if blanks.contains(&body_index) {
            blank_styles.insert((raw_style, next));
        }
        if !checked.insert(next) {
            continue;
        }
        let initial = next;
        let mut chain = BTreeSet::new();
        while let Some(id) = next {
            ensure!(chain.insert(id), "Cyclic active style");
            work.links(1)?;
            limit(chain.len() <= 64)?;
            let style = index.get(id).context("Missing active style")?;
            work.spend(4 * style.size)?;
            if let Some(r) = style.one("rPr")? {
                reader_properties(r, work)?;
            }
            ensure!(
                style.attr("type")?.unwrap_or("paragraph") == "paragraph",
                "Wrong active style type"
            );
            if let Some(p) = style.one("pPr")?
                && !p.all("numPr").is_empty()
            {
                return Ok(true);
            }
            next = style
                .one("basedOn")?
                .map(|s| s.attr("val"))
                .transpose()?
                .flatten();
        }
        lengths.insert(initial, chain.len());
    }
    // Charge the future existing empty-font resolver's cached style chains.
    for (_, id) in blank_styles {
        work.links(*lengths.get(&id).unwrap_or(&0))?;
    }
    let mut checked_characters = BTreeSet::new();
    for r in body.all("r") {
        work.spend(3 * r.size)?;
        let direct = r.one("rPr")?;
        if let Some(r) = direct {
            reader_properties(r, work)?;
        }
        let reference = direct.map(|r| r.one("rStyle")).transpose()?.flatten();
        let Some(reference) = reference else {
            continue;
        };
        let id = reference.attr("val")?.context("Missing character style")?;
        if !checked_characters.insert(id) {
            continue;
        }
        let mut next = Some(id);
        let mut chain = BTreeSet::new();
        while let Some(id) = next {
            ensure!(chain.insert(id), "Cyclic character style");
            work.links(1)?;
            limit(chain.len() <= 64)?;
            let style = index.get(id).context("Missing character style")?;
            work.spend(4 * style.size)?;
            if let Some(r) = style.one("rPr")? {
                reader_properties(r, work)?;
            }
            ensure!(
                style.attr("type")? == Some("character"),
                "Wrong character style type"
            );
            next = style
                .one("basedOn")?
                .map(|s| s.attr("val"))
                .transpose()?
                .flatten();
        }
    }
    Ok(false)
}

fn data_part(part: &PartName) -> Result<()> {
    let name = part.as_str().to_ascii_lowercase();
    ensure!(
        name != "/[content_types].xml" && !(name.contains("/_rels/") && name.ends_with(".rels")),
        "Data role aliases an OPC control part"
    );
    Ok(())
}

/// New-proof-only reader identity guard; unrelated template styles are not
/// inspected here and existing converter behavior stays unchanged.
fn reader_properties(props: &Xml, work: &mut Work) -> Result<()> {
    work.spend(3 * props.size)?;
    props.identity()?;
    for child in &props.children {
        child.identity()?;
        ensure!(child.children.is_empty(), "Wrapped reader font property");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node() -> Node {
        serde_json::from_value(json!({"type":"doc","attrs":{"styleProfile":"standard-v2"},"content":[
            {"type":"paragraph","content":[{"type":"text","text":"one two","marks":[{"type":"fontFamily","attrs":{"name":"Probe Face"}},{"type":"textColor","attrs":{"rgb":"558ED5"}}]}]}
        ]})).unwrap()
    }
    fn proof() -> Proof {
        Proof {
            blocks: vec![Block::Text("one two".into())],
            paragraphs: vec![Paragraph::Text(vec![Range {
                text: "one two".into(),
                name: "Probe Face".into(),
            }])],
            records: vec![FontFamilyMetadata {
                name: "Probe Face".into(),
                family: FontFamilyClass::Swiss,
            }],
        }
    }
    #[test]
    fn atomic_positive_adds_only_root_field_and_exact_message() {
        let mut doc = node();
        let old = serde_json::to_value(&doc).unwrap();
        let mut risks = BTreeSet::from(["preexisting risk".into()]);
        attach(Audit(Outcome::Proved(proof())), &mut doc, &mut risks);
        assert_eq!(
            risks,
            BTreeSet::from(["preexisting risk".into(), PRESERVED.into()])
        );
        assert_eq!(
            doc.attrs.remove("fontFamilyMetadata").unwrap(),
            json!([{"name":"Probe Face","family":"swiss"}])
        );
        assert_eq!(serde_json::to_value(doc).unwrap(), old);
    }
    #[test]
    fn absent_is_exact_and_late_binding_failure_never_leaks_metadata() {
        let mut cases = vec![node(); 5];
        cases[0].content[0].content[0].text = Some("two one".into());
        cases[1].content[0].content[0].marks[0]
            .attrs
            .insert("name".into(), "Other".into());
        cases[2].content[0].content[0].marks.remove(0);
        let duplicate = cases[3].content[0].clone();
        cases[3].content.push(duplicate);
        cases[4].content[0].kind = "pageBreak".into();
        for mut doc in cases {
            let old = serde_json::to_value(&doc).unwrap();
            let mut risks = BTreeSet::from(["preexisting".into()]);
            attach(Audit(Outcome::Absent), &mut doc, &mut risks);
            assert_eq!(serde_json::to_value(&doc).unwrap(), old);
            assert_eq!(risks, BTreeSet::from(["preexisting".into()]));
            attach(Audit(Outcome::Proved(proof())), &mut doc, &mut risks);
            assert_eq!(serde_json::to_value(doc).unwrap(), old);
            assert_eq!(
                risks,
                BTreeSet::from(["preexisting".into(), Reason::Binding.message()])
            );
        }
    }
    #[test]
    fn late_candidate_validation_rolls_back_document_and_success_message() {
        let mut doc = node();
        doc.attrs.insert("unknown".into(), true.into());
        let old = serde_json::to_value(&doc).unwrap();
        let mut risks = BTreeSet::from(["existing".into()]);
        attach(Audit(Outcome::Proved(proof())), &mut doc, &mut risks);
        assert_eq!(serde_json::to_value(doc).unwrap(), old);
        assert_eq!(
            risks,
            BTreeSet::from(["existing".into(), Reason::Candidate.message()])
        );
    }
    #[test]
    fn exact_range_binding_permits_format_splits_without_normalizing_text() {
        let mut doc = node();
        let mut first = doc.content[0].content[0].clone();
        first.text = Some("one ".into());
        let mut second = first.clone();
        second.text = Some("two".into());
        second.marks.pop();
        doc.content[0].content = vec![first, second];
        assert!(bind(&proof(), &doc).is_ok());
        doc.content[0].content[0].text = Some("one".into());
        assert!(bind(&proof(), &doc).is_err());
    }
    #[test]
    fn table_expanded_identity_accepts_alias_but_not_duplicate_or_foreign_family() {
        let data = format!(
            "<x:fonts xmlns:x=\"{W}\"><x:font x:name=\"A &amp; B\"><x:family x:val=\"roman\"/></x:font></x:fonts>"
        );
        let records = font_table(data.as_bytes()).unwrap();
        assert_eq!(records[0].name, "A & B");
        assert_eq!(records[0].family.as_deref(), Some("roman"));
        for children in [
            "<w:family w:val=\"swiss\"/><w:family w:val=\"swiss\"/>",
            "<w:family w:val=\"unknown\"/>",
            "<x:family xmlns:x=\"urn:foreign\" w:val=\"swiss\"/>",
            "<w:extension><w:family w:val=\"swiss\"/></w:extension>",
        ] {
            let data = format!(
                "<w:fonts xmlns:w=\"{W}\"><w:font w:name=\"A\">{children}</w:font></w:fonts>"
            );
            assert!(font_table(data.as_bytes()).is_err(), "{children}");
        }
    }
    #[test]
    fn resource_budget_counts_depth_attributes_and_aggregate_nodes() {
        let mut budget = Budget { bytes: 0, nodes: 0 };
        let nested = format!("{}{}", "<w>".repeat(129), "</w>".repeat(129));
        assert!(
            budget
                .inspect(nested.as_bytes())
                .unwrap_err()
                .downcast_ref::<Limit>()
                .is_some()
        );
        let mut budget = Budget {
            bytes: 0,
            nodes: 100_000,
        };
        assert!(
            budget
                .inspect(b"<w/>")
                .unwrap_err()
                .downcast_ref::<Limit>()
                .is_some()
        );
        let mut budget = Budget { bytes: 0, nodes: 0 };
        let oversized = format!("<w a=\"{}\"/>", "a".repeat(16_385));
        assert!(
            budget
                .inspect(oversized.as_bytes())
                .unwrap_err()
                .downcast_ref::<Limit>()
                .is_some()
        );
    }
    #[test]
    fn shared_budget_reserves_distinct_blank_cache_keys_and_checked_products() {
        let styles = format!(
            "<w:styles xmlns:w=\"{W}\"><w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"/></w:styles>"
        );
        let main = format!(
            "<w:document xmlns:w=\"{W}\"><w:body><w:p/><w:p><w:pPr><w:pStyle w:val=\"Normal\"/></w:pPr></w:p></w:body></w:document>"
        );
        let mut work = Work::default();
        assert!(!active_numbering(styles.as_bytes(), main.as_bytes(), &[0, 1], &mut work).unwrap());
        // One actual preflight chain; the existing blank resolver subsequently
        // traverses it twice for distinct None and Some("Normal") cache keys.
        assert_eq!(work.links, 3);
        assert!(
            work.spend_product(usize::MAX, 2)
                .unwrap_err()
                .downcast_ref::<Limit>()
                .is_some()
        );
    }
    #[test]
    fn selected_targets_require_complete_utf8_and_no_decoded_controls() {
        for target in [
            "font%FFTable.xml",
            "font%C2.xml",
            "font%80.xml",
            "font%C0%AF.xml",
            "font%7f.xml",
            "font%C2%80.xml",
            "font%00.xml",
            "font%.xml",
            "font%2fpart.xml",
        ] {
            assert!(safe_target(target).is_err(), "{target}");
        }
        for target in [
            "fontTable.xml",
            "../metadata/fontTable.xml",
            "fonts/caf%C3%A9.xml",
            "fonts/café.xml",
        ] {
            assert!(safe_target(target).is_ok(), "{target}");
        }
    }
    #[test]
    fn bare_break_and_blank_are_different_binding_obligations() {
        let mut proof = proof();
        proof.blocks.push(Block::PageBreak);
        proof.paragraphs.push(Paragraph::BareBreak);
        let mut doc = node();
        doc.content
            .push(serde_json::from_value(json!({"type":"pageBreak"})).unwrap());
        assert!(bind(&proof, &doc).is_ok());
        doc.content[1]
            .attrs
            .insert("breakParagraph".into(), json!({}));
        assert!(bind(&proof, &doc).is_err());
        proof.blocks[1] = Block::Text(String::new());
        proof.paragraphs[1] = Paragraph::Empty(ImageParagraphFont {
            family: "Arial".into(),
            half_points: 22,
            bold: false,
            italic: false,
        });
        doc.content[1] = serde_json::from_value(json!({"type":"paragraph"})).unwrap();
        assert!(bind(&proof, &doc).is_err());
        doc.content[1].attrs.insert(
            "emptyParagraphFont".into(),
            json!({"family":"Arial","halfPoints":22,"bold":false,"italic":false}),
        );
        assert!(bind(&proof, &doc).is_ok());
    }
}
