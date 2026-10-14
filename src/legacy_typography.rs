//! Independent, deliberately bounded source proof for binary DOC Latin fonts
//! and encoded Normal paragraph identity and spacing values.
//!
//! This does not trust permissive legacy formatting recovery, implement a general
//! Word style engine, or promise appearance. All reads use the import snapshot.
use crate::document::{
    BreakParagraph, BreakParagraphFont, FontFamilyClass, FontFamilyMetadata, Mark, Node,
};
use anyhow::{Context, Result, ensure};
use office_oxide::cfb::{CfbReader, EntryType};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
    ops::Range,
};

const MAX_PROPERTIES: usize = 1024 * 1024;
const MAX_RUNS: usize = 10_000;
const MAX_CP: usize = 1_000_000;
const PRESERVED: &str = "Legacy DOC literal Latin font names and effective sizes are preserved for audited body text; layout and local font substitution may differ";
const NORMAL_PRESERVED: &str =
    "Legacy DOC encoded Normal paragraph identity is preserved for audited body paragraphs";

#[derive(Debug)]
pub(crate) enum Audit {
    Proven(Vec<Block>),
    Unsupported(BTreeSet<String>),
}
#[derive(Debug)]
pub(crate) enum Block {
    /// Requires built-in Normal (istd/sti 0, paragraph kind), unbased, unlinked
    /// and unrevisioned, with no active paragraph modifiers except spacing.
    /// In particular, outline declarations and actual heading styles are never
    /// represented here. A future broader font proof needs a separate variant;
    /// font evidence alone must not authorize removal of heading identity.
    NormalParagraph {
        runs: Vec<Run>,
        spacing: ParagraphSpacing,
    },
    /// Exact standalone U+000C/CR source pair. Metrics alone do not authorize
    /// the document flag: the DOP and section proof must promote every pair.
    SourcePageBreak {
        cp_end: u32,
        metrics: std::result::Result<BreakParagraph, String>,
        families: [u8; 4],
        split: bool,
        fallback: Option<String>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EncodedLineSpacing {
    Multiple(u16),
    Minimum(u16),
    Exact(u16),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ParagraphSpacing {
    before: u16,
    after: u16,
    line: EncodedLineSpacing,
}
impl Default for ParagraphSpacing {
    fn default() -> Self {
        // MS-DOC paragraph defaults: zero gaps and single line spacing.
        // These are source values, not the editor profile's 0/120/360.
        Self {
            before: 0,
            after: 0,
            line: EncodedLineSpacing::Multiple(240),
        }
    }
}
impl ParagraphSpacing {
    fn supported_line(self) -> std::result::Result<u16, String> {
        match self.line {
            EncodedLineSpacing::Multiple(value) if (120..=2400).contains(&value) => Ok(value),
            // A zero minimum has the normative single-line lower bound.
            // A zero multiple, by contrast, is valid but unsupported.
            EncodedLineSpacing::Minimum(0) => Ok(240),
            EncodedLineSpacing::Multiple(value) => Err(format!(
                "line multiple {value}/240 is outside supported 120–2400 units"
            )),
            EncodedLineSpacing::Minimum(value) => Err(format!(
                "minimum line spacing of {value} twips is outside the supported multiple-line subset"
            )),
            EncodedLineSpacing::Exact(value) => Err(format!(
                "exact line spacing of {value} twips is outside the supported multiple-line subset"
            )),
        }
    }
}
#[derive(Debug)]
pub(crate) struct Run {
    text: String,
    font: String,
    families: [u8; 2],
    hps: u16,
    direct_hps: Option<u16>,
}
#[derive(Clone, Debug)]
struct Properties {
    ascii: u16,
    other: u16,
    hps: u16,
    direct_hps: Option<u16>,
}
#[derive(Debug)]
struct Font {
    name: String,
    charset: u8,
    family: u8,
}

/// Font-table declarations have document-wide scope. Commit all active source
/// classifications together, or none; an ambiguous entry must never choose the
/// metadata of whichever FFN record happened to be visited first.
#[derive(Default)]
struct FamilyMetadataProof {
    records: BTreeMap<String, FontFamilyClass>,
    aliases: BTreeMap<String, String>,
    reasons: BTreeSet<String>,
}
impl FamilyMetadataProof {
    fn add(&mut self, name: &str, families: &[u8]) {
        if families.iter().any(|family| !matches!(family, 1 | 2)) {
            self.reasons.insert(
                "an emitted Latin font has a valid family outside the supported Roman/Swiss subset"
                    .into(),
            );
            return;
        }
        if !families.iter().all(|family| *family == families[0]) {
            self.reasons.insert(
                "effective Latin slots or retained break characters disagree on font family".into(),
            );
            return;
        }
        let family = if families[0] == 1 {
            FontFamilyClass::Roman
        } else {
            FontFamilyClass::Swiss
        };
        let folded = name.to_ascii_lowercase();
        if self
            .aliases
            .get(&folded)
            .is_some_and(|previous| previous != name)
        {
            self.reasons
                .insert("emitted font names contain distinct ASCII-case aliases".into());
        }
        self.aliases.insert(folded, name.to_owned());
        if self
            .records
            .get(name)
            .is_some_and(|previous| *previous != family)
        {
            self.reasons
                .insert("one emitted literal font name has conflicting source families".into());
        }
        self.records.insert(name.to_owned(), family);
    }
    fn metadata(&self) -> Vec<FontFamilyMetadata> {
        if self.reasons.is_empty() {
            self.records
                .iter()
                .map(|(name, family)| FontFamilyMetadata {
                    name: name.clone(),
                    family: *family,
                })
                .collect()
        } else {
            Vec::new()
        }
    }
}
#[derive(Debug)]
struct Style<'a> {
    base: &'a [u8],
    record_length: usize,
    name_empty: bool,
    upx: Vec<&'a [u8]>,
}
#[derive(Clone, Debug)]
struct FkpRun<'a> {
    range: Range<usize>,
    style: Option<u16>,
    properties: &'a [u8],
}

fn bytes(data: &[u8], offset: usize, length: usize) -> Result<&[u8]> {
    let end = offset
        .checked_add(length)
        .context("DOC typography offset overflow")?;
    data.get(offset..end)
        .context("DOC typography structure is truncated")
}
fn u16_at(data: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(bytes(data, offset, 2)?.try_into()?))
}
fn u32_at(data: &[u8], offset: usize) -> Result<usize> {
    Ok(u32::from_le_bytes(bytes(data, offset, 4)?.try_into()?) as usize)
}
fn utf16(data: &[u8]) -> Result<String> {
    ensure!(
        data.len().is_multiple_of(2),
        "DOC typography UTF-16 byte length is odd"
    );
    String::from_utf16(
        &data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|v| u16::from_le_bytes([v[0], v[1]]))
            .collect::<Vec<_>>(),
    )
    .context("DOC typography name contains invalid UTF-16")
}
fn unsupported(reasons: &mut BTreeSet<String>, reason: impl Into<String>) {
    reasons.insert(reason.into());
}
fn stream(reader: &mut CfbReader<Cursor<&[u8]>>, name: &str, limit: usize) -> Result<Vec<u8>> {
    // No ambiguous embedded/orphan duplicate can silently select our evidence.
    ensure!(
        reader
            .entries()
            .iter()
            .filter(|e| e.entry_type != EntryType::Empty && e.name.eq_ignore_ascii_case(name))
            .count()
            == 1,
        "DOC typography stream {name} is missing or ambiguous"
    );
    let index = reader
        .find_entry(name)
        .with_context(|| format!("DOC typography stream {name} is not a root stream"))?;
    let length = reader.entries()[index].stream_size;
    ensure!(
        length <= limit as u64,
        "DOC typography stream exceeds source bound"
    );
    let data = reader.read_stream_by_index(index)?;
    ensure!(
        data.len() as u64 == length && reader.truncated_streams().is_empty(),
        "DOC typography stream is truncated"
    );
    Ok(data)
}

pub(crate) fn audit(source: &[u8]) -> Result<(Audit, Option<crate::legacy_page_geometry::Audit>)> {
    audit_inner(source).context("Cannot prove legacy DOC typography; source remains read-only")
}
fn audit_inner(source: &[u8]) -> Result<(Audit, Option<crate::legacy_page_geometry::Audit>)> {
    let mut cfb = CfbReader::new(Cursor::new(source))?;
    ensure!(
        cfb.warnings().is_empty(),
        "DOC compound structure reported uncertainty"
    );
    let word = stream(&mut cfb, "WordDocument", source.len())?;
    let flags = u16_at(&word, 10)?;
    let table = stream(
        &mut cfb,
        if flags & 0x200 != 0 {
            "1Table"
        } else {
            "0Table"
        },
        source.len(),
    )?;
    let mut typography = audit_streams(&word, &table)?;
    // Do not broaden or alter the historical Unsupported typography route.
    // Its source identity is not strong enough to authorize page attachment.
    let page = if let Audit::Proven(blocks) = &mut typography {
        // The proof decodes one Windows-1252 source byte to one Unicode scalar.
        // Reuse that exact body identity to locate possible section-end marks;
        // no second piece-table parser or inferred paragraph boundary is needed.
        let mut cp = 0u32;
        let mut break_ends = BTreeSet::new();
        for block in blocks.iter() {
            match block {
                Block::NormalParagraph { runs, .. } => {
                    for run in runs {
                        cp += run.text.chars().count() as u32;
                    }
                    cp += 1; // Encoded CR, including true empty paragraphs.
                }
                Block::SourcePageBreak { cp_end, .. } => {
                    cp += 1; // Encoded U+000C before the paragraph CR.
                    ensure!(*cp_end == cp, "DOC break paragraph CP identity mismatch");
                    break_ends.insert(cp);
                    cp += 1;
                }
            }
        }
        ensure!(
            cp as usize == u32_at(&word, 0x4c)?,
            "DOC page body CP identity mismatch"
        );
        let page = crate::legacy_page_geometry::audit_streams(&word, &table, &break_ends)?;
        promote_break_paragraphs(blocks, &page, &break_ends);
        Some(page)
    } else {
        None
    };
    Ok((typography, page))
}

fn promote_break_paragraphs(
    blocks: &mut [Block],
    page: &crate::legacy_page_geometry::Audit,
    break_ends: &BTreeSet<u32>,
) {
    if break_ends.is_empty() || !page.requests_split_break_semantics() {
        return;
    }
    let proof = page.split_break_semantics(break_ends).and_then(|()| {
        for block in blocks.iter() {
            if let Block::SourcePageBreak { metrics, .. } = block {
                metrics.as_ref().map_err(Clone::clone)?;
            }
        }
        Ok(())
    });
    for block in blocks {
        if let Block::SourcePageBreak {
            split, fallback, ..
        } = block
        {
            *split = proof.is_ok();
            *fallback = proof.as_ref().err().cloned();
        }
    }
}

fn audit_streams(word: &[u8], table: &[u8]) -> Result<Audit> {
    let mut reasons = BTreeSet::new();
    ensure!(u16_at(word, 0)? == 0xa5ec, "Invalid Word97+ FIB identity");
    ensure!(
        u16_at(word, 10)? & 0x8100 == 0,
        "Encrypted or obfuscated DOC typography is unsupported"
    );
    if u16_at(word, 10)? & 0x2000 != 0 {
        unsupported(
            &mut reasons,
            "FIB installation-language override replaces source Normal fonts",
        );
    }
    // Validate variable FIB section sizes before using any Word97 offsets.
    let csw = u16_at(word, 32)? as usize;
    let lw = 34 + 2 * csw;
    let cslw = u16_at(word, lw)? as usize;
    let fc = lw + 2 + 4 * cslw;
    let count = u16_at(word, fc)? as usize;
    let new_at = fc + 2 + 8 * count;
    let csw_new = u16_at(word, new_at)? as usize;
    bytes(word, new_at + 2, csw_new * 2)?;
    ensure!(
        csw == 14 && cslw == 22 && fc == 0x98,
        "Invalid Word97+ FIB section sizes"
    );
    let base_version = u16_at(word, 2)?;
    let version = if csw_new == 0 {
        base_version
    } else {
        u16_at(word, new_at + 2)?
    };
    let expected = match version {
        0xc1 => Some((93, 0)),
        0xd9 => Some((108, 2)),
        0x101 => Some((136, 2)),
        0x10c => Some((164, 2)),
        0x112 => Some((183, 5)),
        _ => None,
    };
    if let Some((pairs, extra)) = expected {
        ensure!(
            count == pairs && csw_new == extra,
            "DOC FIB version and section sizes disagree"
        );
    } else {
        unsupported(&mut reasons, "unrecognized FIB version");
        return Ok(Audit::Unsupported(reasons));
    }
    if base_version != 0xc1 && base_version != version {
        unsupported(
            &mut reasons,
            "superseded FIB base version is outside the audited subset",
        );
    }
    if !matches!(version, 0xc1 | 0x112) {
        unsupported(
            &mut reasons,
            "FIB version outside audited Word97/Word2007 layouts",
        );
    }
    if u16_at(word, 6)? & 0x3ff != 9 {
        unsupported(
            &mut reasons,
            "source code page is not proven English Windows-1252",
        );
    }
    let cp = u32_at(word, 0x4c)?;
    ensure!(
        cp <= MAX_CP,
        "DOC typography source character limit exceeded"
    );
    if [0x50, 0x54, 0x5c, 0x60, 0x64, 0x68]
        .into_iter()
        .any(|p| u32_at(word, p).is_ok_and(|v| v != 0))
    {
        unsupported(
            &mut reasons,
            "side-story text is outside audited body typography",
        );
    }
    let mut aggregate = 0usize;
    let mut part = |pair: usize| -> Result<&[u8]> {
        let offset = u32_at(word, 0x9a + pair * 8)?;
        let length = u32_at(word, 0x9e + pair * 8)?;
        aggregate = aggregate
            .checked_add(length)
            .context("DOC typography resource overflow")?;
        ensure!(
            aggregate <= MAX_PROPERTIES,
            "DOC typography property-byte limit exceeded"
        );
        if length == 0 {
            Ok(&table[..0])
        } else {
            bytes(table, offset, length)
        }
    };
    let stsh = part(1)?;
    let font_bytes = part(15)?;
    let chpx = part(12)?;
    let papx = part(13)?;
    let clx = part(33)?;
    let fonts = parse_fonts(font_bytes)?;
    let (styles, defaults) = parse_styles(stsh, &mut reasons)?;
    ensure!(
        valid_font_index(u16_at(stsh, 16)?, &fonts),
        "DOC default East Asian font index is out of bounds"
    );
    if u16_at(stsh, 0)? >= 20 {
        ensure!(
            valid_font_index(u16_at(stsh, 20)?, &fonts),
            "DOC default complex-script font index is out of bounds"
        );
    }
    // FKP and style framing are checked before returning any unsupported result.
    let character_runs = parse_fkp(word, chpx, false, &mut aggregate)?;
    let paragraph_runs = parse_fkp(word, papx, true, &mut aggregate)?;
    let mut active_prcs = Vec::new();
    let piece = parse_piece(word, clx, cp, &mut reasons, &mut active_prcs)?;
    let mut validation_runs = character_runs.clone();
    validation_runs.extend(active_prcs.into_iter().map(|properties| FkpRun {
        range: 0..0,
        style: None,
        properties,
    }));
    validate_referenced_styles(
        &styles,
        &paragraph_runs,
        &validation_runs,
        &fonts,
        defaults,
        &mut reasons,
    )?;
    let mut normal = Properties {
        ascii: defaults.0,
        other: defaults.1,
        hps: 20,
        direct_hps: None,
    };
    let mut normal_spacing = ParagraphSpacing::default();
    ensure!(
        valid_font_index(defaults.0, &fonts) && valid_font_index(defaults.1, &fonts),
        "DOC STSHI default font index is out of bounds"
    );
    let active: BTreeSet<_> = paragraph_runs
        .iter()
        .map(|r| r.style.unwrap_or(0))
        .collect();
    for index in &active {
        let style = styles
            .get(*index as usize)
            .and_then(Option::as_ref)
            .context("DOC active paragraph style is missing")?;
        if *index != 0 {
            unsupported(&mut reasons, "active paragraph style is not unbased Normal");
        }
        validate_active_style(style, *index, true, &mut reasons)?;
        for (i, payload) in style.upx.iter().enumerate() {
            if i == 0 {
                ensure!(
                    u16_at(payload, 0)? == *index,
                    "DOC paragraph style UPX identity mismatch"
                );
                let mut spacing = ParagraphSpacing::default();
                apply_paragraph(&payload[2..], &mut spacing, &mut reasons)?;
                if *index == 0 {
                    normal_spacing = spacing;
                }
            } else if i == 1 {
                let mut properties = normal.clone();
                apply_character(payload, &mut properties, false, &fonts, &mut reasons)?;
                if *index == 0 && i == 1 {
                    normal = properties;
                }
            }
        }
    }
    if let Some(default_character) = styles.get(10).and_then(Option::as_ref) {
        validate_active_style(default_character, 10, false, &mut reasons)?;
        for payload in default_character.upx.iter().take(1) {
            // Parse before classifying, so malformed unsupported styles cannot fall back.
            let mut properties = normal.clone();
            apply_character(payload, &mut properties, false, &fonts, &mut reasons)?;
            if !payload.is_empty() {
                unsupported(&mut reasons, "default character style has modifiers");
            }
        }
    } else {
        unsupported(&mut reasons, "default character style 10 is absent");
    }
    let mut effective = Vec::new();
    validate_font(&normal, &fonts, &mut reasons)?;
    for run in &character_runs {
        let mut properties = normal.clone();
        apply_character(run.properties, &mut properties, true, &fonts, &mut reasons)?;
        validate_font(&properties, &fonts, &mut reasons)?;
        effective.push(properties);
    }
    let mut paragraph_spacing = Vec::with_capacity(paragraph_runs.len());
    for run in &paragraph_runs {
        let mut spacing = normal_spacing;
        apply_paragraph(run.properties, &mut spacing, &mut reasons)?;
        paragraph_spacing.push(spacing);
    }
    let Some(range) = piece else {
        return Ok(Audit::Unsupported(reasons));
    };
    complete_coverage(&character_runs, &range)?;
    complete_coverage(&paragraph_runs, &range)?;
    let raw = &word[range.clone()];
    if !raw.ends_with(b"\r") {
        unsupported(&mut reasons, "body does not end in a paragraph mark");
    }
    // Decode every byte even when another valid declaration is out of subset.
    decode_latin(raw, &mut reasons)?;
    let mut expected_start = range.start;
    let mut paragraphs = Vec::new();
    for (i, byte) in raw.iter().enumerate() {
        if *byte == b'\r' {
            paragraphs.push(expected_start..range.start + i + 1);
            expected_start = range.start + i + 1;
        }
    }
    if expected_start == range.end {
        ensure!(
            paragraphs.len() == paragraph_runs.len()
                && paragraphs
                    .iter()
                    .zip(&paragraph_runs)
                    .all(|(p, r)| *p == r.range),
            "DOC PAPX boundaries disagree with source paragraphs"
        );
    }
    if !reasons.is_empty() {
        return Ok(Audit::Unsupported(reasons));
    }
    let mut blocks = Vec::new();
    let mut run_index = 0;
    for (paragraph, spacing) in paragraphs.into_iter().zip(paragraph_spacing) {
        let content = paragraph.start..paragraph.end - 1;
        if &word[content.clone()] == b"\x0c" {
            let metric = |position: usize| -> Result<(BreakParagraphFont, [u8; 2])> {
                let index = character_runs.partition_point(|run| run.range.end <= position);
                let properties = &effective[index];
                Ok((
                    BreakParagraphFont {
                        family: fonts[properties.ascii as usize].name.clone(),
                        half_points: properties.hps,
                    },
                    [
                        fonts[properties.ascii as usize].family,
                        fonts[properties.other as usize].family,
                    ],
                ))
            };
            let (font, font_families) = metric(content.start)?;
            let (mark_font, mark_families) = metric(content.end)?;
            let metrics = if font != mark_font {
                Err("page-break character and trailing paragraph mark have different Latin font metrics".into())
            } else {
                spacing.supported_line().map(|line_spacing| BreakParagraph {
                    font,
                    space_before: spacing.before,
                    space_after: spacing.after,
                    line_spacing,
                })
            };
            blocks.push(Block::SourcePageBreak {
                cp_end: (content.end - range.start) as u32,
                metrics,
                families: [
                    font_families[0],
                    font_families[1],
                    mark_families[0],
                    mark_families[1],
                ],
                split: false,
                fallback: None,
            });
            continue;
        }
        if word[content.clone()].contains(&0x0c) {
            unsupported(&mut reasons, "manual break is not a standalone paragraph");
            continue;
        }
        let mut runs = Vec::new();
        while run_index < character_runs.len()
            && character_runs[run_index].range.end <= content.start
        {
            run_index += 1;
        }
        let mut k = run_index;
        while k < character_runs.len() && character_runs[k].range.start < content.end {
            let run = &character_runs[k];
            let start = content.start.max(run.range.start);
            let end = content.end.min(run.range.end);
            if start < end {
                let props = &effective[k];
                runs.push(Run {
                    text: decode_latin(&word[start..end], &mut reasons)?,
                    font: fonts[props.ascii as usize].name.clone(),
                    families: [
                        fonts[props.ascii as usize].family,
                        fonts[props.other as usize].family,
                    ],
                    hps: props.hps,
                    direct_hps: props.direct_hps,
                });
            }
            k += 1;
        }
        // The complete source proof above also establishes encoded Normal
        // identity. Neither the display name nor legacy title heuristics do.
        blocks.push(Block::NormalParagraph { runs, spacing });
    }
    ensure!(
        blocks.len() <= MAX_RUNS,
        "DOC typography paragraph limit exceeded"
    );
    if reasons.is_empty() {
        Ok(Audit::Proven(blocks))
    } else {
        Ok(Audit::Unsupported(reasons))
    }
}

fn parse_fonts(data: &[u8]) -> Result<Vec<Font>> {
    if data.is_empty() {
        return Ok(Vec::new());
    }
    let count = u16_at(data, 0)? as usize;
    ensure!(
        count <= 256 && u16_at(data, 2)? == 0,
        "DOC font table header is invalid or exceeds bound"
    );
    let mut offset = 4;
    let mut out = Vec::new();
    for _ in 0..count {
        let length = *bytes(data, offset, 1)?.first().unwrap() as usize;
        offset += 1;
        let entry = bytes(data, offset, length)?;
        ensure!(
            length >= 41 && (length - 39).is_multiple_of(2),
            "DOC FFN record length is invalid"
        );
        ensure!(u16_at(entry, 1)? <= 1000, "DOC FFN font weight is invalid");
        ensure!(
            entry[0] & 3 <= 2 && (entry[0] >> 4) & 7 <= 5,
            "DOC FFN pitch or family is invalid"
        );
        ensure!(
            matches!(
                entry[3],
                0 | 1
                    | 2
                    | 77
                    | 128
                    | 129
                    | 130
                    | 134
                    | 136
                    | 161
                    | 162
                    | 163
                    | 177
                    | 178
                    | 186
                    | 204
                    | 222
                    | 238
                    | 255
            ),
            "DOC FFN charset is invalid"
        );
        let names = &entry[39..];
        let units: Vec<_> = names
            .as_chunks::<2>()
            .0
            .iter()
            .map(|v| u16::from_le_bytes([v[0], v[1]]))
            .collect();
        let first_end = units
            .iter()
            .position(|v| *v == 0)
            .context("DOC font name is not terminated")?;
        ensure!(first_end != 0, "DOC font name is empty");
        let alternate = entry[4] as usize;
        if alternate == 0 {
            ensure!(
                first_end + 1 == units.len(),
                "DOC FFN has an unindexed alternate name"
            );
        } else {
            ensure!(
                alternate == first_end + 1
                    && alternate < units.len() - 1
                    && units.last() == Some(&0)
                    && !units[alternate..units.len() - 1].contains(&0),
                "DOC alternate font name index or termination is invalid"
            );
            utf16(&names[alternate * 2..names.len() - 2])?;
        }
        let name = utf16(&names[..first_end * 2])?;
        out.push(Font {
            name,
            charset: entry[3],
            family: (entry[0] >> 4) & 7,
        });
        offset += length;
    }
    ensure!(
        offset == data.len(),
        "DOC font table has trailing or unframed bytes"
    );
    Ok(out)
}

type StyleTable<'a> = (Vec<Option<Style<'a>>>, (u16, u16));

fn parse_styles<'a>(data: &'a [u8], reasons: &mut BTreeSet<String>) -> Result<StyleTable<'a>> {
    let header_length = u16_at(data, 0)? as usize;
    let header = bytes(data, 2, header_length)?;
    ensure!(header_length >= 18, "DOC STSHI header is truncated");
    let count = u16_at(header, 0)? as usize;
    let base_length = u16_at(header, 2)? as usize;
    ensure!(
        (15..4094).contains(&count),
        "DOC style count is invalid or exceeds bound"
    );
    ensure!(
        matches!(base_length, 10 | 18),
        "DOC STD base length is outside audited structure"
    );
    let defaults = (u16_at(header, 12)?, u16_at(header, 16)?);
    let mut offset = header_length + 2;
    let mut styles = Vec::new();
    for index in 0..count {
        let length = u16_at(data, offset)? as usize;
        offset += 2;
        if length == 0 {
            styles.push(None);
            continue;
        }
        let record = bytes(data, offset, length)?;
        let base = bytes(record, 0, base_length)?;
        let fixed = match index {
            0..=9 => Some(index as u16),
            10 => Some(65),
            11 => Some(105),
            12 => Some(107),
            _ => None,
        };
        ensure!(
            !matches!(index, 13 | 14),
            "DOC reserved style slot is not empty"
        );
        if let Some(sti) = fixed {
            ensure!(
                u16_at(base, 0)? & 0xfff == sti,
                "DOC fixed style slot identity is invalid"
            );
        }
        let chars = u16_at(record, base_length)? as usize;
        let name_end = base_length + 2 + 2 * chars;
        utf16(bytes(record, base_length + 2, 2 * chars)?)?;
        ensure!(
            u16_at(record, name_end)? == 0,
            "DOC style name is not terminated"
        );
        let mut pos = name_end + 2;
        if !pos.is_multiple_of(2) {
            pos += 1;
        }
        let cupx = (u16_at(base, 4)? & 0xf) as usize;
        let mut upx = Vec::new();
        for _ in 0..cupx {
            let size = u16_at(record, pos)? as usize;
            pos += 2;
            upx.push(bytes(record, pos, size)?);
            pos += size;
            if !pos.is_multiple_of(2) {
                bytes(record, pos, 1)?;
                pos += 1;
            }
        }
        if pos != record.len() {
            unsupported(reasons, "style contains unimplemented trailing properties");
        }
        ensure!(pos <= record.len(), "DOC style UPX framing is truncated");
        styles.push(Some(Style {
            base,
            record_length: length,
            name_empty: chars == 0,
            upx,
        }));
        offset += length;
    }
    ensure!(
        offset == data.len(),
        "DOC STSH records do not consume the declared table"
    );
    Ok((styles, defaults))
}

// Traverse active references only for structural validation. No inheritance is
// applied: basedOn and explicit character styles still get historical fallback.
fn validate_referenced_styles(
    styles: &[Option<Style<'_>>],
    paragraphs: &[FkpRun<'_>],
    characters: &[FkpRun<'_>],
    fonts: &[Font],
    defaults: (u16, u16),
    reasons: &mut BTreeSet<String>,
) -> Result<()> {
    fn character_refs(
        data: &[u8],
        queue: &mut Vec<(u16, u16)>,
        styles: &[Option<Style<'_>>],
        depth: usize,
    ) -> Result<()> {
        ensure!(depth <= 8, "DOC nested character modifier depth exceeded");
        walk_sprms(data, false, |op, operand| {
            match op {
                0x4a30 => queue.push((u16_at(operand, 0)?, 2)),
                0x4600 => queue.push((u16_at(operand, 0)?, 1)),
                0xca47 => character_refs(operand, queue, styles, depth + 1)?,
                0xca31 => {
                    ensure!(
                        bytes(operand, 0, 1)?[0] == 0,
                        "DOC style permutation fLong is invalid"
                    );
                    let first = u16_at(operand, 1)? as usize;
                    let last = u16_at(operand, 3)? as usize;
                    ensure!(
                        last >= first
                            && last < styles.len()
                            && operand.len() == 5 + 2 * (last - first + 1),
                        "DOC style permutation range or mapping is malformed"
                    );
                    for i in 0..=last - first {
                        queue.push((u16_at(operand, 5 + 2 * i)?, 2));
                    }
                }
                _ => {}
            }
            Ok(())
        })
    }
    let mut queue: Vec<_> = paragraphs
        .iter()
        .map(|r| (r.style.unwrap_or(0), 1))
        .collect();
    if styles.get(10).and_then(Option::as_ref).is_some() {
        queue.push((10, 2));
    }
    for run in characters {
        character_refs(run.properties, &mut queue, styles, 0)?;
        let mut props = Properties {
            ascii: defaults.0,
            other: defaults.1,
            hps: 20,
            direct_hps: None,
        };
        apply_character(run.properties, &mut props, false, fonts, reasons)?;
        validate_font(&props, fonts, reasons)?;
    }
    let mut visited = BTreeSet::new();
    let mut chain_hops = 0;
    while let Some((index, kind)) = queue.pop() {
        if !visited.insert((index, kind)) {
            continue;
        }
        ensure!(
            matches!(kind, 1 | 2),
            "DOC active linked style kind is invalid"
        );
        validate_style_chain(styles, index, kind, &mut chain_hops)?;
        let style = styles
            .get(index as usize)
            .and_then(Option::as_ref)
            .context("DOC referenced active style is missing")?;
        validate_active_style(style, index, kind == 1, reasons)?;
        let next = u16_at(style.base, 4)? >> 4;
        ensure!(
            styles.get(next as usize).and_then(Option::as_ref).is_some(),
            "DOC active next-style reference is missing"
        );
        if style.base.len() == 18 {
            let link = u16_at(style.base, 10)? & 0xfff;
            if link != 0 {
                let linked = styles
                    .get(link as usize)
                    .and_then(Option::as_ref)
                    .context("DOC active linked style is missing")?;
                queue.push((link, u16_at(linked.base, 2)? & 15));
            }
        }
        let base = u16_at(style.base, 2)? >> 4;
        if base != 0xfff {
            queue.push((base, kind));
        }
        if kind == 1 {
            let paragraph = style.upx[0];
            ensure!(
                u16_at(paragraph, 0)? == index,
                "DOC referenced paragraph style UPX identity mismatch"
            );
            apply_paragraph(&paragraph[2..], &mut ParagraphSpacing::default(), reasons)?;
        }
        let character = style.upx[usize::from(kind == 1)];
        character_refs(character, &mut queue, styles, 0)?;
        let mut props = Properties {
            ascii: defaults.0,
            other: defaults.1,
            hps: 20,
            direct_hps: None,
        };
        apply_character(character, &mut props, false, fonts, reasons)?;
        validate_font(&props, fonts, reasons)?;
    }
    Ok(())
}

fn validate_style_chain(
    styles: &[Option<Style<'_>>],
    mut index: u16,
    kind: u16,
    chain_hops: &mut usize,
) -> Result<()> {
    let mut visited = BTreeSet::new();
    loop {
        *chain_hops += 1;
        ensure!(
            *chain_hops <= MAX_RUNS,
            "DOC active style traversal bound exceeded"
        );
        ensure!(
            visited.insert(index),
            "DOC active style inheritance is cyclic"
        );
        let style = styles
            .get(index as usize)
            .and_then(Option::as_ref)
            .context("DOC active base style is missing")?;
        let base = u16_at(style.base, 2)?;
        ensure!(
            base & 15 == kind,
            "DOC active base style has the wrong kind"
        );
        index = base >> 4;
        if index == 0xfff {
            return Ok(());
        }
    }
}

fn validate_active_style(
    style: &Style<'_>,
    index: u16,
    paragraph: bool,
    reasons: &mut BTreeSet<String>,
) -> Result<()> {
    ensure!(
        u16_at(style.base, 6)? as usize == style.record_length && !style.name_empty,
        "DOC active style length or name is invalid"
    );
    let revision = style.base.len() == 18 && u16_at(style.base, 10)? & 0x1000 != 0;
    let identity = u16_at(style.base, 0)? & 0xfff;
    let kind_base = u16_at(style.base, 2)?;
    ensure!(
        kind_base & 0xf == if paragraph { 1 } else { 2 },
        "DOC active style has the wrong kind"
    );
    ensure!(
        style.upx.len() == if paragraph { 2 } else { 1 } + usize::from(revision),
        "DOC active style UPX count is invalid"
    );
    if kind_base >> 4 != 0xfff || identity != if paragraph { 0 } else { 65 } {
        unsupported(
            reasons,
            format!("style {index} has inheritance or a nondefault identity"),
        );
    }
    // StdfPost2000 linked style (low 12 bits) and revision flag (bit 12).
    if style.base.len() == 18 && u16_at(style.base, 10)? & 0x1fff != 0 {
        unsupported(reasons, "active style is linked or revision-marked");
    }
    Ok(())
}

fn parse_piece<'a>(
    word: &[u8],
    data: &'a [u8],
    cp: usize,
    reasons: &mut BTreeSet<String>,
    active_prcs: &mut Vec<&'a [u8]>,
) -> Result<Option<Range<usize>>> {
    let mut p = 0;
    let mut prcs = Vec::new();
    let mut used_prcs = BTreeSet::new();
    while bytes(data, p, 1)?[0] == 1 {
        let size = u16_at(data, p + 1)? as usize;
        bytes(data, p + 3, size)?;
        // Validate PRC operands as well, although no piece modifier is supported.
        walk_sprms(bytes(data, p + 3, size)?, false, |_, _| Ok(()))?;
        p += 3 + size;
        prcs.push(bytes(data, p - size, size)?);
        unsupported(
            reasons,
            "piece property blocks are outside the audited subset",
        );
    }
    ensure!(
        bytes(data, p, 1)?[0] == 2,
        "DOC CLX has no valid piece table"
    );
    let size = u32_at(data, p + 1)?;
    let plc = bytes(data, p + 5, size)?;
    ensure!(
        p + 5 + size == data.len() && size >= 16 && (size - 4) % 12 == 0,
        "DOC piece table framing is invalid"
    );
    let count = (size - 4) / 12;
    ensure!(
        count <= MAX_RUNS,
        "DOC piece count exceeds typography bound"
    );
    ensure!(
        u32_at(plc, 0)? == 0,
        "DOC piece coverage does not start at zero"
    );
    let mut result = None;
    for i in 0..count {
        let start = u32_at(plc, i * 4)?;
        let end = u32_at(plc, (i + 1) * 4)?;
        ensure!(
            end > start && end <= MAX_CP,
            "DOC piece CP range is invalid"
        );
        let at = 4 * (count + 1) + 8 * i;
        let descriptor = u16_at(plc, at)?;
        let packed = u32_at(plc, at + 2)?;
        ensure!(
            packed & 0x8000_0000 == 0,
            "DOC piece FC reserved bit is set"
        );
        let compressed = packed & 0x4000_0000 != 0;
        let packed_fc = packed & 0x3fff_ffff;
        if compressed {
            ensure!(packed_fc % 2 == 0, "DOC compressed piece FC is not even");
        }
        let fc = if compressed { packed_fc / 2 } else { packed_fc };
        let length = (end - start) * if compressed { 1 } else { 2 };
        bytes(word, fc, length)?;
        ensure!(descriptor & 4 == 0, "DOC piece dirty flag must be zero");
        ensure!(
            descriptor & 1 == 0 || !word[fc..fc + length].contains(&b'\r'),
            "DOC piece paragraph flag contradicts text"
        );
        // Pcd.fR1 and fR2 are undefined and MUST be ignored (MS-DOC 2.9.177).
        let prm = u16_at(plc, at + 6)?;
        ensure!(
            prm & 1 == 0 || ((prm >> 1) as usize) < prcs.len(),
            "DOC complex piece modifier references a missing PRC"
        );
        if prm & 1 != 0 && used_prcs.insert(prm >> 1) {
            active_prcs.push(prcs[(prm >> 1) as usize]);
        }
        if prm != 0 {
            unsupported(
                reasons,
                "piece modifiers or flags are outside the audited subset",
            );
        }
        if !compressed {
            unsupported(
                reasons,
                "Unicode pieces are outside the audited Latin subset",
            );
        }
        if count == 1 && compressed && start == 0 && end == cp {
            result = Some(fc..fc + length);
        }
    }
    if count != 1 || u32_at(plc, count * 4)? != cp {
        unsupported(reasons, "piece table is not one complete body-only piece");
    }
    Ok(result)
}

fn parse_fkp<'a>(
    word: &'a [u8],
    plc: &[u8],
    paragraph: bool,
    aggregate: &mut usize,
) -> Result<Vec<FkpRun<'a>>> {
    ensure!(
        plc.len() >= 12 && (plc.len() - 4).is_multiple_of(8),
        "DOC FKP index framing is invalid"
    );
    let count = (plc.len() - 4) / 8;
    ensure!(
        count <= MAX_RUNS && count <= word.len() / 512,
        "DOC FKP page bound exceeded"
    );
    let mut visited = BTreeSet::new();
    let mut out = Vec::new();
    for i in 0..count {
        let start = u32_at(plc, 4 * i)?;
        let end = u32_at(plc, 4 * (i + 1))?;
        ensure!(
            start < end,
            "DOC FKP index has a reversed or empty interval"
        );
        let packed = u32_at(plc, 4 * (count + 1) + 4 * i)?;
        ensure!(
            packed & !0x3f_ffff == 0 && visited.insert(packed),
            "DOC FKP page index is invalid or duplicated"
        );
        let page = bytes(word, packed * 512, 512)?;
        let runs = page[511] as usize;
        let descriptor_size = if paragraph { 13 } else { 1 };
        let header_end = 4 * (runs + 1) + descriptor_size * runs;
        ensure!(
            runs > 0 && header_end <= 511,
            "DOC FKP run count is invalid"
        );
        ensure!(
            u32_at(page, 0)? == start && u32_at(page, runs * 4)? == end,
            "DOC FKP page and index coverage disagree"
        );
        let mut payload_ranges: Vec<Range<usize>> = Vec::new();
        for j in 0..runs {
            let a = u32_at(page, j * 4)?;
            let b = u32_at(page, (j + 1) * 4)?;
            ensure!(a < b, "DOC FKP run has a reversed or empty interval");
            let offset = page[4 * (runs + 1) + descriptor_size * j] as usize * 2;
            let (style, properties) = if offset == 0 {
                (paragraph.then_some(0), &page[0..0])
            } else {
                ensure!(
                    offset >= header_end && offset < 511,
                    "DOC FKP payload overlaps its header"
                );
                let (data_start, data_end, style) = if paragraph {
                    let reread = page[offset] == 0;
                    let p = offset + usize::from(reread);
                    let length = *bytes(page, p, 1)?.first().unwrap() as usize * 2;
                    let end = p + length + usize::from(reread);
                    ensure!(
                        length >= if reread { 2 } else { 4 } && end <= 511,
                        "DOC PAPX payload length is invalid"
                    );
                    (p + 3, end, Some(u16_at(page, p + 1)?))
                } else {
                    (offset + 1, offset + 1 + page[offset] as usize, None)
                };
                ensure!(
                    data_end <= 511 && data_start <= data_end,
                    "DOC FKP payload is truncated"
                );
                let storage = offset..data_end;
                ensure!(
                    payload_ranges
                        .iter()
                        .all(|r| *r == storage || r.end <= storage.start || storage.end <= r.start),
                    "DOC FKP payloads partially overlap"
                );
                payload_ranges.push(storage);
                (style, bytes(page, data_start, data_end - data_start)?)
            };
            *aggregate += properties.len();
            ensure!(
                *aggregate <= MAX_PROPERTIES && out.len() < MAX_RUNS,
                "DOC typography property/run bound exceeded"
            );
            out.push(FkpRun {
                range: a..b,
                style,
                properties,
            });
        }
    }
    Ok(out)
}
fn complete_coverage(runs: &[FkpRun<'_>], range: &Range<usize>) -> Result<()> {
    ensure!(
        runs.first().is_some_and(|r| r.range.start == range.start)
            && runs.last().is_some_and(|r| r.range.end == range.end)
            && runs.windows(2).all(|v| v[0].range.end == v[1].range.start),
        "DOC typography FKP coverage has gaps, overlaps or extra text"
    );
    Ok(())
}

fn walk_sprms(
    data: &[u8],
    pad: bool,
    mut action: impl FnMut(u16, &[u8]) -> Result<()>,
) -> Result<()> {
    let mut p = 0;
    while p < data.len() {
        if pad && p + 1 == data.len() && data[p] == 0 {
            break;
        }
        let opcode = u16_at(data, p)?;
        p += 2;
        let length = match opcode >> 13 {
            0 | 1 => 1,
            2 | 4 | 5 => 2,
            3 => 4,
            7 => 3,
            6 if opcode == 0xd608 => {
                let n = u16_at(data, p)? as usize;
                ensure!(n >= 1, "DOC table SPRM length is invalid");
                p += 2;
                n - 1
            }
            6 => {
                let n = bytes(data, p, 1)?[0] as usize;
                p += 1;
                if opcode == 0xc615 && n == 255 {
                    let del = bytes(data, p, 1)?[0] as usize;
                    let add = bytes(data, p + 1 + 4 * del, 1)?[0] as usize;
                    2 + 4 * del + 3 * add
                } else {
                    n
                }
            }
            _ => unreachable!(),
        };
        action(opcode, bytes(data, p, length)?)?;
        p += length;
    }
    Ok(())
}
fn apply_spacing_operand(op: u16, operand: &[u8], spacing: &mut ParagraphSpacing) -> Result<bool> {
    match op {
        0xa413 | 0xa414 => {
            let value = u16_at(operand, 0)?;
            ensure!(
                value <= 31680,
                "DOC paragraph spacing 0x{op:04X} is outside valid 0–31680 twips"
            );
            if op == 0xa413 {
                spacing.before = value;
            } else {
                spacing.after = value;
            }
        }
        0x6412 => {
            // MS-DOC LSPD: validate every active declaration, even when later
            // overridden or found on an otherwise unsupported property path.
            let value = u16_at(operand, 0)?;
            let flag = u16_at(operand, 2)?;
            ensure!(flag <= 1, "DOC LSPD fMultLinespace must be 0 or 1");
            ensure!(
                value <= 0x7bc0 || value >= 0x8440,
                "DOC LSPD dyaLine is outside the valid signed twip range"
            );
            spacing.line = if value >= 0x8440 {
                // Negative dyaLine is exact for either legal flag.
                EncodedLineSpacing::Exact((65536u32 - u32::from(value)) as u16)
            } else if flag == 0 {
                EncodedLineSpacing::Minimum(value)
            } else {
                EncodedLineSpacing::Multiple(value)
            };
        }
        _ => return Ok(false),
    }
    Ok(true)
}
fn apply_paragraph(
    data: &[u8],
    spacing: &mut ParagraphSpacing,
    reasons: &mut BTreeSet<String>,
) -> Result<()> {
    walk_sprms(data, true, |op, operand| {
        // No expansion of the audited opcode set: other paragraph modifiers
        // can affect identity, inheritance or physical layout.
        if !apply_spacing_operand(op, operand, spacing)? {
            unsupported(
                reasons,
                format!("active paragraph modifier 0x{op:04X} is outside audited typography"),
            );
        }
        Ok(())
    })
}
fn apply_character(
    data: &[u8],
    props: &mut Properties,
    direct: bool,
    fonts: &[Font],
    reasons: &mut BTreeSet<String>,
) -> Result<()> {
    apply_character_nested(data, props, direct, fonts, reasons, 0)
}
fn apply_character_nested(
    data: &[u8],
    props: &mut Properties,
    direct: bool,
    fonts: &[Font],
    reasons: &mut BTreeSet<String>,
    depth: usize,
) -> Result<()> {
    ensure!(depth <= 8, "DOC nested character modifier depth exceeded");
    walk_sprms(data, false, |op, operand| {
        // Active PRCs and wrong-context/nested paragraph modifiers still get
        // their old unsupported result, but cannot conceal malformed spacing.
        apply_spacing_operand(op, operand, &mut ParagraphSpacing::default())?;

        match op {
            0xca47 => {
                let mut nested = props.clone();
                apply_character_nested(operand, &mut nested, false, fonts, reasons, depth + 1)?;
                validate_font(&nested, fonts, reasons)?;
                unsupported(
                    reasons,
                    "active majority/reset modifier is outside audited typography",
                );
            }
            0x4a43 => {
                let hps = u16_at(operand, 0)?;
                ensure!(
                    (2..=3276).contains(&hps),
                    "DOC active font size is invalid or outside editor bounds"
                );
                props.hps = hps;
                if direct {
                    props.direct_hps = Some(hps);
                }
            }
            0x4a4f | 0x4a51 => {
                let index = u16_at(operand, 0)?;
                ensure!(
                    valid_font_index(index, fonts),
                    "DOC active font index is out of bounds"
                );
                if op == 0x4a4f {
                    props.ascii = index;
                } else {
                    props.other = index;
                }
            }
            0x4a50 | 0x4a5e | 0x6a09 => {
                ensure!(
                    valid_font_index(u16_at(operand, 0)?, fonts),
                    "DOC active script/symbol font index is out of bounds"
                );
                unsupported(
                    reasons,
                    format!(
                        "script or symbol font selector 0x{op:04X} is outside audited typography"
                    ),
                );
            }
            0x486d | 0x486e | 0x4873 | 0x4874 => {
                if u16_at(operand, 0)? & 0x3ff != 9 {
                    unsupported(reasons, "active text language is not proven English Latin");
                }
            }
            // CS-only language/size have no effect in this positively classified
            // Latin subset; any forcing/bidi operator is outside the whitelist.
            0x4a61 => ensure!(
                u16_at(operand, 0)? <= 3276,
                "DOC complex-script font size is invalid"
            ),
            0x485f | 0x6815 | 0x6816 | 0x2a42 | 0x6870 => {}
            _ => unsupported(
                reasons,
                format!("active character modifier 0x{op:04X} is outside audited typography"),
            ),
        }
        Ok(())
    })
}
fn valid_font_index(index: u16, fonts: &[Font]) -> bool {
    if fonts.is_empty() {
        index == 0
    } else {
        (index as usize) < fonts.len()
    }
}
fn validate_font(props: &Properties, fonts: &[Font], reasons: &mut BTreeSet<String>) -> Result<()> {
    if fonts.is_empty() {
        ensure!(
            props.ascii == 0 && props.other == 0,
            "DOC font index must be zero when the font table is absent or empty"
        );
        unsupported(
            reasons,
            "source font table is absent or empty; literal names cannot be proved",
        );
        return Ok(());
    }
    for index in [props.ascii, props.other] {
        let font = fonts
            .get(index as usize)
            .context("DOC active default font index is out of bounds")?;
        ensure!(
            crate::document::supported_font_family(&font.name),
            "DOC active font name is invalid or outside editor bounds"
        );
        if font.charset != 0 {
            unsupported(reasons, "active font charset is not ANSI Latin");
        }
    }
    if fonts[props.ascii as usize].name != fonts[props.other as usize].name {
        unsupported(reasons, "active Latin font slots select different faces");
    }
    Ok(())
}
fn decode_latin(raw: &[u8], reasons: &mut BTreeSet<String>) -> Result<String> {
    const CP1252: [char; 32] = [
        '€', '\0', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\0', 'Ž', '\0', '\0',
        '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\0', 'ž', 'Ÿ',
    ];
    let mut text = String::new();
    for byte in raw {
        let ch = if (0x80..=0x9f).contains(byte) {
            CP1252[(byte - 0x80) as usize]
        } else {
            char::from(*byte)
        };
        ensure!(
            ch != '\0',
            "DOC source contains an undefined Windows-1252 byte"
        );
        if !(matches!(ch, '\r' | '\u{c}')
            || (' '..='~').contains(&ch)
            || ('\u{a0}'..='\u{ff}').contains(&ch) && ch != 'µ'
            || CP1252.contains(&ch))
        {
            unsupported(
                reasons,
                "source text contains unsupported controls or non-Latin characters",
            );
        }
        text.push(ch);
    }
    Ok(text)
}

pub(crate) fn attach(
    audit: Audit,
    document: &mut Node,
    risks: &mut BTreeSet<String>,
) -> Result<()> {
    let Audit::Proven(blocks) = audit else {
        if let Audit::Unsupported(reasons) = audit {
            for reason in reasons {
                risks.insert(format!(
                    "Legacy DOC font names and effective sizes are not preserved: {reason}"
                ));
            }
        }
        return Ok(());
    };
    ensure!(
        blocks.len() == document.content.len(),
        "DOC typography source-to-editor block identity mismatch; source remains read-only"
    );
    let mut candidate = document.clone();
    ensure!(
        !candidate
            .attrs
            .contains_key("splitPageBreakAndParagraphMark"),
        "DOC parser introduced unproved split-page-break compatibility"
    );
    ensure!(
        !candidate.attrs.contains_key("fontFamilyMetadata"),
        "DOC parser introduced unproved font family metadata"
    );
    let mut family_proof = FamilyMetadataProof::default();
    let mut generated = 1 + candidate.content.len();
    let mut preserved_spacing = 0;
    let mut unsupported_spacing = BTreeMap::<String, usize>::new();
    let mut break_paragraphs = 0;
    let mut preserved_breaks = 0;
    let mut break_fallbacks = BTreeSet::new();
    let mut empty_paragraphs = 0;
    for (block, node) in blocks.into_iter().zip(&mut candidate.content) {
        match block {
            Block::SourcePageBreak { .. } => {
                ensure!(
                    node.kind == "pageBreak"
                        && node.attrs.is_empty()
                        && node.content.is_empty()
                        && node.text.is_none()
                        && node.marks.is_empty(),
                    "DOC typography manual-break identity mismatch"
                );
                if let Block::SourcePageBreak {
                    metrics,
                    families,
                    split,
                    fallback,
                    ..
                } = block
                {
                    if split {
                        let metrics = metrics.map_err(anyhow::Error::msg)?;
                        family_proof.add(&metrics.font.family, &families);
                        node.attrs
                            .insert("breakParagraph".into(), serde_json::to_value(metrics)?);
                        preserved_breaks += 1;
                    }
                    if let Some(reason) = fallback {
                        break_fallbacks.insert(reason);
                    }
                }
                break_paragraphs += 1;
            }
            Block::NormalParagraph { runs, spacing } => {
                ensure!(
                    matches!(node.kind.as_str(), "paragraph" | "heading")
                        && node
                            .content
                            .iter()
                            .all(|n| n.kind == "text" && n.text.is_some()),
                    "DOC typography editor structure is outside proved body paragraphs"
                );
                let expected: String = runs.iter().map(|r| r.text.as_str()).collect();
                let actual: String = node
                    .content
                    .iter()
                    .map(|n| n.text.as_deref().unwrap_or(""))
                    .collect();
                ensure!(
                    expected == actual,
                    "DOC typography source-to-editor text identity mismatch; source remains read-only"
                );
                if node.kind == "heading" {
                    node.kind = "paragraph".into();
                    node.attrs.remove("level");
                }
                // Apply a complete supported tuple only after exact binding.
                // A valid unsupported final line value preserves the previous
                // tuple while retaining independently proven fonts/Normal.
                match spacing.supported_line() {
                    Ok(line) => {
                        for (key, value) in [
                            ("spaceBefore", spacing.before),
                            ("spaceAfter", spacing.after),
                            ("lineSpacing", line),
                        ] {
                            node.attrs.insert(key.into(), value.into());
                        }
                        preserved_spacing += 1;
                    }
                    Err(reason) => *unsupported_spacing.entry(reason).or_default() += 1,
                }
                if expected.is_empty() {
                    empty_paragraphs += 1;
                }
                let mut output = Vec::new();
                let mut source_start = 0;
                let mut current = 0;
                let mut text_index = 0;
                for run in runs {
                    let source_end = source_start + run.text.len();
                    while text_index < node.content.len() && current < source_end {
                        let text = &node.content[text_index];
                        let value = text.text.as_deref().unwrap();
                        let next = current + value.len();
                        let start = source_start.max(current);
                        let end = source_end.min(next);
                        if start < end {
                            family_proof.add(&run.font, &run.families);
                            ensure!(
                                value.is_char_boundary(start - current)
                                    && value.is_char_boundary(end - current),
                                "DOC typography run splits a source character"
                            );
                            for mark in &text.marks {
                                if mark.kind == "fontSize" {
                                    ensure!(
                                        mark.attrs.get("halfPoints").and_then(|v| v.as_u64())
                                            == run.direct_hps.map(u64::from),
                                        "DOC parser direct font size disagrees with audited source"
                                    );
                                }
                                ensure!(
                                    mark.kind != "fontFamily" && mark.kind != "link",
                                    "DOC parser introduced unproved font or link"
                                );
                            }
                            if run.direct_hps.is_some() {
                                ensure!(
                                    text.marks.iter().any(|m| m.kind == "fontSize"),
                                    "DOC parser omitted audited direct font size"
                                );
                            }
                            let mut new = text.clone();
                            new.text = Some(value[start - current..end - current].to_owned());
                            new.marks.retain(|m| m.kind != "fontSize");
                            new.marks.push(Mark {
                                kind: "fontSize".into(),
                                attrs: serde_json::Map::from_iter([(
                                    "halfPoints".into(),
                                    run.hps.into(),
                                )]),
                            });
                            new.marks.push(Mark {
                                kind: "fontFamily".into(),
                                attrs: serde_json::Map::from_iter([(
                                    "name".into(),
                                    run.font.clone().into(),
                                )]),
                            });
                            output.push(new);
                            generated += 1;
                            ensure!(
                                generated <= MAX_RUNS,
                                "DOC typography editor node limit exceeded"
                            );
                        }
                        if next > source_end {
                            break;
                        }
                        current = next;
                        text_index += 1;
                    }
                    source_start = source_end;
                }
                node.content = output;
            }
        }
    }
    ensure!(
        preserved_breaks == 0 || preserved_breaks == break_paragraphs,
        "DOC split-page-break proof is incomplete across source paragraphs"
    );
    if preserved_breaks > 0 {
        candidate
            .attrs
            .insert("splitPageBreakAndParagraphMark".into(), true.into());
    }
    let metadata = family_proof.metadata();
    if !metadata.is_empty() {
        candidate.attrs.insert(
            "fontFamilyMetadata".into(),
            serde_json::to_value(&metadata)?,
        );
    }
    crate::document::validate(&candidate)?;
    *document = candidate;
    if !metadata.is_empty() {
        risks.insert(format!(
            "Legacy DOC source Roman/Swiss font family metadata is preserved for {} emitted literal font name(s); font availability, browser printing and other consumers may differ",
            metadata.len()
        ));
    }
    for reason in family_proof.reasons {
        risks.insert(format!(
            "Legacy DOC source font family metadata is not preserved for any font: {reason}"
        ));
    }
    risks.insert(PRESERVED.into());
    risks.insert(NORMAL_PRESERVED.into());
    risks.insert("Font names are preserved; unavailable fonts may be substituted locally".into());
    let normalized_spacing: usize = unsupported_spacing.values().sum();
    if preserved_spacing + normalized_spacing > 0 {
        let other = if preserved_breaks > 0 { "other " } else { "" };
        risks.insert(format!(
            "Legacy DOC encoded paragraph spacing values preserved for {preserved_spacing} audited Normal paragraph(s); {normalized_spacing} paragraph(s) retain normalized spacing; source layout, grids and {other}compatibility settings are not proven"
        ));
    }
    for (reason, count) in unsupported_spacing {
        risks.insert(format!(
            "Legacy DOC encoded paragraph spacing not preserved for {count} audited Normal paragraph(s): {reason}"
        ));
    }
    if preserved_breaks > 0 {
        risks.insert(format!(
            "Legacy DOC split-page-break compatibility and complete Latin paragraph-mark metrics are preserved for {preserved_breaks} standalone manual break paragraph(s); browser printing does not reproduce the retained next-page mark line"
        ));
    } else if break_paragraphs > 0 {
        risks.insert(format!(
            "Legacy DOC break-only paragraph spacing and baseline fonts are not preserved ({break_paragraphs} paragraph(s)); standalone page-break nodes are retained"
        ));
    }
    for reason in break_fallbacks {
        risks.insert(format!("Legacy DOC split-page-break paragraph semantics are not preserved for any source break: {reason}"));
    }
    if empty_paragraphs > 0 {
        risks.insert(format!(
            "Legacy DOC empty-paragraph baseline fonts are not preserved ({empty_paragraphs} paragraph(s)); encoded spacing does not establish their line geometry"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn font_table(name: &str) -> Vec<u8> {
        let mut f = vec![1, 0, 0, 0, 0, 0x16, 0x90, 1, 0, 0];
        f.resize(44, 0);
        for c in name.encode_utf16().chain([0]) {
            f.extend(c.to_le_bytes());
        }
        f[4] = (f.len() - 5) as u8;
        f
    }
    fn props() -> Properties {
        Properties {
            ascii: 0,
            other: 0,
            hps: 20,
            direct_hps: None,
        }
    }
    fn parsed_font() -> Vec<Font> {
        parse_fonts(&font_table("Times New Roman")).unwrap()
    }
    fn text_node(text: &str) -> Node {
        Node {
            kind: "text".into(),
            attrs: Default::default(),
            text: Some(text.into()),
            content: vec![],
            marks: vec![],
        }
    }
    fn document(text: &str) -> Node {
        Node {
            kind: "doc".into(),
            attrs: Default::default(),
            text: None,
            marks: vec![],
            content: vec![Node {
                kind: "paragraph".into(),
                attrs: Default::default(),
                text: None,
                marks: vec![],
                content: vec![text_node(text)],
            }],
        }
    }
    fn run(text: &str, hps: u16) -> Run {
        Run {
            text: text.into(),
            font: "Arial".into(),
            families: [2; 2],
            hps,
            direct_hps: None,
        }
    }
    fn bare_break() -> Block {
        Block::SourcePageBreak {
            cp_end: 0,
            metrics: Err("unproved test break".into()),
            families: [0; 4],
            split: false,
            fallback: None,
        }
    }
    fn normal(runs: Vec<Run>) -> Block {
        Block::NormalParagraph {
            runs,
            spacing: ParagraphSpacing::default(),
        }
    }
    fn with_spacing(runs: Vec<Run>, spacing: ParagraphSpacing) -> Block {
        Block::NormalParagraph { runs, spacing }
    }
    fn spacing_attrs(
        before: u16,
        after: u16,
        line: u16,
    ) -> serde_json::Map<String, serde_json::Value> {
        serde_json::from_value(
            serde_json::json!({"spaceBefore":before,"spaceAfter":after,"lineSpacing":line}),
        )
        .unwrap()
    }
    fn heading(text: &str, level: u8) -> Node {
        let mut node = document(text).content.remove(0);
        node.kind = "heading".into();
        node.attrs.insert("level".into(), level.into());
        node
    }
    fn assert_unchanged_after_refusal(audit: Audit, mut document: Node) {
        let before = serde_json::to_value(&document).unwrap();
        let mut risks = BTreeSet::from(["existing risk".into()]);
        let old_risks = risks.clone();
        assert!(attach(audit, &mut document, &mut risks).is_err());
        assert_eq!(serde_json::to_value(document).unwrap(), before);
        assert_eq!(risks, old_risks);
    }
    fn gap(op: u16, value: u16) -> Vec<u8> {
        [op.to_le_bytes().as_slice(), value.to_le_bytes().as_slice()].concat()
    }
    fn lspd(value: u16, flag: u16) -> Vec<u8> {
        [
            0x6412u16.to_le_bytes().as_slice(),
            value.to_le_bytes().as_slice(),
            flag.to_le_bytes().as_slice(),
        ]
        .concat()
    }
    fn resolve(style: &[u8], direct: &[u8]) -> ParagraphSpacing {
        let mut spacing = ParagraphSpacing::default();
        let mut reasons = BTreeSet::new();
        apply_paragraph(style, &mut spacing, &mut reasons).unwrap();
        apply_paragraph(direct, &mut spacing, &mut reasons).unwrap();
        assert!(reasons.is_empty());
        spacing
    }
    fn fixture_spacing(style: &[u8], direct: [&[u8]; 4]) -> (Vec<u8>, Vec<u8>) {
        // The committed independently authored fixture has one plain piece,
        // four known CR ranges and Normal's PAPX UPX at table bytes 48..52.
        // Author raw SPRM bytes; do not ask the product parser for expectations.
        assert!(style.len().is_multiple_of(2));
        let source = include_bytes!("../tests/fixtures/doc-typography/blank.doc");
        let mut cfb = CfbReader::new(Cursor::new(source.as_slice())).unwrap();
        let mut word = stream(&mut cfb, "WordDocument", source.len()).unwrap();
        let mut table = stream(&mut cfb, "1Table", source.len()).unwrap();
        assert_eq!(&table[48..54], &[2, 0, 0, 0, 4, 0]);
        table.splice(52..52, style.iter().copied());
        table[48..50].copy_from_slice(&(2 + style.len() as u16).to_le_bytes());
        table[20..22].copy_from_slice(&(36 + style.len() as u16).to_le_bytes());
        table[28..30].copy_from_slice(&(36 + style.len() as u16).to_le_bytes());
        word[0xa6..0xaa].copy_from_slice(&(146u32 + style.len() as u32).to_le_bytes());
        for i in 0..93 {
            let p = 0x9a + i * 8;
            let offset = u32::from_le_bytes(word[p..p + 4].try_into().unwrap());
            if offset >= 52 {
                word[p..p + 4].copy_from_slice(&(offset + style.len() as u32).to_le_bytes());
            }
        }
        let page = &mut word[2048..2560];
        page.fill(0);
        page[511] = 4;
        for (i, value) in [1024u32, 1031, 1032, 1034, 1041].into_iter().enumerate() {
            page[4 * i..4 * i + 4].copy_from_slice(&value.to_le_bytes());
        }
        let mut end = 510;
        for (i, props) in direct.into_iter().enumerate() {
            let mut props = props.to_vec();
            if props.len() % 2 == 1 {
                props.push(0);
            }
            let mut payload = vec![0, ((2 + props.len()) / 2) as u8, 0, 0];
            payload.extend(props);
            end = (end - payload.len()) & !1;
            assert!(end > 72);
            page[end..end + payload.len()].copy_from_slice(&payload);
            page[20 + 13 * i] = (end / 2) as u8;
        }
        (word, table)
    }
    fn split_page_proof(
        flag: Option<bool>,
        section_end: Option<u32>,
    ) -> crate::legacy_page_geometry::Audit {
        let mut word = vec![0u8; 2048];
        let mut table = vec![0u8; 800];
        word[2..4].copy_from_slice(&0xc1u16.to_le_bytes());
        word[0x98..0x9a].copy_from_slice(&183u16.to_le_bytes());
        let new_at = 0x9a + 183 * 8;
        word[new_at..new_at + 2].copy_from_slice(&5u16.to_le_bytes());
        word[new_at + 2..new_at + 4].copy_from_slice(&0x112u16.to_le_bytes());
        word[0x4c..0x50].copy_from_slice(&17u32.to_le_bytes());
        if let Some(flag) = flag {
            word[0x192..0x196].copy_from_slice(&64u32.to_le_bytes());
            word[0x196..0x19a].copy_from_slice(&674u32.to_le_bytes());
            table[64 + 512..64 + 516]
                .copy_from_slice(&(if flag { 0x0800_0000u32 } else { 0 }).to_le_bytes());
        }
        let ends = if let Some(end) = section_end {
            vec![end, 17]
        } else {
            vec![17]
        };
        word[0xce..0xd2].copy_from_slice(&((4 + 16 * ends.len()) as u32).to_le_bytes());
        for (i, end) in ends.iter().enumerate() {
            table[(i + 1) * 4..(i + 2) * 4].copy_from_slice(&end.to_le_bytes());
            let sed = 4 * (ends.len() + 1) + 12 * i + 2;
            table[sed..sed + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        }
        crate::legacy_page_geometry::audit_streams(&word, &table, &BTreeSet::from([9])).unwrap()
    }
    fn source_break(split: bool) -> Block {
        Block::SourcePageBreak {
            cp_end: 9,
            families: [2; 4],
            metrics: Ok(BreakParagraph {
                font: BreakParagraphFont {
                    family: "Calibri".into(),
                    half_points: 22,
                },
                space_before: 0,
                space_after: 200,
                line_spacing: 276,
            }),
            split,
            fallback: None,
        }
    }
    fn break_node() -> Node {
        serde_json::from_value(serde_json::json!({"type":"pageBreak"})).unwrap()
    }
    fn split_character_fixture(mark_hps: u16) -> (Vec<u8>, Vec<u8>) {
        let (mut word, table) = fixture_spacing(
            &[gap(0xa414, 200), lspd(276, 1)].concat(),
            [&[], &[], &[], &[]],
        );
        let page = &mut word[1536..2048];
        page.fill(0);
        page[511] = 4;
        for (i, fc) in [1024u32, 1032, 1033, 1034, 1041].into_iter().enumerate() {
            page[i * 4..i * 4 + 4].copy_from_slice(&fc.to_le_bytes());
        }
        for (i, hps) in [(1, 22u16), (2, mark_hps)] {
            let pos = 480 + i * 8;
            page[20 + i] = (pos / 2) as u8;
            page[pos] = 4;
            page[pos + 1..pos + 3].copy_from_slice(&0x4a43u16.to_le_bytes());
            page[pos + 3..pos + 5].copy_from_slice(&hps.to_le_bytes());
        }
        (word, table)
    }
    #[test]
    fn break_source_captures_exact_cp_spacing_and_both_separate_chpx_runs() {
        let (word, table) = split_character_fixture(22);
        let Audit::Proven(blocks) = audit_streams(&word, &table).unwrap() else {
            panic!("body proof");
        };
        let Block::SourcePageBreak {
            cp_end,
            metrics,
            split,
            fallback,
            ..
        } = &blocks[2]
        else {
            panic!("break identity");
        };
        assert_eq!(*cp_end, 9);
        assert!(!split);
        assert!(fallback.is_none());
        assert_eq!(
            metrics.as_ref().unwrap(),
            &BreakParagraph {
                font: BreakParagraphFont {
                    family: "Times New Roman".into(),
                    half_points: 22
                },
                space_before: 0,
                space_after: 200,
                line_spacing: 276
            }
        );
    }
    #[test]
    fn differing_cr_font_size_is_not_borrowed_from_break_or_following_text() {
        let (word, table) = split_character_fixture(24);
        let Audit::Proven(mut blocks) = audit_streams(&word, &table).unwrap() else {
            panic!("body proof");
        };
        let Block::SourcePageBreak { metrics, .. } = &blocks[2] else {
            panic!("break identity");
        };
        assert!(
            metrics
                .as_ref()
                .unwrap_err()
                .contains("different Latin font metrics")
        );
        promote_break_paragraphs(
            &mut blocks,
            &split_page_proof(Some(true), None),
            &BTreeSet::from([9]),
        );
        let Block::SourcePageBreak {
            split, fallback, ..
        } = &blocks[2]
        else {
            panic!("break identity");
        };
        assert!(!split);
        assert!(
            fallback
                .as_ref()
                .unwrap()
                .contains("different Latin font metrics")
        );
    }
    #[test]
    fn unsupported_break_spacing_preserves_the_independent_body_proof() {
        let line = lspd(300, 0);
        let (word, table) = fixture_spacing(&[], [&[], &[], &line, &[]]);
        let Audit::Proven(mut blocks) = audit_streams(&word, &table).unwrap() else {
            panic!("body proof");
        };
        promote_break_paragraphs(
            &mut blocks,
            &split_page_proof(Some(true), None),
            &BTreeSet::from([9]),
        );
        let Block::SourcePageBreak {
            metrics,
            split,
            fallback,
            ..
        } = &blocks[2]
        else {
            panic!("break identity");
        };
        assert!(!split);
        assert!(
            metrics
                .as_ref()
                .unwrap_err()
                .contains("minimum line spacing")
        );
        assert!(fallback.as_ref().unwrap().contains("minimum line spacing"));
    }
    #[test]
    fn explicit_split_proves_all_breaks_independently_of_unsupported_page_geometry() {
        let mut blocks = vec![source_break(false), normal(vec![]), source_break(false)];
        promote_break_paragraphs(
            &mut blocks,
            &split_page_proof(Some(true), None),
            &BTreeSet::from([9]),
        );
        let mut doc = document("");
        doc.content = vec![
            break_node(),
            serde_json::from_value(serde_json::json!({"type":"paragraph"})).unwrap(),
            break_node(),
        ];
        let mut risks = BTreeSet::new();
        attach(Audit::Proven(blocks), &mut doc, &mut risks).unwrap();
        assert_eq!(doc.attrs["splitPageBreakAndParagraphMark"], true);
        assert_eq!(doc.content.len(), 3);
        assert_eq!(
            doc.content[0].attrs["breakParagraph"],
            doc.content[2].attrs["breakParagraph"]
        );
        assert!(doc.content[1].content.is_empty());
        assert!(!doc.content[1].attrs.contains_key("emptyParagraphFont"));
        assert!(
            risks
                .iter()
                .any(|r| r.contains("preserved for 2 standalone manual break"))
        );
        assert!(
            risks
                .iter()
                .any(|r| r.contains("other compatibility settings"))
        );
    }
    #[test]
    fn one_unproved_break_prevents_global_promotion_without_discarding_normal_fonts() {
        let mut blocks = vec![
            source_break(false),
            normal(vec![run("same", 20)]),
            bare_break(),
        ];
        promote_break_paragraphs(
            &mut blocks,
            &split_page_proof(Some(true), None),
            &BTreeSet::from([9]),
        );
        let mut doc = document("same");
        doc.content.insert(0, break_node());
        doc.content.push(break_node());
        let mut risks = BTreeSet::new();
        attach(Audit::Proven(blocks), &mut doc, &mut risks).unwrap();
        assert!(!doc.attrs.contains_key("splitPageBreakAndParagraphMark"));
        assert!(doc.content[0].attrs.is_empty() && doc.content[2].attrs.is_empty());
        assert_eq!(doc.content[1].content[0].marks[1].attrs["name"], "Arial");
        assert!(
            risks
                .iter()
                .any(|r| r.contains("not preserved for any source break: unproved test break"))
        );
    }
    #[test]
    fn source_off_absent_and_zero_breaks_never_invent_split_state() {
        for flag in [None, Some(false)] {
            let mut blocks = vec![source_break(false)];
            promote_break_paragraphs(
                &mut blocks,
                &split_page_proof(flag, None),
                &BTreeSet::from([9]),
            );
            let mut doc = document("");
            doc.content = vec![break_node()];
            let mut risks = BTreeSet::new();
            attach(Audit::Proven(blocks), &mut doc, &mut risks).unwrap();
            assert!(doc.attrs.is_empty() && doc.content[0].attrs.is_empty());
            assert!(!risks.iter().any(|r| r.contains("split-page-break")));
        }
        let mut blocks = vec![normal(vec![run("same", 20)])];
        promote_break_paragraphs(
            &mut blocks,
            &split_page_proof(Some(true), None),
            &BTreeSet::new(),
        );
        let mut doc = document("same");
        attach(Audit::Proven(blocks), &mut doc, &mut BTreeSet::new()).unwrap();
        assert!(!doc.attrs.contains_key("splitPageBreakAndParagraphMark"));
    }
    #[test]
    fn section_end_prevents_global_manual_break_semantics() {
        let mut blocks = vec![source_break(false)];
        promote_break_paragraphs(
            &mut blocks,
            &split_page_proof(Some(true), Some(9)),
            &BTreeSet::from([9]),
        );
        let Block::SourcePageBreak {
            split, fallback, ..
        } = &blocks[0]
        else {
            panic!("break identity");
        };
        assert!(!split);
        assert!(
            fallback
                .as_ref()
                .unwrap()
                .contains("section-ending character")
        );
    }
    #[test]
    fn late_text_or_validation_failure_keeps_promoted_breaks_and_all_risks_atomic() {
        let mut doc = document("actual");
        doc.content.insert(0, break_node());
        assert_unchanged_after_refusal(
            Audit::Proven(vec![source_break(true), normal(vec![run("other", 20)])]),
            doc,
        );
        let mut doc = document("same");
        doc.content.insert(0, break_node());
        doc.attrs.insert("unknown".into(), true.into());
        assert_unchanged_after_refusal(
            Audit::Proven(vec![source_break(true), normal(vec![run("same", 20)])]),
            doc,
        );
    }
    #[test]
    fn source_attachment_rejects_partial_promotion_or_preexisting_authored_metadata() {
        let mut doc = document("");
        doc.content = vec![break_node(), break_node()];
        assert_unchanged_after_refusal(
            Audit::Proven(vec![source_break(true), source_break(false)]),
            doc,
        );
        let mut doc = document("");
        doc.content = vec![break_node()];
        doc.attrs
            .insert("splitPageBreakAndParagraphMark".into(), true.into());
        assert_unchanged_after_refusal(Audit::Proven(vec![source_break(true)]), doc);
        let mut doc = document("");
        doc.content = vec![break_node()];
        doc.content[0]
            .attrs
            .insert("breakParagraph".into(), serde_json::json!({}));
        assert_unchanged_after_refusal(Audit::Proven(vec![source_break(true)]), doc);
    }
    #[test]
    fn paragraph_spacing_defaults_are_source_zero_zero_single() {
        assert_eq!(
            resolve(&[], &[]),
            ParagraphSpacing {
                before: 0,
                after: 0,
                line: EncodedLineSpacing::Multiple(240)
            }
        );
        assert_eq!(resolve(&[], &lspd(0, 0)).supported_line().unwrap(), 240);
        assert!(
            resolve(&[], &lspd(0, 1))
                .supported_line()
                .unwrap_err()
                .contains("0/240")
        );
    }
    #[test]
    fn paragraph_spacing_supported_integer_boundaries_are_exact() {
        for before in [0, 1, 31680] {
            for after in [0, 1, 31680] {
                for line in [120, 240, 241, 276, 360, 2400] {
                    let p = resolve(
                        &gap(0xa413, before),
                        &[gap(0xa414, after), lspd(line, 1)].concat(),
                    );
                    assert_eq!(
                        (p.before, p.after, p.supported_line().unwrap()),
                        (before, after, line)
                    );
                }
            }
        }
    }
    #[test]
    fn paragraph_spacing_normal_then_direct_last_wins_by_component() {
        let style = [gap(0xa413, 100), gap(0xa414, 200), lspd(276, 1)].concat();
        let direct = [gap(0xa414, 300), gap(0xa414, 0), lspd(360, 1), lspd(241, 1)].concat();
        assert_eq!(
            resolve(&style, &direct),
            ParagraphSpacing {
                before: 100,
                after: 0,
                line: EncodedLineSpacing::Multiple(241)
            }
        );
    }
    #[test]
    fn paragraph_spacing_classifies_only_final_valid_value() {
        for first in [
            lspd(0xffff, 0),
            lspd(0x8440, 1),
            lspd(300, 0),
            lspd(0, 1),
            lspd(31680, 1),
        ] {
            assert_eq!(
                resolve(&first, &lspd(276, 1)).supported_line().unwrap(),
                276
            );
            assert_eq!(
                resolve(&[], &[first, lspd(240, 1)].concat())
                    .supported_line()
                    .unwrap(),
                240
            );
        }
    }
    #[test]
    fn paragraph_spacing_negative_is_exact_for_either_legal_flag() {
        for flag in [0, 1] {
            for (encoded, twips) in [(0xffff, 1), (0xfe98, 360), (0x8440, 31680)] {
                let p = resolve(&[], &lspd(encoded, flag));
                assert_eq!(p.line, EncodedLineSpacing::Exact(twips));
                assert!(
                    p.supported_line()
                        .unwrap_err()
                        .contains(&format!("exact line spacing of {twips} twips"))
                );
            }
        }
    }
    #[test]
    fn paragraph_spacing_valid_unsupported_values_do_not_poison_typography() {
        for line in [
            lspd(1, 0),
            lspd(31680, 0),
            lspd(0, 1),
            lspd(119, 1),
            lspd(2401, 1),
            lspd(31680, 1),
        ] {
            let p = resolve(&[], &line);
            assert!(p.supported_line().is_err());
        }
    }
    #[test]
    fn paragraph_spacing_rejects_every_malformed_value_even_if_overridden() {
        let invalid = [
            gap(0xa413, 31681),
            gap(0xa413, 65535),
            gap(0xa414, 31681),
            gap(0xa414, 65535),
            lspd(0x7bc1, 0),
            lspd(0x8000, 1),
            lspd(0x843f, 1),
            lspd(240, 2),
            lspd(0xffff, 65535),
        ];
        for value in invalid {
            for suffix in [vec![], lspd(240, 1), vec![5, 0x24, 1], gap(0xa413, 0)] {
                let data = [value.clone(), suffix].concat();
                assert!(
                    apply_paragraph(
                        &data,
                        &mut ParagraphSpacing::default(),
                        &mut BTreeSet::new()
                    )
                    .is_err()
                );
            }
        }
    }
    #[test]
    fn paragraph_spacing_rejects_truncated_operands() {
        for operation in [gap(0xa413, 0), gap(0xa414, 0), lspd(240, 1)] {
            for end in 1..operation.len() {
                assert!(
                    apply_paragraph(
                        &operation[..end],
                        &mut ParagraphSpacing::default(),
                        &mut BTreeSet::new()
                    )
                    .is_err()
                );
            }
        }
    }
    #[test]
    fn spacing_does_not_expand_paragraph_opcode_whitelist() {
        // Automatic gaps, contextual spacing, line-unit gaps, grid and outline.
        for (op, operand) in [
            (0x245bu16, vec![0]),
            (0x245c, vec![0]),
            (0x246d, vec![0]),
            (0x4458, vec![0, 0]),
            (0x4459, vec![0, 0]),
            (0x2447, vec![0]),
            (0x2640, vec![9]),
        ] {
            let mut reasons = BTreeSet::new();
            let bytes = [op.to_le_bytes().as_slice(), &operand].concat();
            apply_paragraph(&bytes, &mut ParagraphSpacing::default(), &mut reasons).unwrap();
            assert!(reasons.iter().any(|r| r.contains(&format!("0x{op:04X}"))));
        }
    }
    #[test]
    fn spacing_source_proof_uses_normal_then_each_distinct_papx() {
        let style = [gap(0xa414, 200), lspd(276, 1)].concat();
        let direct = [gap(0xa413, 100), lspd(360, 1)].concat();
        let blank = [gap(0xa414, 0), lspd(0, 0)].concat();
        let last = lspd(0, 1);
        let (word, table) = fixture_spacing(&style, [&direct, &blank, &[], &last]);
        let Audit::Proven(blocks) = audit_streams(&word, &table).unwrap() else {
            panic!("source proof unexpectedly unsupported")
        };
        assert_eq!(blocks.len(), 4);
        for (index, expected) in [
            (0, (100, 200, Ok(360))),
            (1, (0, 0, Ok(240))),
            (3, (0, 200, Err(()))),
        ] {
            let Block::NormalParagraph { spacing, .. } = &blocks[index] else {
                panic!("paragraph identity")
            };
            assert_eq!(
                (
                    spacing.before,
                    spacing.after,
                    spacing.supported_line().map_err(|_| ())
                ),
                expected
            );
        }
        assert!(matches!(blocks[2], Block::SourcePageBreak { .. }));
    }
    #[test]
    fn spacing_source_proof_rejects_malformed_style_or_papx_despite_fallback() {
        for malformed in [
            gap(0xa413, 31681),
            gap(0xa414, 65535),
            lspd(0x8000, 1),
            lspd(240, 2),
        ] {
            let (word, table) = fixture_spacing(&malformed, [&[], &[], &[], &[]]);
            let error = audit_streams(&word, &table).unwrap_err().to_string();
            assert!(
                error.contains("spacing") || error.contains("LSPD"),
                "{error}"
            );
            let direct = [malformed.clone(), lspd(240, 1)].concat();
            let (word, table) = fixture_spacing(&[], [&[], &[], &[], &direct]);
            let error = audit_streams(&word, &table).unwrap_err().to_string();
            assert!(
                error.contains("spacing") || error.contains("LSPD"),
                "{error}"
            );
            let unknown = [5, 0x24, 1];
            let (word, table) = fixture_spacing(&malformed, [&unknown, &[], &[], &[]]);
            let error = audit_streams(&word, &table).unwrap_err().to_string();
            assert!(
                error.contains("spacing") || error.contains("LSPD"),
                "{error}"
            );
            let (word, table) = fixture_spacing(&[], [&unknown, &[], &[], &malformed]);
            let error = audit_streams(&word, &table).unwrap_err().to_string();
            assert!(
                error.contains("spacing") || error.contains("LSPD"),
                "{error}"
            );
        }
    }
    #[test]
    fn malformed_spacing_in_active_prc_or_nested_character_path_is_rejected() {
        for malformed in [
            gap(0xa413, 31681),
            gap(0xa414, 65535),
            lspd(0x8000, 1),
            lspd(240, 2),
        ] {
            assert!(
                apply_character(
                    &malformed,
                    &mut props(),
                    false,
                    &parsed_font(),
                    &mut BTreeSet::new()
                )
                .is_err()
            );
            let nested = [vec![0x47, 0xca, malformed.len() as u8], malformed].concat();
            assert!(
                apply_character(
                    &nested,
                    &mut props(),
                    false,
                    &parsed_font(),
                    &mut BTreeSet::new()
                )
                .is_err()
            );
        }
        let mut reasons = BTreeSet::new();
        apply_character(
            &lspd(276, 1),
            &mut props(),
            false,
            &parsed_font(),
            &mut reasons,
        )
        .unwrap();
        assert_eq!(
            reasons,
            BTreeSet::from([
                "active character modifier 0x6412 is outside audited typography".into()
            ])
        );
    }
    #[test]
    fn spacing_unsupported_tuple_is_atomic_without_revoking_fonts_or_normal() {
        let mut n = document("Title");
        n.content[0] = heading("Title", 1);
        n.content[0].attrs.extend(spacing_attrs(80, 140, 300));
        let mut risks = BTreeSet::from(["existing".into()]);
        let p = resolve(
            &[gap(0xa413, 500), gap(0xa414, 600)].concat(),
            &lspd(300, 0),
        );
        attach(
            Audit::Proven(vec![with_spacing(vec![run("Title", 20)], p)]),
            &mut n,
            &mut risks,
        )
        .unwrap();
        assert_eq!(n.content[0].kind, "paragraph");
        assert_eq!(n.content[0].attrs, spacing_attrs(80, 140, 300));
        assert_eq!(n.content[0].content[0].marks.len(), 2);
        assert!(
            risks.contains(PRESERVED)
                && risks.contains(NORMAL_PRESERVED)
                && risks.contains("existing")
        );
        assert!(
            risks
                .iter()
                .any(|r| r.contains("preserved for 0 audited Normal paragraph(s); 1 paragraph(s)"))
        );
        assert!(
            risks
                .iter()
                .any(|r| r.contains("minimum line spacing of 300 twips"))
        );
        assert!(
            !risks
                .iter()
                .any(|r| r.contains("font names and effective sizes are not preserved"))
        );
    }
    #[test]
    fn spacing_supported_and_unsupported_paragraphs_are_counted_independently() {
        let mut n = document("same");
        n.content.push(document("same").content.remove(0));
        let mut risks = BTreeSet::new();
        attach(
            Audit::Proven(vec![
                normal(vec![run("same", 20)]),
                with_spacing(vec![run("same", 20)], resolve(&[], &lspd(0, 1))),
            ]),
            &mut n,
            &mut risks,
        )
        .unwrap();
        assert_eq!(n.content[0].attrs, spacing_attrs(0, 0, 240));
        assert!(n.content[1].attrs.is_empty());
        assert!(
            risks
                .iter()
                .any(|r| r.contains("preserved for 1 audited Normal paragraph(s); 1 paragraph(s)"))
        );
    }
    #[test]
    fn spacing_preserves_exact_whitespace_and_distinguishes_truly_empty() {
        let mut n = document(" \u{a0} ");
        n.content.push(Node {
            content: vec![],
            ..document("").content.remove(0)
        });
        n.content.push(document(" ").content.remove(0));
        let mut risks = BTreeSet::new();
        attach(
            Audit::Proven(vec![
                normal(vec![run(" \u{a0} ", 20)]),
                normal(vec![]),
                normal(vec![run(" ", 20)]),
            ]),
            &mut n,
            &mut risks,
        )
        .unwrap();
        assert_eq!(n.content[0].content[0].text.as_deref(), Some(" \u{a0} "));
        assert!(n.content[1].content.is_empty());
        assert_eq!(n.content[2].content[0].text.as_deref(), Some(" "));
        assert!(risks.iter().any(|r| {
            r.contains("empty-paragraph baseline fonts are not preserved (1 paragraph(s))")
        }));
    }
    #[test]
    fn spacing_failure_keeps_model_and_all_diagnostics_atomic() {
        let mut n = document("Title");
        n.content[0] = heading("Title", 1);
        n.content.push(document("Actual").content.remove(0));
        assert_unchanged_after_refusal(
            Audit::Proven(vec![
                with_spacing(vec![run("Title", 20)], resolve(&[], &lspd(0, 1))),
                normal(vec![run("Other", 20)]),
            ]),
            n,
        );
    }
    #[test]
    fn spacing_break_only_paragraph_cannot_smuggle_attributes_or_content() {
        for change in [
            serde_json::json!({"type":"pageBreak","attrs":{"spaceAfter":200}}),
            serde_json::json!({"type":"pageBreak","text":"x"}),
            serde_json::json!({"type":"pageBreak","marks":[{"type":"bold"}]}),
            serde_json::json!({"type":"pageBreak","content":[{"type":"text","text":"x"}]}),
        ] {
            let mut n = document("Title");
            n.content.push(serde_json::from_value(change).unwrap());
            assert_unchanged_after_refusal(
                Audit::Proven(vec![normal(vec![run("Title", 20)]), bare_break()]),
                n,
            );
        }
    }
    #[test]
    fn encoded_normal_replaces_each_inferred_heading_level() {
        for level in 1..=3 {
            let mut n = document("Encoded Normal title");
            n.content[0] = heading("Encoded Normal title", level);
            let mut risks = BTreeSet::new();
            attach(
                Audit::Proven(vec![normal(vec![run("Encoded Normal title", 22)])]),
                &mut n,
                &mut risks,
            )
            .unwrap();
            assert_eq!(n.content[0].kind, "paragraph");
            assert_eq!(n.content[0].attrs, spacing_attrs(0, 0, 240));
            assert_eq!(
                n.content[0].content[0].text.as_deref(),
                Some("Encoded Normal title")
            );
            assert!(risks.contains(NORMAL_PRESERVED));
            assert!(!risks.iter().any(|r| r.contains("inferred heading")));
        }
    }
    #[test]
    fn normal_identity_repair_preserves_other_attrs_marks_and_exact_runs() {
        let mut n = document("A’bc");
        n.attrs = serde_json::from_value(serde_json::json!({
            "styleProfile":"standard-v2", "pageGeometry":{
                "widthTwips":11906,"heightTwips":16838,"marginTopTwips":1440,
                "marginRightTwips":1440,"marginBottomTwips":1440,"marginLeftTwips":1440
            }
        }))
        .unwrap();
        n.content[0] = heading("A’bc", 2);
        n.content[0].attrs.extend(serde_json::Map::from_iter([
            ("textAlign".into(), "center".into()),
            ("spaceBefore".into(), 80.into()),
            ("spaceAfter".into(), 140.into()),
            ("lineSpacing".into(), 300.into()),
            ("indentLeftTwips".into(), 120.into()),
            ("indentRightTwips".into(), 240.into()),
            ("firstLineIndentTwips".into(), 60.into()),
            ("keepNext".into(), false.into()),
            ("keepLines".into(), true.into()),
            ("pageBreakBefore".into(), false.into()),
        ]));
        n.content[0].content[0].marks = serde_json::from_value(serde_json::json!([
            {"type":"bold"}, {"type":"italic"}, {"type":"underline"},
            {"type":"strike"}, {"type":"superscript"}, {"type":"code"},
            {"type":"textColor","attrs":{"rgb":"1234AB"}}
        ]))
        .unwrap();
        let mut expected = n.clone();
        expected.content[0].kind = "paragraph".into();
        expected.content[0].attrs.remove("level");
        expected.content[0].attrs.extend(spacing_attrs(0, 0, 240));
        let mut first = expected.content[0].content[0].clone();
        first.text = Some("A’".into());
        first.marks.extend(
            serde_json::from_value::<Vec<Mark>>(serde_json::json!([
                {"type":"fontSize","attrs":{"halfPoints":22}},
                {"type":"fontFamily","attrs":{"name":"Arial"}}
            ]))
            .unwrap(),
        );
        let mut second = expected.content[0].content[0].clone();
        second.text = Some("bc".into());
        second.marks.extend(
            serde_json::from_value::<Vec<Mark>>(serde_json::json!([
                {"type":"fontSize","attrs":{"halfPoints":32}},
                {"type":"fontFamily","attrs":{"name":"Times New Roman"}}
            ]))
            .unwrap(),
        );
        expected.content[0].content = vec![first, second];
        let mut second_source = run("bc", 32);
        second_source.font = "Times New Roman".into();
        second_source.families = [1; 2];
        expected.attrs.insert(
            "fontFamilyMetadata".into(),
            serde_json::json!([
                {"name":"Arial","family":"swiss"},
                {"name":"Times New Roman","family":"roman"}
            ]),
        );
        attach(
            Audit::Proven(vec![normal(vec![run("A’", 22), second_source])]),
            &mut n,
            &mut BTreeSet::new(),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(n).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }
    #[test]
    fn normal_identity_repair_keeps_blank_and_break_positions() {
        let mut n: Node = serde_json::from_value(serde_json::json!({"type":"doc","content":[
            {"type":"paragraph"}, {"type":"paragraph"},
            {"type":"heading","attrs":{"level":1},"content":[{"type":"text","text":"Title"}]},
            {"type":"paragraph"}, {"type":"pageBreak"}, {"type":"paragraph"},
            {"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"LATER"}]},
            {"type":"paragraph"}
        ]}))
        .unwrap();
        let mut before = n.clone();
        for index in [0, 1, 3, 5, 7] {
            before.content[index].attrs.extend(spacing_attrs(0, 0, 240));
        }
        attach(
            Audit::Proven(vec![
                normal(vec![]),
                normal(vec![]),
                normal(vec![run("Title", 20)]),
                normal(vec![]),
                bare_break(),
                normal(vec![]),
                normal(vec![run("LATER", 20)]),
                normal(vec![]),
            ]),
            &mut n,
            &mut BTreeSet::new(),
        )
        .unwrap();
        assert_eq!(n.content.len(), 8);
        for index in [0, 1, 3, 4, 5, 7] {
            assert_eq!(
                serde_json::to_value(&n.content[index]).unwrap(),
                serde_json::to_value(&before.content[index]).unwrap()
            );
        }
        for index in [2, 6] {
            assert_eq!(n.content[index].kind, "paragraph");
            assert_eq!(n.content[index].attrs, spacing_attrs(0, 0, 240));
        }
    }
    #[test]
    fn empty_normal_identity_repair_retains_existing_baseline_font() {
        let mut n: Node = serde_json::from_value(serde_json::json!({"type":"doc","content":[
            {"type":"heading","attrs":{"level":1,"textAlign":"right",
                "emptyParagraphFont":{"family":"Arial","halfPoints":22,"bold":false,"italic":true}}}
        ]}))
        .unwrap();
        let mut expected = n.clone();
        expected.content[0].kind = "paragraph".into();
        expected.content[0].attrs.remove("level");
        expected.content[0].attrs.extend(spacing_attrs(0, 0, 240));
        attach(
            Audit::Proven(vec![normal(vec![])]),
            &mut n,
            &mut BTreeSet::new(),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(n).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }
    #[test]
    fn unsupported_keeps_heading_identity_and_entire_model() {
        let mut n = document("Title");
        n.content[0] = heading("Title", 1);
        let before = serde_json::to_value(&n).unwrap();
        let mut risks = BTreeSet::from(["existing risk".into()]);
        attach(
            Audit::Unsupported(BTreeSet::from(["active outline".into()])),
            &mut n,
            &mut risks,
        )
        .unwrap();
        assert_eq!(serde_json::to_value(n).unwrap(), before);
        assert_eq!(
            risks,
            BTreeSet::from([
                "existing risk".into(),
                "Legacy DOC font names and effective sizes are not preserved: active outline"
                    .into(),
            ])
        );
    }
    #[test]
    fn same_length_or_trimmed_text_cannot_authorize_normal_repair() {
        for source in ["Other", " Title", "Title "] {
            let mut n = document("Title");
            n.content[0] = heading("Title", 1);
            assert_unchanged_after_refusal(Audit::Proven(vec![normal(vec![run(source, 20)])]), n);
        }
    }
    #[test]
    fn late_identity_failure_rolls_back_normal_repair_and_notices() {
        let mut n = document("Title");
        n.content[0] = heading("Title", 1);
        n.content.push(document("Actual").content.remove(0));
        assert_unchanged_after_refusal(
            Audit::Proven(vec![
                normal(vec![run("Title", 20)]),
                normal(vec![run("Wrong!", 20)]),
            ]),
            n,
        );
    }
    #[test]
    fn late_break_failure_rolls_back_normal_repair_and_notices() {
        let mut n = document("Title");
        n.content[0] = heading("Title", 1);
        n.content.push(document("Actual").content.remove(0));
        assert_unchanged_after_refusal(
            Audit::Proven(vec![normal(vec![run("Title", 20)]), bare_break()]),
            n,
        );
    }
    #[test]
    fn final_validation_failure_rolls_back_normal_repair_and_notices() {
        let mut n = document("Title");
        n.content[0] = heading("Title", 1);
        n.content[0].attrs.insert("unknown".into(), true.into());
        assert_unchanged_after_refusal(Audit::Proven(vec![normal(vec![run("Title", 20)])]), n);
    }
    #[test]
    fn outline_declarations_never_prove_encoded_normal_identity() {
        for level in 0..=9 {
            let mut reasons = BTreeSet::new();
            apply_paragraph(
                &[0x40, 0x26, level],
                &mut ParagraphSpacing::default(),
                &mut reasons,
            )
            .unwrap();
            assert!(reasons.iter().any(|r| r.contains("0x2640")));
        }
        assert!(
            apply_paragraph(
                &[0x40, 0x26],
                &mut ParagraphSpacing::default(),
                &mut BTreeSet::new()
            )
            .is_err()
        );
    }
    #[test]
    fn non_normal_and_inherited_style_identities_are_not_proven() {
        for (identity, base_index) in [(1u16, 0xfffu16), (2, 0xfff), (3, 0xfff), (0, 15)] {
            let mut base = [0u8; 10];
            base[..2].copy_from_slice(&identity.to_le_bytes());
            base[2..4].copy_from_slice(&((base_index << 4) | 1).to_le_bytes());
            base[6..8].copy_from_slice(&10u16.to_le_bytes());
            let style = Style {
                base: &base,
                record_length: 10,
                name_empty: false,
                upx: vec![&[], &[]],
            };
            let mut reasons = BTreeSet::new();
            validate_active_style(&style, identity, true, &mut reasons).unwrap();
            assert!(!reasons.is_empty());
        }
    }
    #[test]
    fn font_literal_name_is_not_substituted() {
        assert_eq!(parsed_font()[0].name, "Times New Roman");
    }
    #[test]
    fn truncated_font_table_refused() {
        let mut f = font_table("Arial");
        f.pop();
        assert!(parse_fonts(&f).is_err());
    }
    #[test]
    fn font_table_trailing_bytes_refused() {
        let mut f = font_table("Arial");
        f.push(0);
        assert!(parse_fonts(&f).is_err());
    }
    #[test]
    fn bad_font_count_refused() {
        let mut f = font_table("Arial");
        f[..2].copy_from_slice(&257u16.to_le_bytes());
        assert!(parse_fonts(&f).is_err());
    }
    #[test]
    fn unterminated_font_name_refused() {
        let mut f = font_table("Arial");
        let n = f.len();
        f[n - 2] = 1;
        assert!(parse_fonts(&f).is_err());
    }
    #[test]
    fn invalid_font_utf16_refused() {
        let mut f = font_table("Arial");
        f[44] = 0;
        f[45] = 0xd8;
        assert!(parse_fonts(&f).is_err());
    }
    #[test]
    fn invalid_alternate_name_refused() {
        let mut f = font_table("Arial");
        f[9] = 1;
        assert!(parse_fonts(&f).is_err());
    }
    #[test]
    fn invalid_font_weight_refused() {
        let mut f = font_table("Arial");
        f[6..8].copy_from_slice(&1001u16.to_le_bytes());
        assert!(parse_fonts(&f).is_err());
    }
    #[test]
    fn invalid_font_charset_refused() {
        let mut f = font_table("Arial");
        f[8] = 3;
        assert!(parse_fonts(&f).is_err());
    }
    #[test]
    fn invalid_font_family_refused() {
        let mut f = font_table("Arial");
        f[5] = 0x66;
        assert!(parse_fonts(&f).is_err());
    }
    #[test]
    fn default_size_is_normative_twenty() {
        assert_eq!(props().hps, 20);
    }
    #[test]
    fn direct_size_overrides_style_size() {
        let mut p = props();
        let mut r = BTreeSet::new();
        apply_character(&[0x43, 0x4a, 22, 0], &mut p, false, &parsed_font(), &mut r).unwrap();
        apply_character(&[0x43, 0x4a, 32, 0], &mut p, true, &parsed_font(), &mut r).unwrap();
        assert_eq!((p.hps, p.direct_hps), (32, Some(32)));
        assert!(r.is_empty());
    }
    #[test]
    fn truncated_size_refused_after_unsupported_opcode() {
        let mut p = props();
        let mut r = BTreeSet::new();
        assert!(
            apply_character(
                &[0x35, 8, 1, 0x43, 0x4a, 22],
                &mut p,
                true,
                &parsed_font(),
                &mut r
            )
            .is_err()
        );
    }
    #[test]
    fn out_of_bounds_font_index_refused() {
        let mut p = props();
        let mut r = BTreeSet::new();
        assert!(
            apply_character(&[0x4f, 0x4a, 1, 0], &mut p, true, &parsed_font(), &mut r).is_err()
        );
    }
    #[test]
    fn invalid_halfpoint_size_refused() {
        let mut p = props();
        let mut r = BTreeSet::new();
        assert!(
            apply_character(&[0x43, 0x4a, 1, 0], &mut p, true, &parsed_font(), &mut r).is_err()
        );
    }
    #[test]
    fn reset_modifier_is_visible_fallback() {
        let mut p = props();
        let mut r = BTreeSet::new();
        apply_character(&[0x33, 0x2a, 0], &mut p, true, &parsed_font(), &mut r).unwrap();
        assert!(!r.is_empty());
    }
    #[test]
    fn unknown_variable_operand_is_strictly_bounded() {
        assert!(walk_sprms(&[0xff, 0xc8, 4, 1], false, |_, _| Ok(())).is_err());
    }
    #[test]
    fn special_variable_operand_is_strictly_bounded() {
        assert!(walk_sprms(&[0x15, 0xc6, 255, 3], false, |_, _| Ok(())).is_err());
        assert!(walk_sprms(&[8, 0xd6, 0, 0], false, |_, _| Ok(())).is_err());
    }
    #[test]
    fn cp1252_smart_apostrophe_is_exact() {
        let mut r = BTreeSet::new();
        assert_eq!(decode_latin(b"It\x92s", &mut r).unwrap(), "It’s");
        assert!(r.is_empty());
    }
    #[test]
    fn undefined_cp1252_byte_refused() {
        assert!(decode_latin(&[0x81], &mut BTreeSet::new()).is_err());
    }
    #[test]
    fn complex_or_control_text_is_not_proven() {
        let mut r = BTreeSet::new();
        decode_latin(&[0xb5, 1], &mut r).unwrap();
        assert!(!r.is_empty());
    }
    #[test]
    fn source_identity_mismatch_refused_without_mutation() {
        let mut n = document("different");
        let before = n.clone();
        assert!(
            attach(
                Audit::Proven(vec![normal(vec![run("source", 20)])]),
                &mut n,
                &mut BTreeSet::new()
            )
            .is_err()
        );
        assert_eq!(
            serde_json::to_value(n).unwrap(),
            serde_json::to_value(before).unwrap()
        );
    }
    #[test]
    fn raw_font_family_values_preserve_ignored_bits_and_existing_invalid_refusal() {
        for family in 0..=7 {
            let mut raw = font_table("Literal");
            raw[5] = (raw[5] & 0x0f) | (family << 4) | 0x88;
            let parsed = parse_fonts(&raw);
            if family <= 5 {
                assert_eq!(parsed.unwrap()[0].family, family);
            } else {
                assert!(parsed.is_err());
            }
        }
    }
    fn family_fixture_fonts(word: &mut [u8], table: &mut Vec<u8>, fonts: &[(&str, u8)]) {
        let mut raw = vec![fonts.len() as u8, 0, 0, 0];
        for (name, family) in fonts {
            let mut entry = font_table(name);
            entry[5] = (entry[5] & 0x0f) | (family << 4);
            raw.extend_from_slice(&entry[4..]);
        }
        word[0x112..0x116].copy_from_slice(&(table.len() as u32).to_le_bytes());
        word[0x116..0x11a].copy_from_slice(&(raw.len() as u32).to_le_bytes());
        table.extend(raw);
    }
    fn family_fixture_doc() -> Node {
        serde_json::from_value(serde_json::json!({"type":"doc","content":[
            {"type":"paragraph","content":[{"type":"text","text":"hello."}]},
            {"type":"paragraph"}, {"type":"pageBreak"},
            {"type":"paragraph","content":[{"type":"text","text":"world."}]}
        ]}))
        .unwrap()
    }
    fn family_fixture_chpx(word: &mut [u8], boundaries: &[u32], font_indices: &[u16]) {
        assert_eq!(boundaries.len(), font_indices.len() + 1);
        let page = &mut word[1536..2048];
        page.fill(0);
        page[511] = font_indices.len() as u8;
        for (index, fc) in boundaries.iter().enumerate() {
            page[4 * index..4 * index + 4].copy_from_slice(&fc.to_le_bytes());
        }
        for (index, font) in font_indices.iter().enumerate() {
            let pos = 400 + index * 12;
            page[4 * boundaries.len() + index] = (pos / 2) as u8;
            page[pos] = 8;
            for (offset, op) in [(1, 0x4a4fu16), (5, 0x4a51u16)] {
                page[pos + offset..pos + offset + 2].copy_from_slice(&op.to_le_bytes());
                page[pos + offset + 2..pos + offset + 4].copy_from_slice(&font.to_le_bytes());
            }
        }
    }
    #[test]
    fn raw_ordinary_cr_empty_and_unretained_break_families_do_not_block_text_metadata() {
        let (mut word, mut table) = fixture_spacing(&[], [&[], &[], &[], &[]]);
        family_fixture_fonts(&mut word, &mut table, &[("Literal", 1), ("Unemitted", 5)]);
        family_fixture_chpx(&mut word, &[1024, 1030, 1034, 1040, 1041], &[0, 1, 0, 1]);
        let audit = audit_streams(&word, &table).unwrap();
        let mut doc = family_fixture_doc();
        let mut risks = BTreeSet::new();
        attach(audit, &mut doc, &mut risks).unwrap();
        assert_eq!(
            doc.attrs["fontFamilyMetadata"],
            serde_json::json!([{"name":"Literal","family":"roman"}])
        );
        assert!(doc.content[1].content.is_empty() && doc.content[2].attrs.is_empty());
        assert!(
            !risks
                .iter()
                .any(|risk| risk.contains("metadata is not preserved"))
        );
    }
    #[test]
    fn raw_break_and_trailing_cr_family_mismatch_keeps_prior_metric_proof() {
        let (mut word, mut table) = fixture_spacing(&[], [&[], &[], &[], &[]]);
        family_fixture_fonts(&mut word, &mut table, &[("Literal", 1), ("Literal", 2)]);
        family_fixture_chpx(&mut word, &[1024, 1033, 1034, 1041], &[0, 1, 0]);
        let Audit::Proven(mut blocks) = audit_streams(&word, &table).unwrap() else {
            panic!("body proof")
        };
        let Block::SourcePageBreak {
            families, metrics, ..
        } = &blocks[2]
        else {
            panic!("break proof")
        };
        assert_eq!(*families, [1, 1, 2, 2]);
        assert!(metrics.is_ok());
        promote_break_paragraphs(
            &mut blocks,
            &split_page_proof(Some(true), None),
            &BTreeSet::from([9]),
        );
        let mut doc = family_fixture_doc();
        let mut risks = BTreeSet::new();
        attach(Audit::Proven(blocks), &mut doc, &mut risks).unwrap();
        assert!(!doc.attrs.contains_key("fontFamilyMetadata"));
        assert_eq!(doc.attrs["splitPageBreakAndParagraphMark"], true);
        assert_eq!(
            doc.content[2].attrs["breakParagraph"]["font"]["family"],
            "Literal"
        );
        assert!(
            risks
                .iter()
                .any(|risk| risk.ends_with("disagree on font family"))
        );
    }
    #[test]
    fn unused_source_alias_conflict_or_unsupported_font_never_enters_metadata() {
        for unused in [("literal", 1), ("Literal", 1), ("Unused", 5)] {
            let (mut word, mut table) = fixture_spacing(&[], [&[], &[], &[], &[]]);
            family_fixture_fonts(&mut word, &mut table, &[("Literal", 2), unused]);
            let audit = audit_streams(&word, &table).unwrap();
            let mut doc = family_fixture_doc();
            let mut risks = BTreeSet::new();
            attach(audit, &mut doc, &mut risks).unwrap();
            assert_eq!(
                doc.attrs["fontFamilyMetadata"],
                serde_json::json!([
                    {"name":"Literal","family":"swiss"}
                ])
            );
            assert!(
                !risks
                    .iter()
                    .any(|risk| risk.contains("metadata is not preserved"))
            );
        }
    }
    #[test]
    fn raw_effective_same_name_slots_with_different_family_only_fall_back_metadata() {
        let (mut word, mut table) = fixture_spacing(&[], [&[], &[], &[], &[]]);
        family_fixture_fonts(&mut word, &mut table, &[("Literal", 1), ("Literal", 2)]);
        // STSHI ftcOther is index 1; ftcAscii remains index 0.
        let stsh = u32_at(&word, 0xa2).unwrap();
        table[stsh + 18..stsh + 20].copy_from_slice(&1u16.to_le_bytes());
        let audit = audit_streams(&word, &table).unwrap();
        assert!(matches!(audit, Audit::Proven(_)));
        let mut doc = family_fixture_doc();
        let mut risks = BTreeSet::new();
        attach(audit, &mut doc, &mut risks).unwrap();
        assert!(!doc.attrs.contains_key("fontFamilyMetadata"));
        assert_eq!(doc.content[0].content[0].marks[1].attrs["name"], "Literal");
        assert!(
            risks
                .iter()
                .any(|risk| risk.ends_with("disagree on font family"))
        );
    }
    #[test]
    fn unsupported_families_aliases_and_conflicts_fall_back_all_metadata_only() {
        for (name, families, reason) in [
            ("Other", [0, 0], "outside the supported"),
            ("Other", [3, 3], "outside the supported"),
            ("Other", [4, 4], "outside the supported"),
            ("Other", [5, 5], "outside the supported"),
            ("Other", [1, 2], "disagree on font family"),
            ("Arial", [1, 1], "conflicting source families"),
            ("ARIAL", [2, 2], "ASCII-case aliases"),
        ] {
            let mut second = run("y", 22);
            second.font = name.into();
            second.families = families;
            let mut doc = document("xy");
            let mut risks = BTreeSet::from(["existing".into()]);
            attach(
                Audit::Proven(vec![normal(vec![run("x", 20), second])]),
                &mut doc,
                &mut risks,
            )
            .unwrap();
            assert!(!doc.attrs.contains_key("fontFamilyMetadata"));
            assert_eq!(doc.content[0].attrs, spacing_attrs(0, 0, 240));
            assert_eq!(doc.content[0].content[1].marks[1].attrs["name"], name);
            assert!(risks.contains("existing") && risks.iter().any(|risk| risk.contains(reason)));
            assert!(
                !risks
                    .iter()
                    .any(|risk| risk.contains("metadata is preserved"))
            );
        }
    }
    #[test]
    fn exact_duplicate_families_collapse_without_name_inference() {
        let mut second = run("y", 22);
        second.font = "Z Literal".into();
        second.families = [1; 2];
        let mut doc = document("xyz");
        attach(
            Audit::Proven(vec![normal(vec![run("x", 20), second, run("z", 24)])]),
            &mut doc,
            &mut BTreeSet::new(),
        )
        .unwrap();
        assert_eq!(
            doc.attrs["fontFamilyMetadata"],
            serde_json::json!([
                {"name":"Arial","family":"swiss"}, {"name":"Z Literal","family":"roman"}
            ])
        );
    }
    #[test]
    fn retained_break_family_disagreement_preserves_old_split_metrics_atomically() {
        let mut block = source_break(true);
        if let Block::SourcePageBreak { families, .. } = &mut block {
            *families = [2, 2, 1, 1];
        }
        let mut doc = document("x");
        doc.content.push(break_node());
        let mut risks = BTreeSet::new();
        attach(
            Audit::Proven(vec![normal(vec![run("x", 20)]), block]),
            &mut doc,
            &mut risks,
        )
        .unwrap();
        assert!(!doc.attrs.contains_key("fontFamilyMetadata"));
        assert_eq!(doc.attrs["splitPageBreakAndParagraphMark"], true);
        assert_eq!(
            doc.content[1].attrs["breakParagraph"]["font"]["family"],
            "Calibri"
        );
        assert!(
            risks
                .iter()
                .any(|risk| risk.ends_with("disagree on font family"))
        );
    }
    #[test]
    fn only_emitted_text_and_retained_break_metrics_supply_family_evidence() {
        let mut ignored = source_break(false);
        if let Block::SourcePageBreak { families, .. } = &mut ignored {
            *families = [5; 4];
        }
        let mut doc = document("x");
        doc.content.extend([
            break_node(),
            serde_json::from_value(serde_json::json!({"type":"paragraph"})).unwrap(),
        ]);
        let mut empty = run("", 20);
        empty.families = [5; 2];
        let mut risks = BTreeSet::new();
        attach(
            Audit::Proven(vec![
                normal(vec![run("x", 20)]),
                ignored,
                normal(vec![empty]),
            ]),
            &mut doc,
            &mut risks,
        )
        .unwrap();
        assert_eq!(
            doc.attrs["fontFamilyMetadata"],
            serde_json::json!([{"name":"Arial","family":"swiss"}])
        );
        assert!(
            !risks
                .iter()
                .any(|risk| risk.contains("metadata is not preserved"))
        );
    }
    #[test]
    fn family_evidence_and_diagnostics_never_escape_late_identity_or_model_failure() {
        let mut doc = document("x");
        doc.content.push(text_node("invalid top-level node"));
        assert_unchanged_after_refusal(
            Audit::Proven(vec![
                normal(vec![run("x", 20)]),
                normal(vec![run("bad", 20)]),
            ]),
            doc,
        );
        let mut doc = document("x");
        doc.attrs.insert("unknown".into(), true.into());
        assert_unchanged_after_refusal(Audit::Proven(vec![normal(vec![run("x", 20)])]), doc);
        let mut doc = document("x");
        doc.attrs.insert(
            "fontFamilyMetadata".into(),
            serde_json::json!([{"name":"Arial","family":"roman"}]),
        );
        assert_unchanged_after_refusal(Audit::Proven(vec![normal(vec![run("x", 20)])]), doc);
    }
    #[test]
    fn exact_run_split_preserves_existing_marks() {
        let mut n = document("a’bc");
        n.content[0].content[0].marks.push(Mark {
            kind: "bold".into(),
            attrs: Default::default(),
        });
        attach(
            Audit::Proven(vec![normal(vec![run("a’", 20), run("bc", 32)])]),
            &mut n,
            &mut BTreeSet::new(),
        )
        .unwrap();
        assert_eq!(n.content[0].content.len(), 2);
        assert!(
            n.content[0]
                .content
                .iter()
                .all(|n| n.marks.iter().any(|m| m.kind == "bold"))
        );
    }
    #[test]
    fn unproved_direct_size_refused() {
        let mut n = document("x");
        n.content[0].content[0].marks.push(Mark {
            kind: "fontSize".into(),
            attrs: serde_json::Map::from_iter([("halfPoints".into(), 22.into())]),
        });
        assert!(
            attach(
                Audit::Proven(vec![normal(vec![run("x", 20)])]),
                &mut n,
                &mut BTreeSet::new()
            )
            .is_err()
        );
    }
    #[test]
    fn unsupported_preserves_entire_old_model() {
        let mut n = document("unchanged");
        let before = n.clone();
        let mut risks = BTreeSet::new();
        attach(
            Audit::Unsupported(BTreeSet::from(["multiple pieces".into()])),
            &mut n,
            &mut risks,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(before).unwrap(),
            serde_json::to_value(n).unwrap()
        );
        assert!(
            risks
                .iter()
                .any(|s| s.contains("not preserved: multiple pieces"))
        );
    }
    #[test]
    fn fkp_gap_refused() {
        let rs = vec![
            FkpRun {
                range: 0..2,
                style: None,
                properties: &[],
            },
            FkpRun {
                range: 3..4,
                style: None,
                properties: &[],
            },
        ];
        assert!(complete_coverage(&rs, &(0..4)).is_err());
    }
    #[test]
    fn fkp_overlapping_payload_refused() {
        let mut word = vec![0; 512];
        word[511] = 2;
        word[0..4].copy_from_slice(&10u32.to_le_bytes());
        word[4..8].copy_from_slice(&20u32.to_le_bytes());
        word[8..12].copy_from_slice(&30u32.to_le_bytes());
        word[12] = 100;
        word[13] = 101;
        word[200] = 5;
        word[202] = 5;
        let mut plc = vec![];
        for v in [10u32, 30, 0] {
            plc.extend(v.to_le_bytes());
        }
        assert!(parse_fkp(&word, &plc, false, &mut 0).is_err());
    }
    #[test]
    fn source_bounds_arithmetic_is_checked() {
        assert!(bytes(&[0], usize::MAX, 2).is_err());
    }
    #[test]
    fn complex_piece_modifier_requires_existing_prc() {
        let mut clx = vec![2];
        for value in [16u32, 0, 2] {
            clx.extend(value.to_le_bytes());
        }
        clx.extend(0u16.to_le_bytes());
        clx.extend(0x4000_0000u32.to_le_bytes());
        clx.extend(1u16.to_le_bytes());
        assert!(parse_piece(b"x\r", &clx, 2, &mut BTreeSet::new(), &mut Vec::new()).is_err());
    }
    #[test]
    fn character_style_reference_must_exist() {
        let runs = [FkpRun {
            range: 0..1,
            style: None,
            properties: &[0x30, 0x4a, 0xff, 0xff],
        }];
        assert!(
            validate_referenced_styles(
                &[],
                &[],
                &runs,
                &parsed_font(),
                (0, 0),
                &mut BTreeSet::new()
            )
            .is_err()
        );
    }
    #[test]
    fn malformed_inherited_character_properties_are_refused() {
        let mut normal = [0u8; 10];
        normal[2..4].copy_from_slice(&0x00f1u16.to_le_bytes());
        normal[4] = 2;
        normal[6] = 10;
        let mut inherited = [0u8; 10];
        inherited[2..4].copy_from_slice(&0xfff1u16.to_le_bytes());
        inherited[4] = 2;
        inherited[6] = 10;
        let mut character = [0u8; 10];
        character[0] = 65;
        character[2..4].copy_from_slice(&0xfff2u16.to_le_bytes());
        character[4] = 1;
        character[6] = 10;
        let mut styles: Vec<Option<Style<'_>>> = (0..16).map(|_| None).collect();
        styles[0] = Some(Style {
            base: &normal,
            record_length: 10,
            name_empty: false,
            upx: vec![&[0, 0], &[]],
        });
        styles[10] = Some(Style {
            base: &character,
            record_length: 10,
            name_empty: false,
            upx: vec![&[]],
        });
        styles[15] = Some(Style {
            base: &inherited,
            record_length: 10,
            name_empty: false,
            upx: vec![&[15, 0], &[0x4f, 0x4a, 0xff, 0xff]],
        });
        let paragraphs = [FkpRun {
            range: 0..1,
            style: Some(0),
            properties: &[],
        }];
        assert!(
            validate_referenced_styles(
                &styles,
                &paragraphs,
                &[],
                &parsed_font(),
                (0, 0),
                &mut BTreeSet::new()
            )
            .is_err()
        );
    }
    #[test]
    fn malformed_inert_complex_size_is_refused() {
        assert!(
            apply_character(
                &[0x61, 0x4a, 0xff, 0xff],
                &mut props(),
                true,
                &parsed_font(),
                &mut BTreeSet::new()
            )
            .is_err()
        );
    }
    #[test]
    fn majority_nested_truncation_is_refused() {
        assert!(
            apply_character(
                &[0x47, 0xca, 1, 0x43],
                &mut props(),
                true,
                &parsed_font(),
                &mut BTreeSet::new()
            )
            .is_err()
        );
    }
    #[test]
    fn majority_nested_bad_font_is_refused() {
        assert!(
            apply_character(
                &[0x47, 0xca, 4, 0x4f, 0x4a, 0xff, 0xff],
                &mut props(),
                true,
                &parsed_font(),
                &mut BTreeSet::new()
            )
            .is_err()
        );
    }
    #[test]
    fn majority_recursion_is_bounded() {
        let mut data = vec![];
        for _ in 0..10 {
            let mut outer = vec![0x47, 0xca, data.len() as u8];
            outer.extend(data);
            data = outer;
        }
        assert!(
            apply_character(
                &data,
                &mut props(),
                true,
                &parsed_font(),
                &mut BTreeSet::new()
            )
            .is_err()
        );
    }
    #[test]
    fn inactive_script_font_selector_still_checks_index() {
        for op in [0x4a50u16, 0x4a5e, 0x6a09] {
            let mut data = op.to_le_bytes().to_vec();
            data.extend([0xff, 0xff]);
            if op == 0x6a09 {
                data.extend([0, 0]);
            }
            assert!(
                apply_character(
                    &data,
                    &mut props(),
                    true,
                    &parsed_font(),
                    &mut BTreeSet::new()
                )
                .is_err()
            );
        }
    }
    #[test]
    fn absent_or_empty_font_table_has_no_invented_name() {
        assert!(parse_fonts(&[]).unwrap().is_empty());
        assert!(parse_fonts(&[0, 0, 0, 0]).unwrap().is_empty());
        let mut reasons = BTreeSet::new();
        validate_font(&props(), &[], &mut reasons).unwrap();
        assert!(!reasons.is_empty());
    }
    #[test]
    fn absent_font_table_requires_zero_indices() {
        let mut value = props();
        value.other = 1;
        assert!(validate_font(&value, &[], &mut BTreeSet::new()).is_err());
        assert!(
            apply_character(
                &[0x4f, 0x4a, 1, 0],
                &mut props(),
                true,
                &[],
                &mut BTreeSet::new()
            )
            .is_err()
        );
    }
}
