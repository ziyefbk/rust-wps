use crate::{Inspection, PdfSourceIdentity};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    any::Any,
    io::Read,
    panic::{UnwindSafe, catch_unwind},
    time::Instant,
};

const MAX_INPUT_BYTES: usize = 64 * 1024 * 1024;
const MAX_PANIC_DETAIL_BYTES: usize = 512;

// Accept an already-open reader, never a pathname: validation and parsing must
// observe exactly the same bounded bytes even if the directory entry changes.
// This is not an atomic filesystem snapshot of concurrent in-place writes, nor
// a decompression/parser resource sandbox.
pub(super) fn inspect(
    reader: impl Read,
    allow_text_failure: bool,
    started: Instant,
) -> Result<Inspection> {
    inspect_with_identity(reader, allow_text_failure, started).map(|(report, _)| report)
}

pub(super) fn inspect_with_identity(
    reader: impl Read,
    allow_text_failure: bool,
    started: Instant,
) -> Result<(Inspection, PdfSourceIdentity)> {
    inspect_identity_with_extractor(reader, allow_text_failure, started, |source| {
        pdf_extract::extract_text_from_mem(source).map_err(Into::into)
    })
}

#[cfg(test)]
fn inspect_with_extractor(
    reader: impl Read,
    allow_text_failure: bool,
    started: Instant,
    extract: impl FnOnce(&[u8]) -> Result<String> + UnwindSafe,
) -> Result<Inspection> {
    inspect_identity_with_extractor(reader, allow_text_failure, started, extract)
        .map(|(report, _)| report)
}

fn inspect_identity_with_extractor(
    reader: impl Read,
    allow_text_failure: bool,
    started: Instant,
    extract: impl FnOnce(&[u8]) -> Result<String> + UnwindSafe,
) -> Result<(Inspection, PdfSourceIdentity)> {
    // Read/validation failures (including reader panics) are not extraction
    // failures and must never turn into preview candidates.
    let source = read_source(reader)?;
    let identity = source_identity(&source);
    let bytes = identity.bytes;
    let mut text_error = None;
    // Only the extractor sees this boundary: immutable validated bytes and
    // locally owned parser state. No shared mutable state is asserted safe.
    // The normal panic hook may still report to stderr. This catches unwinds,
    // not aborts, OOM, stack overflows, hangs, or excessive parser resources.
    let text = match catch_unwind(|| extract(&source)) {
        Ok(Ok(text)) => text,
        Ok(Err(error)) if allow_text_failure => {
            text_error = Some(format!("{error}"));
            String::new()
        }
        Ok(Err(error)) => {
            return Err(error)
                .context("PDF parser failed (encrypted or malformed PDF may be unsupported)");
        }
        Err(payload) => {
            let message = panic_message(payload.as_ref());
            ensure!(allow_text_failure, "{message}");
            text_error = Some(message);
            String::new()
        }
    };
    let mut warnings = vec![
        "Text order and Chinese font mappings require verification; this is not a page renderer"
            .into(),
        "Scanned PDF requires OCR, which is not implemented".into(),
    ];
    if text.trim().is_empty() {
        warnings.push("No text extracted; this does not prove the document is empty".into());
    }
    Ok((
        Inspection {
            format: "pdf".into(),
            backend: "pdf-extract 0.12.1".into(),
            bytes,
            elapsed_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
            text,
            warnings,
            text_error,
        },
        identity,
    ))
}

fn source_identity(source: &[u8]) -> PdfSourceIdentity {
    PdfSourceIdentity {
        bytes: source.len() as u64,
        sha256: Sha256::digest(source).into(),
    }
}

pub(super) fn read_verified(reader: impl Read, expected: &PdfSourceIdentity) -> Result<Vec<u8>> {
    let source = read_source(reader).context("PDF 源文件无法验证，请重新打开文档后再预览")?;
    ensure!(
        source_identity(&source) == *expected,
        "PDF 源文件已改变，请重新打开文档后再预览"
    );
    Ok(source)
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str));
    let Some(message) = message else {
        return "PDF text extractor panicked: non-string panic payload".into();
    };
    // Escape control/formatting characters and bound both work and output.
    // Never invoke an unknown payload's Debug/Display implementation.
    let mut detail = String::new();
    for character in message.chars() {
        let escaped = character.escape_debug().to_string();
        if detail.len() + escaped.len() > MAX_PANIC_DETAIL_BYTES {
            detail.push('…');
            break;
        }
        detail.push_str(&escaped);
    }
    format!("PDF text extractor panicked: {detail}")
}

fn read_source(reader: impl Read) -> Result<Vec<u8>> {
    let mut source = Vec::new();
    reader
        .take(MAX_INPUT_BYTES as u64 + 1)
        .read_to_end(&mut source)?;
    ensure!(
        source.len() <= MAX_INPUT_BYTES,
        "Exploration limit: input must be at most 64 MiB"
    );
    ensure!(
        source.starts_with(b"%PDF-"),
        "File signature does not match .pdf; RTF/HTML disguised as DOC are not supported yet"
    );
    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs::{self, File, OpenOptions},
        io::{self, Cursor, Write},
        sync::atomic::{AtomicBool, Ordering},
    };

    #[test]
    fn identity_has_known_sha256_and_actual_length() {
        let identity = source_identity(b"%PDF-");
        assert_eq!(identity.bytes, 5);
        assert_eq!(
            identity.sha256,
            [
                56, 82, 60, 8, 119, 150, 229, 213, 221, 28, 249, 186, 209, 251, 2, 103, 129, 168,
                56, 221, 157, 210, 207, 138, 245, 139, 159, 101, 2, 164, 103, 120
            ]
        );
        assert_eq!(
            read_verified(b"%PDF-".as_slice(), &identity).unwrap(),
            b"%PDF-"
        );
    }

    #[test]
    fn extraction_identity_uses_exact_validated_buffer() {
        let source = pdf("Identity extraction original");
        let (report, identity) =
            inspect_identity_with_extractor(source.as_slice(), true, Instant::now(), |bytes| {
                assert_eq!(bytes, source);
                Ok("exact text".into())
            })
            .unwrap();
        assert_eq!(identity, source_identity(&source));
        assert_eq!(identity.bytes, report.bytes);
        assert_eq!(report.text, "exact text");
        let keys = serde_json::to_value(report).unwrap();
        assert!(keys.get("sha256").is_none());
        assert!(keys.get("pdf_identity").is_none());
    }

    #[test]
    fn preview_identity_survives_returned_error_and_extractor_unwind() {
        let source = pdf("Failed extractor identity");
        for extract in [
            ((|_: &[u8]| Err(anyhow::anyhow!("encrypted or malformed")))
                as fn(&[u8]) -> Result<String>),
            |_: &[u8]| panic!("identity panic"),
        ] {
            let (report, identity) =
                inspect_identity_with_extractor(source.as_slice(), true, Instant::now(), extract)
                    .unwrap();
            assert!(report.text.is_empty());
            assert!(report.text_error.is_some());
            assert_eq!(identity, source_identity(&source));
            assert!(
                inspect_identity_with_extractor(source.as_slice(), false, Instant::now(), extract)
                    .is_err()
            );
        }
    }

    #[test]
    fn identity_stays_with_open_handle_after_path_replacement_or_removal() {
        for remove in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("source.pdf");
            let a = pdf("A original");
            let b = pdf("B replacement");
            fs::write(&path, &a).unwrap();
            let file = File::open(&path).unwrap();
            fs::rename(&path, dir.path().join("original.pdf")).unwrap();
            if !remove {
                fs::write(&path, b).unwrap();
            }
            let (report, identity) = inspect_with_identity(file, true, Instant::now()).unwrap();
            assert_eq!(identity, source_identity(&a));
            assert!(report.text.contains("A original"));
        }
    }

    #[test]
    fn delivery_rejects_different_and_same_length_bytes_without_updating_identity() {
        let a = pdf("Original A");
        let b = pdf("Original B");
        assert_eq!(a.len(), b.len());
        let identity = source_identity(&a);
        for changed in [&b, &pdf("Different length replacement")] {
            assert!(
                read_verified(changed.as_slice(), &identity)
                    .unwrap_err()
                    .to_string()
                    .contains("重新打开")
            );
        }
        assert_eq!(read_verified(a.as_slice(), &identity).unwrap(), a);
    }

    #[test]
    fn public_preview_and_delivery_reject_replacement_then_accept_explicit_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.pdf");
        let a = pdf("A original");
        let b = pdf("B replacement");
        fs::write(&path, &a).unwrap();
        let preview = crate::inspect_for_preview_with_identity(&path).unwrap();
        let identity = preview.pdf_identity.unwrap();
        fs::write(&path, &b).unwrap();
        assert!(crate::read_verified_pdf(&path, &identity).is_err());
        let next = crate::inspect_for_preview_with_identity(&path).unwrap();
        let bytes = crate::read_verified_pdf(&path, &next.pdf_identity.unwrap()).unwrap();
        fs::write(&path, &a).unwrap();
        assert_eq!(bytes, b); // later path writes cannot change already verified delivery
        assert_eq!(crate::read_verified_pdf(&path, &identity).unwrap(), a); // A -> B -> A is allowed
    }

    #[test]
    fn verified_delivery_rejects_missing_io_signature_length_and_limit_failures() {
        let identity = source_identity(b"%PDF-valid");
        assert!(read_verified(FailingReader, &identity).is_err());
        assert!(read_verified(b"not PDF".as_slice(), &identity).is_err());
        assert!(read_verified(b"%PDF-".as_slice(), &identity).is_err());
        assert!(
            format!(
                "{:#}",
                read_verified(CountedInfiniteReader { bytes_read: 0 }, &identity).unwrap_err()
            )
            .contains("64 MiB")
        );
        let dir = tempfile::tempdir().unwrap();
        assert!(crate::read_verified_pdf(&dir.path().join("missing.pdf"), &identity).is_err());
    }

    #[test]
    fn exact_limit_identity_and_delivery_do_not_require_parsing_or_second_buffer() {
        let mut bytes = vec![b' '; MAX_INPUT_BYTES];
        bytes[..5].copy_from_slice(b"%PDF-");
        let identity = source_identity(&bytes);
        let delivered = read_verified(bytes.as_slice(), &identity).unwrap();
        assert_eq!(delivered.len(), MAX_INPUT_BYTES);
        assert_eq!(delivered, bytes);
    }

    fn check_panic_policy(extract: fn(&[u8]) -> Result<String>, detail: &str) {
        let source = pdf("No partial text may escape");
        let expected = format!("PDF text extractor panicked: {detail}");
        let strict =
            inspect_with_extractor(source.as_slice(), false, Instant::now(), extract).unwrap_err();
        assert_eq!(strict.to_string(), expected);
        let preview =
            inspect_with_extractor(source.as_slice(), true, Instant::now(), extract).unwrap();
        assert!(preview.text.is_empty());
        assert_eq!(preview.bytes, source.len() as u64);
        assert_eq!(preview.text_error.as_deref(), Some(expected.as_str()));
        assert!(
            preview
                .warnings
                .iter()
                .any(|w| w.starts_with("No text extracted"))
        );
        let json = serde_json::to_value(preview).unwrap();
        assert_eq!(json["text"], "");
        assert_eq!(json["text_error"], expected);
        assert_eq!(json["bytes"], source.len());
    }

    #[test]
    fn borrowed_string_extractor_panic_has_explicit_strict_and_preview_policies() {
        check_panic_policy(
            |_| panic!("injected borrowed panic"),
            "injected borrowed panic",
        );
    }

    #[test]
    fn owned_string_extractor_panic_has_explicit_strict_and_preview_policies() {
        check_panic_policy(
            |_| std::panic::panic_any(String::from("injected owned panic")),
            "injected owned panic",
        );
    }

    #[test]
    fn nonstring_extractor_panic_uses_generic_diagnostic() {
        check_panic_policy(
            |_| std::panic::panic_any(123_u32),
            "non-string panic payload",
        );
    }

    #[test]
    fn panic_diagnostics_escape_controls_and_bound_utf8_detail() {
        let prefix = "PDF text extractor panicked: ";
        let raw = "a\n\r\t\0\u{1b}\u{202e}\u{2066}b";
        let diagnostic = panic_message(&raw);
        assert_eq!(
            diagnostic,
            format!("{prefix}a\\n\\r\\t\\0\\u{{1b}}\\u{{202e}}\\u{{2066}}b")
        );
        assert!(!diagnostic.chars().any(char::is_control));
        for raw in ["x".repeat(10_000), "中".repeat(10_000), "\n".repeat(10_000)] {
            let diagnostic = panic_message(&raw);
            assert!(diagnostic.starts_with(prefix));
            assert!(diagnostic.ends_with('…'));
            assert!(diagnostic.len() <= prefix.len() + MAX_PANIC_DETAIL_BYTES + '…'.len_utf8());
        }
        assert_eq!(panic_message(&""), prefix);
        assert_eq!(
            panic_message(&"x".repeat(MAX_PANIC_DETAIL_BYTES)),
            format!("{prefix}{}", "x".repeat(MAX_PANIC_DETAIL_BYTES))
        );
    }

    #[test]
    fn callable_seam_preserves_success_blank_text_and_returned_errors() {
        let source = pdf("Validated source");
        for allow_text_failure in [false, true] {
            for text in ["exact \n text\u{301}", "", " \n "] {
                let report = inspect_with_extractor(
                    source.as_slice(),
                    allow_text_failure,
                    Instant::now(),
                    |validated| {
                        assert_eq!(validated, source);
                        Ok(text.into())
                    },
                )
                .unwrap();
                assert_eq!(report.text, text);
                assert!(report.text_error.is_none());
                assert!(
                    serde_json::to_value(report)
                        .unwrap()
                        .get("text_error")
                        .is_none()
                );
            }
            let result = inspect_with_extractor(
                source.as_slice(),
                allow_text_failure,
                Instant::now(),
                |_| Err(anyhow::anyhow!("injected returned error")),
            );
            if allow_text_failure {
                let report = result.unwrap();
                assert!(report.text.is_empty());
                assert_eq!(
                    report.text_error.as_deref(),
                    Some("injected returned error")
                );
            } else {
                let error = result.unwrap_err();
                assert_eq!(
                    error.to_string(),
                    "PDF parser failed (encrypted or malformed PDF may be unsupported)"
                );
                assert_eq!(error.root_cause().to_string(), "injected returned error");
            }
        }
    }

    #[test]
    fn invalid_signature_never_calls_extractor_in_either_policy() {
        for allow_text_failure in [false, true] {
            let called = AtomicBool::new(false);
            let error = inspect_with_extractor(
                b"not PDF".as_slice(),
                allow_text_failure,
                Instant::now(),
                |_| {
                    called.store(true, Ordering::Relaxed);
                    Ok("must not be called".into())
                },
            )
            .unwrap_err();
            assert!(error.to_string().contains("File signature"));
            assert!(!called.load(Ordering::Relaxed));
        }
    }

    #[test]
    fn oversized_source_never_calls_extractor_in_either_policy() {
        for allow_text_failure in [false, true] {
            let called = AtomicBool::new(false);
            let error = inspect_with_extractor(
                CountedInfiniteReader { bytes_read: 0 },
                allow_text_failure,
                Instant::now(),
                |_| {
                    called.store(true, Ordering::Relaxed);
                    Ok("must not be called".into())
                },
            )
            .unwrap_err();
            assert!(error.to_string().contains("64 MiB"));
            assert!(!called.load(Ordering::Relaxed));
        }
    }

    #[test]
    fn read_error_never_calls_extractor_in_either_policy() {
        for allow_text_failure in [false, true] {
            let called = AtomicBool::new(false);
            let reader = Cursor::new(pdf("Valid prefix")).chain(FailingReader);
            let error = inspect_with_extractor(reader, allow_text_failure, Instant::now(), |_| {
                called.store(true, Ordering::Relaxed);
                Ok("must not be called".into())
            })
            .unwrap_err();
            assert_eq!(error.to_string(), "injected PDF read failure");
            assert!(!called.load(Ordering::Relaxed));
        }
    }

    #[test]
    fn panicking_reader_is_not_mislabeled_as_an_extractor_failure() {
        struct PanickingReader;
        impl Read for PanickingReader {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                panic!("injected reader panic");
            }
        }
        for allow_text_failure in [false, true] {
            let called = AtomicBool::new(false);
            let panic = catch_unwind(|| {
                inspect_with_extractor(PanickingReader, allow_text_failure, Instant::now(), |_| {
                    called.store(true, Ordering::Relaxed);
                    Ok("must not be called".into())
                })
            })
            .unwrap_err();
            assert_eq!(panic.downcast_ref::<&str>(), Some(&"injected reader panic"));
            assert!(!called.load(Ordering::Relaxed));
        }
    }

    #[test]
    fn normal_extraction_after_a_caught_panic_has_no_stale_state() {
        check_panic_policy(|_| panic!("failed extraction"), "failed extraction");
        let source = pdf("Fresh successful extraction");
        let report = inspect(source.as_slice(), true, Instant::now()).unwrap();
        assert!(report.text.contains("Fresh successful extraction"));
        assert!(!report.text.contains("No partial text may escape"));
        assert!(report.text_error.is_none());
    }

    // Independent hand-assembled PDF, with exact xref offsets and no parser API.
    fn pdf(text: &str) -> Vec<u8> {
        let stream = format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET");
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".into(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".into(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
            format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        ];
        let mut source = "%PDF-1.4\n".to_owned();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(source.len());
            source.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
        }
        let xref = source.len();
        source.push_str("xref\n0 6\n0000000000 65535 f \n");
        for offset in offsets {
            source.push_str(&format!("{offset:010} 00000 n \n"));
        }
        source.push_str(&format!(
            "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
        ));
        source.into_bytes()
    }

    #[test]
    fn opened_file_remains_the_source_after_path_replacement() {
        for allow_text_failure in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("source.pdf");
            let moved = dir.path().join("original.pdf");
            let original = pdf("Opened original A");
            let replacement = pdf("Replacement B has different text and length");
            fs::write(&path, &original).unwrap();
            let file = File::open(&path).unwrap();
            fs::rename(&path, &moved).unwrap();
            fs::write(&path, &replacement).unwrap();
            let report = inspect(file, allow_text_failure, Instant::now()).unwrap();
            assert!(report.text.contains("Opened original A"));
            assert!(!report.text.contains("Replacement B"));
            assert_eq!(report.bytes, original.len() as u64);
            assert!(report.text_error.is_none());
            assert_eq!(fs::read(moved).unwrap(), original);
            assert_eq!(fs::read(path).unwrap(), replacement);
        }
    }

    #[test]
    fn opened_file_remains_readable_after_path_removal() {
        for allow_text_failure in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("source.pdf");
            let original = pdf("Unlinked original");
            fs::write(&path, &original).unwrap();
            let file = File::open(&path).unwrap();
            fs::remove_file(&path).unwrap();
            let report = inspect(file, allow_text_failure, Instant::now()).unwrap();
            assert!(report.text.contains("Unlinked original"));
            assert_eq!(report.bytes, original.len() as u64);
            assert!(!path.exists());
        }
    }

    #[test]
    fn opened_source_growth_uses_actual_snapshot_length() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.pdf");
        let mut source = pdf("Growing original");
        fs::write(&path, &source).unwrap();
        let file = File::open(&path).unwrap();
        assert_eq!(file.metadata().unwrap().len(), source.len() as u64);
        let padding = b"\n% harmless trailing PDF comment\n";
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(padding)
            .unwrap();
        source.extend_from_slice(padding);
        let report = inspect(file, false, Instant::now()).unwrap();
        assert!(report.text.contains("Growing original"));
        assert_eq!(report.bytes, source.len() as u64);
        assert_eq!(fs::read(path).unwrap(), source);
    }

    #[test]
    fn opened_source_shrink_uses_actual_snapshot_length() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.pdf");
        let source = pdf("Shrinking original");
        fs::write(&path, &source).unwrap();
        let writer = OpenOptions::new().write(true).open(&path).unwrap();
        writer.set_len(MAX_INPUT_BYTES as u64 + 1).unwrap();
        let file = File::open(&path).unwrap();
        assert_eq!(file.metadata().unwrap().len(), MAX_INPUT_BYTES as u64 + 1);
        writer.set_len(source.len() as u64).unwrap();
        let report = inspect(file, false, Instant::now()).unwrap();
        assert!(report.text.contains("Shrinking original"));
        assert_eq!(report.bytes, source.len() as u64);
        assert_eq!(fs::read(path).unwrap(), source);
    }

    #[test]
    fn opened_source_growth_past_limit_is_rejected_in_both_policies() {
        for allow_text_failure in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("source.pdf");
            let source = pdf("Small original");
            fs::write(&path, &source).unwrap();
            let file = File::open(&path).unwrap();
            assert_eq!(file.metadata().unwrap().len(), source.len() as u64);
            OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .set_len(MAX_INPUT_BYTES as u64 + 1)
                .unwrap();
            let error = inspect(file, allow_text_failure, Instant::now()).unwrap_err();
            assert!(error.to_string().contains("64 MiB"));
            assert_eq!(
                fs::metadata(path).unwrap().len(),
                MAX_INPUT_BYTES as u64 + 1
            );
        }
    }

    #[test]
    fn signature_is_checked_on_the_bytes_actually_read() {
        for allow_text_failure in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("source.pdf");
            fs::write(&path, pdf("Initially valid")).unwrap();
            let file = File::open(&path).unwrap();
            fs::write(&path, b"not a PDF any more").unwrap();
            let error = inspect(file, allow_text_failure, Instant::now()).unwrap_err();
            assert!(error.to_string().contains("File signature"));
            assert_eq!(fs::read(path).unwrap(), b"not a PDF any more");
        }
    }

    struct CountedInfiniteReader {
        bytes_read: usize,
    }

    impl Read for CountedInfiniteReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            for (offset, byte) in buffer.iter_mut().enumerate() {
                *byte = b"%PDF-"
                    .get(self.bytes_read + offset)
                    .copied()
                    .unwrap_or(b' ');
            }
            self.bytes_read += buffer.len();
            Ok(buffer.len())
        }
    }

    #[test]
    fn reader_never_consumes_more_than_limit_plus_one_byte() {
        let mut reader = CountedInfiniteReader { bytes_read: 0 };
        let error = read_source(&mut reader).unwrap_err();
        assert!(error.to_string().contains("64 MiB"));
        assert_eq!(reader.bytes_read, MAX_INPUT_BYTES + 1);
    }

    #[test]
    fn exactly_the_input_limit_is_accepted_by_snapshot_validation() {
        let mut reader = CountedInfiniteReader { bytes_read: 0 };
        let source = read_source((&mut reader).take(MAX_INPUT_BYTES as u64)).unwrap();
        assert_eq!(source.len(), MAX_INPUT_BYTES);
        assert_eq!(reader.bytes_read, MAX_INPUT_BYTES);
        assert!(source.starts_with(b"%PDF-"));
        // Signature and length do not prove this synthetic buffer is a valid PDF.
    }

    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("injected PDF read failure"))
        }
    }

    #[test]
    fn read_errors_after_a_valid_prefix_are_not_preview_candidates() {
        for allow_text_failure in [false, true] {
            let reader = Cursor::new(pdf("Valid prefix")).chain(FailingReader);
            let error = inspect(reader, allow_text_failure, Instant::now()).unwrap_err();
            assert_eq!(error.to_string(), "injected PDF read failure");
        }
    }

    #[test]
    fn short_reads_do_not_require_a_whole_signature_in_the_first_read() {
        struct ShortReader(Cursor<Vec<u8>>);
        impl Read for ShortReader {
            fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
                let limit = buffer.len().min(1);
                self.0.read(&mut buffer[..limit])
            }
        }
        let source = pdf("One byte at a time");
        let report = inspect(
            ShortReader(Cursor::new(source.clone())),
            false,
            Instant::now(),
        )
        .unwrap();
        assert!(report.text.contains("One byte at a time"));
        assert_eq!(report.bytes, source.len() as u64);
    }

    #[test]
    fn empty_or_partial_signatures_are_rejected_under_both_policies() {
        for source in [b"".as_slice(), b"%", b"%PDF", b"not PDF", b" %PDF-"] {
            for allow_text_failure in [false, true] {
                let error = inspect(source, allow_text_failure, Instant::now()).unwrap_err();
                assert!(error.to_string().contains("File signature"));
            }
        }
    }

    #[test]
    fn malformed_and_encrypted_snapshots_preserve_strict_and_preview_policies() {
        for source in [
            b"%PDF-1.7\ninvalid".as_slice(),
            include_bytes!("../tests/fixtures/encrypted.pdf").as_slice(),
            include_bytes!("../tests/fixtures/encrypted-text.pdf").as_slice(),
        ] {
            let strict = inspect(source, false, Instant::now()).unwrap_err();
            assert!(strict.to_string().starts_with("PDF parser failed"));
            let preview = inspect(source, true, Instant::now()).unwrap();
            assert!(preview.text.is_empty());
            assert!(
                preview
                    .text_error
                    .as_ref()
                    .is_some_and(|error| !error.is_empty())
            );
            assert_eq!(preview.bytes, source.len() as u64);
            assert!(
                preview
                    .warnings
                    .iter()
                    .any(|warning| warning.starts_with("No text extracted"))
            );
        }
    }

    #[test]
    fn a_valid_blank_pdf_is_not_an_extraction_failure() {
        let source = pdf("");
        for allow_text_failure in [false, true] {
            let report = inspect(source.as_slice(), allow_text_failure, Instant::now()).unwrap();
            assert!(report.text.trim().is_empty());
            assert!(report.text_error.is_none());
            assert_eq!(report.bytes, source.len() as u64);
            assert!(
                report
                    .warnings
                    .iter()
                    .any(|warning| warning.starts_with("No text extracted"))
            );
        }
    }
}
