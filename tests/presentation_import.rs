use rust_wps::{
    presentation_import::import_pptx_bytes,
    presentation_model::{Presentation, export_pptx},
};
use std::io::{Cursor, Read, Write};

fn source() -> Vec<u8> {
    let mut model: Presentation =
        serde_json::from_str(include_str!("../docs/examples/presentation-v1.json")).unwrap();
    model.slides[0].hidden = true;
    model.slides[0].notes = "导入备注\n".into();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("source.pptx");
    export_pptx(&model, &file).unwrap();
    let bytes = std::fs::read(file).unwrap();
    let mut input = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..input.len() {
        let mut entry = input.by_index(i).unwrap();
        if entry.name().starts_with("rust-wps/") {
            continue;
        }
        if entry.name() == "[Content_Types].xml" {
            let mut xml = String::new();
            entry.read_to_string(&mut xml).unwrap();
            xml=xml.replace("<Override PartName=\"/rust-wps/presentation-v1.json\" ContentType=\"application/json\"/>","");
            output
                .start_file(
                    "[Content_Types].xml",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            output.write_all(xml.as_bytes()).unwrap();
        } else {
            output.raw_copy_file(entry).unwrap();
        }
    }
    output.finish().unwrap().into_inner()
}
#[test]
fn simple_external_package_imports_as_new_model_with_visible_risks() {
    let bytes = source();
    let report = import_pptx_bytes(&bytes).unwrap();
    assert!(report.blockers.is_empty(), "{:?}", report.blockers);
    assert!(!report.risks.is_empty());
    let model = report.model.unwrap();
    assert_eq!(model.slides.len(), 2);
    assert!(model.slides[0].hidden);
    assert_eq!(model.slides[0].notes, "导入备注\n");
    assert!(model.slides[0].text_boxes[0].text.contains("简文演示"));
    let dir = tempfile::tempdir().unwrap();
    export_pptx(&model, &dir.path().join("copy.pptx")).unwrap();
}
#[test]
fn complex_package_is_blocked_and_never_returns_partial_editable_model() {
    let report = import_pptx_bytes(include_bytes!("fixtures/basic-slides.pptx")).unwrap();
    assert!(report.model.is_none());
    assert!(report.blockers.iter().any(|s| s.contains("grpSp")));
    assert!(
        !report
            .blockers
            .iter()
            .any(|s| s.contains("母版或布局含文字"))
    );
}

fn with_extra_part(name: &str, xml: &str) -> Vec<u8> {
    let mut input = zip::ZipArchive::new(Cursor::new(source())).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..input.len() {
        output.raw_copy_file(input.by_index(i).unwrap()).unwrap();
    }
    output
        .start_file(name, zip::write::SimpleFileOptions::default())
        .unwrap();
    output.write_all(xml.as_bytes()).unwrap();
    output.finish().unwrap().into_inner()
}

#[test]
fn empty_table_styles_and_plain_view_settings_are_explicitly_supported() {
    for (name, xml) in [
        (
            "ppt/tableStyles.xml",
            "<a:tblStyleLst xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" def=\"{00000000-0000-0000-0000-000000000000}\"/>",
        ),
        (
            "ppt/viewProps.xml",
            "<p:viewPr xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:normalViewPr/></p:viewPr>",
        ),
    ] {
        let report = import_pptx_bytes(&with_extra_part(name, xml)).unwrap();
        assert!(report.blockers.is_empty(), "{:?}", report.blockers);
        assert!(report.model.is_some());
        assert!(report.risks.iter().any(|s| s.contains("视图设置")));
    }
}

#[test]
fn actual_table_styles_and_unknown_view_extensions_still_block() {
    for (name, xml, message) in [
        (
            "ppt/tableStyles.xml",
            "<a:tblStyleLst xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:tblStyle styleId=\"x\" styleName=\"custom\"/></a:tblStyleLst>",
            "实际样式",
        ),
        (
            "ppt/viewProps.xml",
            "<p:viewPr xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:extLst/></p:viewPr>",
            "extLst",
        ),
    ] {
        let report = import_pptx_bytes(&with_extra_part(name, xml)).unwrap();
        assert!(report.model.is_none());
        assert!(
            report.blockers.iter().any(|s| s.contains(message)),
            "{:?}",
            report.blockers
        );
    }
}

fn with_transform(attributes: &str) -> Vec<u8> {
    let mut input = zip::ZipArchive::new(Cursor::new(source())).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..input.len() {
        let mut entry = input.by_index(i).unwrap();
        if entry.name() == "ppt/slides/slide1.xml" {
            let mut xml = String::new();
            entry.read_to_string(&mut xml).unwrap();
            assert!(xml.contains("<a:xfrm>"));
            let xml = xml.replacen("<a:xfrm>", &format!("<a:xfrm {attributes}>"), 1);
            output
                .start_file(
                    "ppt/slides/slide1.xml",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            output.write_all(xml.as_bytes()).unwrap();
        } else {
            output.raw_copy_file(entry).unwrap();
        }
    }
    output.finish().unwrap().into_inner()
}

#[test]
fn equivalent_zero_rotation_and_false_flips_import_without_changing_geometry() {
    let expected = import_pptx_bytes(&source()).unwrap().model.unwrap();
    for attrs in [
        "flipH=\"false\" flipV=\"false\" rot=\"0\"",
        "flipH=\"0\" flipV=\"false\" rot=\"+000\"",
        "flipH=\" false \" flipV=\"0\" rot=\"-0\"",
        "flipH=\"f&#97;lse\" rot=\"0000\"",
    ] {
        let report = import_pptx_bytes(&with_transform(attrs)).unwrap();
        assert!(report.blockers.is_empty(), "{attrs}: {:?}", report.blockers);
        assert_eq!(
            serde_json::to_value(report.model.unwrap()).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
    }
}

#[test]
fn effective_or_invalid_transform_attributes_never_grant_editable_model() {
    for attrs in [
        "flipH=\"true\"",
        "flipV=\"1\"",
        "rot=\"60000\"",
        "rot=\"-60000\"",
        "flipH=\"FALSE\"",
        "flipV=\"no\"",
        "rot=\"NaN\"",
        "rot=\"4294967296\"",
    ] {
        let report = import_pptx_bytes(&with_transform(attrs)).unwrap();
        assert!(report.model.is_none(), "{attrs}");
        assert!(
            report.blockers.iter().any(|s| s.contains("旋转")),
            "{attrs}: {:?}",
            report.blockers
        );
    }
}

#[test]
fn real_poi_deck_imports_unique_layout_and_master_geometry_and_reopens() {
    let bytes = include_bytes!("fixtures/apache-poi/sample.pptx");
    let inventory = rust_wps::presentation::inspect_pptx_bytes(bytes).unwrap();
    let report = import_pptx_bytes(bytes).unwrap();
    assert!(report.blockers.is_empty(), "{:?}", report.blockers);
    assert!(
        report
            .risks
            .iter()
            .any(|s| s.contains("4 个对象") && s.contains("固定"))
    );
    let mut model = report.model.unwrap();
    assert_eq!(model.slides.len(), 2);
    for (page, source) in model.slides.iter().zip(inventory.slides) {
        assert_eq!(page.text_boxes.len(), source.shapes.len());
        for (box_model, shape) in page.text_boxes.iter().zip(source.shapes) {
            assert_eq!(box_model.text, shape.text);
            assert_eq!(
                Some(box_model.bounds_emu),
                shape.layout_bounds_emu.or(shape.master_bounds_emu)
            );
        }
    }
    model.slides[0].text_boxes[0].text = "编辑后的真实文稿".into();
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("edited.pptx");
    export_pptx(&model, &output).unwrap();
    let reopened = rust_wps::presentation_model::load_authored_pptx(&output)
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_value(reopened).unwrap(),
        serde_json::to_value(model).unwrap()
    );
}

#[test]
fn ambiguous_inheritance_still_refuses_partial_editable_model() {
    for bytes in [
        include_bytes!("fixtures/layout-placeholders-ambiguous.pptx").as_slice(),
        include_bytes!("fixtures/master-placeholders-ambiguous.pptx").as_slice(),
    ] {
        let report = import_pptx_bytes(bytes).unwrap();
        assert!(report.model.is_none());
        assert!(report.blockers.iter().any(|s| s.contains("继承位置")));
    }
}
