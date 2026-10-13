use std::{fs, io::Write};
use zip::{ZipWriter, write::SimpleFileOptions};

#[test]
fn docx_unicode_table_and_source_preservation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("中文.DOCX");
    let mut zip = ZipWriter::new(fs::File::create(&path).unwrap());
    for (name, text) in [
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        (
            "word/document.xml",
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>你好 Rust &amp; Office</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>单元格</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#,
        ),
    ] {
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    let before = fs::read(&path).unwrap();
    let report = rust_wps::inspect(&path).unwrap();
    assert_eq!(report.format, "docx");
    assert!(
        report.text.contains("你好 Rust & Office"),
        "{}",
        report.text
    );
    assert!(report.text.contains("单元格"));
    assert_eq!(before, fs::read(&path).unwrap());
}

#[test]
fn rejects_disguised_and_corrupt_files() {
    let dir = tempfile::tempdir().unwrap();
    for (name, bytes) in [
        ("fake.doc", b"{\\rtf1 text}".as_slice()),
        (
            "broken.doc",
            &[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1],
        ),
        ("broken.docx", b"PK\x03\x04invalid"),
        ("broken.pdf", b"%PDF-1.7\ninvalid"),
        ("future.xlsx", b"PK\x03\x04invalid"),
        ("empty.docx", b""),
    ] {
        let path = dir.path().join(name);
        fs::write(&path, bytes).unwrap();
        assert!(rust_wps::inspect(&path).is_err(), "{name}");
    }
    assert!(rust_wps::inspect(&dir.path().join("missing.docx")).is_err());
}

// Minimal PDF fixture assembled with correct xref offsets, independent of the parser.
#[test]
fn reads_pdf_text_and_preserves_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("text.pdf");
    let stream = "BT /F1 12 Tf 72 720 Td (Hello Rust PDF) Tj ET";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
    ];
    let mut pdf = "%PDF-1.4\n".to_string();
    let mut offsets = vec![0];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
    }
    let xref = pdf.len();
    pdf.push_str("xref\n0 6\n0000000000 65535 f \n");
    for offset in &offsets[1..] {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
    ));
    fs::write(&path, &pdf).unwrap();
    let report = rust_wps::inspect(&path).unwrap();
    assert!(report.text.contains("Hello Rust PDF"), "{}", report.text);
    assert_eq!(fs::read_to_string(&path).unwrap(), pdf);
}

#[test]
fn enforces_input_size_limit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("huge.docx");
    fs::File::create(&path)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    assert!(
        rust_wps::inspect(&path)
            .unwrap_err()
            .to_string()
            .contains("64 MiB")
    );
}

#[test]
fn pdf_text_failure_is_a_preview_candidate_but_not_a_successful_inspection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.pdf");
    fs::write(&path, b"%PDF-1.7\ninvalid").unwrap();
    assert!(rust_wps::inspect(&path).is_err());
    let report = rust_wps::inspect_for_preview(&path).unwrap();
    assert!(report.text_error.is_some());
    assert!(report.text.is_empty());
    for bytes in [b"not pdf".as_slice(), b""] {
        fs::write(&path, bytes).unwrap();
        assert!(rust_wps::inspect_for_preview(&path).is_err());
    }
}

#[test]
fn encrypted_pdf_keeps_preview_available_without_claiming_extracted_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("encrypted.pdf");
    let bytes = include_bytes!("fixtures/encrypted.pdf");
    fs::write(&path, bytes).unwrap();
    assert!(rust_wps::inspect(&path).is_err());
    let report = rust_wps::inspect_for_preview(&path).unwrap();
    assert!(report.text_error.is_some());
    assert!(report.text.is_empty());
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn rejects_compressed_docx_expansion_before_xml_parsing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("compressed.docx");
    let mut zip = ZipWriter::new(fs::File::create(&path).unwrap());
    zip.start_file(
        "word/document.xml",
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
    )
    .unwrap();
    let chunk = vec![0u8; 1024 * 1024];
    for _ in 0..129 {
        zip.write_all(&chunk).unwrap();
    }
    zip.finish().unwrap();
    assert!(fs::metadata(&path).unwrap().len() < 64 * 1024 * 1024);
    assert!(
        rust_wps::inspect(&path)
            .unwrap_err()
            .to_string()
            .contains("Expanded DOCX exceeds 128 MiB")
    );
}

#[test]
fn rejects_crc_mismatch_before_office_parsing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crc.docx");
    let mut zip = ZipWriter::new(fs::File::create(&path).unwrap());
    zip.start_file(
        "word/document.xml",
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )
    .unwrap();
    zip.write_all(b"unique crc marker").unwrap();
    zip.finish().unwrap();
    let mut bytes = fs::read(&path).unwrap();
    let at = bytes
        .windows(17)
        .position(|w| w == b"unique crc marker")
        .unwrap();
    bytes[at] ^= 1;
    fs::write(&path, bytes).unwrap();
    assert!(
        rust_wps::inspect(&path)
            .unwrap_err()
            .to_string()
            .to_lowercase()
            .contains("checksum")
    );
}
