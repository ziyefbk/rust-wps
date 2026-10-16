//! Opt-in exact replacement of a deliberately small marked-content subset.
//! The legacy extraction entry points do not call this module. Parsing and
//! decompression still use lopdf and are NOT a hard memory/time sandbox.
use super::*;
use lopdf::content::Operation;
use std::collections::{HashMap as Map, HashSet};

const MAX_DEPTH: usize = 64;
const MAX_OBJECT_VISITS: usize = 200_000;
const MAX_PAGES: usize = 1_000;
const MAX_OPERATIONS: usize = 500_000;
const MAX_DECODED_BYTES: usize = 32 * 1024 * 1024;
const MAX_REPLACEMENT_BYTES: usize = 64 * 1024;
const MAX_OUTPUT_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActualTextStatus {
    NotDetected,
    Applied,
    Unknown,
}

#[derive(Debug)]
pub struct ActualTextExtraction {
    pub text: String,
    pub status: ActualTextStatus,
    /// A bounded audit diagnostic for Unknown; never source text.
    pub diagnostic: Option<String>,
}

fn unsupported(message: impl Into<String>) -> OutputError {
    OutputError::ActualTextUnsupported(message.into())
}

fn resolve<'a>(doc: &'a Document, mut object: &'a Object) -> Result<&'a Object, OutputError> {
    let mut visited = HashSet::new();
    while let Object::Reference(id) = object {
        if visited.len() >= MAX_DEPTH || !visited.insert(*id) {
            return Err(unsupported("property reference cycle or depth limit"));
        }
        object = doc
            .objects
            .get(id)
            .ok_or_else(|| unsupported("missing property reference"))?;
    }
    Ok(object)
}

fn dictionary<'a>(doc: &'a Document, object: &'a Object) -> Result<&'a Dictionary, OutputError> {
    resolve(doc, object)?
        .as_dict()
        .map_err(|_| unsupported("expected a property/resource dictionary"))
}

fn resources<'a>(
    doc: &'a Document,
    mut dict: &'a Dictionary,
    empty: &'a Dictionary,
) -> Result<&'a Dictionary, OutputError> {
    let mut visited = HashSet::new();
    let mut nearest = None;
    for _ in 0..MAX_DEPTH {
        if nearest.is_none() {
            if let Ok(value) = dict.get(b"Resources") {
                nearest = Some(dictionary(doc, value)?);
            }
        }
        // Validate the complete parent chain even after finding Resources:
        // the legacy interpreter also inherits MediaBox through that chain.
        let Ok(parent) = dict.get(b"Parent") else {
            return Ok(nearest.unwrap_or(empty));
        };
        if let Object::Reference(id) = parent {
            if !visited.insert(*id) {
                return Err(unsupported("page resource inheritance cycle"));
            }
        }
        dict = dictionary(doc, parent)?;
    }
    Err(unsupported("page resource inheritance depth limit"))
}

fn decode_string(doc: &Document, value: &Object) -> Result<String, OutputError> {
    let bytes = resolve(doc, value)?
        .as_str()
        .map_err(|_| unsupported("ActualText must be a text string"))?;
    if bytes.len() > MAX_REPLACEMENT_BYTES {
        return Err(unsupported("ActualText string exceeds 64 KiB"));
    }
    let text = if let Some(bytes) = bytes.strip_prefix(b"\xfe\xff") {
        if bytes.len() % 2 != 0 {
            return Err(unsupported("odd UTF-16BE ActualText byte count"));
        }
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|p| u16::from_be_bytes([p[0], p[1]]))
            .collect();
        String::from_utf16(&units)
            .map_err(|_| unsupported("invalid UTF-16BE ActualText surrogate"))?
    } else if bytes.starts_with(b"\xef\xbb\xbf") || bytes.starts_with(b"\xff\xfe") {
        return Err(unsupported(
            "ActualText encoding is outside PDFDocEncoding/UTF-16BE support",
        ));
    } else {
        let mut text = String::new();
        for byte in bytes {
            // Table D.2 reserves the low controls; TAB/LF/CR are ordinary text
            // whitespace and must be retained, unlike lopdf's lossy mapping.
            let code = match byte {
                9 | 10 | 13 => u16::from(*byte),
                0..=23 => return Err(unsupported("unsupported PDFDocEncoding control byte")),
                _ => {
                    let code = PDFDocEncoding[*byte as usize];
                    if code == 0 {
                        return Err(unsupported("undefined PDFDocEncoding ActualText byte"));
                    }
                    code
                }
            };
            text.push(
                char::from_u32(u32::from(code))
                    .ok_or_else(|| unsupported("invalid PDFDocEncoding ActualText character"))?,
            );
        }
        text
    };
    if text.contains('\u{1b}') {
        return Err(unsupported(
            "ActualText language escape sequences are unsupported",
        ));
    }
    if text.len() > MAX_REPLACEMENT_BYTES {
        return Err(unsupported("decoded ActualText exceeds 64 KiB"));
    }
    Ok(text)
}

pub(super) fn replacement(
    doc: &Document,
    resources: &Dictionary,
    op: &Operation,
) -> Result<Option<String>, OutputError> {
    if op.operator == "BMC" {
        if op.operands.len() != 1 || op.operands[0].as_name().is_err() {
            return Err(unsupported("malformed BMC"));
        }
        return Ok(None);
    }
    if op.operands.len() != 2 || op.operands[0].as_name().is_err() {
        return Err(unsupported("malformed BDC"));
    }
    let props = match &op.operands[1] {
        Object::Dictionary(dict) => dict,
        Object::Name(name) => {
            let props = resources
                .get(b"Properties")
                .map_err(|_| unsupported("missing Properties resource"))?;
            dictionary(doc, props)?
                .get(name)
                .map_err(|_| unsupported("unresolved named property"))
                .and_then(|obj| dictionary(doc, obj))?
        }
        _ => {
            return Err(unsupported(
                "BDC properties must be an inline dictionary or resource name",
            ))
        }
    };
    let Ok(value) = props.get(b"ActualText") else {
        return Ok(None);
    };
    if op.operands[0].as_name()? != b"Span" {
        return Err(unsupported(
            "only Span marked-content ActualText is supported",
        ));
    }
    decode_string(doc, value).map(Some)
}

// lopdf 0.42's content lexer does not recognize digits in operators. In
// particular, it parses d1 as d followed by a stray numeric operand. Only this
// leading Type3 declaration is recognized here; the remaining bytes still go
// through the strict content decoder. No PDF bytes or extraction glyphs change.
fn type3_body(mut bytes: &[u8]) -> Result<&[u8], OutputError> {
    fn space(byte: u8) -> bool {
        matches!(byte, 0 | 9 | 10 | 12 | 13 | 32)
    }
    fn number(token: &[u8]) -> bool {
        if token.is_empty() || token.len() > 64 {
            return false;
        }
        let digits = if matches!(token[0], b'+' | b'-') {
            &token[1..]
        } else {
            token
        };
        let mut decimal = false;
        let mut count = 0;
        for byte in digits {
            if byte.is_ascii_digit() {
                count += 1;
            } else if *byte == b'.' && !decimal {
                decimal = true;
            } else {
                return false;
            }
        }
        count > 0
            && std::str::from_utf8(token)
                .ok()
                .and_then(|s| s.parse::<f64>().ok())
                .is_some_and(f64::is_finite)
    }
    let mut operands = 0;
    loop {
        // PDF whitespace and comments can separate declaration tokens. Strings,
        // names and composite objects cannot be numeric width operands.
        loop {
            while bytes.first().is_some_and(|b| space(*b)) {
                bytes = &bytes[1..];
            }
            if bytes.first() != Some(&b'%') {
                break;
            }
            let end = bytes
                .iter()
                .position(|b| matches!(b, b'\r' | b'\n'))
                .unwrap_or(bytes.len());
            bytes = &bytes[end..];
        }
        let end = bytes
            .iter()
            .position(|b| space(*b) || b"()<>[]{}/%".contains(b))
            .unwrap_or(bytes.len());
        let token = &bytes[..end];
        if token == b"d0" || token == b"d1" {
            let expected = if token == b"d0" { 2 } else { 6 };
            if operands != expected {
                return Err(unsupported("invalid leading Type3 width operand count"));
            }
            return Ok(&bytes[end..]);
        }
        if operands >= 6 || !number(token) {
            return Err(unsupported(
                "Type3 audit requires a leading d0/d1 with finite numeric operands",
            ));
        }
        operands += 1;
        bytes = &bytes[end..];
    }
}

#[derive(Default)]
struct Audit {
    found: bool,
    first_problem: Option<String>,
    visits: usize,
    operations: usize,
    decoded_bytes: usize,
    replacement_bytes: usize,
    replacements: usize,
    has_do: bool,
    legacy_unsafe: bool,
    form_edges: Map<ObjectId, Vec<ObjectId>>,
}
impl Audit {
    fn problem(&mut self, message: impl Into<String>) {
        if self.first_problem.is_none() {
            self.first_problem = Some(message.into());
        }
    }
    fn visit(&mut self, object: &Object, depth: usize) -> Result<(), OutputError> {
        self.visits += 1;
        if self.visits > MAX_OBJECT_VISITS || depth > MAX_DEPTH {
            return Err(unsupported("ActualText object audit limit"));
        }
        // Indirect objects are visited once by the outer object-table loop.
        match object {
            Object::Array(items) => {
                for item in items {
                    self.visit(item, depth + 1)?;
                }
            }
            Object::Dictionary(dict) => self.visit_dict(dict, depth)?,
            Object::Stream(stream) => self.visit_dict(&stream.dict, depth)?,
            _ => {}
        }
        Ok(())
    }
    fn visit_dict(&mut self, dict: &Dictionary, depth: usize) -> Result<(), OutputError> {
        if dict.has(b"ActualText") {
            self.found = true;
            if dict
                .get(b"Type")
                .and_then(Object::as_name)
                .is_ok_and(|t| t == b"StructElem" || t == b"StructTreeRoot")
                || dict.has(b"S")
            {
                self.problem("structure-tree ActualText is unsupported");
            }
        }
        for (_, value) in dict.iter() {
            self.visit(value, depth + 1)?;
        }
        Ok(())
    }
    fn bytes(&mut self, amount: usize) -> Result<(), OutputError> {
        self.decoded_bytes = self
            .decoded_bytes
            .checked_add(amount)
            .ok_or_else(|| unsupported("content byte overflow"))?;
        if self.decoded_bytes > MAX_DECODED_BYTES {
            return Err(unsupported("decoded content exceeds 32 MiB audit limit"));
        }
        Ok(())
    }
    fn content(
        &mut self,
        doc: &Document,
        bytes: &[u8],
        resources: &Dictionary,
        form: bool,
        form_id: Option<ObjectId>,
        type3: bool,
    ) -> Result<(), OutputError> {
        let (body, header_error, header_operations) = if type3 {
            match type3_body(bytes) {
                Ok(body) => (body, None, 1),
                // A failed header never becomes an absence claim. If the full
                // strict decoder can still read it, collect positive evidence
                // from its operands before returning the header failure.
                Err(error) => (bytes, Some(error), 0),
            }
        } else {
            (bytes, None, 0)
        };
        let content = Content::decode_strict(body)?;
        self.operations = self
            .operations
            .checked_add(content.operations.len() + header_operations)
            .ok_or_else(|| unsupported("operation count overflow"))?;
        if self.operations > MAX_OPERATIONS {
            return Err(unsupported("ActualText content operation limit"));
        }
        // Collect all parsed positive evidence before validating scopes, so an
        // earlier malformed operator cannot hide a later known replacement.
        for op in &content.operations {
            for value in &op.operands {
                self.visit(value, 0)?;
            }
        }
        if let Some(error) = header_error {
            return Err(error);
        }
        // (replacement scope, covered text bytes); nonreplacement nesting is
        // kept too, so its EMC cannot end an enclosing replacement.
        let mut stack: Vec<(bool, bool, bool)> = Vec::new();
        let mut in_text = false;
        for op in &content.operations {
            if type3 && op.operator == "d" {
                // The decoder cannot distinguish a real dash-setting d from a
                // later malformed d0/d1. Refuse this narrow ambiguous subset.
                return Err(unsupported(
                    "Type3 dash or non-leading width operators are outside the audit subset",
                ));
            }
            if stack.iter().any(|frame| frame.0)
                && !matches!(
                    op.operator.as_str(),
                    "BMC"
                        | "BDC"
                        | "EMC"
                        | "BT"
                        | "ET"
                        | "Tj"
                        | "TJ"
                        | "Tc"
                        | "Tw"
                        | "Tz"
                        | "TL"
                        | "Tf"
                        | "Ts"
                        | "Tm"
                        | "Td"
                        | "TD"
                        | "T*"
                        | "q"
                        | "Q"
                        | "cm"
                        | "gs"
                        | "g"
                        | "G"
                        | "rg"
                        | "RG"
                        | "k"
                        | "K"
                        | "CS"
                        | "cs"
                        | "SC"
                        | "SCN"
                        | "sc"
                        | "scn"
                        | "w"
                        | "J"
                        | "j"
                        | "M"
                        | "d"
                        | "ri"
                        | "i"
                )
            {
                return Err(unsupported(
                    "non-text or unsupported operator inside ActualText",
                ));
            }
            match op.operator.as_str() {
                "BMC" | "BDC" => {
                    if stack.len() >= MAX_DEPTH {
                        return Err(unsupported("marked-content nesting exceeds 64"));
                    }
                    let replacement = replacement(doc, resources, op)?;
                    if let Some(text) = &replacement {
                        self.found = true;
                        if stack.iter().any(|frame| frame.0) {
                            return Err(unsupported(
                                "overlapping nested ActualText replacements are unsupported",
                            ));
                        }
                        self.replacements += 1;
                        self.replacement_bytes = self
                            .replacement_bytes
                            .checked_add(text.len())
                            .ok_or_else(|| unsupported("replacement size overflow"))?;
                        if self.replacement_bytes > MAX_OUTPUT_BYTES {
                            return Err(unsupported("ActualText replacements exceed 2 MiB"));
                        }
                        if form {
                            self.problem(
                                "ActualText in Form/content-resource streams is unsupported",
                            );
                        }
                    }
                    stack.push((replacement.is_some(), false, in_text));
                }
                "EMC" => {
                    if !op.operands.is_empty() {
                        return Err(unsupported("malformed EMC"));
                    }
                    let (actual, text, started_in_text) =
                        stack.pop().ok_or_else(|| unsupported("unbalanced EMC"))?;
                    if in_text != started_in_text {
                        return Err(unsupported("crossing marked-content and text scopes"));
                    }
                    if actual && !text {
                        return Err(unsupported(
                            "ActualText without covered text is unsupported",
                        ));
                    }
                }
                "BT" => {
                    if in_text {
                        return Err(unsupported("nested BT"));
                    }
                    in_text = true;
                }
                "ET" => {
                    if !in_text {
                        return Err(unsupported("unbalanced ET"));
                    }
                    in_text = false;
                }
                "Do" => {
                    self.has_do = true;
                    if let Some(from) = form_id {
                        let name = op
                            .operands
                            .first()
                            .and_then(|o| o.as_name().ok())
                            .ok_or_else(|| unsupported("invalid Form Do"))?;
                        let xobjects = resources
                            .get(b"XObject")
                            .map_err(|_| unsupported("missing Form XObject resources"))?;
                        let target = dictionary(doc, xobjects)?
                            .get(name)
                            .map_err(|_| unsupported("missing Form XObject"))?;
                        if let Object::Reference(id) = target {
                            // The legacy reader dereferences these objects too.
                            let stream = resolve(doc, target)?
                                .as_stream()
                                .map_err(|_| unsupported("invalid Form XObject target"))?;
                            if stream
                                .dict
                                .get(b"Subtype")
                                .and_then(Object::as_name)
                                .is_ok_and(|n| n == b"Form")
                            {
                                let mut target_id = *id;
                                let mut seen = HashSet::new();
                                while let Some(Object::Reference(next)) =
                                    doc.objects.get(&target_id)
                                {
                                    if seen.len() >= MAX_DEPTH || !seen.insert(target_id) {
                                        return Err(unsupported("Form reference cycle"));
                                    }
                                    target_id = *next;
                                }
                                self.form_edges.entry(from).or_default().push(target_id);
                            }
                        }
                    }
                }
                "Tj" | "TJ" if stack.iter().any(|f| f.0) => {
                    if !in_text {
                        return Err(unsupported(
                            "ActualText text-showing operator outside BT/ET",
                        ));
                    }
                    let has_text = if op.operator == "Tj" {
                        if op.operands.len() != 1 {
                            return Err(unsupported("malformed ActualText Tj"));
                        }
                        !op.operands[0]
                            .as_str()
                            .map_err(|_| unsupported("ActualText Tj requires a string"))?
                            .is_empty()
                    } else {
                        if op.operands.len() != 1 {
                            return Err(unsupported("malformed ActualText TJ"));
                        }
                        let values = op.operands[0]
                            .as_array()
                            .map_err(|_| unsupported("ActualText TJ requires an array"))?;
                        let mut nonempty = false;
                        for value in values {
                            match value {
                                Object::String(bytes, _) => nonempty |= !bytes.is_empty(),
                                Object::Integer(_) | Object::Real(_) => {}
                                _ => return Err(unsupported("unsupported ActualText TJ operand")),
                            }
                        }
                        nonempty
                    };
                    if has_text {
                        for frame in &mut stack {
                            if frame.0 {
                                frame.1 = true;
                            }
                        }
                    }
                }
                "'" | "\"" | "BI" | "ID" | "EI" if stack.iter().any(|f| f.0) => {
                    return Err(unsupported("unsupported operator inside ActualText"))
                }
                _ => {}
            }
        }
        if !stack.is_empty() || in_text {
            return Err(unsupported("unclosed marked-content or text scope"));
        }
        Ok(())
    }
}

fn stream_bytes(doc: &Document, value: &Object) -> Result<Vec<u8>, OutputError> {
    let stream = resolve(doc, value)?
        .as_stream()
        .map_err(|_| unsupported("invalid content stream reference"))?;
    if stream.dict.has(b"F") {
        return Err(unsupported("external content streams are unsupported"));
    }
    stream.decompressed_content().map_err(Into::into)
}

fn page_bytes(
    doc: &Document,
    page: &Dictionary,
    audit: &mut Audit,
) -> Result<Vec<u8>, OutputError> {
    let Ok(contents) = page.get(b"Contents") else {
        return Ok(Vec::new());
    };
    let contents = resolve(doc, contents)?;
    let mut joined = Vec::new();
    let values = match contents {
        Object::Array(values) => values.as_slice(),
        _ => std::slice::from_ref(contents),
    };
    for value in values {
        let bytes = stream_bytes(doc, value)?;
        audit.bytes(bytes.len() + 1)?;
        joined.extend_from_slice(&bytes);
        joined.push(b'\n');
    }
    Ok(joined)
}

/// Run legacy extraction unchanged unless the parsed source supplies ActualText.
/// Known unsupported replacement is an error, never glyph-level fallback.
pub fn extract_text_from_mem_with_actual_text(
    buffer: &[u8],
) -> Result<ActualTextExtraction, OutputError> {
    let mut doc = Document::load_mem(buffer)?;
    maybe_decrypt(&mut doc)?;
    let mut audit = Audit::default();
    for object in doc.objects.values() {
        if let Err(error) = audit.visit(object, 0) {
            audit.problem(error.to_string());
            break;
        }
    }
    let empty = Dictionary::new();
    let pages = doc.get_pages();
    if pages.len() > MAX_PAGES {
        audit.problem("page count exceeds 1000 ActualText audit limit");
    }
    for (_, id) in pages.iter().take(MAX_PAGES) {
        let result = (|| {
            let page = doc.get_dictionary(*id)?;
            let resources = match resources(&doc, page, &empty) {
                Ok(resources) => resources,
                Err(error) => {
                    audit.legacy_unsafe = true;
                    return Err(error);
                }
            };
            let bytes = page_bytes(&doc, page, &mut audit)?;
            audit.content(&doc, &bytes, resources, false, None, false)
        })();
        if let Err(error) = result {
            audit.problem(error.to_string());
        }
        if audit.decoded_bytes > MAX_DECODED_BYTES
            || audit.operations > MAX_OPERATIONS
            || audit.visits > MAX_OBJECT_VISITS
        {
            break;
        }
    }
    // Form streams are examined independently, including unused ones. This is
    // conservative, but never rejects an ordinary no-ActualText Form PDF.
    for (id, object) in &doc.objects {
        let Object::Stream(stream) = object else {
            continue;
        };
        let is_form = stream
            .dict
            .get(b"Subtype")
            .and_then(Object::as_name)
            .is_ok_and(|n| n == b"Form");
        let is_pattern = stream.dict.has(b"PatternType");
        if !is_form && !is_pattern {
            continue;
        }
        let result = (|| {
            let resources = match stream.dict.get(b"Resources") {
                Ok(value) => dictionary(&doc, value)?,
                Err(_) => &empty,
            };
            let bytes = stream_bytes(&doc, object)?;
            audit.bytes(bytes.len())?;
            audit.content(&doc, &bytes, resources, true, Some(*id), false)
        })();
        if let Err(error) = result {
            // An incomplete Form audit cannot safely delegate to the legacy
            // recursive interpreter; an invocation cycle may be hidden.
            audit.legacy_unsafe = true;
            audit.problem(error.to_string());
        }
        if audit.decoded_bytes > MAX_DECODED_BYTES
            || audit.operations > MAX_OPERATIONS
            || audit.visits > MAX_OBJECT_VISITS
        {
            audit.legacy_unsafe = true;
            break;
        }
    }
    // Type3 glyph programs are content streams too. Inspect them without
    // interpreting their drawing operators or changing the legacy glyph path.
    for (font_id, object) in &doc.objects {
        let Object::Dictionary(font) = object else {
            continue;
        };
        if !font
            .get(b"Subtype")
            .and_then(Object::as_name)
            .is_ok_and(|n| n == b"Type3")
        {
            continue;
        }
        let result = (|| {
            let charprocs = font
                .get(b"CharProcs")
                .map_err(|_| unsupported("missing Type3 CharProcs"))?;
            let resource_dict = match font.get(b"Resources") {
                Ok(value) => dictionary(&doc, value)?,
                Err(_) => &empty,
            };
            for (_, procedure) in dictionary(&doc, charprocs)?.iter() {
                let result = (|| {
                    let bytes = stream_bytes(&doc, procedure)?;
                    audit.bytes(bytes.len())?;
                    audit.content(&doc, &bytes, resource_dict, true, None, true)
                })();
                result.map_err(|error| {
                    unsupported(format!(
                        "Type3 font object {:?}, CharProc object {:?}: {}",
                        font_id,
                        procedure.as_reference().ok(),
                        error
                    ))
                })?;
            }
            Ok::<(), OutputError>(())
        })();
        if let Err(error) = result {
            audit.problem(error.to_string());
        }
        if audit.decoded_bytes > MAX_DECODED_BYTES
            || audit.operations > MAX_OPERATIONS
            || audit.visits > MAX_OBJECT_VISITS
        {
            break;
        }
    }
    // Refuse proven Form invocation cycles before delegating to the legacy
    // recursive interpreter, including files without ActualText. This is a
    // parser error, not evidence that replacement text was present.
    fn visit_form(
        id: ObjectId,
        edges: &Map<ObjectId, Vec<ObjectId>>,
        active: &mut HashSet<ObjectId>,
        done: &mut HashSet<ObjectId>,
    ) -> Result<(), OutputError> {
        if done.contains(&id) {
            return Ok(());
        }
        if active.len() >= MAX_DEPTH || !active.insert(id) {
            return Err(OutputError::PdfError(lopdf::Error::ReferenceLimit));
        }
        if let Some(next) = edges.get(&id) {
            for id in next {
                visit_form(*id, edges, active, done)?;
            }
        }
        active.remove(&id);
        done.insert(id);
        Ok(())
    }
    let mut done = HashSet::new();
    for id in audit.form_edges.keys() {
        if let Err(error) = visit_form(*id, &audit.form_edges, &mut HashSet::new(), &mut done) {
            return Err(if audit.found {
                unsupported("Form invocation cycle or depth limit")
            } else {
                error
            });
        }
    }
    if audit.found {
        if audit.has_do {
            audit.problem(
                "ActualText documents containing XObject Do are outside the supported subset",
            );
        }
        if audit.replacements == 0 {
            audit.problem("ActualText outside supported page marked content");
        }
        if let Some(problem) = audit.first_problem {
            return Err(unsupported(problem));
        }
        let mut writer = BoundedString(String::new());
        {
            let plain = PlainTextOutput::new(&mut writer);
            let mut output = ReplacementOutput::new(plain);
            let mut processor = Processor::new();
            processor.actual_text = true;
            for (number, id) in pages {
                output_doc_inner(number, id, &doc, &mut processor, &mut output, &empty).map_err(
                    |error| {
                        unsupported(format!("checked replacement extraction failed: {}", error))
                    },
                )?;
            }
        }
        Ok(ActualTextExtraction {
            text: writer.0,
            status: ActualTextStatus::Applied,
            diagnostic: None,
        })
    } else {
        if audit.legacy_unsafe {
            return Err(OutputError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "PDF resource/Form audit cannot safely delegate: {}",
                    audit.first_problem.as_deref().unwrap_or("incomplete audit")
                ),
            )));
        }
        let mut text = String::new();
        output_doc(&doc, &mut PlainTextOutput::new(&mut text))?;
        let status = if audit.first_problem.is_some() {
            ActualTextStatus::Unknown
        } else {
            ActualTextStatus::NotDetected
        };
        Ok(ActualTextExtraction {
            text,
            status,
            diagnostic: audit.first_problem,
        })
    }
}

struct BoundedString(String);
impl fmt::Write for BoundedString {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if value.len() > MAX_OUTPUT_BYTES.saturating_sub(self.0.len()) {
            return Err(fmt::Error);
        }
        self.0.push_str(value);
        Ok(())
    }
}
impl<'a> ConvertToFmt for &'a mut BoundedString {
    type Writer = &'a mut BoundedString;
    fn convert(self) -> Self::Writer {
        self
    }
}

#[derive(Clone)]
struct Geometry {
    transform: Transform,
    width: f64,
    spacing: f64,
    font_size: f64,
}
struct Run {
    text: String,
    first: Option<Geometry>,
    last: Option<Geometry>,
}
struct ReplacementOutput<W: ConvertToFmt> {
    inner: PlainTextOutput<W>,
    frames: Vec<bool>,
    run: Option<Run>,
}
impl<W: ConvertToFmt> ReplacementOutput<W> {
    fn new(inner: PlainTextOutput<W>) -> Self {
        Self {
            inner,
            frames: Vec::new(),
            run: None,
        }
    }
}
impl<W: ConvertToFmt> OutputDev for ReplacementOutput<W> {
    fn begin_page(
        &mut self,
        page: u32,
        media: &MediaBox,
        art: Option<ArtBox>,
    ) -> Result<(), OutputError> {
        self.inner.begin_page(page, media, art)
    }
    fn end_page(&mut self) -> Result<(), OutputError> {
        if !self.frames.is_empty() || self.run.is_some() {
            return Err(unsupported("unclosed replacement output scope"));
        }
        self.inner.end_page()
    }
    fn begin_marked_content(&mut self, replacement: Option<&str>) -> Result<(), OutputError> {
        if self.frames.len() >= MAX_DEPTH {
            return Err(unsupported("replacement output depth limit"));
        }
        let owns = replacement.is_some() && self.run.is_none();
        if owns {
            self.run = Some(Run {
                text: replacement.unwrap().to_owned(),
                first: None,
                last: None,
            });
        }
        self.frames.push(owns);
        Ok(())
    }
    fn end_marked_content(&mut self) -> Result<(), OutputError> {
        let owns = self
            .frames
            .pop()
            .ok_or_else(|| unsupported("replacement output scope underflow"))?;
        if owns {
            let run = self
                .run
                .take()
                .ok_or_else(|| unsupported("missing replacement output state"))?;
            let first = run
                .first
                .ok_or_else(|| unsupported("ActualText has no decodable covered glyph"))?;
            let last = run.last.unwrap();
            // Empty replacement means no emitted text, including no synthetic
            // layout separator. Still retain its covered glyph geometry below.
            if !run.text.is_empty() {
                self.inner.first_char = true;
                self.inner.output_character(
                    &first.transform,
                    first.width,
                    first.spacing,
                    first.font_size,
                    &run.text,
                )?;
            }
            let position = last.transform.post_transform(&self.inner.flip_ctm);
            let size = last
                .transform
                .transform_vector(vec2(last.font_size, last.font_size));
            let font_size = (size.x * size.y).sqrt();
            self.inner.last_y = position.m32;
            self.inner.last_end = position.m31 + last.width * font_size;
            self.inner.first_char = false;
        }
        Ok(())
    }
    fn output_character(
        &mut self,
        transform: &Transform,
        width: f64,
        spacing: f64,
        font_size: f64,
        text: &str,
    ) -> Result<(), OutputError> {
        if let Some(run) = &mut self.run {
            if ![
                transform.m11,
                transform.m12,
                transform.m21,
                transform.m22,
                transform.m31,
                transform.m32,
                width,
                spacing,
                font_size,
            ]
            .iter()
            .all(|v| v.is_finite())
            {
                return Err(unsupported("nonfinite ActualText glyph geometry"));
            }
            let size = transform.transform_vector(vec2(font_size, font_size));
            if !(size.x * size.y).sqrt().is_finite() {
                return Err(unsupported("unsupported ActualText glyph orientation"));
            }
            let geometry = Geometry {
                transform: *transform,
                width,
                spacing,
                font_size,
            };
            if run.first.is_none() {
                run.first = Some(geometry.clone());
            }
            run.last = Some(geometry);
            Ok(())
        } else {
            self.inner
                .output_character(transform, width, spacing, font_size, text)
        }
    }
    fn begin_word(&mut self) -> Result<(), OutputError> {
        if self.run.is_none() {
            self.inner.begin_word()?;
        }
        Ok(())
    }
    fn end_word(&mut self) -> Result<(), OutputError> {
        if self.run.is_none() {
            self.inner.end_word()?;
        }
        Ok(())
    }
    fn end_line(&mut self) -> Result<(), OutputError> {
        if self.run.is_none() {
            self.inner.end_line()?;
        }
        Ok(())
    }
}
