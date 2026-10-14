//! Independent, deliberately bounded source proof for binary DOC Latin fonts.
//!
//! This does not trust permissive legacy formatting recovery, implement a general
//! Word style engine, or promise appearance. All reads use the import snapshot.
use crate::document::{Mark, Node};
use anyhow::{Context, Result, ensure};
use office_oxide::cfb::{CfbReader, EntryType};
use std::{collections::BTreeSet, io::Cursor, ops::Range};

const MAX_PROPERTIES: usize = 1024 * 1024;
const MAX_RUNS: usize = 10_000;
const MAX_CP: usize = 1_000_000;
const PRESERVED: &str = "Legacy DOC literal Latin font names and effective sizes are preserved for audited body text; layout and local font substitution may differ";

#[derive(Debug)]
pub(crate) enum Audit {
    Proven(Vec<Block>),
    Unsupported(BTreeSet<String>),
}
#[derive(Debug)]
pub(crate) enum Block {
    Paragraph(Vec<Run>),
    PageBreak,
}
#[derive(Debug)]
pub(crate) struct Run {
    text: String,
    font: String,
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

pub(crate) fn audit(source: &[u8]) -> Result<Audit> {
    audit_inner(source).context("Cannot prove legacy DOC typography; source remains read-only")
}
fn audit_inner(source: &[u8]) -> Result<Audit> {
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
    audit_streams(&word, &table)
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
                apply_paragraph(&payload[2..], &mut reasons)?;
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
    for run in &paragraph_runs {
        apply_paragraph(run.properties, &mut reasons)?;
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
    for paragraph in paragraphs {
        let content = paragraph.start..paragraph.end - 1;
        if &word[content.clone()] == b"\x0c" {
            blocks.push(Block::PageBreak);
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
                    hps: props.hps,
                    direct_hps: props.direct_hps,
                });
            }
            k += 1;
        }
        blocks.push(Block::Paragraph(runs));
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
            apply_paragraph(&paragraph[2..], reasons)?;
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
fn apply_paragraph(data: &[u8], reasons: &mut BTreeSet<String>) -> Result<()> {
    walk_sprms(data, true, |op, _| {
        match op {
            // Spacing is not a font selector. Existing DOC normalization remains.
            0x6412 | 0xa413 | 0xa414 => {}
            _ => unsupported(
                reasons,
                format!("active paragraph modifier 0x{op:04X} is outside audited typography"),
            ),
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
    let mut generated = 1 + candidate.content.len();
    for (block, node) in blocks.into_iter().zip(&mut candidate.content) {
        match block {
            Block::PageBreak => ensure!(
                node.kind == "pageBreak",
                "DOC typography manual-break identity mismatch"
            ),
            Block::Paragraph(runs) => {
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
                    risks.insert("Legacy DOC inferred heading type and weight may differ from the source Normal paragraph".into());
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
    crate::document::validate(&candidate)?;
    *document = candidate;
    risks.insert(PRESERVED.into());
    risks.insert("Font names are preserved; unavailable fonts may be substituted locally".into());
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
            hps,
            direct_hps: None,
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
                Audit::Proven(vec![Block::Paragraph(vec![run("source", 20)])]),
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
    fn exact_run_split_preserves_existing_marks() {
        let mut n = document("a’bc");
        n.content[0].content[0].marks.push(Mark {
            kind: "bold".into(),
            attrs: Default::default(),
        });
        attach(
            Audit::Proven(vec![Block::Paragraph(vec![run("a’", 20), run("bc", 32)])]),
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
                Audit::Proven(vec![Block::Paragraph(vec![run("x", 20)])]),
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
