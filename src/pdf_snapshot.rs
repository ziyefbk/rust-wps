use crate::Inspection;
use anyhow::{Context, Result, ensure};
use std::{io::Read, time::Instant};

const MAX_INPUT_BYTES: usize = 64 * 1024 * 1024;

// Accept an already-open reader, never a pathname: validation and parsing must
// observe exactly the same bounded bytes even if the directory entry changes.
// This is not an atomic filesystem snapshot of concurrent in-place writes, nor
// a decompression/parser resource sandbox or a later PDF.js session identity.
pub(super) fn inspect(
    reader: impl Read,
    allow_text_failure: bool,
    started: Instant,
) -> Result<Inspection> {
    let source = read_source(reader)?;
    let bytes = source.len() as u64;
    let mut text_error = None;
    let text = match pdf_extract::extract_text_from_mem(&source) {
        Ok(text) => text,
        Err(error) if allow_text_failure => {
            text_error = Some(format!("{error}"));
            String::new()
        }
        Err(error) => {
            return Err(error)
                .context("PDF parser failed (encrypted or malformed PDF may be unsupported)");
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
    Ok(Inspection {
        format: "pdf".into(),
        backend: "pdf-extract 0.12.1".into(),
        bytes,
        elapsed_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        text,
        warnings,
        text_error,
    })
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
    };

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
