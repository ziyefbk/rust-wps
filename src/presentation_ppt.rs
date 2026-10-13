//! Preflight for legacy PowerPoint binary records, before semantic parsing.
use anyhow::{Result, ensure};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;

pub fn inspect_ppt(path: &std::path::Path) -> Result<crate::presentation::PresentationReport> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    inspect_ppt_bytes(&bytes)
}

pub fn inspect_ppt_bytes(bytes: &[u8]) -> Result<crate::presentation::PresentationReport> {
    use crate::presentation::{PresentationReport, SlideReport};
    use office_oxide::ppt::{PptDocument, TextType};
    preflight_ole(bytes)?;
    let deck = PptDocument::from_reader(Cursor::new(bytes))?;
    ensure!(
        deck.text_complete(),
        "PPT backend reports incomplete text streams"
    );
    ensure!(deck.slides.len() <= 1000, "PPT exceeds 1000 logical slides");
    let mut slides = Vec::new();
    let mut total = 0usize;
    let mut runs = 0usize;
    for (index, slide) in deck.slides.iter().enumerate() {
        let mut text = String::new();
        let mut notes = String::new();
        for run in slide.text_runs.iter().chain(
            slide
                .tables
                .iter()
                .flat_map(|table| table.rows.iter().flatten().flatten()),
        ) {
            runs += 1;
            total += run.text.len() + 1;
            ensure!(
                runs <= 100000 && total <= 4 * 1024 * 1024,
                "PPT extracted text budget exceeded"
            );
            let destination = if run.text_type == TextType::Notes {
                &mut notes
            } else {
                &mut text
            };
            destination.push_str(&run.text.replace(['\r', '\u{b}'], "\n"));
            destination.push('\n');
        }
        slides.push(SlideReport {
            id: index as u32 + 1,
            name: format!("Slide {}", index + 1),
            hidden: slide.hidden,
            text,
            notes,
            parse_error: None,
            shapes: Vec::new(),
        });
    }
    let mut warnings = deck.warnings().to_vec();
    warnings.push("Legacy PPT text inventory only: layout, images, animations and embedded objects are not rendered; notes listed separately; table text appended in row order.".into());
    let mut master_text = String::new();
    for run in &deck.master_text {
        runs += 1;
        total += run.text.len() + 1;
        ensure!(
            runs <= 100000 && total <= 4 * 1024 * 1024,
            "PPT extracted text budget exceeded"
        );
        master_text.push_str(&run.text.replace(['\r', '\u{b}'], "\n"));
        master_text.push('\n');
    }
    if !master_text.is_empty() {
        warnings.push("Static master text is listed separately once per deck; per-slide applicability and layout are not reconstructed.".into());
    }
    Ok(PresentationReport {
        format: "ppt".into(),
        size_emu: None,
        slides,
        unreadable_parts: Vec::new(),
        has_macros: deck.has_macros(),
        warnings,
        master_text,
    })
}

/// OLE envelope and selected PowerPoint streams, checked before deck parsing.
pub fn preflight_ole(bytes: &[u8]) -> Result<PersistInventory> {
    ensure!(bytes.len() <= 64 * 1024 * 1024, "PPT input exceeds 64 MiB");
    ensure!(
        bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]),
        "File signature does not match OLE PPT"
    );
    let mut cfb = office_oxide::cfb::CfbReader::new(Cursor::new(bytes))?;
    ensure!(
        cfb.entries().len() <= 4096 && cfb.warnings().is_empty(),
        "Invalid or excessive PPT OLE directory"
    );
    let mut total = 0u64;
    for entry in cfb.entries() {
        ensure!(
            entry.stream_size <= 64 * 1024 * 1024,
            "PPT OLE stream exceeds limit"
        );
        total = total
            .checked_add(entry.stream_size)
            .ok_or_else(|| anyhow::anyhow!("PPT OLE stream size overflow"))?;
        ensure!(
            total <= 128 * 1024 * 1024,
            "PPT OLE declared stream budget exceeded"
        );
    }
    let dual = cfb.find_entry_by_path("PP97_DUALSTORAGE/PowerPoint Document");
    let (document_path, user_path) = if dual.is_some() {
        (
            "PP97_DUALSTORAGE/PowerPoint Document",
            "PP97_DUALSTORAGE/Current User",
        )
    } else {
        ("PowerPoint Document", "Current User")
    };
    let document_id = cfb
        .find_entry_by_path(document_path)
        .ok_or_else(|| anyhow::anyhow!("Missing PowerPoint Document stream"))?;
    let user_id = cfb
        .find_entry_by_path(user_path)
        .ok_or_else(|| anyhow::anyhow!("Missing PPT Current User stream"))?;
    ensure!(
        cfb.entries()[user_id].stream_size <= 64 * 1024,
        "PPT Current User stream exceeds limit"
    );
    let document = cfb.read_stream_by_index(document_id)?;
    let current_user = cfb.read_stream_by_index(user_id)?;
    ensure!(
        cfb.truncated_streams().is_empty() && cfb.warnings().is_empty(),
        "Truncated or invalid PPT OLE stream"
    );
    inspect_persist(&document, &current_user)
}

#[derive(Debug, PartialEq, Eq)]
pub struct PersistInventory {
    pub document_id: u32,
    pub offsets: BTreeMap<u32, usize>,
    pub edits: usize,
}

fn word(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        data.get(offset..offset + 4)
            .ok_or_else(|| anyhow::anyhow!("Short PPT index field"))?
            .try_into()?,
    ))
}

/// Strict index validation; unlike the backend, never silently falls back to
/// stale sequential records when Current User or an edit link is malformed.
pub fn inspect_persist(stream: &[u8], current_user: &[u8]) -> Result<PersistInventory> {
    ensure!(
        current_user.len() >= 28 && current_user.len() <= 64 * 1024,
        "Missing or excessive PPT Current User stream"
    );
    ensure!(
        u16::from_le_bytes(current_user[2..4].try_into()?) == 4086 && current_user[0] & 15 != 15,
        "Invalid CurrentUserAtom"
    );
    let cu_length = word(current_user, 4)? as usize;
    ensure!(
        cu_length >= 20
            && cu_length
                .checked_add(8)
                .is_some_and(|end| end <= current_user.len()),
        "Truncated CurrentUserAtom"
    );
    ensure!(
        word(current_user, 12)? == 0xe391_c05f,
        "Encrypted or invalid PPT header token"
    );
    inspect_records(stream)?;
    let mut top = BTreeMap::new();
    let mut position = 0;
    while position < stream.len() {
        let kind = u16::from_le_bytes(stream[position + 2..position + 4].try_into()?);
        let length = word(stream, position + 4)? as usize;
        top.insert(
            position,
            (
                kind,
                stream[position] & 15 == 15,
                position + 8..position + 8 + length,
            ),
        );
        position += 8 + length;
    }
    let mut offset = word(current_user, 16)? as usize;
    let mut seen = BTreeSet::new();
    let mut result = PersistInventory {
        document_id: 0,
        offsets: BTreeMap::new(),
        edits: 0,
    };
    let mut entries = 0usize;
    loop {
        ensure!(seen.insert(offset), "Cyclic PPT edit chain");
        result.edits += 1;
        ensure!(result.edits <= 4096, "PPT edit chain exceeds limit");
        let (kind, container, range) = top
            .get(&offset)
            .ok_or_else(|| anyhow::anyhow!("PPT edit offset is not a top-level record"))?;
        ensure!(
            *kind == 4085 && !container,
            "PPT edit offset targets wrong record"
        );
        let edit = &stream[range.clone()];
        ensure!(matches!(edit.len(), 28 | 32), "Invalid UserEditAtom length");
        if edit.len() == 32 {
            ensure!(
                matches!(word(edit, 28)?, 0 | u32::MAX),
                "Encrypted PPT unsupported"
            );
        }
        if result.edits == 1 {
            result.document_id = word(edit, 16)?;
        }
        let directory = word(edit, 12)? as usize;
        let (kind, container, range) = top
            .get(&directory)
            .ok_or_else(|| anyhow::anyhow!("PPT persist directory offset is invalid"))?;
        ensure!(
            *kind == 6002 && !container,
            "Wrong PPT persist directory record"
        );
        let body = &stream[range.clone()];
        let mut cursor = 0;
        let mut local = BTreeSet::new();
        while cursor < body.len() {
            let packed = word(body, cursor)?;
            cursor += 4;
            let id = packed & 0xfffff;
            let count = packed >> 20;
            ensure!(
                id > 0 && count > 0 && id + count <= 0x100000,
                "Invalid PPT persist ID range"
            );
            entries += count as usize;
            ensure!(entries <= 100000, "PPT persist entries exceed limit");
            for key in id..id + count {
                let target = word(body, cursor)? as usize;
                cursor += 4;
                ensure!(local.insert(key), "Duplicate ID in PPT persist directory");
                ensure!(
                    top.contains_key(&target),
                    "PPT persist target is not a top-level record"
                );
                result.offsets.entry(key).or_insert(target);
            }
        }
        let previous = word(edit, 8)? as usize;
        if previous == 0 {
            break;
        }
        ensure!(previous < offset, "PPT edit chain does not move backwards");
        offset = previous;
    }
    let document = result
        .offsets
        .get(&result.document_id)
        .and_then(|offset| top.get(offset));
    ensure!(
        document.is_some_and(|(kind, container, _)| *kind == 1000 && *container),
        "PPT current DocumentContainer is missing"
    );
    Ok(result)
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct RecordInventory {
    pub records: usize,
    pub text_bytes: usize,
    pub slide_containers: usize,
}

/// Validate the physical stream, including obsolete incremental-save records.
/// This does not establish logical slide order or validate persist references.
pub fn inspect_records(stream: &[u8]) -> Result<RecordInventory> {
    ensure!(
        stream.len() <= 64 * 1024 * 1024,
        "PPT stream exceeds 64 MiB"
    );
    ensure!(!stream.is_empty(), "Empty PowerPoint Document stream");
    let mut inventory = RecordInventory::default();
    let mut ends = vec![stream.len()];
    let mut position = 0usize;
    while let Some(&end) = ends.last() {
        if position == end {
            ends.pop();
            continue;
        }
        let header = stream
            .get(position..position + 8)
            .filter(|_| end - position >= 8)
            .ok_or_else(|| anyhow::anyhow!("Truncated PPT record header"))?;
        let flags = u16::from_le_bytes(header[..2].try_into()?);
        let kind = u16::from_le_bytes(header[2..4].try_into()?);
        let length = u32::from_le_bytes(header[4..8].try_into()?) as usize;
        let body = position + 8;
        let next = body
            .checked_add(length)
            .filter(|next| *next <= end)
            .ok_or_else(|| anyhow::anyhow!("PPT record crosses container boundary"))?;
        inventory.records += 1;
        ensure!(
            inventory.records <= 1_000_000,
            "PPT record count exceeds limit"
        );
        if kind == 1006 && flags & 15 == 15 {
            inventory.slide_containers += 1;
            ensure!(
                inventory.slide_containers <= 10000,
                "PPT physical slide count exceeds limit"
            );
        }
        if matches!(kind, 4000 | 4008 | 4026) {
            if kind != 4008 {
                ensure!(length.is_multiple_of(2), "Odd UTF-16 PPT text length");
            }
            inventory.text_bytes += length;
            ensure!(
                inventory.text_bytes <= 4 * 1024 * 1024,
                "PPT physical text exceeds 4 MiB"
            );
        }
        if flags & 15 == 15 {
            ensure!(ends.len() < 64, "PPT container nesting exceeds limit");
            ends.push(next);
            position = body;
        } else {
            position = next;
        }
    }
    Ok(inventory)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_indexed_ole_fixture_and_refuses_truncated_or_excessive_streams() {
        let bytes = include_bytes!("../tests/fixtures/ppt-indexed-structure.ppt");
        let index = preflight_ole(bytes).unwrap();
        assert_eq!(index.edits, 2);
        assert_eq!(index.document_id, 1);
        assert_eq!(index.offsets.get(&1), Some(&0));
        assert_eq!(index.offsets.get(&2), Some(&72));
        assert!(preflight_ole(&bytes[..2048]).is_err());
        let mut excessive = bytes.to_vec();
        excessive[512 + 2 * 128 + 120..512 + 2 * 128 + 128]
            .copy_from_slice(&(64u64 * 1024 * 1024 + 1).to_le_bytes());
        assert!(preflight_ole(&excessive).is_err());
    }

    #[test]
    fn ole_preflight_rejects_disguised_corrupt_and_unrelated_office_files() {
        assert!(preflight_ole(b"PK\x03\x04").is_err());
        assert!(preflight_ole(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]).is_err());
        assert!(preflight_ole(include_bytes!("../tests/fixtures/basic-biff8.xls")).is_err());
        assert!(preflight_ole(&vec![0; 64 * 1024 * 1024 + 1]).is_err());
    }

    fn record(kind: u16, container: bool, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(if container { 15u16 } else { 0u16 }).to_le_bytes());
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(body);
        out
    }

    fn indexed_deck() -> (Vec<u8>, Vec<u8>, usize) {
        let mut stream = record(1000, true, &[]);
        stream.extend(record(1006, true, &[])); // old slide at 8
        let mut directory = Vec::new();
        directory.extend_from_slice(&(1u32 | (2 << 20)).to_le_bytes());
        directory.extend_from_slice(&0u32.to_le_bytes());
        directory.extend_from_slice(&8u32.to_le_bytes());
        let first_directory = stream.len();
        stream.extend(record(6002, false, &directory));
        let mut edit = [0u8; 28];
        edit[12..16].copy_from_slice(&(first_directory as u32).to_le_bytes());
        edit[16..20].copy_from_slice(&1u32.to_le_bytes());
        let first_edit = stream.len();
        stream.extend(record(4085, false, &edit));
        let new_slide = stream.len();
        stream.extend(record(1006, true, &[]));
        directory.clear();
        directory.extend_from_slice(&(2u32 | (1 << 20)).to_le_bytes());
        directory.extend_from_slice(&(new_slide as u32).to_le_bytes());
        let new_directory = stream.len();
        stream.extend(record(6002, false, &directory));
        edit[8..12].copy_from_slice(&(first_edit as u32).to_le_bytes());
        edit[12..16].copy_from_slice(&(new_directory as u32).to_le_bytes());
        let latest = stream.len();
        stream.extend(record(4085, false, &edit));
        let mut current = [0u8; 20];
        current[4..8].copy_from_slice(&0xe391_c05fu32.to_le_bytes());
        current[8..12].copy_from_slice(&(latest as u32).to_le_bytes());
        (stream, record(4086, false, &current), new_slide)
    }

    #[test]
    fn newest_persist_entry_wins_and_older_document_reference_is_retained() {
        let (stream, current, new_slide) = indexed_deck();
        let index = inspect_persist(&stream, &current).unwrap();
        assert_eq!(index.edits, 2);
        assert_eq!(index.document_id, 1);
        assert_eq!(index.offsets.get(&1), Some(&0));
        assert_eq!(index.offsets.get(&2), Some(&new_slide));
    }

    #[test]
    fn invalid_current_user_edit_links_and_persist_offsets_never_fall_back() {
        let (stream, current, _) = indexed_deck();
        let latest = word(&current, 16).unwrap() as usize;
        let directory = word(&stream, latest + 20).unwrap() as usize;
        for (offset, value) in [
            (latest + 16, latest as u32),
            (latest + 20, 1),
            (directory + 12, 1),
            (latest + 24, 99),
        ] {
            let mut bad = stream.clone();
            bad[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(inspect_persist(&bad, &current).is_err(), "offset {offset}");
        }
        for (offset, value) in [(12, 0xf3d1_c4dfu32), (16, 1), (4, 999)] {
            let mut bad = current.clone();
            bad[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(inspect_persist(&stream, &bad).is_err());
        }
        assert!(inspect_persist(&stream, &[]).is_err());
    }

    #[test]
    fn inventories_nested_and_incremental_records_without_decoding_atom_payloads() {
        let text = record(4000, false, &[0x2d, 0x4e]);
        let mut stream = record(1006, true, &text);
        stream.extend(record(1006, true, &record(4008, false, b"latest")));
        stream.extend(record(999, false, &[255; 8]));
        assert_eq!(
            inspect_records(&stream).unwrap(),
            RecordInventory {
                records: 5,
                text_bytes: 8,
                slide_containers: 2,
            }
        );
    }

    #[test]
    fn refuses_truncation_boundary_crossing_and_odd_unicode() {
        assert!(inspect_records(&[]).is_err());
        assert!(inspect_records(&[0; 7]).is_err());
        assert!(inspect_records(&record(4000, false, &[1])).is_err());
        let child = record(4008, false, b"ab");
        let mut stream = record(1006, true, &child[..9]);
        stream.push(0);
        assert!(inspect_records(&stream).is_err());
        let valid = record(4008, false, b"abc");
        for end in 0..valid.len() {
            assert!(inspect_records(&valid[..end]).is_err());
        }
    }

    #[test]
    fn refuses_deep_containers_and_text_budget_before_semantic_parser() {
        let mut stream = record(1, false, &[]);
        for _ in 0..64 {
            stream = record(2, true, &stream);
        }
        assert!(inspect_records(&stream).is_err());
        assert!(inspect_records(&record(4008, false, &vec![0; 4 * 1024 * 1024 + 1])).is_err());
    }
}
