use rust_wps::presentation_edit::replace_pptx;
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
};
fn parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut part = zip.by_index(i).unwrap();
            let mut data = Vec::new();
            part.read_to_end(&mut data).unwrap();
            (part.name().to_owned(), data)
        })
        .collect()
}
#[test]
fn replacement_keeps_notes_hidden_pages_dimensions_and_verified_model_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.pptx");
    let output = dir.path().join("edited.pptx");
    let mut json: serde_json::Value =
        serde_json::from_str(include_str!("../docs/examples/presentation-v1.json")).unwrap();
    json["size_emu"] = serde_json::json!([9144000, 6858000]);
    json["slides"][0]["notes"] = serde_json::json!("简文演示\n备注保留 <&>\n");
    json["slides"][0]["hidden"] = serde_json::json!(true);
    let model: rust_wps::presentation_model::Presentation = serde_json::from_value(json).unwrap();
    rust_wps::presentation_model::export_pptx(&model, &input).unwrap();
    let before = std::fs::read(&input).unwrap();
    assert!(
        rust_wps::presentation_model::load_authored_pptx(&input)
            .unwrap()
            .is_some()
    );
    let report = replace_pptx(&input, &output, "简文演示", "替换正文").unwrap();
    assert_eq!(report.replacements, 1);
    assert_eq!(report.changed_parts, vec!["ppt/slides/slide1.xml"]);
    let after = std::fs::read(&output).unwrap();
    let original = parts(&before);
    let changed = parts(&after);
    for (name, data) in original {
        if name != "ppt/slides/slide1.xml" {
            assert_eq!(changed.get(&name), Some(&data), "{name}");
        }
    }
    let report = rust_wps::presentation::inspect_pptx_bytes(&after).unwrap();
    assert_eq!(report.size_emu, Some([9144000, 6858000]));
    assert!(report.slides[0].hidden);
    assert_eq!(report.slides[0].notes, "简文演示\n备注保留 <&>\n");
    assert!(report.slides[0].text.contains("替换正文"));
    assert!(!report.slides[0].text.contains("备注保留"));
    // A byte-preserving external edit deliberately invalidates the embedded
    // authored model; the worker must reopen the edited copy read-only.
    assert!(rust_wps::presentation_model::load_authored_pptx(&output).is_err());
    assert_eq!(std::fs::read(input).unwrap(), before);
}

#[test]
fn replacement_preserves_source_other_parts_styles_and_images() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.pptx");
    let output = dir.path().join("edited.pptx");
    let source = include_bytes!("fixtures/basic-slides.pptx");
    std::fs::write(&input, source).unwrap();
    let result = replace_pptx(&input, &output, "简文 PPTX <test>", "修改 & <内容>").unwrap();
    assert_eq!(result.replacements, 1);
    assert_eq!(result.changed_parts, vec!["ppt/slides/slide1.xml"]);
    assert_eq!(std::fs::read(&input).unwrap(), source);
    let after = std::fs::read(&output).unwrap();
    let original = parts(source);
    let edited = parts(&after);
    for (name, data) in &original {
        if name != "ppt/slides/slide1.xml" {
            assert_eq!(edited.get(name), Some(data), "{name}");
        }
    }
    let reopened = rust_wps::presentation::inspect_pptx_bytes(&after).unwrap();
    assert_eq!(reopened.slides[0].shapes[0].text, "修改 & <内容>");
    assert_eq!(
        reopened.slides[0].shapes[0].runs[0].font_size_hundredths_pt,
        Some(2400)
    );
    assert!(reopened.slides[0].shapes[2].image.is_some());
}
#[test]
fn failures_leave_existing_files_and_source_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.pptx");
    let output = dir.path().join("edited.pptx");
    let source = include_bytes!("fixtures/basic-slides.pptx");
    std::fs::write(&input, source).unwrap();
    assert!(replace_pptx(&input, &output, "missing", "new").is_err());
    assert!(!output.exists());
    assert!(replace_pptx(&input, &output, "简文", " bad").is_err());
    assert!(!output.exists());
    std::fs::write(&output, b"keep").unwrap();
    assert!(replace_pptx(&input, &output, "简文", "new").is_err());
    assert_eq!(std::fs::read(&output).unwrap(), b"keep");
    assert_eq!(std::fs::read(&input).unwrap(), source);
    assert!(replace_pptx(&input, &input, "简文", "new").is_err());
}

#[test]
fn signed_and_macro_enabled_packages_are_refused() {
    use std::io::Write;
    for signed in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("source.pptx");
        let output = dir.path().join("edited.pptx");
        let mut entries = parts(include_bytes!("fixtures/basic-slides.pptx"));
        if signed {
            entries.insert("_xmlsignatures/sig1.xml".into(), b"<Signature/>".to_vec());
        } else {
            let ct = entries.get_mut("[Content_Types].xml").unwrap();
            *ct=String::from_utf8(ct.clone()).unwrap().replace("application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml","application/vnd.ms-powerpoint.presentation.macroEnabled.main+xml").into_bytes();
        }
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in entries {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&data).unwrap();
        }
        let source = zip.finish().unwrap().into_inner();
        std::fs::write(&input, &source).unwrap();
        assert!(replace_pptx(&input, &output, "简文", "new").is_err());
        assert!(!output.exists());
        assert_eq!(std::fs::read(&input).unwrap(), source);
    }
}
