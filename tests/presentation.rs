use rust_wps::presentation::inspect_pptx_bytes;

#[test]
fn mixed_paragraph_alignment_is_never_reported_as_uniform() {
    use office_oxide::{
        ir::ParagraphAlignment,
        pptx::write::{PptxWriter, Run},
    };
    let mut writer = PptxWriter::new();
    writer
        .add_slide()
        .add_rich_text_aligned(
            &[Run::new("Center paragraph")],
            Some(ParagraphAlignment::Center),
        )
        .add_rich_text_aligned(
            &[Run::new("Right paragraph")],
            Some(ParagraphAlignment::Right),
        );
    let mut bytes = std::io::Cursor::new(Vec::new());
    writer.write_to(&mut bytes).unwrap();
    let report = inspect_pptx_bytes(bytes.get_ref()).unwrap();
    let body = report.slides[0]
        .shapes
        .iter()
        .find(|s| s.text.contains("Center paragraph") && s.text.contains("Right paragraph"))
        .unwrap();
    assert!(body.alignment.is_none());
}

#[test]
fn unique_layout_placeholder_geometry_is_separate_from_direct_geometry() {
    let bytes = include_bytes!("fixtures/layout-placeholders.pptx");
    let report = inspect_pptx_bytes(bytes).unwrap();
    let shapes = &report.slides[0].shapes;
    assert_eq!(shapes[0].bounds_emu, None);
    assert_eq!(
        shapes[0].layout_bounds_emu,
        Some([685800, 2130425, 7772400, 1470025])
    );
    assert_eq!(
        shapes[1].layout_bounds_emu,
        Some([1371600, 3886200, 6400800, 1752600])
    );
    assert_eq!(
        shapes[2].bounds_emu,
        Some([914400, 3657600, 5486400, 914400])
    );
    assert_eq!(shapes[2].layout_bounds_emu, None);
    let audit = rust_wps::presentation_import::import_pptx_bytes(bytes).unwrap();
    assert!(audit.model.is_none());
    assert!(!audit.blockers.iter().any(|s| s.contains("继承位置")));
}

#[test]
fn duplicate_layout_placeholder_indices_never_choose_arbitrary_geometry() {
    let report = inspect_pptx_bytes(include_bytes!(
        "fixtures/layout-placeholders-ambiguous.pptx"
    ))
    .unwrap();
    assert_eq!(report.slides[0].shapes[0].layout_bounds_emu, None);
    assert!(report.slides[0].shapes[1].layout_bounds_emu.is_some());
}

#[test]
fn unique_master_role_supplies_only_missing_layout_geometry() {
    let bytes = include_bytes!("fixtures/master-placeholders.pptx");
    let report = inspect_pptx_bytes(bytes).unwrap();
    let shapes = &report.slides[0].shapes;
    assert_eq!(shapes[0].bounds_emu, None);
    assert_eq!(shapes[0].layout_bounds_emu, None);
    assert_eq!(
        shapes[0].master_bounds_emu,
        Some([457200, 274638, 8229600, 1143000])
    );
    assert!(shapes[1].layout_bounds_emu.is_none());
    assert_eq!(
        shapes[1].master_bounds_emu,
        Some([457200, 1600200, 8229600, 4525963])
    );
    assert!(shapes[2].bounds_emu.is_some());
    assert!(shapes[2].master_bounds_emu.is_none());
    assert!(
        rust_wps::presentation_import::import_pptx_bytes(bytes)
            .unwrap()
            .model
            .is_none()
    );
    let ambiguous = inspect_pptx_bytes(include_bytes!(
        "fixtures/master-placeholders-ambiguous.pptx"
    ))
    .unwrap();
    assert!(ambiguous.slides[0].shapes[0].master_bounds_emu.is_none());
    assert!(ambiguous.slides[0].shapes[1].master_bounds_emu.is_some());
}

#[test]
fn real_pptx_master_static_text_is_readable_and_excludes_placeholder_prompts() {
    let report = inspect_pptx_bytes(include_bytes!("fixtures/apache-poi/WithMaster.pptx")).unwrap();
    assert_eq!(
        report.master_text,
        "This text comes from the Master Slide\n"
    );
    assert!(!report.master_text.contains("Master Title"));
    assert!(
        report
            .slides
            .iter()
            .all(|slide| !slide.text.contains("This text comes from the Master Slide"))
    );
    let audit = rust_wps::presentation_import::import_pptx_bytes(include_bytes!(
        "fixtures/apache-poi/WithMaster.pptx"
    ))
    .unwrap();
    assert!(audit.model.is_none());
    assert!(audit.blockers.iter().any(|s| s.contains("母版")));
}

#[test]
fn notes_are_separate_from_slide_surface_and_follow_page_order() {
    let report = inspect_pptx_bytes(include_bytes!("fixtures/notes-slides.pptx")).unwrap();
    assert_eq!(report.slides.len(), 3);
    assert_eq!(report.slides[0].notes, "第一页备注 <test>");
    assert_eq!(report.slides[1].notes, "第二页备注");
    assert!(report.slides[2].notes.is_empty());
    assert!(report.slides.iter().all(|slide| slide.text.is_empty()));
}

#[test]
fn ordered_slides_geometry_groups_text_and_hidden_state() {
    let report = inspect_pptx_bytes(include_bytes!("fixtures/basic-slides.pptx")).unwrap();
    assert_eq!(report.size_emu, Some([10972800, 6172200]));
    assert_eq!(report.slides.len(), 2);
    assert_eq!(report.slides[0].id, 256);
    assert!(report.slides[0].text.contains("简文 PPTX <test>"));
    assert!(report.slides[0].text.contains("组合文字"));
    assert_eq!(report.slides[0].shapes[0].text, "简文 PPTX <test>");
    let run = &report.slides[0].shapes[0].runs[0];
    assert_eq!(run.font_size_hundredths_pt, Some(2400));
    assert_eq!(run.bold, Some(true));
    assert_eq!(run.italic, Some(true));
    assert_eq!(run.color_rgb, Some([160, 32, 64]));
    assert_eq!(report.slides[0].shapes[1].children[0].text, "组合文字");
    assert_eq!(
        report.slides[0].shapes[0].bounds_emu,
        Some([914400, 1828800, 3657600, 914400])
    );
    assert_eq!(report.slides[0].shapes[1].children.len(), 1);
    assert!(
        report.slides[0].shapes[2]
            .image
            .as_deref()
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
    assert!(!report.slides[0].hidden);
    assert!(report.slides[1].hidden);
    assert!(report.unreadable_parts.is_empty());
    assert!(!report.has_macros);
}

#[test]
fn malformed_and_truncated_packages_fail() {
    assert!(inspect_pptx_bytes(b"<html>not a presentation</html>").is_err());
    let fixture = include_bytes!("fixtures/basic-slides.pptx");
    assert!(inspect_pptx_bytes(&fixture[..fixture.len() / 2]).is_err());
}

#[test]
fn xml_depth_and_dtd_are_rejected_before_backend_parsing() {
    use std::io::{Cursor, Write};
    for xml in [
        format!("{}{}", "<x>".repeat(65), "</x>".repeat(65)),
        "<!DOCTYPE x [<!ENTITY value 'text'>]><x/>".to_owned(),
        "<x>".to_owned(),
    ] {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file(
            "ppt/presentation.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        let error = inspect_pptx_bytes(&bytes).unwrap_err().to_string();
        assert!(
            error.contains("complexity") || error.contains("DTD") || error.contains("Truncated"),
            "{error}"
        );
    }
}
