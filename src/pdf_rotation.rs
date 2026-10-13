//! Deliberately narrow, append-only PDF page rotation. This is not a repairing
//! PDF parser or a content interpreter. Unsupported inputs stay read-only.
use anyhow::{Context, Result, bail, ensure};
use pdf_extract::{
    Dictionary, Document, IncrementalDocument, LoadOptions, Object, ObjectId, StringFormat,
    xref::{XrefEntry, XrefType},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
const MAX_OBJECTS: usize = 20_000;
const MAX_TOKENS: usize = 500_000;
const MAX_DEPTH: usize = 64;
const MAX_PAGES: usize = 1_000;
const MAX_TREE: usize = 4_096;
const UNSUPPORTED: &str = "此 PDF 不在当前旋转另存的安全支持范围内";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub page: u32,
    pub pages: u32,
    pub object_id: u32,
    pub generation: u16,
    /// Effective, normalized source rotation from the displayed PDF.js page.
    pub rotation: i32,
}

/// Byte identity only: not evidence of PDF validity or a filesystem snapshot.
pub fn identity(bytes: &[u8]) -> crate::PdfSourceIdentity {
    crate::PdfSourceIdentity {
        bytes: bytes.len() as u64,
        sha256: Sha256::digest(bytes).into(),
    }
}

fn ws(byte: u8) -> bool {
    matches!(byte, 0 | 9 | 10 | 12 | 13 | 32)
}
fn delimiter(byte: u8) -> bool {
    ws(byte) || b"()<>[]{}/%".contains(&byte)
}

#[derive(Clone)]
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
    tokens: usize,
}
impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            pos: 0,
            tokens: 0,
        }
    }
    fn skip(&mut self) {
        loop {
            while self.bytes.get(self.pos).is_some_and(|b| ws(*b)) {
                self.pos += 1;
            }
            if self.bytes.get(self.pos) != Some(&b'%') {
                break;
            }
            while self
                .bytes
                .get(self.pos)
                .is_some_and(|b| !matches!(b, b'\n' | b'\r'))
            {
                self.pos += 1;
            }
        }
    }
    fn tick(&mut self) -> Result<()> {
        self.tokens += 1;
        ensure!(self.tokens <= MAX_TOKENS, "PDF token budget exceeded");
        Ok(())
    }
    fn word(&mut self) -> Result<&'a [u8]> {
        self.skip();
        self.tick()?;
        let start = self.pos;
        while self.bytes.get(self.pos).is_some_and(|b| !delimiter(*b)) {
            self.pos += 1;
        }
        ensure!(
            self.pos > start && self.pos - start <= 128,
            "Invalid or oversized PDF token"
        );
        Ok(&self.bytes[start..self.pos])
    }
    fn keyword(&mut self, expected: &[u8]) -> Result<()> {
        ensure!(
            self.word()? == expected,
            "Expected PDF {}",
            String::from_utf8_lossy(expected)
        );
        Ok(())
    }
    fn integer(&mut self) -> Result<i64> {
        let word = self.word()?;
        ensure!(
            integer_word(word),
            "Only bounded integer PDF values are supported"
        );
        Ok(std::str::from_utf8(word)?.parse::<i64>()?)
    }
    fn name(&mut self, escaped: bool) -> Result<Vec<u8>> {
        self.skip();
        self.tick()?;
        ensure!(self.bytes.get(self.pos) == Some(&b'/'), "Expected PDF name");
        self.pos += 1;
        let start = self.pos;
        while self.bytes.get(self.pos).is_some_and(|b| !delimiter(*b)) {
            self.pos += 1;
        }
        ensure!(
            self.pos > start && self.pos - start <= 256,
            "Invalid PDF name"
        );
        let raw = &self.bytes[start..self.pos];
        ensure!(
            raw.iter().all(|b| (33..=126).contains(b)),
            "Non-ASCII PDF name unsupported"
        );
        let mut name = Vec::with_capacity(raw.len());
        let mut i = 0;
        while i < raw.len() {
            if raw[i] == b'#' {
                ensure!(
                    escaped && i + 2 < raw.len(),
                    "Escaped PDF names unsupported here"
                );
                name.push(hex(raw[i + 1])?.checked_mul(16).unwrap() + hex(raw[i + 2])?);
                i += 3;
            } else {
                name.push(raw[i]);
                i += 1;
            }
        }
        Ok(name)
    }
    fn hex_string(&mut self) -> Result<Vec<u8>> {
        self.skip();
        self.tick()?;
        ensure!(
            self.bytes.get(self.pos) == Some(&b'<') && self.bytes.get(self.pos + 1) != Some(&b'<'),
            "Expected PDF hexadecimal string"
        );
        self.pos += 1;
        let mut digits = Vec::new();
        while let Some(&b) = self.bytes.get(self.pos) {
            self.pos += 1;
            if b == b'>' {
                ensure!(
                    digits.len() % 2 == 0,
                    "Odd PDF ID hexadecimal string unsupported"
                );
                return Ok(digits
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|p| p[0] * 16 + p[1])
                    .collect());
            }
            if !ws(b) {
                digits.push(hex(b)?);
            }
            ensure!(digits.len() <= 256, "PDF ID too long");
        }
        bail!("Unterminated PDF hexadecimal string")
    }
    fn small_value(&mut self, allow_hex: bool, array: bool) -> Result<Object> {
        self.skip();
        match self.bytes.get(self.pos) {
            Some(b'/') => Ok(Object::Name(self.name(false)?)),
            Some(b'[') if !array => {
                self.pos += 1;
                self.tick()?;
                let mut values = Vec::new();
                loop {
                    self.skip();
                    if self.bytes.get(self.pos) == Some(&b']') {
                        self.pos += 1;
                        break;
                    }
                    ensure!(values.len() < MAX_TREE, "PDF simple array budget exceeded");
                    values.push(self.small_value(allow_hex, true)?);
                }
                Ok(Object::Array(values))
            }
            Some(b'<') if allow_hex => Ok(Object::String(
                self.hex_string()?,
                StringFormat::Hexadecimal,
            )),
            _ => self.integer_reference(),
        }
    }
    fn integer_reference(&mut self) -> Result<Object> {
        let number = self.integer()?;
        ensure!(
            i32::try_from(number).is_ok(),
            "PDF integer outside signed 32-bit range"
        );
        let mut look = self.clone();
        if let Ok(generation) = look.integer()
            && look.keyword(b"R").is_ok()
        {
            ensure!(
                number > 0 && number <= u32::MAX as i64 && generation == 0,
                "Only generation-zero PDF references supported"
            );
            *self = look;
            Ok(Object::Reference((number as u32, 0)))
        } else {
            Ok(Object::Integer(number))
        }
    }
    fn finish_span(&mut self) -> Result<()> {
        loop {
            while self.bytes.get(self.pos).is_some_and(|b| ws(*b)) {
                self.pos += 1;
            }
            if self.bytes.get(self.pos) != Some(&b'%') {
                break;
            }
            while self
                .bytes
                .get(self.pos)
                .is_some_and(|b| !matches!(b, b'\n' | b'\r'))
            {
                self.pos += 1;
            }
            ensure!(
                self.pos < self.bytes.len(),
                "PDF comment crosses object boundary"
            );
        }
        ensure!(
            self.pos == self.bytes.len(),
            "Extra or unindexed PDF object material"
        );
        Ok(())
    }
    fn small_dictionary(&mut self, allowed: &[&[u8]], trailer: bool) -> Result<Dictionary> {
        self.skip();
        self.tick()?;
        ensure!(
            self.bytes.get(self.pos..self.pos + 2) == Some(b"<<"),
            "Expected PDF dictionary"
        );
        self.pos += 2;
        let mut dict = Dictionary::new();
        loop {
            self.skip();
            if self.bytes.get(self.pos..self.pos + 2) == Some(b">>") {
                self.pos += 2;
                break;
            }
            let name = self.name(false)?;
            ensure!(
                allowed.contains(&name.as_slice()),
                "Unsupported PDF property /{}",
                String::from_utf8_lossy(&name)
            );
            ensure!(
                !dict.has(&name),
                "Duplicate PDF property /{}",
                String::from_utf8_lossy(&name)
            );
            let value = self.small_value(trailer && name == b"ID", false)?;
            dict.set(name, value);
        }
        Ok(dict)
    }
}
fn integer_word(word: &[u8]) -> bool {
    let digits = if matches!(word.first(), Some(b'+' | b'-')) {
        &word[1..]
    } else {
        word
    };
    !digits.is_empty() && digits.iter().all(u8::is_ascii_digit)
}
fn hex(b: u8) -> Result<u8> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => bail!("Invalid hexadecimal PDF byte"),
    }
}

struct Raw {
    offsets: Vec<usize>,
    ends: Vec<usize>,
    xref: usize,
    trailer: Dictionary,
}
fn raw_source(source: &[u8]) -> Result<Raw> {
    ensure!(
        source.len() as u64 <= MAX_BYTES
            && source.starts_with(b"%PDF-1.")
            && source.get(7).is_some_and(|b| (b'0'..=b'7').contains(b))
            && source.get(8).is_some_and(|b| matches!(b, b'\n' | b'\r')),
        "Unsupported PDF header or byte limit"
    );
    let eof = source
        .iter()
        .rposition(|b| !ws(*b))
        .context("Missing PDF EOF")?
        + 1;
    ensure!(
        source[..eof].ends_with(b"%%EOF"),
        "Missing exact terminal PDF EOF"
    );
    let start = source[..eof - 5]
        .windows(9)
        .rposition(|w| w == b"startxref")
        .context("Missing PDF startxref")?;
    let mut tail = Cursor::new(&source[start..eof - 5]);
    tail.keyword(b"startxref")?;
    let xref = usize::try_from(tail.integer()?)?;
    tail.finish_span()?;
    ensure!(
        xref < start && source.get(xref..xref + 4) == Some(b"xref"),
        "Invalid PDF xref offset"
    );
    let mut cursor = Cursor::new(&source[xref..start]);
    cursor.keyword(b"xref")?;
    ensure!(
        cursor.integer()? == 0,
        "Only one contiguous classic xref subsection supported"
    );
    let count = usize::try_from(cursor.integer()?)?;
    ensure!(
        (2..=MAX_OBJECTS + 1).contains(&count),
        "PDF object count outside rotation budget"
    );
    // The subsection ends with exactly one line break; records are exactly20 bytes.
    while matches!(cursor.bytes.get(cursor.pos), Some(b' ' | b'\t')) {
        cursor.pos += 1;
    }
    if cursor.bytes.get(cursor.pos) == Some(&b'\r') {
        cursor.pos += 1;
    }
    ensure!(
        cursor.bytes.get(cursor.pos) == Some(&b'\n'),
        "Invalid classic xref header"
    );
    cursor.pos += 1;
    let mut offsets = vec![0];
    for id in 0..count {
        let record = cursor
            .bytes
            .get(cursor.pos..cursor.pos + 20)
            .context("Truncated classic xref")?;
        cursor.pos += 20;
        ensure!(
            record[..10].iter().all(u8::is_ascii_digit)
                && record[10] == b' '
                && record[11..16].iter().all(u8::is_ascii_digit)
                && record[16] == b' '
                && (record[18..] == *b" \n" || record[18..] == *b"\r\n"),
            "Invalid fixed-width classic xref record"
        );
        let offset = std::str::from_utf8(&record[..10])?.parse::<usize>()?;
        if id == 0 {
            ensure!(
                offset == 0 && &record[11..18] == b"65535 f",
                "Invalid PDF object-zero free entry"
            );
        } else {
            ensure!(
                &record[11..18] == b"00000 n" && (9..xref).contains(&offset),
                "Only live generation-zero PDF objects supported"
            );
            offsets.push(offset);
        }
    }
    cursor.keyword(b"trailer")?;
    let trailer = cursor.small_dictionary(&[b"Size", b"Root", b"Info", b"ID"], true)?;
    cursor.finish_span()?;
    ensure!(
        trailer.get(b"Size")?.as_i64()? == count as i64,
        "PDF Size/xref mismatch"
    );
    reference(trailer.get(b"Root")?)?;
    if let Ok(info) = trailer.get(b"Info") {
        reference(info)?;
    }
    if let Ok(ids) = trailer.get(b"ID") {
        let ids = ids.as_array()?;
        ensure!(ids.len()==2 && ids.iter().all(|id| matches!(id,Object::String(bytes,StringFormat::Hexadecimal) if bytes.len()==16)), "Only two 16-byte hexadecimal PDF IDs supported");
    }
    let mut physical: Vec<_> = offsets
        .iter()
        .copied()
        .enumerate()
        .skip(1)
        .map(|(id, offset)| (offset, id))
        .collect();
    physical.sort_unstable();
    ensure!(
        physical.windows(2).all(|p| p[0].0 < p[1].0),
        "Duplicate PDF object offsets"
    );
    let mut prefix = Cursor::new(&source[..physical[0].0]);
    prefix.finish_span()?;
    let mut ends = vec![0; count];
    for (index, &(offset, id)) in physical.iter().enumerate() {
        ends[id] = physical.get(index + 1).map_or(xref, |p| p.0);
        let mut object = Cursor::new(&source[offset..ends[id]]);
        ensure!(
            object.pos == 0 && object.bytes.first().is_some_and(u8::is_ascii_digit),
            "Xref must point exactly to object start"
        );
        ensure!(
            object.integer()? == id as i64 && object.integer()? == 0,
            "PDF xref/object header mismatch"
        );
        object.keyword(b"obj")?;
    }
    Ok(Raw {
        offsets,
        ends,
        xref,
        trailer,
    })
}
fn reference(value: &Object) -> Result<ObjectId> {
    let id = value.as_reference()?;
    ensure!(id.0 > 0 && id.1 == 0, "Invalid generation-zero reference");
    Ok(id)
}
fn load(source: &[u8]) -> Result<Document> {
    Document::load_mem_with_options(
        source,
        LoadOptions {
            strict: true,
            ..Default::default()
        },
    )
    .context("PDF strict load failed")
}

// This lexical guard proves physical framing only. It neither decodes streams
// nor builds an arbitrary PDF object model. lopdf supplies the unchanged model.
// All decoded key names must be unique, including escaped Length aliases.
fn prohibited(name: &[u8]) -> bool {
    [
        b"Linearized".as_slice(),
        b"ObjStm",
        b"XRef",
        b"Encrypt",
        b"ByteRange",
        b"Sig",
        b"DocMDP",
        b"FieldMDP",
        b"AcroForm",
        b"Annots",
        b"Annot",
        b"OpenAction",
        b"AA",
        b"JavaScript",
        b"JS",
        b"StructTreeRoot",
        b"StructParent",
        b"StructParents",
        b"Action",
        b"Launch",
        b"GoTo",
        b"GoToR",
        b"GoToE",
        b"URI",
        b"SubmitForm",
        b"ResetForm",
        b"ImportData",
        b"Named",
        b"SetOCGState",
        b"Rendition",
        b"Trans",
        b"GoTo3DView",
        b"Sound",
        b"Movie",
        b"Hide",
    ]
    .contains(&name)
}
fn frame_value(
    cursor: &mut Cursor<'_>,
    depth: usize,
    refs: &mut Vec<ObjectId>,
    length: &mut Option<Object>,
) -> Result<bool> {
    ensure!(depth <= MAX_DEPTH, "PDF lexical nesting budget exceeded");
    cursor.skip();
    cursor.tick()?;
    match cursor
        .bytes
        .get(cursor.pos)
        .copied()
        .context("Missing PDF object value")?
    {
        b'<' if cursor.bytes.get(cursor.pos + 1) == Some(&b'<') => {
            cursor.pos += 2;
            let mut keys = BTreeSet::new();
            loop {
                cursor.skip();
                if cursor.bytes.get(cursor.pos..cursor.pos + 2) == Some(b">>") {
                    cursor.pos += 2;
                    break;
                }
                let key = cursor.name(true)?;
                ensure!(
                    !prohibited(&key),
                    "Unsupported sensitive PDF name /{}",
                    String::from_utf8_lossy(&key)
                );
                ensure!(
                    keys.insert(key.clone()),
                    "Duplicate decoded PDF dictionary key"
                );
                ensure!(keys.len() <= 4096, "PDF dictionary key budget exceeded");
                if depth == 0 && key == b"Length" {
                    let value = cursor.integer_reference()?;
                    if let Object::Reference(id) = value {
                        refs.push(id);
                    }
                    *length = Some(value);
                } else {
                    frame_value(cursor, depth + 1, refs, &mut None)?;
                }
            }
            Ok(true)
        }
        b'[' => {
            cursor.pos += 1;
            loop {
                cursor.skip();
                if cursor.bytes.get(cursor.pos) == Some(&b']') {
                    cursor.pos += 1;
                    break;
                }
                frame_value(cursor, depth + 1, refs, &mut None)?;
            }
            Ok(false)
        }
        b'(' => {
            cursor.pos += 1;
            let mut nesting = 1;
            let start = cursor.pos;
            while nesting > 0 {
                let b = *cursor
                    .bytes
                    .get(cursor.pos)
                    .context("Unterminated PDF literal string")?;
                cursor.pos += 1;
                match b {
                    b'\\' => {
                        let next = *cursor
                            .bytes
                            .get(cursor.pos)
                            .context("Unterminated PDF string escape")?;
                        cursor.pos += 1;
                        if next == b'\r' && cursor.bytes.get(cursor.pos) == Some(&b'\n') {
                            cursor.pos += 1;
                        }
                    }
                    b'(' => {
                        nesting += 1;
                        ensure!(nesting <= MAX_DEPTH, "PDF literal nesting budget exceeded");
                    }
                    b')' => nesting -= 1,
                    _ => {}
                }
                ensure!(
                    cursor.pos - start <= 1024 * 1024,
                    "PDF literal string budget exceeded"
                );
            }
            Ok(false)
        }
        b'<' => {
            cursor.pos += 1;
            let start = cursor.pos;
            loop {
                let b = *cursor
                    .bytes
                    .get(cursor.pos)
                    .context("Unterminated PDF hex string")?;
                cursor.pos += 1;
                if b == b'>' {
                    break;
                }
                ensure!(ws(b) || b.is_ascii_hexdigit(), "Malformed PDF hex string");
                ensure!(
                    cursor.pos - start <= 2 * 1024 * 1024,
                    "PDF hex string budget exceeded"
                );
            }
            Ok(false)
        }
        b'/' => {
            let name = cursor.name(true)?;
            ensure!(
                !prohibited(&name),
                "Unsupported sensitive PDF name /{}",
                String::from_utf8_lossy(&name)
            );
            Ok(false)
        }
        _ => {
            let word = cursor.word()?;
            if ![b"null".as_slice(), b"true", b"false"].contains(&word) {
                let unsigned = if matches!(word.first(), Some(b'+' | b'-')) {
                    &word[1..]
                } else {
                    word
                };
                ensure!(
                    !unsigned.is_empty()
                        && unsigned.iter().all(|b| b.is_ascii_digit() || *b == b'.')
                        && unsigned.iter().filter(|b| **b == b'.').count() <= 1
                        && unsigned.iter().any(u8::is_ascii_digit),
                    "Unsupported PDF scalar token"
                );
                if integer_word(word) {
                    let mut look = cursor.clone();
                    if let Ok(generation) = look.integer()
                        && look.keyword(b"R").is_ok()
                    {
                        let id = std::str::from_utf8(word)?.parse::<u32>()?;
                        ensure!(
                            id > 0 && generation == 0,
                            "Only generation-zero PDF references supported"
                        );
                        refs.push((id, 0));
                        *cursor = look;
                    }
                }
            }
            Ok(false)
        }
    }
}
fn body<'a>(source: &'a [u8], raw: &Raw, id: u32) -> Result<Cursor<'a>> {
    let index = id as usize;
    ensure!(index > 0 && index < raw.offsets.len(), "Missing PDF object");
    let mut cursor = Cursor::new(&source[raw.offsets[index]..raw.ends[index]]);
    ensure!(
        cursor.integer()? == id as i64 && cursor.integer()? == 0,
        "PDF object ID mismatch"
    );
    cursor.keyword(b"obj")?;
    Ok(cursor)
}
fn audit_frames(source: &[u8], raw: &Raw, doc: &Document) -> Result<()> {
    let mut tokens = 0;
    for id in 1..raw.offsets.len() {
        let mut cursor = body(source, raw, id as u32)?;
        let mut refs = Vec::new();
        let mut length = None;
        let dictionary = frame_value(&mut cursor, 0, &mut refs, &mut length)?;
        for object in refs {
            ensure!(
                doc.objects.contains_key(&object),
                "Missing referenced PDF object"
            );
        }
        cursor.skip();
        let parsed = &doc.objects[&(id as u32, 0)];
        if let Object::Stream(stream) = parsed {
            ensure!(dictionary, "PDF stream must have dictionary");
            cursor.keyword(b"stream")?;
            if cursor.bytes.get(cursor.pos) == Some(&b'\r') {
                cursor.pos += 1;
                if cursor.bytes.get(cursor.pos) == Some(&b'\n') {
                    cursor.pos += 1;
                }
            } else {
                ensure!(
                    cursor.bytes.get(cursor.pos) == Some(&b'\n'),
                    "PDF stream requires exact EOL"
                );
                cursor.pos += 1;
            }
            let length = match length.context("Missing PDF stream Length")? {
                Object::Integer(n) => n,
                Object::Reference((object, 0)) => {
                    let mut number = body(source, raw, object)?;
                    let n = number.integer()?;
                    number.keyword(b"endobj")?;
                    number.finish_span()?;
                    ensure!(
                        doc.objects.get(&(object, 0)) == Some(&Object::Integer(n)),
                        "PDF Length raw/parser mismatch"
                    );
                    n
                }
                _ => bail!("Unsupported indirect PDF stream Length"),
            };
            let length = usize::try_from(length)?;
            let end = cursor
                .pos
                .checked_add(length)
                .context("PDF stream Length overflow")?;
            let payload = cursor
                .bytes
                .get(cursor.pos..end)
                .context("PDF stream crosses next object")?;
            ensure!(
                stream.content == payload
                    && stream
                        .start_position
                        .is_none_or(|position| position == raw.offsets[id] + cursor.pos),
                "PDF stream length/position disagrees with raw bytes"
            );
            cursor.pos = end;
            if cursor.bytes.get(cursor.pos) == Some(&b'\r') {
                cursor.pos += 1;
            }
            if cursor.bytes.get(cursor.pos) == Some(&b'\n') {
                cursor.pos += 1;
            }
            // Do not search for endstream or accept a comment/space-skipped fallback.
            ensure!(
                cursor.bytes.get(cursor.pos..cursor.pos + 9) == Some(b"endstream"),
                "PDF stream Length/endstream mismatch"
            );
            cursor.keyword(b"endstream")?;
        } else {
            ensure!(length.is_none() || dictionary, "Invalid PDF stream framing");
        }
        cursor.keyword(b"endobj")?;
        cursor.finish_span()?;
        tokens += cursor.tokens;
        ensure!(
            tokens <= MAX_TOKENS,
            "PDF aggregate lexical token budget exceeded"
        );
    }
    Ok(())
}

const INHERITED: &[&[u8]] = &[b"Resources", b"MediaBox", b"CropBox", b"Rotate"];
fn raw_dictionary(
    source: &[u8],
    raw: &Raw,
    doc: &Document,
    id: ObjectId,
    allowed: &[&[u8]],
) -> Result<Dictionary> {
    let mut cursor = body(source, raw, id.0)?;
    let dict = cursor.small_dictionary(allowed, false)?;
    cursor.keyword(b"endobj")?;
    cursor.finish_span()?;
    ensure!(
        doc.objects.get(&id) == Some(&Object::Dictionary(dict.clone())),
        "PDF raw dictionary/parser mismatch"
    );
    Ok(dict)
}
#[derive(Clone, Debug, PartialEq)]
struct Page {
    id: ObjectId,
    rotation: i32,
    media: [i64; 4],
    crop: [i64; 4],
    resources: ObjectId,
    contents: Vec<ObjectId>,
}
fn box_value(value: &Object) -> Result<[i64; 4]> {
    let values = value.as_array()?;
    ensure!(values.len() == 4, "PDF box needs four integers");
    let mut box_ = [0; 4];
    for (i, value) in values.iter().enumerate() {
        box_[i] = value.as_i64()?;
        ensure!(
            (-1_000_000..=1_000_000).contains(&box_[i]),
            "PDF coordinate budget exceeded"
        );
    }
    ensure!(
        box_[2] > box_[0]
            && box_[3] > box_[1]
            && box_[2] - box_[0] <= 14_400
            && box_[3] - box_[1] <= 14_400,
        "PDF box dimensions unsupported"
    );
    Ok(box_)
}
struct Tree<'a> {
    source: &'a [u8],
    raw: &'a Raw,
    doc: &'a Document,
    visited: BTreeSet<ObjectId>,
    pages: Vec<Page>,
}
impl Tree<'_> {
    fn walk(
        &mut self,
        id: ObjectId,
        parent: Option<ObjectId>,
        inherited: &Dictionary,
        depth: usize,
    ) -> Result<usize> {
        ensure!(
            depth <= MAX_DEPTH && self.visited.len() < MAX_TREE && self.visited.insert(id),
            "PDF tree cycle/shared node or tree budget exceeded"
        );
        let mut allowed = vec![
            b"Type".as_slice(),
            b"Parent",
            b"Kids",
            b"Count",
            b"Contents",
        ];
        allowed.extend_from_slice(INHERITED);
        let dict = raw_dictionary(self.source, self.raw, self.doc, id, &allowed)?;
        match parent {
            Some(parent) => ensure!(
                reference(dict.get(b"Parent")?)? == parent,
                "PDF Parent mismatch"
            ),
            None => ensure!(!dict.has(b"Parent"), "PDF root Pages must not have Parent"),
        }
        let mut effective = inherited.clone();
        for key in INHERITED {
            if let Ok(value) = dict.get(key) {
                effective.set(key.to_vec(), value.clone());
            }
        }
        // Validate declarations even when a child would override them.
        if let Ok(value) = dict.get(b"MediaBox") {
            box_value(value)?;
        }
        if let Ok(value) = dict.get(b"CropBox") {
            box_value(value)?;
        }
        if let Ok(value) = dict.get(b"Rotate") {
            ensure!(
                value.as_i64()? % 90 == 0,
                "PDF Rotate must be integer multiple of 90"
            );
        }
        if let Ok(value) = dict.get(b"Resources") {
            let id = reference(value)?;
            ensure!(
                matches!(self.doc.objects.get(&id), Some(Object::Dictionary(_))),
                "PDF Resources must reference a dictionary"
            );
        }
        match dict.get(b"Type")?.as_name()? {
            b"Pages" => {
                ensure!(!dict.has(b"Contents"), "Pages cannot have Contents");
                let count = dict.get(b"Count")?.as_i64()?;
                ensure!(
                    (1..=MAX_PAGES as i64).contains(&count),
                    "PDF page Count budget exceeded"
                );
                let kids = dict.get(b"Kids")?.as_array()?;
                ensure!(
                    !kids.is_empty() && kids.len() <= MAX_TREE,
                    "PDF Kids budget exceeded"
                );
                let mut actual = 0;
                for kid in kids {
                    actual += self.walk(reference(kid)?, Some(id), &effective, depth + 1)?;
                }
                ensure!(actual == count as usize, "PDF subtree Count mismatch");
                Ok(actual)
            }
            b"Page" => {
                ensure!(
                    parent.is_some() && !dict.has(b"Kids") && !dict.has(b"Count"),
                    "Invalid PDF Page tree fields"
                );
                ensure!(self.pages.len() < MAX_PAGES, "PDF page budget exceeded");
                let media = box_value(effective.get(b"MediaBox")?)?;
                let crop = effective
                    .get(b"CropBox")
                    .ok()
                    .map(box_value)
                    .transpose()?
                    .unwrap_or(media);
                ensure!(
                    crop[0] >= media[0]
                        && crop[1] >= media[1]
                        && crop[2] <= media[2]
                        && crop[3] <= media[3],
                    "PDF CropBox must be inside MediaBox"
                );
                let rotation = match effective.get(b"Rotate") {
                    Ok(value) => value.as_i64()?.rem_euclid(360) as i32,
                    Err(_) => 0,
                };
                let resources = reference(effective.get(b"Resources")?)?;
                let contents = match dict.get(b"Contents") {
                    Err(_) => vec![],
                    Ok(Object::Array(values)) => {
                        ensure!(values.len() <= 1024, "PDF content array budget exceeded");
                        values.iter().map(reference).collect::<Result<Vec<_>>>()?
                    }
                    Ok(value) => vec![reference(value)?],
                };
                for content in &contents {
                    ensure!(
                        matches!(self.doc.objects.get(content), Some(Object::Stream(_))),
                        "PDF Contents must reference streams"
                    );
                }
                self.pages.push(Page {
                    id,
                    rotation,
                    media,
                    crop,
                    resources,
                    contents,
                });
                Ok(1)
            }
            _ => bail!("Invalid PDF tree node Type"),
        }
    }
}
struct Audited {
    raw: Raw,
    doc: Document,
    pages: Vec<Page>,
}
fn audit(source: &[u8]) -> Result<Audited> {
    let raw = raw_source(source)?;
    let doc = load(source)?;
    ensure!(
        !doc.is_encrypted() && !doc.was_encrypted(),
        "Encrypted PDF rotation unsupported"
    );
    ensure!(
        matches!(
            doc.reference_table.cross_reference_type,
            XrefType::CrossReferenceTable
        ) && doc.xref_start == raw.xref
            && doc.max_id as usize + 1 == raw.offsets.len()
            && doc.objects.len() + 1 == raw.offsets.len()
            && doc.reference_table.entries.len() + 1 == raw.offsets.len()
            && doc.reference_table.size as usize == raw.offsets.len(),
        "PDF loaded object/xref set mismatch"
    );
    ensure!(
        doc.trailer == raw.trailer,
        "PDF raw trailer/parser mismatch"
    );
    for id in 1..raw.offsets.len() {
        ensure!(
            matches!(doc.reference_table.get(id as u32),Some(XrefEntry::Normal{offset,generation:0}) if *offset as usize==raw.offsets[id])
                && doc.objects.contains_key(&(id as u32, 0)),
            "PDF raw/loaded xref mismatch"
        );
    }
    if let Ok(info) = raw.trailer.get(b"Info") {
        ensure!(
            matches!(
                doc.objects.get(&reference(info)?),
                Some(Object::Dictionary(_))
            ),
            "PDF Info must reference an existing dictionary"
        );
    }
    audit_frames(source, &raw, &doc)?;
    let root = reference(raw.trailer.get(b"Root")?)?;
    let catalog = raw_dictionary(source, &raw, &doc, root, &[b"Type", b"Pages"])?;
    ensure!(
        catalog.get(b"Type")?.as_name()? == b"Catalog",
        "Invalid PDF Catalog"
    );
    let mut tree = Tree {
        source,
        raw: &raw,
        doc: &doc,
        visited: BTreeSet::new(),
        pages: Vec::new(),
    };
    tree.walk(
        reference(catalog.get(b"Pages")?)?,
        None,
        &Dictionary::new(),
        0,
    )?;
    let pages = tree.pages;
    Ok(Audited { raw, doc, pages })
}
fn chosen(audit: &Audited, page: u32) -> Result<Selection> {
    let entry = audit
        .pages
        .get(page.checked_sub(1).context("PDF page starts at 1")? as usize)
        .context("PDF page out of range")?;
    Ok(Selection {
        page,
        pages: audit.pages.len() as u32,
        object_id: entry.id.0,
        generation: entry.id.1,
        rotation: entry.rotation,
    })
}
/// Derive a selection for command-line use; the desktop must pass its captured
/// PDF.js identity instead. Both routes are audited again before mutation.
pub fn selection(source: &[u8], page: u32) -> Result<Selection> {
    chosen(&audit(source).context(UNSUPPORTED)?, page)
}

fn output_ids(source: &[u8], audit: &Audited, selection: &Selection, target: i32) -> Object {
    let mut seed = Sha256::new();
    seed.update(b"jianwen-pdf-rotation-id-v1\0");
    seed.update(identity(source).sha256);
    seed.update(selection.object_id.to_be_bytes());
    seed.update(target.to_be_bytes());
    let second = seed.finalize()[..16].to_vec();
    let first = audit
        .raw
        .trailer
        .get(b"ID")
        .ok()
        .and_then(|ids| ids.as_array().ok())
        .map(|ids| ids[0].clone())
        .unwrap_or_else(|| {
            Object::String(
                identity(source).sha256[..16].to_vec(),
                StringFormat::Hexadecimal,
            )
        });
    Object::Array(vec![
        first,
        Object::String(second, StringFormat::Hexadecimal),
    ])
}
struct BoundedOutput(Vec<u8>);
impl std::io::Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self
            .0
            .len()
            .checked_add(bytes.len())
            .is_none_or(|size| size as u64 > MAX_BYTES)
        {
            return Err(std::io::Error::other("Rotated PDF exceeds 64 MiB"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// Add an explicit effective clockwise 90 Rotate to exactly the displayed page.
/// The complete source is retained as a prefix, including all resource streams.
/// A saved result contains a prior revision and is deliberately not accepted as
/// a new rotation source in this first release.
pub fn rotate(source: &[u8], selection: &Selection) -> Result<Vec<u8>> {
    let audit = audit(source).context(UNSUPPORTED)?;
    ensure!(
        &chosen(&audit, selection.page)? == selection,
        "PDF 页面身份、页数或方向已变化，请重新打开文档"
    );
    let id = (selection.object_id, selection.generation);
    let target = (selection.rotation + 90) % 360;
    let mut replacement = audit.doc.objects[&id].as_dict()?.clone();
    replacement.set("Rotate", target);
    let ids = output_ids(source, &audit, selection, target);
    if let Ok(old) = audit.raw.trailer.get(b"ID") {
        ensure!(
            old.as_array()?[1] != ids.as_array()?[1],
            "PDF changed ID unexpectedly matches original"
        );
    }
    let mut incremental = IncrementalDocument::create_from(source.to_vec(), audit.doc.clone());
    incremental
        .new_document
        .objects
        .insert(id, Object::Dictionary(replacement.clone()));
    incremental.new_document.trailer.set("ID", ids.clone());
    let mut output = BoundedOutput(Vec::with_capacity(
        source.len().saturating_add(4096).min(MAX_BYTES as usize),
    ));
    incremental
        .save_to(&mut output)
        .context("Cannot create bounded PDF rotation candidate")?;
    verify_candidate(source, &output.0, &audit, selection, &replacement, &ids)?;
    Ok(output.0)
}
fn verify_candidate(
    source: &[u8],
    output: &[u8],
    audit: &Audited,
    selection: &Selection,
    replacement: &Dictionary,
    ids: &Object,
) -> Result<()> {
    ensure!(
        output.len() as u64 <= MAX_BYTES && output.starts_with(source),
        "PDF candidate did not preserve source prefix"
    );
    let loaded = load(output)?;
    let selected = (selection.object_id, 0);
    ensure!(
        loaded.objects.len() == audit.doc.objects.len()
            && loaded.max_id == audit.doc.max_id
            && !loaded.was_encrypted()
            && !loaded.is_encrypted(),
        "PDF candidate object set changed"
    );
    let mut selected_offset = None;
    for (id, object) in &audit.doc.objects {
        let expected = if *id == selected {
            Object::Dictionary(replacement.clone())
        } else {
            object.clone()
        };
        ensure!(
            loaded.objects.get(id) == Some(&expected),
            "PDF candidate changed an unrelated object"
        );
        let expected_offset = if *id == selected {
            let Some(XrefEntry::Normal {
                offset,
                generation: 0,
            }) = loaded.reference_table.get(id.0)
            else {
                bail!("Invalid candidate selected xref")
            };
            ensure!(
                *offset as usize >= source.len(),
                "PDF replacement object was not appended"
            );
            selected_offset = Some(*offset as usize);
            *offset as usize
        } else {
            audit.raw.offsets[id.0 as usize]
        };
        ensure!(
            matches!(loaded.reference_table.get(id.0),Some(XrefEntry::Normal{offset,generation:0}) if *offset as usize==expected_offset),
            "PDF candidate changed an unrelated active offset"
        );
    }
    let offset = selected_offset.context("Missing PDF candidate replacement")?;
    let mut prefix = Cursor::new(&output[source.len()..offset]);
    prefix.finish_span()?;
    let mut page = Cursor::new(&output[offset..loaded.xref_start]);
    ensure!(
        page.integer()? == selected.0 as i64 && page.integer()? == 0,
        "Invalid candidate page header"
    );
    page.keyword(b"obj")?;
    let mut allowed = vec![b"Type".as_slice(), b"Parent", b"Contents"];
    allowed.extend_from_slice(INHERITED);
    ensure!(
        &page.small_dictionary(&allowed, false)? == replacement,
        "Candidate raw page differs from intended dictionary"
    );
    page.keyword(b"endobj")?;
    page.finish_span()?;
    // Audit the generated incremental trailer directly: lopdf removes Prev on load.
    let mut tail = Cursor::new(&output[loaded.xref_start..]);
    tail.keyword(b"xref")?;
    let mut records = BTreeMap::new();
    loop {
        tail.skip();
        if tail.bytes.get(tail.pos..tail.pos + 7) == Some(b"trailer") {
            break;
        }
        let start = u32::try_from(tail.integer()?)?;
        let count = usize::try_from(tail.integer()?)?;
        ensure!(
            count <= 2 && records.len() + count <= 2,
            "Unexpected candidate xref records"
        );
        for index in 0..count {
            let offset = tail.integer()?;
            let generation = tail.integer()?;
            let kind = tail.word()?;
            ensure!(
                records
                    .insert(start + index as u32, (offset, generation, kind.to_vec()))
                    .is_none(),
                "Duplicate candidate xref record"
            );
        }
    }
    ensure!(
        records.len() == 2
            && records.get(&0) == Some(&(0, 65535, b"f".to_vec()))
            && records.get(&selected.0) == Some(&(offset as i64, 0, b"n".to_vec())),
        "Candidate xref mutation exceeds selected page"
    );
    tail.keyword(b"trailer")?;
    let raw_trailer = tail.small_dictionary(&[b"Size", b"Root", b"Info", b"ID", b"Prev"], true)?;
    let mut expected_trailer = audit.raw.trailer.clone();
    expected_trailer.set("ID", ids.clone());
    ensure!(
        loaded.trailer == expected_trailer,
        "Candidate loaded trailer mismatch"
    );
    expected_trailer.set("Prev", audit.raw.xref as i64);
    ensure!(
        raw_trailer == expected_trailer,
        "Candidate trailer bookkeeping mismatch"
    );
    tail.keyword(b"startxref")?;
    ensure!(
        tail.integer()? == loaded.xref_start as i64,
        "Candidate startxref mismatch"
    );
    while tail.bytes.get(tail.pos).is_some_and(|b| ws(*b)) {
        tail.pos += 1;
    }
    ensure!(
        tail.bytes.get(tail.pos..) == Some(b"%%EOF"),
        "Candidate EOF mismatch"
    );
    let mut merged = Raw {
        offsets: audit.raw.offsets.clone(),
        ends: audit.raw.ends.clone(),
        xref: loaded.xref_start,
        trailer: loaded.trailer.clone(),
    };
    merged.offsets[selected.0 as usize] = offset;
    merged.ends[selected.0 as usize] = loaded.xref_start;
    let root = reference(loaded.trailer.get(b"Root")?)?;
    let catalog = raw_dictionary(output, &merged, &loaded, root, &[b"Type", b"Pages"])?;
    let mut tree = Tree {
        source: output,
        raw: &merged,
        doc: &loaded,
        visited: BTreeSet::new(),
        pages: Vec::new(),
    };
    tree.walk(
        reference(catalog.get(b"Pages")?)?,
        None,
        &Dictionary::new(),
        0,
    )?;
    let mut expected_pages = audit.pages.clone();
    expected_pages[selection.page as usize - 1].rotation = (selection.rotation + 90) % 360;
    ensure!(
        tree.pages == expected_pages,
        "Candidate page order/geometry/resource identity changed"
    );
    Ok(())
}

/// Bounded command-line export. Desktop publication additionally fences sessions.
pub fn rotate_file(
    input: &std::path::Path,
    output: &std::path::Path,
    page: u32,
) -> Result<Selection> {
    use std::io::{Read, Write};
    ensure!(
        output
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("pdf")),
        "Use a .pdf destination"
    );
    ensure!(
        !output.try_exists()? && std::fs::symlink_metadata(output).is_err(),
        "Refusing to overwrite an existing destination"
    );
    let mut source = Vec::new();
    std::fs::File::open(input)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut source)?;
    let selected = selection(&source, page)?;
    let candidate = rotate(&source, &selected)?;
    let original = identity(&source);
    crate::read_verified_pdf(input, &original)?;
    let directory = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let mut pending = tempfile::NamedTempFile::new_in(directory)?;
    pending.write_all(&candidate)?;
    pending.as_file().sync_all()?;
    pending
        .persist_noclobber(output)
        .map_err(|e| e.error)
        .context("Cannot publish new PDF without overwriting")?;
    Ok(Selection {
        rotation: (selected.rotation + 90) % 360,
        ..selected
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pdf(objects: &[Vec<u8>], extra: &str) -> Vec<u8> {
        let mut bytes = b"%PDF-1.4\n".to_vec();
        let mut offsets = vec![0];
        for (i, object) in objects.iter().enumerate() {
            offsets.push(bytes.len());
            bytes.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            bytes.extend_from_slice(object);
            bytes.extend_from_slice(b"\nendobj\n");
        }
        let xref = bytes.len();
        bytes.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
        );
        for offset in offsets.iter().skip(1) {
            bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        bytes.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R {extra} >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        bytes
    }
    fn objects() -> Vec<Vec<u8>> {
        [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 /Resources 4 0 R /MediaBox [0 0 612 792] >>",
            "<< /Type /Page /Parent 2 0 R /Contents 5 0 R >>",
            "<< >>",
            "<< /Length 0 >>\nstream\n\nendstream",
        ]
        .iter()
        .map(|s| s.as_bytes().to_vec())
        .collect()
    }
    fn source() -> Vec<u8> {
        pdf(&objects(), "")
    }
    fn reject_object(index: usize, value: &str) {
        let mut objects = objects();
        objects[index] = value.as_bytes().to_vec();
        assert!(
            selection(&pdf(&objects, ""), 1).is_err(),
            "accepted {value}"
        );
    }
    #[test]
    fn simple_integer_arrays_are_not_references() {
        let mut c = Cursor::new(b"[0 0 612 792]");
        assert_eq!(
            c.small_value(false, false).unwrap(),
            Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()])
        );
    }
    #[test]
    fn simple_source_rotation_is_deterministic_prefix_preserving_and_not_repeatable() {
        let source = source();
        let selected = selection(&source, 1).unwrap();
        assert_eq!(
            selected,
            Selection {
                page: 1,
                pages: 1,
                object_id: 3,
                generation: 0,
                rotation: 0
            }
        );
        let output = rotate(&source, &selected).unwrap();
        assert!(output.starts_with(&source));
        assert_eq!(output, rotate(&source, &selected).unwrap());
        let loaded = load(&output).unwrap();
        assert_eq!(
            loaded.objects[&(3, 0)]
                .as_dict()
                .unwrap()
                .get(b"Rotate")
                .unwrap()
                .as_i64()
                .unwrap(),
            90
        );
        assert!(selection(&output, 1).is_err());
    }
    #[test]
    fn existing_first_id_is_preserved_and_second_changes() {
        let source = pdf(
            &objects(),
            "/ID [<00112233445566778899AABBCCDDEEFF> <FFEEDDCCBBAA99887766554433221100>]",
        );
        let output = rotate(&source, &selection(&source, 1).unwrap()).unwrap();
        let old = load(&source).unwrap();
        let new = load(&output).unwrap();
        let old = old.trailer.get(b"ID").unwrap().as_array().unwrap();
        let new = new.trailer.get(b"ID").unwrap().as_array().unwrap();
        assert_eq!(old[0], new[0]);
        assert_ne!(old[1], new[1]);
    }
    #[test]
    fn inherited_rotation_and_crop_are_effective_and_only_leaf_changes() {
        for angle in [-450_i32, -90, 0, 90, 270, 360, 450] {
            let mut o = objects();
            o[1]=format!("<< /Type /Pages /Kids [3 0 R] /Count 1 /Resources 4 0 R /MediaBox [0 0 612 792] /CropBox [10 20 600 780] /Rotate {angle} >>").into_bytes();
            let bytes = pdf(&o, "");
            let s = selection(&bytes, 1).unwrap();
            assert_eq!(s.rotation, angle.rem_euclid(360));
            let after = load(&rotate(&bytes, &s).unwrap()).unwrap();
            let before = load(&bytes).unwrap();
            assert_eq!(before.objects[&(2, 0)], after.objects[&(2, 0)]);
        }
    }
    #[test]
    fn nested_tree_preserves_order_and_shared_resources() {
        let mut o = objects();
        o[1]=b"<< /Type /Pages /Kids [6 0 R 7 0 R] /Count 2 /Resources 4 0 R /MediaBox [0 0 612 792] >>".to_vec();
        o[2] = b"<< /Type /Page /Parent 6 0 R /Contents [5 0 R] >>".to_vec();
        o.push(b"<< /Type /Pages /Parent 2 0 R /Kids [3 0 R] /Count 1 /Rotate 90 >>".to_vec());
        o.push(b"<< /Type /Page /Parent 2 0 R >>".to_vec());
        let bytes = pdf(&o, "");
        assert_eq!(selection(&bytes, 1).unwrap().object_id, 3);
        assert_eq!(selection(&bytes, 2).unwrap().object_id, 7);
        rotate(&bytes, &selection(&bytes, 2).unwrap()).unwrap();
    }
    #[test]
    fn selection_is_bound_to_all_displayed_identity_fields() {
        let bytes = source();
        let expected = selection(&bytes, 1).unwrap();
        for changed in [
            Selection {
                page: 0,
                ..expected.clone()
            },
            Selection {
                page: 2,
                ..expected.clone()
            },
            Selection {
                pages: 2,
                ..expected.clone()
            },
            Selection {
                object_id: 4,
                ..expected.clone()
            },
            Selection {
                generation: 1,
                ..expected.clone()
            },
            Selection {
                rotation: 90,
                ..expected.clone()
            },
        ] {
            assert!(rotate(&bytes, &changed).is_err());
        }
    }
    #[test]
    fn raw_critical_duplicate_escape_real_nested_and_extra_tokens_reject() {
        for value in [
            "<< /Type /Page /Type /Page /Parent 2 0 R >>",
            "<< /Type /Page /Parent 2 0 R /Ro#74ate 90 >>",
            "<< /Type /Page /Parent 2 0 R /Rotate 90.0 >>",
            "<< /Type /Page /Parent 2 0 R /Resources << >> >>",
            "<< /Type /Page /Parent 2 0 R /Rotate 90 garbage >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612.0 792] >>",
            "<< /Type /Page /Parent 2 0 R /Rotate (90) >>",
        ] {
            reject_object(2, value);
        }
    }
    #[test]
    fn ambiguous_trailers_and_prior_revisions_reject() {
        for extra in [
            "/Size 6",
            "/Root 1 0 R",
            "/Ro#6Ft 1 0 R",
            "/Prev 0",
            "/XRefStm 1",
            "/Encrypt 4 0 R",
            "/En#63rypt 4 0 R",
            "/Unknown 0",
            "/ID [(text) (text)]",
            "/ID [<AB> <CD>]",
        ] {
            assert!(
                selection(&pdf(&objects(), extra), 1).is_err(),
                "accepted {extra}"
            );
        }
    }
    #[test]
    fn sensitive_active_and_orphan_names_reject() {
        for key in [
            "Annots",
            "AA",
            "UserUnit",
            "BleedBox",
            "StructParents",
            "Dur",
            "Metadata",
        ] {
            reject_object(2, &format!("<< /Type /Page /Parent 2 0 R /{key} 4 0 R >>"));
        }
        for value in [
            "<< /Type /Sig >>",
            "<< /Type /Action /S /Launch >>",
            "<< /ByteRange [0 1 2 3] >>",
            "<< /Linearized 1 >>",
            "<< /Type /ObjStm >>",
            "<< /Type /XRef >>",
            "<< /En#63rypt 1 >>",
            "<< /AcroForm 4 0 R >>",
        ] {
            let mut o = objects();
            o.push(value.as_bytes().to_vec());
            assert!(selection(&pdf(&o, ""), 1).is_err());
        }
    }
    #[test]
    fn raw_geometry_and_rotation_bounds_reject() {
        for property in [
            "/MediaBox [0 0 0 792]",
            "/MediaBox [0 0 14401 792]",
            "/MediaBox [-1000001 0 0 792]",
            "/MediaBox [0 0 612]",
            "/CropBox [-1 0 612 792]",
            "/CropBox [0 0 613 792]",
            "/Rotate 91",
            "/Rotate 9007199254740990",
            "/Rotate 9223372036854775800",
            "/Rotate -9223372036854775808",
        ] {
            reject_object(2, &format!("<< /Type /Page /Parent 2 0 R {property} >>"));
        }
    }
    #[test]
    fn tree_wrong_parents_counts_types_shared_children_and_cycles_reject() {
        for value in [
            "<< /Type /Pages /Kids [3 0 R] /Count 2 >>",
            "<< /Type /Pages /Kids [3 0 R 3 0 R] /Count 2 >>",
            "<< /Type /Pages /Kids [2 0 R] /Count 1 >>",
            "<< /Type /Pages /Parent 1 0 R /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Pages /Kids [] /Count 0 >>",
        ] {
            reject_object(1, value);
        }
        for value in [
            "<< /Type /Page /Parent 1 0 R >>",
            "<< /Type /Pages /Parent 2 0 R /Kids [2 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /Count 1 >>",
            "<< /Type /Other /Parent 2 0 R >>",
        ] {
            reject_object(2, value);
        }
    }
    #[test]
    fn missing_or_wrong_resource_and_content_objects_reject() {
        for prop in [
            "/Resources 5 0 R",
            "/Resources 999 0 R",
            "/Contents 4 0 R",
            "/Contents [5 0 R 999 0 R]",
            "/Contents [] /Resources 999 0 R",
        ] {
            reject_object(2, &format!("<< /Type /Page /Parent 2 0 R {prop} >>"));
        }
        reject_object(1, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    }
    #[test]
    fn direct_and_one_hop_raw_integer_lengths_are_supported() {
        let mut o = objects();
        o[4] = b"<< /Length 6 0 R >>\nstream\nq Q\nendstream".to_vec();
        o.push(b"3".to_vec());
        selection(&pdf(&o, ""), 1).unwrap();
    }
    #[test]
    fn stream_lengths_duplicates_aliases_and_boundary_confusion_reject() {
        for stream in [
            "<< /Length 2 >>\nstream\n\nendstream",
            "<< /Length 0 /Length 0 >>\nstream\n\nendstream",
            "<< /Length 0 /Len#67th 0 >>\nstream\n\nendstream",
            "<< /Length 0.0 >>\nstream\n\nendstream",
            "<< /Length 0 >>\nstream \n\nendstream",
            "<< /Length 0 >>\nstream\n garbage endstream",
            "<< /Length 0 >>\nstream\n%hidden\nendstream",
        ] {
            reject_object(4, stream);
        }
        for length in ["6 0 R", "0.0", "0 0", "-1"] {
            let mut o = objects();
            o[4] = b"<< /Length 6 0 R >>\nstream\n\nendstream".to_vec();
            o.push(length.as_bytes().to_vec());
            assert!(selection(&pdf(&o, ""), 1).is_err());
        }
    }
    #[test]
    fn embedded_object_headers_and_unindexed_objects_reject() {
        let mut o = objects();
        o[3] = b"<< >>\nendobj\n77 0 obj << >>".to_vec();
        assert!(selection(&pdf(&o, ""), 1).is_err());
        for value in ["(unterminated", "<< /A (unterminated >>"] {
            reject_object(3, value);
        }
    }
    #[test]
    fn indexed_fake_header_inside_stream_is_rejected() {
        let mut o = objects();
        let payload = b"4 0 obj\n<< >>\nendobj\n";
        o[4] = [
            format!("<< /Length {} >>\nstream\n", payload.len()).as_bytes(),
            payload,
            b"endstream",
        ]
        .concat();
        let mut bytes = pdf(&o, "");
        let fake = bytes
            .windows(payload.len())
            .rposition(|w| w == payload)
            .unwrap();
        let raw = raw_source(&bytes).unwrap();
        let record = raw.xref + format!("xref\n0 {}\n", o.len() + 1).len() + 4 * 20;
        bytes[record..record + 10].copy_from_slice(format!("{fake:010}").as_bytes());
        assert!(selection(&bytes, 1).is_err());
    }
    #[test]
    fn indexed_fake_header_inside_literal_is_rejected() {
        let mut o = objects();
        o.push(b"(4 0 obj << >> endobj)".to_vec());
        let mut bytes = pdf(&o, "");
        let fake = raw_source(&bytes).unwrap().offsets[4];
        let actual = bytes.windows(10).rposition(|w| w == b"(4 0 obj <").unwrap() + 1;
        assert_ne!(fake, actual);
        let raw = raw_source(&bytes).unwrap();
        let record = raw.xref + format!("xref\n0 {}\n", o.len() + 1).len() + 4 * 20;
        bytes[record..record + 10].copy_from_slice(format!("{actual:010}").as_bytes());
        assert!(selection(&bytes, 1).is_err());
    }
    #[test]
    fn xref_records_headers_and_terminal_material_are_exact() {
        let good = source();
        let raw = raw_source(&good).unwrap();
        for changed in [
            good.iter()
                .copied()
                .chain(b"junk".iter().copied())
                .collect::<Vec<_>>(),
            good[..good.len() - 6].to_vec(),
        ] {
            assert!(selection(&changed, 1).is_err());
        }
        let mut bad = good.clone();
        bad[raw.offsets[3]] = b'9';
        assert!(selection(&bad, 1).is_err());
        let mut bad = good.clone();
        let start = bad.windows(9).rposition(|w| w == b"startxref").unwrap();
        let tail = format!("startxref\n{}\n%%EOF\n", raw.xref - 1);
        bad.truncate(start);
        bad.extend_from_slice(tail.as_bytes());
        assert!(selection(&bad, 1).is_err());
    }
    #[test]
    fn comment_cannot_continue_across_a_verified_object_boundary() {
        let mut bytes = source();
        let raw = raw_source(&bytes).unwrap();
        let boundary = raw.ends[4];
        assert_eq!(bytes[boundary - 1], b'\n');
        bytes[boundary - 1] = b'%';
        assert!(selection(&bytes, 1).is_err());
    }
    #[test]
    fn terminal_comments_cannot_hide_startxref_or_eof_markers() {
        let original = source();
        for marker in [b"startxref".as_slice(), b"%%EOF"] {
            let at = original
                .windows(marker.len())
                .rposition(|part| part == marker)
                .unwrap();
            let mut changed = original.clone();
            changed.splice(at..at, b"% hidden ".iter().copied());
            assert!(selection(&changed, 1).is_err());
        }
    }
    #[test]
    fn missing_endobj_and_parser_skipped_objects_reject() {
        reject_object(3, "<< >>\nBAD");
        reject_object(3, "definitely-not-an-object");
        let mut bytes = source();
        let raw = raw_source(&bytes).unwrap();
        let start = raw.offsets[4];
        let end = raw.ends[4];
        let at = start
            + bytes[start..end]
                .windows(6)
                .position(|w| w == b"endobj")
                .unwrap();
        bytes[at..at + 6].copy_from_slice(b"      ");
        assert!(selection(&bytes, 1).is_err());
    }
    #[test]
    fn framing_nesting_and_token_budgets_are_enforced() {
        let mut o = objects();
        o.push(
            format!(
                "{}0{}",
                "[".repeat(MAX_DEPTH + 2),
                "]".repeat(MAX_DEPTH + 2)
            )
            .into_bytes(),
        );
        assert!(selection(&pdf(&o, ""), 1).is_err());
        let mut c = Cursor::new(b"0");
        c.tokens = MAX_TOKENS;
        assert!(c.integer().is_err());
    }
    #[test]
    fn tree_page_and_node_budgets_are_enforced() {
        reject_object(1, "<< /Type /Pages /Kids [3 0 R] /Count 1001 >>");
        let mut o = objects();
        o[1] =
            b"<< /Type /Pages /Kids [6 0 R] /Count 1 /Resources 4 0 R /MediaBox [0 0 612 792] >>"
                .to_vec();
        for id in 6..=71 {
            o.push(
                format!(
                    "<< /Type /Pages /Parent {} 0 R /Kids [{} 0 R] /Count 1 >>",
                    if id == 6 { 2 } else { id - 1 },
                    if id == 71 { 3 } else { id + 1 }
                )
                .into_bytes(),
            );
        }
        o[2] = b"<< /Type /Page /Parent 71 0 R >>".to_vec();
        assert!(selection(&pdf(&o, ""), 1).is_err());
    }
    #[test]
    fn object_array_dictionary_and_input_budgets_reject_before_publication() {
        let original = source();
        let text = String::from_utf8(original.clone())
            .unwrap()
            .replace("xref\n0 6\n", "xref\n0 20002\n");
        assert!(selection(text.as_bytes(), 1).is_err());
        let mut o = objects();
        o[1] = format!(
            "<< /Type /Pages /Kids [{}] /Count 1 >>",
            "3 0 R ".repeat(MAX_TREE + 1)
        )
        .into_bytes();
        assert!(selection(&pdf(&o, ""), 1).is_err());
        let mut o = objects();
        o.push(
            format!(
                "<< {} >>",
                (0..4097).map(|i| format!("/K{i} 0 ")).collect::<String>()
            )
            .into_bytes(),
        );
        assert!(selection(&pdf(&o, ""), 1).is_err());
        let mut oversized = vec![0; MAX_BYTES as usize + 1];
        oversized[..9].copy_from_slice(b"%PDF-1.4\n");
        assert!(selection(&oversized, 1).is_err());
        for info in ["/Info 999 0 R", "/Info 5 0 R"] {
            assert!(selection(&pdf(&objects(), info), 1).is_err());
        }
    }
    #[test]
    fn output_write_limit_does_not_partially_extend() {
        use std::io::Write;
        let mut out = BoundedOutput(vec![0; MAX_BYTES as usize]);
        assert!(out.write_all(b"x").is_err());
        assert_eq!(out.0.len(), MAX_BYTES as usize);
    }
    #[test]
    fn file_export_keeps_source_and_refuses_existing_and_alias_targets() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("source.pdf");
        let output = dir.path().join("rotated.pdf");
        let bytes = source();
        std::fs::write(&input, &bytes).unwrap();
        rotate_file(&input, &output, 1).unwrap();
        let saved = std::fs::read(&output).unwrap();
        assert!(rotate_file(&input, &output, 1).is_err());
        assert!(rotate_file(&input, &input, 1).is_err());
        let alias = dir.path().join("alias.pdf");
        std::fs::hard_link(&input, &alias).unwrap();
        assert!(rotate_file(&input, &alias, 1).is_err());
        assert_eq!(std::fs::read(&input).unwrap(), bytes);
        assert_eq!(std::fs::read(&output).unwrap(), saved);
    }
}
