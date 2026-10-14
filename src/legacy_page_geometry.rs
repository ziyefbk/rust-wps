//! Bounded binary-DOC single-section page proof, after complete body typography
//! proof. No inferred page defaults, general section conversion or layout engine.
use crate::document::{Node, PageGeometry};
use anyhow::{Context, Result, ensure};
use std::collections::{BTreeMap, BTreeSet};

const MAX_PROPERTIES: usize = 1024 * 1024;
const MAX_SECTIONS: usize = 10_000;
const MAX_OPERATIONS: usize = 10_000;
const PRESERVED: &str = "Legacy DOC single-section page size and four body margins are preserved as exact encoded twips; source layout and pagination require visual verification";
const BREAK_RISK: &str =
    "Legacy DOC body manual page breaks are preserved; section layout is not preserved";

#[derive(Debug)]
pub(crate) struct Audit {
    geometry: Option<PageGeometry>,
    reasons: BTreeSet<String>,
    split_breaks: Option<bool>,
    section_ends: Option<BTreeSet<u32>>,
}

impl Audit {
    pub(crate) fn requests_split_break_semantics(&self) -> bool {
        self.split_breaks == Some(true)
    }
    /// Compatibility and section identity are independent of supported page
    /// dimensions, grids, margins or printer settings. Only exact source CPs
    /// and an explicitly encoded Word2007+ true value authorize promotion.
    pub(crate) fn split_break_semantics(
        &self,
        break_ends: &BTreeSet<u32>,
    ) -> std::result::Result<(), String> {
        match self.split_breaks {
            Some(true) => {}
            Some(false) => {
                return Err("source split-page-break compatibility flag is explicitly off".into());
            }
            None => {
                return Err(
                    "source Word2007+ split-page-break compatibility flag is absent".into(),
                );
            }
        }
        let section_ends = self
            .section_ends
            .as_ref()
            .ok_or_else(|| "exact source section boundaries are not proven".to_string())?;
        if !section_ends.is_disjoint(break_ends) {
            return Err(
                "a source break is a section-ending character, not a manual page break".into(),
            );
        }
        Ok(())
    }
}

fn bytes(data: &[u8], offset: usize, length: usize) -> Result<&[u8]> {
    let end = offset
        .checked_add(length)
        .context("DOC page offset overflow")?;
    data.get(offset..end)
        .context("DOC page structure is truncated")
}
fn u16_at(data: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(bytes(data, offset, 2)?.try_into()?))
}
fn u32_at(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(bytes(data, offset, 4)?.try_into()?))
}
fn charge(aggregate: &mut usize, count: usize) -> Result<()> {
    *aggregate = aggregate
        .checked_add(count)
        .context("DOC page resource overflow")?;
    ensure!(
        *aggregate <= MAX_PROPERTIES,
        "DOC page property-byte limit exceeded"
    );
    Ok(())
}
fn part<'a>(word: &[u8], table: &'a [u8], pair: usize, aggregate: &mut usize) -> Result<&'a [u8]> {
    let offset = u32_at(word, 0x9a + pair * 8)? as usize;
    let length = u32_at(word, 0x9e + pair * 8)? as usize;
    charge(aggregate, length)?;
    if length == 0 {
        Ok(&table[..0])
    } else {
        bytes(table, offset, length)
    }
}
fn reason(reasons: &mut BTreeSet<String>, text: &str) {
    reasons.insert(text.into());
}

/// Called only for the source streams whose Word97/Word2007 FIB, body character
/// count, text, fonts and paragraph identity were proved by legacy_typography.
pub(crate) fn audit_streams(
    word: &[u8],
    table: &[u8],
    break_ends: &BTreeSet<u32>,
) -> Result<Audit> {
    audit_inner(word, table, break_ends)
        .context("Cannot prove legacy DOC page geometry; source remains read-only")
}
fn audit_inner(word: &[u8], table: &[u8], break_ends: &BTreeSet<u32>) -> Result<Audit> {
    let mut reasons = BTreeSet::new();
    let mut aggregate = 0;
    let count = u16_at(word, 0x98)? as usize;
    let new_at = 0x9a + count * 8;
    let new_count = u16_at(word, new_at)?;
    let version = if new_count == 0 {
        u16_at(word, 2)?
    } else {
        u16_at(word, new_at + 2)?
    };
    // Defense in depth: the caller must not expand this proof via a new FIB.
    ensure!(
        matches!((version, count, new_count), (0xc1, 93, 0) | (0x112, 183, 5)),
        "DOC page proof requires the audited FIB layout"
    );
    if bytes(word, 19, 1)?[0] & 4 != 0 {
        reason(
            &mut reasons,
            "FIB installation-language page override is active",
        );
    }
    for offset in [0x50, 0x54, 0x5c, 0x60, 0x64, 0x68] {
        if u32_at(word, offset)? != 0 {
            reason(
                &mut reasons,
                "side-story text prevents proof of body margins",
            );
        }
    }
    let sed = part(word, table, 6, &mut aggregate)?;
    let dop = part(word, table, 31, &mut aggregate)?;
    // Zero-length records have no referenced bytes; their offsets are ignored.
    for (pair, why) in [
        (11, "header/footer records prevent proof of body margins"),
        (
            27,
            "printer driver settings are outside the audited page subset",
        ),
        (
            28,
            "portrait printer settings are outside the audited page subset",
        ),
        (
            29,
            "landscape printer settings are outside the audited page subset",
        ),
    ] {
        if !part(word, table, pair, &mut aggregate)?.is_empty() {
            ensure!(
                pair != 11 || u32_at(word, 0x54)? != 0,
                "DOC header/footer records exist without header text"
            );
            reason(&mut reasons, why);
        }
    }
    let split_breaks = audit_dop(dop, version, &mut reasons)?;
    if sed.is_empty() {
        reason(&mut reasons, "section table is absent");
        return Ok(Audit {
            geometry: None,
            reasons,
            split_breaks,
            section_ends: None,
        });
    }
    ensure!(
        sed.len() >= 20 && (sed.len() - 4).is_multiple_of(16),
        "DOC PlcfSed length is invalid"
    );
    let sections = (sed.len() - 4) / 16;
    ensure!(sections <= MAX_SECTIONS, "DOC page section limit exceeded");
    let cp = u32_at(word, 0x4c)?;
    let mut previous = u32_at(sed, 0)?;
    let mut section_ends = BTreeSet::new();
    ensure!(previous == 0, "DOC PlcfSed does not start at body CP zero");
    for i in 1..=sections {
        let next = u32_at(sed, i * 4)?;
        ensure!(
            next > previous && next <= i32::MAX as u32,
            "DOC PlcfSed CPs are invalid or nonmonotonic"
        );
        if i < sections {
            ensure!(next < cp, "DOC section starts outside the main body");
            ensure!(
                break_ends.contains(&next),
                "DOC section boundary has no source end-of-section marker"
            );
            section_ends.insert(next);
        }
        previous = next;
    }
    ensure!(previous >= cp, "DOC PlcfSed does not cover the main body");
    if sections != 1 {
        reason(
            &mut reasons,
            "multiple sections are outside the audited page subset",
        );
    }
    if previous != cp {
        reason(
            &mut reasons,
            "section end is not exactly the proven body end",
        );
    }
    let mut geometry = None;
    let mut operations = 0;
    for i in 0..sections {
        let fc = u32_at(sed, 4 * (sections + 1) + 12 * i + 2)?;
        // The old author controls use the historical no-SEPX sentinel. This is
        // an explicit compatibility fallback, never an inferred default page.
        if fc == u32::MAX {
            reason(&mut reasons, "section properties are absent");
            continue;
        }
        ensure!(fc <= i32::MAX as u32, "DOC SEPX offset is negative");
        let fc = fc as usize;
        let cb = u16_at(word, fc)?;
        ensure!(cb <= i16::MAX as u16, "DOC SEPX byte length is negative");
        charge(&mut aggregate, cb as usize + 2)?;
        let properties = bytes(word, fc + 2, cb as usize)?;
        let section = parse_section(properties, &mut reasons, &mut operations)?;
        if sections == 1 {
            geometry = section;
        }
        // fn, fnMpr and fcMpr are undefined and MUST be ignored, including the
        // real samples' nonzero fn/fnMpr and FFFFFFFF fcMpr values.
    }
    if !reasons.is_empty() {
        geometry = None;
    }
    Ok(Audit {
        geometry,
        reasons,
        split_breaks,
        section_ends: (previous == cp).then_some(section_ends),
    })
}

fn audit_dop(dop: &[u8], version: u16, reasons: &mut BTreeSet<String>) -> Result<Option<bool>> {
    if dop.is_empty() {
        reason(reasons, "document page settings are absent");
        return Ok(None);
    }
    ensure!(
        (version == 0xc1 && dop.len() == 500)
            || (version == 0x112 && matches!(dop.len(), 674 | 690 | 694)),
        "DOC DOP length disagrees with the effective FIB version"
    );
    ensure!(
        u16_at(dop, 8)? & 0xfdff == u16_at(dop, 84)? & 0xfdff,
        "DOC DOP compatibility flag copies disagree"
    );
    if u32_at(dop, 84)? & 0x8000_0000 != 0 {
        reason(
            reasons,
            "printer metrics are outside the audited page subset",
        );
    }
    for (active, why) in [
        (
            dop[0] & 1 != 0,
            "facing-page headers are outside the audited page subset",
        ),
        (
            u32_at(dop, 4)? & 0x0020_0000 != 0,
            "mirrored margins are outside the audited page subset",
        ),
        (
            u16_at(dop, 82)? & 0x8000 != 0,
            "top gutter mode is outside the audited page subset",
        ),
        (
            dop[90] & 0x20 != 0,
            "two-pages-per-sheet mode is outside the audited page subset",
        ),
    ] {
        if active {
            reason(reasons, why);
        }
    }
    if version == 0x112 {
        ensure!(
            u32_at(dop, 84)? & 0xffff_fdff == u32_at(dop, 508)? & 0xffff_fdff,
            "DOC DOP compatibility flag copies disagree"
        );
        ensure!(
            dop[548] & 0x80 == 0 || dop[548] & 0x40 != 0,
            "DOC reverse bookfold requires bookfold mode"
        );
        if dop[548] & 0xc0 != 0 {
            reason(reasons, "bookfold mode is outside the audited page subset");
        }
        if dop[598] & 1 != 0 {
            reason(
                reasons,
                "reading-layout page lock is outside the audited page subset",
            );
        }
    }
    // Dop97 does not contain this field. Its absence must not be interpreted
    // as either a source-encoded false value or a consumer default.
    Ok((version == 0x112).then(|| u32_at(dop, 512).expect("validated DOP") & 0x0800_0000 != 0))
}

fn parse_section(
    data: &[u8],
    reasons: &mut BTreeSet<String>,
    operations: &mut usize,
) -> Result<Option<PageGeometry>> {
    let mut values = BTreeMap::<u16, i64>::new();
    let mut offset = 0;
    while offset < data.len() {
        *operations += 1;
        ensure!(
            *operations <= MAX_OPERATIONS,
            "DOC page operation limit exceeded"
        );
        let op = u16_at(data, offset)?;
        offset += 2;
        let length = match op >> 13 {
            0 | 1 => 1,
            2 | 4 | 5 => 2,
            3 => 4,
            7 => 3,
            6 if op == 0xd608 => {
                let length = u16_at(data, offset)? as usize;
                ensure!(length >= 1, "DOC table SPRM length is invalid");
                offset += 2;
                length - 1
            }
            6 => {
                let length = bytes(data, offset, 1)?[0] as usize;
                offset += 1;
                if op == 0xc615 && length == 255 {
                    let removed = bytes(data, offset, 1)?[0] as usize;
                    let added = bytes(data, offset + 1 + 4 * removed, 1)?[0] as usize;
                    2 + 4 * removed + 3 * added
                } else {
                    length
                }
            }
            _ => unreachable!(),
        };
        let operand = bytes(data, offset, length)?;
        offset += length;
        let mut value = match length {
            1 => i64::from(operand[0]),
            2 => i64::from(u16_at(operand, 0)?),
            4 => i64::from(u32_at(operand, 0)?),
            _ => 0,
        };
        // Validate every known declaration, including overwritten declarations.
        // Unknown operations receive framing only and block the entire proof.
        match op {
            0xb01f | 0xb020 => ensure!(
                (144..=31680).contains(&value),
                "DOC page dimension 0x{op:04X} is invalid"
            ),
            0xb021 | 0xb022 | 0xb017 | 0xb018 | 0x900c | 0x9016 => {
                ensure!(value <= 31680, "DOC page distance 0x{op:04X} is invalid")
            }
            0x9023 | 0x9024 => {
                value = i64::from(u16_at(operand, 0)? as i16);
                ensure!(
                    (-31665..=31665).contains(&value),
                    "DOC page margin 0x{op:04X} is invalid"
                );
            }
            0x301d => ensure!(matches!(value, 1 | 2), "DOC page orientation is invalid"),
            0x3005 | 0x3006 | 0x300a | 0x3011 | 0x3012 | 0x3019 | 0x3228 | 0x322a | 0x3239 => {
                ensure!(value <= 1, "DOC section Bool8 0x{op:04X} is invalid");
            }
            0x3009 => ensure!(value <= 4, "DOC section break kind is invalid"),
            0x500b => ensure!(value <= 43, "DOC section column count is invalid"),
            0x5015 => ensure!(
                value <= 100,
                "DOC section line numbering interval is invalid"
            ),
            0x301a | 0x5032 => ensure!(
                value <= 3,
                "DOC section layout enumeration 0x{op:04X} is invalid"
            ),
            0x5033 => ensure!(value <= 5, "DOC section text flow is invalid"),
            0x9031 => ensure!(
                (1..=31680).contains(&value),
                "DOC section grid line pitch is invalid"
            ),
            0x7030 => {
                value = i64::from(u32_at(operand, 0)? as i32);
                ensure!(
                    (-670925..=6488064).contains(&value),
                    "DOC section grid character pitch is invalid"
                );
            }
            0xf203 | 0xf204 => {
                let distance = u16_at(operand, 1)?;
                ensure!(
                    operand[0] <= 43 && distance <= 31680 && (op != 0xf203 || distance >= 718),
                    "DOC section column operand is invalid"
                );
                reason(
                    reasons,
                    "explicit column geometry is outside the audited page subset",
                );
            }
            0xb025 | 0x5026 | 0x703a => {} // Gutter, paper tie-breaker, revision save ID.
            // Paper bin interpretation is implementation-specific, not page geometry.
            0x5007 | 0x5008 => reason(
                reasons,
                "printer paper source is outside the audited page subset",
            ),
            _ => {
                reasons.insert(format!("unproven section property 0x{op:04X}"));
            }
        }
        values.insert(op, value);
    }
    for (op, neutral, why) in [
        (
            0x3005,
            1,
            "uneven columns are outside the audited page subset",
        ),
        (
            0x3009,
            2,
            "non-default section break mode is outside the audited page subset",
        ),
        (
            0x300a,
            0,
            "first-page special layout is outside the audited page subset",
        ),
        (
            0x500b,
            0,
            "multiple columns are outside the audited page subset",
        ),
        (
            0x5015,
            0,
            "line numbering is outside the audited page subset",
        ),
        (
            0x3019,
            0,
            "column separator lines are outside the audited page subset",
        ),
        (
            0x301a,
            0,
            "non-top vertical alignment is outside the audited page subset",
        ),
        (
            0xb025,
            0,
            "nonzero gutter is outside the audited page subset",
        ),
        (
            0x3228,
            0,
            "right-to-left section layout is outside the audited page subset",
        ),
        (
            0x322a,
            0,
            "right-to-left gutter is outside the audited page subset",
        ),
        (
            0x5032,
            0,
            "active document grid is outside the audited page subset",
        ),
        (
            0x5033,
            0,
            "non-default text flow is outside the audited page subset",
        ),
        (
            0x3239,
            0,
            "section revision properties are outside the audited page subset",
        ),
    ] {
        if values.get(&op).is_some_and(|value| *value != neutral) {
            reason(reasons, why);
        }
    }
    // Grid pitch and column gap do not activate their respective layout modes.
    // Header/footer offsets are inert only because absent side stories/records
    // are separately required. Paper preference and rsid do not alter the six values.
    let required = [0xb01f, 0xb020, 0x9023, 0xb022, 0x9024, 0xb021];
    if required.iter().any(|op| !values.contains_key(op)) {
        reason(
            reasons,
            "explicit page dimensions and all four margins are required",
        );
        return Ok(None);
    }
    if [0x9023, 0x9024].iter().any(|op| values[op] < 0) {
        reason(
            reasons,
            "negative fixed margins are outside the audited page subset",
        );
        return Ok(None);
    }
    let page = PageGeometry {
        width_twips: values[&0xb01f] as u64,
        height_twips: values[&0xb020] as u64,
        margin_top_twips: values[&0x9023] as u64,
        margin_right_twips: values[&0xb022] as u64,
        margin_bottom_twips: values[&0x9024] as u64,
        margin_left_twips: values[&0xb021] as u64,
    };
    let orientation = values.get(&0x301d).copied().unwrap_or(1);
    if (orientation == 1) != (page.width_twips < page.height_twips) {
        reason(reasons, "page dimensions disagree with section orientation");
    }
    if let Err(error) = crate::document::validate_page_geometry(&page) {
        reasons.insert(format!(
            "encoded page is outside the editor subset: {error}"
        ));
    }
    Ok(Some(page))
}

pub(crate) fn attach(
    audit: Audit,
    document: &mut Node,
    risks: &mut BTreeSet<String>,
) -> Result<()> {
    let Some(geometry) = audit.geometry else {
        for why in audit.reasons {
            risks.insert(format!("Legacy DOC page geometry is not preserved: {why}"));
        }
        return Ok(());
    };
    ensure!(
        audit.reasons.is_empty(),
        "DOC page proof contains unresolved reasons"
    );
    ensure!(
        !document.attrs.contains_key("pageGeometry"),
        "DOC parser introduced unproved page geometry"
    );
    let mut candidate = document.clone();
    candidate
        .attrs
        .insert("pageGeometry".into(), serde_json::to_value(geometry)?);
    crate::document::validate(&candidate)?;
    *document = candidate;
    risks.remove(BREAK_RISK);
    risks.insert("Legacy DOC body manual page breaks are preserved; only audited single-section page geometry is preserved".into());
    risks.insert(PRESERVED.into());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn audit_streams(word: &[u8], table: &[u8]) -> Result<Audit> {
        super::audit_streams(word, table, &BTreeSet::from([20]))
    }
    fn set16(data: &mut [u8], offset: usize, value: u16) {
        data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    fn set32(data: &mut [u8], offset: usize, value: u32) {
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn op(code: u16, value: i64) -> Vec<u8> {
        let mut bytes = code.to_le_bytes().to_vec();
        let length = match code >> 13 {
            0 | 1 => 1,
            3 => 4,
            _ => 2,
        };
        bytes.extend_from_slice(&value.to_le_bytes()[..length]);
        bytes
    }
    fn page(width: u16, height: u16) -> Vec<u8> {
        [
            (0xb01f, width),
            (0xb020, height),
            (0xb021, 1800),
            (0xb022, 1800),
            (0x9023, 1440),
            (0x9024, 1440),
        ]
        .into_iter()
        .flat_map(|(code, value)| op(code, i64::from(value)))
        .collect()
    }
    fn fixture(properties: &[u8], modern: bool) -> (Vec<u8>, Vec<u8>) {
        let mut word = vec![0; 65536];
        let mut table = vec![0; 2048];
        set16(&mut word, 2, 0xc1);
        set32(&mut word, 0x4c, 48);
        let count = if modern { 183 } else { 93 };
        set16(&mut word, 0x98, count);
        if modern {
            set16(&mut word, 0x9a + usize::from(count) * 8, 5);
            set16(&mut word, 0x9c + usize::from(count) * 8, 0x112);
        }
        set32(&mut word, 0x9a + 6 * 8, 100);
        set32(&mut word, 0x9e + 6 * 8, 20);
        set32(&mut table, 104, 48);
        set32(&mut table, 110, 4096);
        set16(&mut word, 4096, properties.len() as u16);
        word[4098..4098 + properties.len()].copy_from_slice(properties);
        set32(&mut word, 0x9a + 31 * 8, 200);
        set32(&mut word, 0x9e + 31 * 8, if modern { 674 } else { 500 });
        (word, table)
    }
    fn proved(properties: &[u8]) -> PageGeometry {
        let (word, table) = fixture(properties, false);
        let a = audit_streams(&word, &table).unwrap();
        assert!(a.reasons.is_empty(), "{:?}", a.reasons);
        a.geometry.unwrap()
    }
    fn fallback(word: &[u8], table: &[u8], why: &str) {
        let a = audit_streams(word, table).unwrap();
        assert!(a.geometry.is_none());
        assert!(a.reasons.iter().any(|s| s.contains(why)), "{:?}", a.reasons);
    }
    #[test]
    fn split_flag_is_explicit_only_in_the_three_audited_word2007_dop_layouts() {
        for length in [674, 690, 694] {
            for active in [false, true] {
                let (mut w, mut t) = fixture(&page(12240, 15840), true);
                set32(&mut w, 0x9e + 31 * 8, length);
                set32(&mut t, 200 + 512, if active { 0x0800_0000 } else { 0 });
                let a = audit_streams(&w, &t).unwrap();
                assert_eq!(a.requests_split_break_semantics(), active);
                assert_eq!(
                    a.split_break_semantics(&BTreeSet::from([20])).is_ok(),
                    active
                );
            }
        }
        let (w, mut t) = fixture(&page(12240, 15840), false);
        // Unreferenced bytes beyond a real Dop97 cannot turn an absent field on.
        set32(&mut t, 200 + 512, 0x0800_0000);
        let a = audit_streams(&w, &t).unwrap();
        assert!(!a.requests_split_break_semantics());
        assert!(
            a.split_break_semantics(&BTreeSet::from([20]))
                .unwrap_err()
                .contains("absent")
        );
    }
    #[test]
    fn split_proof_is_independent_of_page_geometry_but_requires_exact_sections() {
        let (mut w, mut t) = fixture(&[], true);
        set32(&mut t, 200 + 512, 0x0800_0000);
        let a = audit_streams(&w, &t).unwrap();
        assert!(a.geometry.is_none());
        assert!(a.split_break_semantics(&BTreeSet::from([20])).is_ok());
        set32(&mut t, 104, 49);
        let a = audit_streams(&w, &t).unwrap();
        assert!(
            a.split_break_semantics(&BTreeSet::from([20]))
                .unwrap_err()
                .contains("exact source section")
        );
        set32(&mut w, 0x9e + 6 * 8, 0);
        let a = audit_streams(&w, &t).unwrap();
        assert!(
            a.split_break_semantics(&BTreeSet::from([20]))
                .unwrap_err()
                .contains("exact source section")
        );
    }
    #[test]
    fn section_ending_cp_never_qualifies_as_a_split_manual_break() {
        let (mut w, mut t) = fixture(&[], true);
        set32(&mut t, 200 + 512, 0x0800_0000);
        set32(&mut w, 0x9e + 6 * 8, 36);
        t[100..136].fill(0);
        set32(&mut t, 104, 20);
        set32(&mut t, 108, 48);
        set32(&mut t, 114, u32::MAX);
        set32(&mut t, 126, u32::MAX);
        let a = audit_streams(&w, &t).unwrap();
        assert!(a.geometry.is_none());
        assert!(
            a.split_break_semantics(&BTreeSet::from([20]))
                .unwrap_err()
                .contains("section-ending")
        );
    }
    #[test]
    fn exact_original_sizes_and_preset_tolerance_never_round() {
        for (width, height) in [
            (12240, 15840),
            (11907, 16840),
            (8391, 11906),
            (11904, 16836),
            (11908, 16840),
        ] {
            let result = proved(&page(width, height));
            assert_eq!(result.width_twips, u64::from(width));
            assert_eq!(result.height_twips, u64::from(height));
            assert_eq!(result.margin_left_twips, 1800);
        }
    }
    #[test]
    fn landscape_requires_explicit_matching_orientation() {
        let mut p = page(15840, 12240);
        let (w, t) = fixture(&p, false);
        fallback(&w, &t, "orientation");
        p.extend(op(0x301d, 2));
        assert_eq!(proved(&p).width_twips, 15840);
        p.extend(op(0x301d, 1));
        let (w, t) = fixture(&p, false);
        fallback(&w, &t, "orientation");
    }
    #[test]
    fn scalar_overrides_are_last_wins_without_resetting_other_components() {
        let mut p = page(12240, 15840);
        p.extend(op(0xb021, 0));
        p.extend(op(0xb025, 100));
        p.extend(op(0xb025, 0));
        p.extend(op(0x5032, 1));
        p.extend(op(0x5032, 0));
        let result = proved(&p);
        assert_eq!(result.margin_left_twips, 0);
        assert_eq!(result.margin_right_twips, 1800);
    }
    #[test]
    fn valid_inactive_values_and_ignored_sed_fields_do_not_activate_features() {
        let mut p = page(11907, 16840);
        for (code, value) in [
            (0x5026, 9),
            (0x703a, 4594569),
            (0x9031, 360),
            (0x7030, 4096),
            (0xb017, 709),
            (0xb018, 709),
            (0x900c, 708),
            (0x3005, 1),
            (0x3009, 2),
            (0x300a, 0),
            (0x500b, 0),
            (0x301a, 0),
            (0x5032, 0),
            (0x5033, 0),
            (0x3228, 0),
            (0x322a, 0),
        ] {
            p.extend(op(code, value));
        }
        let (mut w, mut t) = fixture(&p, true);
        set16(&mut t, 108, 14);
        set16(&mut t, 114, 4);
        set32(&mut t, 116, u32::MAX);
        // An inactive actual-page preference is not a reading-layout lock.
        set32(&mut t, 200 + 620, 0x200);
        for pair in [11, 27, 28, 29] {
            set32(&mut w, 0x9a + pair * 8, u32::MAX);
        }
        assert_eq!(
            audit_streams(&w, &t).unwrap().geometry.unwrap().width_twips,
            11907
        );
    }
    #[test]
    fn every_required_component_must_be_explicit() {
        let p = page(12240, 15840);
        for i in 0..6 {
            let mut missing = p.clone();
            missing.drain(i * 4..i * 4 + 4);
            let (w, t) = fixture(&missing, false);
            fallback(&w, &t, "all four margins");
        }
    }
    #[test]
    fn missing_dop_sed_and_historical_sepx_sentinel_are_unproved_only() {
        let (word, table) = fixture(&page(12240, 15840), false);
        for pair in [6, 31] {
            let mut w = word.clone();
            set32(&mut w, 0x9e + pair * 8, 0);
            set32(&mut w, 0x9a + pair * 8, u32::MAX);
            fallback(&w, &table, "absent");
        }
        let mut t = table;
        set32(&mut t, 110, u32::MAX);
        fallback(&word, &t, "section properties are absent");
    }
    #[test]
    fn supported_dop_versions_have_exact_lengths() {
        for (modern, length) in [(false, 500), (true, 674), (true, 690), (true, 694)] {
            let (mut w, t) = fixture(&page(12240, 15840), modern);
            set32(&mut w, 0x9e + 31 * 8, length);
            assert!(audit_streams(&w, &t).unwrap().geometry.is_some());
        }
        for (modern, length) in [
            (false, 499),
            (false, 501),
            (true, 500),
            (true, 673),
            (true, 675),
            (true, 695),
        ] {
            let (mut w, t) = fixture(&page(12240, 15840), modern);
            set32(&mut w, 0x9e + 31 * 8, length);
            assert!(format!("{:#}", audit_streams(&w, &t).unwrap_err()).contains("DOP length"));
        }
    }
    #[test]
    fn all_document_page_override_flags_block_geometry() {
        for (offset, mask, why) in [
            (0, 1, "facing"),
            (6, 0x20, "mirrored"),
            (83, 0x80, "top gutter"),
            (90, 0x20, "two-pages"),
            (548, 0x40, "bookfold"),
            (548, 0xc0, "bookfold"),
            (598, 1, "reading-layout"),
        ] {
            let (w, mut t) = fixture(&page(12240, 15840), true);
            t[200 + offset] |= mask;
            fallback(&w, &t, why);
        }
        let (mut w, t) = fixture(&page(12240, 15840), false);
        w[19] |= 4;
        fallback(&w, &t, "installation-language page");
    }
    #[test]
    fn malformed_reverse_bookfold_is_not_an_unsupported_fallback() {
        let (w, mut t) = fixture(&page(12240, 15840), true);
        t[200 + 548] |= 0x80;
        assert!(format!("{:#}", audit_streams(&w, &t).unwrap_err()).contains("reverse bookfold"));
    }
    #[test]
    fn header_and_printer_records_are_bounded_before_fallback() {
        for pair in [11, 27, 28, 29] {
            let (mut w, t) = fixture(&page(12240, 15840), false);
            set32(&mut w, 0x9a + pair * 8, 1900);
            set32(&mut w, 0x9e + pair * 8, 4);
            if pair == 11 {
                assert!(
                    format!("{:#}", audit_streams(&w, &t).unwrap_err())
                        .contains("without header text")
                );
                set32(&mut w, 0x54, 1);
            }
            fallback(
                &w,
                &t,
                if pair == 11 {
                    "header/footer"
                } else {
                    "printer"
                },
            );
            set32(&mut w, 0x9a + pair * 8, u32::MAX);
            assert!(format!("{:#}", audit_streams(&w, &t).unwrap_err()).contains("truncated"));
        }
    }
    #[test]
    fn side_story_counts_never_prove_body_margins() {
        for offset in [0x50, 0x54, 0x5c, 0x60, 0x64, 0x68] {
            let (mut w, t) = fixture(&page(12240, 15840), false);
            set32(&mut w, offset, 1);
            fallback(&w, &t, "side-story");
        }
    }
    #[test]
    fn active_section_modes_have_geometry_only_fallbacks() {
        for (code, value, why) in [
            (0x3005, 0, "uneven"),
            (0x3009, 0, "break mode"),
            (0x300a, 1, "first-page"),
            (0x500b, 1, "multiple columns"),
            (0x5015, 1, "line numbering"),
            (0x3019, 1, "separator"),
            (0x301a, 1, "vertical"),
            (0xb025, 1, "nonzero gutter"),
            (0x3228, 1, "right-to-left section"),
            (0x322a, 1, "right-to-left gutter"),
            (0x5032, 1, "document grid"),
            (0x5033, 1, "text flow"),
            (0x3239, 1, "revision"),
            (0x5007, 1, "paper source"),
        ] {
            let mut p = page(12240, 15840);
            p.extend(op(code, value));
            let (w, t) = fixture(&p, false);
            fallback(&w, &t, why);
        }
    }
    #[test]
    fn unsupported_physical_subset_and_fixed_margins_do_not_partially_apply() {
        for (code, value, why) in [
            (0xb01f, 14000, "editor subset"),
            (0xb021, 5000, "editor subset"),
            (0x9023, -1440, "negative fixed"),
        ] {
            let mut p = page(12240, 15840);
            p.extend(op(code, value));
            let (w, t) = fixture(&p, false);
            fallback(&w, &t, why);
        }
        let mut p = page(8391, 11906);
        p.extend(op(0xb021, 4320));
        p.extend(op(0xb022, 4320));
        let (w, t) = fixture(&p, false);
        fallback(&w, &t, "content area");
    }
    #[test]
    fn malformed_known_operands_cannot_be_hidden_by_overwrite_or_unknown() {
        for (code, bad, good) in [
            (0xb01f, 143, 12240),
            (0xb020, 31681, 15840),
            (0xb021, 31681, 1440),
            (0xb022, 65535, 1440),
            (0x9023, 31666, 1440),
            (0x9024, -31666, 1440),
            (0xb017, 31681, 720),
            (0x301d, 0, 1),
            (0x3005, 2, 1),
            (0x3009, 5, 2),
            (0x500b, 44, 0),
            (0x5015, 101, 0),
            (0x301a, 4, 0),
            (0x5032, 4, 0),
            (0x5033, 6, 0),
            (0x9031, 0, 360),
            (0x7030, -670926, 0),
            (0x7030, 6488065, 0),
        ] {
            let mut p = page(12240, 15840);
            p.extend(op(0x3030, 0));
            p.extend(op(code, bad));
            p.extend(op(code, good));
            let (w, t) = fixture(&p, false);
            assert!(
                format!("{:#}", audit_streams(&w, &t).unwrap_err()).contains("invalid"),
                "{code:x}"
            );
        }
    }
    #[test]
    fn unknown_well_framed_operations_block_and_truncated_operations_refuse() {
        let mut p = page(12240, 15840);
        p.extend([0x50, 0xd2, 2, 0, 0]);
        let (w, t) = fixture(&p, false);
        fallback(&w, &t, "0xD250");
        for tail in [
            vec![0],
            vec![0x50, 0xd2],
            vec![0x50, 0xd2, 2, 0],
            vec![0x1f, 0xb0, 0],
        ] {
            let mut p = page(12240, 15840);
            p.extend(tail);
            let (w, t) = fixture(&p, false);
            assert!(audit_streams(&w, &t).is_err());
        }
    }
    #[test]
    fn signed_sepx_offset_and_length_and_table_bounds_refuse() {
        let (word, table) = fixture(&page(12240, 15840), false);
        for fc in [0x80000000, 0xfffffffe, 65535] {
            let mut t = table.clone();
            set32(&mut t, 110, fc);
            assert!(audit_streams(&word, &t).is_err());
        }
        let mut w = word.clone();
        set16(&mut w, 4096, 0x8000);
        assert!(format!("{:#}", audit_streams(&w, &table).unwrap_err()).contains("negative"));
        let mut w = word;
        set32(&mut w, 0x9a + 6 * 8, u32::MAX);
        assert!(audit_streams(&w, &table).is_err());
    }
    #[test]
    fn section_cp_errors_are_not_normalized_and_longer_final_sentinel_is_unproved() {
        let (word, table) = fixture(&page(12240, 15840), false);
        for end in [0, 47, 0x80000000] {
            let mut t = table.clone();
            set32(&mut t, 104, end);
            assert!(audit_streams(&word, &t).is_err());
        }
        let mut t = table.clone();
        set32(&mut t, 100, 1);
        assert!(audit_streams(&word, &t).is_err());
        let mut t = table;
        set32(&mut t, 104, 49);
        fallback(&word, &t, "exactly the proven body end");
    }
    #[test]
    fn all_sections_are_framed_before_multisection_fallback() {
        let (mut w, mut t) = fixture(&page(12240, 15840), false);
        set32(&mut w, 0x9e + 6 * 8, 36);
        t[100..136].fill(0);
        set32(&mut t, 104, 20);
        set32(&mut t, 108, 48);
        set32(&mut t, 114, 4096);
        set32(&mut t, 126, 4096);
        fallback(&w, &t, "multiple sections");
        set32(&mut t, 126, 65535);
        assert!(audit_streams(&w, &t).is_err());
    }
    #[test]
    fn page_property_and_operation_work_is_bounded() {
        let (mut w, t) = fixture(&page(12240, 15840), false);
        set32(&mut w, 0x9e + 31 * 8, MAX_PROPERTIES as u32);
        assert!(
            format!("{:#}", audit_streams(&w, &t).unwrap_err()).contains("property-byte limit")
        );
        let p = op(0x300a, 0).repeat(MAX_OPERATIONS + 1);
        let (w, t) = fixture(&p, false);
        assert!(format!("{:#}", audit_streams(&w, &t).unwrap_err()).contains("operation limit"));
    }
    #[test]
    fn printer_metrics_and_repeated_compatibility_flags_are_checked() {
        let (w, mut t) = fixture(&page(12240, 15840), true);
        t[200 + 87] |= 0x80;
        t[200 + 511] |= 0x80;
        fallback(&w, &t, "printer metrics");
        t[200 + 511] = 0;
        assert!(format!("{:#}", audit_streams(&w, &t).unwrap_err()).contains("copies disagree"));
        let (w, mut t) = fixture(&page(12240, 15840), false);
        t[200 + 8] = 1;
        assert!(format!("{:#}", audit_streams(&w, &t).unwrap_err()).contains("copies disagree"));
    }
    #[test]
    fn final_wall_zero_and_wrong_context_framing_follow_source_rules() {
        let mut p = page(12240, 15840);
        p.extend(op(0x3239, 1));
        p.extend(op(0x3239, 0));
        assert_eq!(proved(&p).width_twips, 12240);
        for tail in [vec![0x08, 0xd6, 1, 0], vec![0x15, 0xc6, 255, 0, 0]] {
            let mut p = page(12240, 15840);
            p.extend(tail);
            let (w, t) = fixture(&p, false);
            fallback(&w, &t, "unproven section property");
        }
    }
    #[test]
    fn undefined_copts60_bit_is_ignored_in_each_copy() {
        for offset in [8, 84, 508] {
            let (w, mut t) = fixture(&page(12240, 15840), true);
            t[200 + offset + 1] |= 2;
            assert!(audit_streams(&w, &t).unwrap().geometry.is_some());
        }
    }
    #[test]
    fn multiple_sections_need_proved_source_end_markers() {
        let (mut w, mut t) = fixture(&page(12240, 15840), false);
        set32(&mut w, 0x9e + 6 * 8, 36);
        t[100..136].fill(0);
        set32(&mut t, 104, 19);
        set32(&mut t, 108, 48);
        set32(&mut t, 114, 4096);
        set32(&mut t, 126, 4096);
        assert!(
            format!("{:#}", audit_streams(&w, &t).unwrap_err()).contains("end-of-section marker")
        );
    }
    fn document() -> Node {
        serde_json::from_value(json!({"type":"doc","attrs":{"styleProfile":"standard-v2"},"content":[{"type":"paragraph","attrs":{"spaceBefore":0,"spaceAfter":120,"lineSpacing":240},"content":[{"type":"text","text":" exact  text ","marks":[{"type":"fontFamily","attrs":{"name":"Arial"}},{"type":"fontSize","attrs":{"halfPoints":20}}]}]},{"type":"paragraph"},{"type":"pageBreak"}]})).unwrap()
    }
    #[test]
    fn page_attach_changes_only_root_page_and_precise_risks() {
        let mut doc = document();
        let before = serde_json::to_value(&doc).unwrap();
        let mut risks = BTreeSet::from([BREAK_RISK.into(), "existing warning".into()]);
        let (w, t) = fixture(&page(12240, 15840), false);
        attach(audit_streams(&w, &t).unwrap(), &mut doc, &mut risks).unwrap();
        let page = doc.attrs.remove("pageGeometry").unwrap();
        assert_eq!(page["widthTwips"], 12240);
        assert_eq!(serde_json::to_value(doc).unwrap(), before);
        assert!(risks.contains(PRESERVED));
        assert!(risks.contains("existing warning"));
        assert!(!risks.contains(BREAK_RISK));
    }
    #[test]
    fn unproved_page_keeps_complete_typography_model_and_old_risks() {
        let mut doc = document();
        let before = serde_json::to_value(&doc).unwrap();
        let mut risks = BTreeSet::from([BREAK_RISK.into()]);
        attach(
            Audit {
                geometry: None,
                reasons: BTreeSet::from(["document page settings are absent".into()]),
                split_breaks: None,
                section_ends: None,
            },
            &mut doc,
            &mut risks,
        )
        .unwrap();
        assert_eq!(serde_json::to_value(doc).unwrap(), before);
        assert!(risks.contains(BREAK_RISK));
        assert!(!risks.contains(PRESERVED));
        assert_eq!(risks.len(), 2);
    }
    #[test]
    fn invalid_candidate_or_conflicting_root_never_mutates_model_or_risks() {
        for conflict in [false, true] {
            let mut doc = document();
            if conflict {
                doc.attrs.insert("pageGeometry".into(), json!({}));
            } else {
                doc.content[0].attrs.insert("unknown".into(), json!(1));
            }
            let before = serde_json::to_value(&doc).unwrap();
            let mut risks = BTreeSet::from(["existing".into()]);
            let old = risks.clone();
            let (w, t) = fixture(&page(12240, 15840), false);
            assert!(attach(audit_streams(&w, &t).unwrap(), &mut doc, &mut risks).is_err());
            assert_eq!(serde_json::to_value(doc).unwrap(), before);
            assert_eq!(risks, old);
        }
    }
}
