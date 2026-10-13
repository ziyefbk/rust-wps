use rust_wps::presentation_model::{Presentation, Slide, TextBox, export_pptx};
fn model() -> Presentation {
    Presentation {
        version: 1,
        size_emu: [10972800, 6172200],
        slides: vec![
            Slide {
                hidden: false,
                notes: String::new(),
                images: vec![],
                text_boxes: vec![TextBox {
                    vertical_alignment: None,
                    alignment: None,
                    bounds_emu: [914400, 914400, 4572000, 1828800],
                    text: "中文 & <标题>\n第二行\n".into(),
                    font_size_hundredths_pt: 2400,
                    bold: true,
                    italic: true,
                    color_rgb: [160, 32, 64],
                }],
            },
            Slide {
                hidden: false,
                notes: String::new(),
                text_boxes: vec![],
                images: vec![],
            },
        ],
    }
}

#[test]
fn text_alignment_survives_hidden_slide_notes_draft_export_and_model_verification() {
    use rust_wps::presentation_model::{TextAlignment, load_authored_pptx, load_draft, save_draft};
    for alignment in [
        TextAlignment::Left,
        TextAlignment::Center,
        TextAlignment::Right,
    ] {
        let mut model = model();
        model.slides[0].hidden = true;
        model.slides[0].notes = "备注".into();
        model.slides[0].text_boxes[0].alignment = Some(alignment);
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("aligned.pptx");
        let draft = directory.path().join("draft.json");
        save_draft(&model, &draft).unwrap();
        let restored = load_draft(&draft).unwrap().unwrap();
        export_pptx(&restored, &output).unwrap();
        let report = rust_wps::presentation::inspect_pptx(&output).unwrap();
        assert_eq!(
            report.slides[0].shapes[0].alignment.as_deref(),
            Some(alignment.ooxml())
        );
        assert!(report.slides[0].hidden);
        assert_eq!(report.slides[0].notes, "备注");
        let imported = rust_wps::presentation_import::import_pptx(&output).unwrap();
        assert!(imported.blockers.is_empty(), "{:?}", imported.blockers);
        assert_eq!(
            imported.model.unwrap().slides[0].text_boxes[0]
                .alignment
                .map(TextAlignment::ooxml),
            Some(alignment.ooxml())
        );
        let reopened = load_authored_pptx(&output).unwrap().unwrap();
        assert_eq!(
            serde_json::to_value(model).unwrap(),
            serde_json::to_value(reopened).unwrap()
        );
    }
    let mut json = serde_json::to_value(model()).unwrap();
    json["slides"][0]["text_boxes"][0]["alignment"] = "justify".into();
    assert!(serde_json::from_value::<Presentation>(json).is_err());
}
#[test]
fn hidden_slides_keep_content_and_notes_across_export_and_model_restore() {
    let dir = tempfile::tempdir().unwrap();
    let mut model = model();
    model.slides[0].hidden = true;
    model.slides[0].notes = "隐藏页备注".into();
    let output = dir.path().join("hidden.pptx");
    let report = export_pptx(&model, &output).unwrap();
    assert!(report.slides[0].hidden);
    assert!(!report.slides[1].hidden);
    assert_eq!(report.slides[0].notes, "隐藏页备注");
    assert!(report.slides[0].text.contains("中文"));
    let loaded = rust_wps::presentation_model::load_authored_pptx(&output)
        .unwrap()
        .unwrap();
    assert!(loaded.slides[0].hidden);
    assert!(!loaded.slides[1].hidden);
}

#[test]
fn speaker_notes_preserve_blank_lines_draft_and_verified_model() {
    let dir = tempfile::tempdir().unwrap();
    for (i, text) in ["中文 <notes>", "首行\n\n末行\n", "\n\n"]
        .iter()
        .enumerate()
    {
        let mut model = model();
        model.slides[0].notes = (*text).into();
        let output = dir.path().join(format!("notes-{i}.pptx"));
        let report = export_pptx(&model, &output).unwrap();
        assert_eq!(report.slides[0].notes, *text);
        assert!(!report.slides[0].text.contains("notes"));
        let loaded = rust_wps::presentation_model::load_authored_pptx(&output)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.slides[0].notes, *text);
        let draft = dir.path().join("draft.json");
        rust_wps::presentation_model::save_draft(&model, &draft).unwrap();
        assert_eq!(
            rust_wps::presentation_model::load_draft(&draft)
                .unwrap()
                .unwrap()
                .slides[0]
                .notes,
            *text
        );
    }
    let mut invalid = model();
    invalid.slides[0].notes = "bad\0notes".into();
    assert!(invalid.validate().is_err());
    invalid.slides[0].notes = "x".repeat(1024 * 1024 + 1);
    assert!(invalid.validate().is_err());
}

#[test]
fn pictures_export_restore_and_reject_invalid_data_without_publishing() {
    use base64::Engine;
    use rust_wps::presentation_model::{Picture, inspect_picture, load_authored_pptx};
    let dir = tempfile::tempdir().unwrap();
    let mut model = model();
    for format in [image::ImageFormat::Png, image::ImageFormat::Jpeg] {
        let mut output = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            32,
            16,
            image::Rgb([30, 90, 180]),
        ))
        .write_to(&mut output, format)
        .unwrap();
        let mime = if format == image::ImageFormat::Png {
            "png"
        } else {
            "jpeg"
        };
        let data_url = format!(
            "data:image/{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(output.into_inner())
        );
        assert_eq!(inspect_picture(&data_url).unwrap(), [32, 16]);
        model.slides[0].images.push(Picture {
            bounds_emu: [0, 0, 914400, 457200],
            data_url,
            alt: "示意 & <图>".into(),
        });
    }
    let path = dir.path().join("pictures.pptx");
    let report = export_pptx(&model, &path).unwrap();
    assert_eq!(report.slides[0].shapes.len(), 3);
    assert_eq!(
        serde_json::to_value(load_authored_pptx(&path).unwrap().unwrap()).unwrap(),
        serde_json::to_value(&model).unwrap()
    );
    let draft = dir.path().join("draft.json");
    rust_wps::presentation_model::save_draft(&model, &draft).unwrap();
    assert_eq!(
        serde_json::to_value(rust_wps::presentation_model::load_draft(&draft).unwrap()).unwrap(),
        serde_json::to_value(Some(&model)).unwrap()
    );
    for bad in [
        "https://example.com/image.png".to_string(),
        model.slides[0].images[0]
            .data_url
            .replace("image/png", "image/jpeg"),
        "data:image/png;base64,aGVsbG8=".to_string(),
        "data:image/svg+xml;base64,PHN2Zy8+".to_string(),
    ] {
        let mut broken = model.clone();
        broken.slides[0].images[0].data_url = bad;
        assert!(export_pptx(&broken, &dir.path().join("invalid.pptx")).is_err());
        assert!(!dir.path().join("invalid.pptx").exists());
    }
    let mut broken = model.clone();
    broken.slides[0].images[0].bounds_emu = [0, 0, i64::MAX, 1];
    assert!(broken.validate().is_err());
    broken = model.clone();
    broken.slides[0].images[0].alt = "bad\0".into();
    assert!(broken.validate().is_err());
    broken = model.clone();
    broken.slides[0].images = vec![model.slides[0].images[0].clone(); 101];
    assert!(
        broken
            .validate()
            .unwrap_err()
            .to_string()
            .contains("maximum 100")
    );
    // Valid PNG bytes followed by padding exercise compressed-byte budgets without giant pixel allocations.
    let raw = base64::engine::general_purpose::STANDARD
        .decode(
            model.slides[0].images[0]
                .data_url
                .split_once(',')
                .unwrap()
                .1,
        )
        .unwrap();
    let mut padded = raw;
    padded.resize(720_000, 0);
    let mut picture = model.slides[0].images[0].clone();
    picture.data_url = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&padded)
    );
    broken = model.clone();
    broken.slides[0].images = vec![picture.clone(); 3];
    assert!(broken.validate().unwrap_err().to_string().contains("2 MiB"));
    padded.resize(1024 * 1024 + 1, 0);
    picture.data_url = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&padded)
    );
    broken.slides[0].images = vec![picture];
    assert!(broken.validate().is_err());
}
#[test]
fn exports_unicode_styles_multiline_and_blank_slide_without_overwriting() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new.pptx");
    let expected = model();
    let report = export_pptx(&expected, &path).unwrap();
    let mut reopened = rust_wps::presentation_model::load_authored_pptx(&path)
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_value(&reopened).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
    assert_eq!(report.slides.len(), 2);
    assert_eq!(
        report.slides[0].shapes[0].text,
        expected.slides[0].text_boxes[0].text
    );
    assert!(report.slides[1].shapes.is_empty());
    let bytes = std::fs::read(&path).unwrap();
    assert!(export_pptx(&expected, &path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    reopened.slides[0].text_boxes[0].text = "重开后继续修改".into();
    let copy = dir.path().join("continued.pptx");
    export_pptx(&reopened, &copy).unwrap();
    assert_eq!(
        rust_wps::presentation_model::load_authored_pptx(&copy)
            .unwrap()
            .unwrap()
            .slides[0]
            .text_boxes[0]
            .text,
        "重开后继续修改"
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn package_model_mismatch_or_extra_parts_never_grant_editing() {
    use std::io::{Cursor, Read, Write};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("authored.pptx");
    export_pptx(&model(), &path).unwrap();
    let original = std::fs::read(&path).unwrap();
    for variant in 0..3 {
        let mut source = zip::ZipArchive::new(Cursor::new(&original)).unwrap();
        let mut target = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for index in 0..source.len() {
            let mut part = source.by_index(index).unwrap();
            let name = part.name().to_owned();
            let mut data = Vec::new();
            part.read_to_end(&mut data).unwrap();
            if (variant == 0 && name == "ppt/slides/slide1.xml")
                || (variant == 1 && name == "rust-wps/presentation-v1.json")
            {
                data = String::from_utf8(data)
                    .unwrap()
                    .replace("中文", "篡改")
                    .into_bytes();
            }
            target
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            target.write_all(&data).unwrap();
        }
        if variant == 2 {
            target
                .start_file("extra.bin", zip::write::SimpleFileOptions::default())
                .unwrap();
            target.write_all(b"unmodeled").unwrap();
        }
        let bytes = target.finish().unwrap().into_inner();
        assert!(rust_wps::presentation_model::load_authored_pptx_bytes(&bytes).is_err());
    }
    assert!(
        rust_wps::presentation_model::load_authored_pptx_bytes(include_bytes!(
            "fixtures/basic-slides.pptx"
        ))
        .unwrap()
        .is_none()
    );
}
#[test]
fn vertical_alignment_roundtrips_drafts_and_preserves_generated_body_properties() {
    use rust_wps::presentation_model::{
        VerticalAlignment, load_authored_pptx_bytes, load_draft, save_draft,
    };
    let dir = tempfile::tempdir().unwrap();
    for (index, alignment) in [
        VerticalAlignment::Top,
        VerticalAlignment::Middle,
        VerticalAlignment::Bottom,
    ]
    .into_iter()
    .enumerate()
    {
        let mut model = model();
        model.slides[0].text_boxes[0].vertical_alignment = Some(alignment);
        let file = dir.path().join(format!("vertical-{index}.pptx"));
        export_pptx(&model, &file).unwrap();
        let bytes = std::fs::read(file).unwrap();
        let report = rust_wps::presentation::inspect_pptx_bytes(&bytes).unwrap();
        assert_eq!(
            report.slides[0].shapes[0].vertical_alignment.as_deref(),
            Some(alignment.ooxml())
        );
        let imported = rust_wps::presentation_import::import_pptx_bytes(&bytes)
            .unwrap()
            .model
            .unwrap();
        assert_eq!(
            imported.slides[0].text_boxes[0]
                .vertical_alignment
                .unwrap()
                .ooxml(),
            alignment.ooxml()
        );
        let loaded = load_authored_pptx_bytes(&bytes).unwrap().unwrap();
        assert_eq!(
            serde_json::to_value(&loaded).unwrap(),
            serde_json::to_value(&model).unwrap()
        );
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut zip.by_name("ppt/slides/slide1.xml").unwrap(), &mut xml)
            .unwrap();
        assert!(xml.contains(&format!("<a:bodyPr anchor=\"{}\" ", alignment.ooxml())));
        assert!(xml.contains("wrap=\"none\""));
        assert!(xml.contains("tIns=\"0\""));
        let draft = dir.path().join("draft.json");
        save_draft(&model, &draft).unwrap();
        assert_eq!(
            load_draft(&draft).unwrap().unwrap().slides[0].text_boxes[0]
                .vertical_alignment
                .unwrap()
                .ooxml(),
            alignment.ooxml()
        );
    }
    let mut invalid = serde_json::to_value(model()).unwrap();
    invalid["slides"][0]["text_boxes"][0]["vertical_alignment"] = "justify".into();
    assert!(serde_json::from_value::<Presentation>(invalid).is_err());
}

#[test]
fn invalid_models_never_publish_outputs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.pptx");
    for variant in 0..5 {
        let mut model = model();
        match variant {
            0 => model.version = 2,
            1 => model.slides.clear(),
            2 => model.slides[0].text_boxes[0].bounds_emu[0] = i64::MAX,
            3 => model.slides[0].text_boxes[0].font_size_hundredths_pt = 0,
            _ => model.slides[0].text_boxes[0].text = "bad\u{0}".into(),
        };
        assert!(export_pptx(&model, &path).is_err());
        assert!(!path.exists());
    }
    assert!(
        serde_json::from_str::<Presentation>(
            r#"{"version":1,"size_emu":[1,2],"slides":[],"unknown":true}"#
        )
        .is_err()
    );
}

#[test]
fn draft_replaces_atomically_and_invalid_content_preserves_recovery() {
    use rust_wps::presentation_model::{load_draft, save_draft};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("presentation-draft.json");
    assert!(load_draft(&path).unwrap().is_none());
    let mut value = model();
    save_draft(&value, &path).unwrap();
    value.slides[0].text_boxes[0].text = "最新草稿".into();
    save_draft(&value, &path).unwrap();
    assert_eq!(
        load_draft(&path).unwrap().unwrap().slides[0].text_boxes[0].text,
        "最新草稿"
    );
    let before = std::fs::read(&path).unwrap();
    value.version = 2;
    assert!(save_draft(&value, &path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    std::fs::write(&path, b"not json").unwrap();
    assert!(load_draft(&path).is_err());
}
