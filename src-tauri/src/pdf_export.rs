use rust_wps::document::{PageGeometry, validate_page_geometry};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc, time::Duration};

#[derive(Serialize)]
pub struct PdfExport {
    path: String,
    bytes: u64,
}

#[tauri::command]
pub async fn export_current_pdf(
    window: tauri::WebviewWindow,
    page: Option<PdfPage>,
    word_style_profile: Option<String>,
    word_page_geometry: Option<PageGeometry>,
) -> Result<Option<PdfExport>, String> {
    let geometry = print_geometry(
        page,
        word_style_profile.as_deref(),
        word_page_geometry.as_ref(),
    )?;
    #[cfg(not(windows))]
    {
        let _ = (window, geometry);
        Err("当前平台尚未接入 PDF 直接导出，请使用打印入口".into())
    }
    #[cfg(windows)]
    {
        let output = tauri::async_runtime::spawn_blocking(|| {
            #[cfg(feature = "native-smoke")]
            if let Some(path) = std::env::var_os("RUST_WPS_SMOKE_PDF_OUTPUT") {
                return Some(PathBuf::from(path));
            }
            rfd::FileDialog::new()
                .add_filter("PDF 文档", &["pdf"])
                .set_file_name("文档.pdf")
                .save_file()
        })
        .await
        .map_err(|e| e.to_string())?;
        let Some(output) = output else {
            return Ok(None);
        };
        if output.exists() {
            return Err("目标文件已存在，请选择新的文件名".into());
        }
        let directory = Arc::new(tempfile::tempdir().map_err(|e| e.to_string())?);
        let generated = directory.path().join("rendered.pdf");
        let render_path = generated.clone();
        let render_directory = directory.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        window
            .with_webview(move |webview| {
                use webview2_com::{
                    Microsoft::Web::WebView2::Win32::{ICoreWebView2_7, ICoreWebView2Environment6},
                    PrintToPdfCompletedHandler,
                };
                use windows::core::{HSTRING, Interface};
                let failure = sender.clone();
                // SAFETY: Tauri runs this closure on the webview's owning UI thread.
                // COM handles and UTF-16 path live through the synchronous call;
                // WebView2 retains its completion callback for the async operation.
                let result = unsafe {
                    (|| -> windows::core::Result<()> {
                        let environment: ICoreWebView2Environment6 =
                            webview.environment().cast()?;
                        let webview: ICoreWebView2_7 =
                            webview.controller().CoreWebView2()?.cast()?;
                        let settings = environment.CreatePrintSettings()?;
                        settings.SetPageWidth(geometry.width_inches)?;
                        settings.SetPageHeight(geometry.height_inches)?;
                        settings.SetMarginTop(geometry.margin_top_inches)?;
                        settings.SetMarginBottom(geometry.margin_bottom_inches)?;
                        settings.SetMarginLeft(geometry.margin_left_inches)?;
                        settings.SetMarginRight(geometry.margin_right_inches)?;
                        settings.SetShouldPrintBackgrounds(true)?;
                        settings.SetShouldPrintHeaderAndFooter(false)?;
                        let callback =
                            PrintToPdfCompletedHandler::create(Box::new(move |status, success| {
                                let _keep_temp_directory_alive = render_directory;
                                let result = status.map_err(|e| e.to_string()).and_then(|()| {
                                    if success {
                                        Ok(())
                                    } else {
                                        Err("WebView2 未能生成 PDF".into())
                                    }
                                });
                                let _ = sender.send(result);
                                Ok(())
                            }));
                        webview.PrintToPdf(
                            &HSTRING::from(render_path.as_os_str()),
                            &settings,
                            &callback,
                        )
                    })()
                };
                if let Err(error) = result {
                    let _ = failure.send(Err(error.to_string()));
                }
            })
            .map_err(|e| e.to_string())?;
        tauri::async_runtime::spawn_blocking(move || {
            let _keep_temp_directory_alive = directory;
            receiver
                .recv_timeout(Duration::from_secs(60))
                .map_err(|e| format!("PDF 导出未完成：{e}"))??;
            publish_pdf(generated, output, page, word_page_geometry.as_ref()).map(Some)
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PdfGeometry {
    width_inches: f64,
    height_inches: f64,
    margin_top_inches: f64,
    margin_right_inches: f64,
    margin_bottom_inches: f64,
    margin_left_inches: f64,
}

fn print_geometry(
    page: Option<PdfPage>,
    word_style_profile: Option<&str>,
    word_page_geometry: Option<&PageGeometry>,
) -> Result<PdfGeometry, String> {
    if page.is_some() && (word_style_profile.is_some() || word_page_geometry.is_some()) {
        return Err("PDF 自定义页面与 Word 样式版本或页面设置不能同时指定".into());
    }
    if let Some(page) = page {
        page.validate()?;
        return Ok(PdfGeometry {
            width_inches: page.width_inches,
            height_inches: page.height_inches,
            margin_top_inches: 0.0,
            margin_right_inches: 0.0,
            margin_bottom_inches: 0.0,
            margin_left_inches: 0.0,
        });
    }
    let margin_inches = match word_style_profile {
        None => 20.0 / 25.4,
        Some(rust_wps::document::STANDARD_STYLE_PROFILE) => 1.0,
        Some(_) => return Err("不支持的 Word 样式版本".into()),
    };
    if let Some(page) = word_page_geometry {
        validate_page_geometry(page).map_err(|e| e.to_string())?;
        return Ok(PdfGeometry {
            width_inches: page.width_twips as f64 / 1440.0,
            height_inches: page.height_twips as f64 / 1440.0,
            margin_top_inches: page.margin_top_twips as f64 / 1440.0,
            margin_right_inches: page.margin_right_twips as f64 / 1440.0,
            margin_bottom_inches: page.margin_bottom_twips as f64 / 1440.0,
            margin_left_inches: page.margin_left_twips as f64 / 1440.0,
        });
    }
    Ok(PdfGeometry {
        width_inches: 210.0 / 25.4,
        height_inches: 297.0 / 25.4,
        margin_top_inches: margin_inches,
        margin_right_inches: margin_inches,
        margin_bottom_inches: margin_inches,
        margin_left_inches: margin_inches,
    })
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdfPage {
    pub width_inches: f64,
    pub height_inches: f64,
    pub expected_pages: u32,
}
impl PdfPage {
    fn validate(self) -> Result<(), String> {
        if !self.width_inches.is_finite()
            || !self.height_inches.is_finite()
            || !(1.0..=100.0).contains(&self.width_inches)
            || !(1.0..=100.0).contains(&self.height_inches)
            || !(1..=1000).contains(&self.expected_pages)
        {
            return Err("PDF 页面尺寸或页数超出支持范围".into());
        }
        Ok(())
    }
}
fn verify_layout(document: &lopdf::Document, layout: PdfPage) -> Result<(), String> {
    layout.validate()?;
    let pages = document.get_pages();
    if pages.len() != layout.expected_pages as usize {
        return Err("生成的 PDF 页数与演示快照不一致".into());
    }
    verify_media_boxes(
        document,
        layout.width_inches,
        layout.height_inches,
        "生成的 PDF 尺寸与演示快照不一致",
    )
}

fn verify_word_layout(document: &lopdf::Document, layout: &PageGeometry) -> Result<(), String> {
    validate_page_geometry(layout).map_err(|e| e.to_string())?;
    verify_media_boxes(
        document,
        layout.width_twips as f64 / 1440.0,
        layout.height_twips as f64 / 1440.0,
        "生成的 PDF 尺寸与 Word 页面设置不一致",
    )?;
    verify_word_visible_pages(document)
}

// Word's explicit geometry describes the visible physical sheet. MediaBox
// alone is insufficient: inherited cropping/rotation and UserUnit can change
// what a consumer displays. This strict check is opt-in only for explicit Word
// geometry; legacy and presentation publication retain their old contract.
fn word_pdf_object<'a>(
    document: &'a lopdf::Document,
    mut object: &'a lopdf::Object,
) -> Result<&'a lopdf::Object, String> {
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..32 {
        let lopdf::Object::Reference(id) = object else {
            return Ok(object);
        };
        if !seen.insert(*id) {
            return Err("PDF 页面属性包含循环引用".into());
        }
        object = document.objects.get(id).ok_or("PDF 页面属性引用缺失")?;
    }
    Err("PDF 页面属性引用层级超出上限".into())
}

fn inherited_word_page_value<'a>(
    document: &'a lopdf::Document,
    mut id: lopdf::ObjectId,
    key: &[u8],
) -> Result<Option<&'a lopdf::Object>, String> {
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..32 {
        if !seen.insert(id) {
            return Err("PDF 页面继承包含循环引用".into());
        }
        let raw = document.objects.get(&id).ok_or("PDF 页面对象缺失")?;
        let dict = word_pdf_object(document, raw)?
            .as_dict()
            .map_err(|e| e.to_string())?;
        if let Ok(value) = dict.get(key) {
            return word_pdf_object(document, value).map(Some);
        }
        let Ok(parent) = dict.get(b"Parent") else {
            return Ok(None);
        };
        id = parent.as_reference().map_err(|e| e.to_string())?;
    }
    Err("PDF 页面继承层级超出上限".into())
}

fn word_pdf_box(document: &lopdf::Document, object: &lopdf::Object) -> Result<[f64; 4], String> {
    let values = word_pdf_object(document, object)?
        .as_array()
        .map_err(|e| e.to_string())?;
    if values.len() != 4 {
        return Err("PDF 页面边界无效".into());
    }
    let mut result = [0.0; 4];
    for (target, value) in result.iter_mut().zip(values) {
        *target = f64::from(
            word_pdf_object(document, value)?
                .as_float()
                .map_err(|e| e.to_string())?,
        );
        if !target.is_finite() {
            return Err("PDF 页面边界不是有限数值".into());
        }
    }
    if result[2] <= result[0] || result[3] <= result[1] {
        return Err("PDF 页面边界无效".into());
    }
    Ok(result)
}

fn verify_word_visible_pages(document: &lopdf::Document) -> Result<(), String> {
    for id in document.get_pages().into_values() {
        if let Some(rotation) = inherited_word_page_value(document, id, b"Rotate")?
            && rotation.as_i64().map_err(|e| e.to_string())? != 0
        {
            return Err("生成的 Word PDF 包含非零页面旋转".into());
        }
        let media =
            inherited_word_page_value(document, id, b"MediaBox")?.ok_or("PDF 缺少页面尺寸")?;
        let media = word_pdf_box(document, media)?;
        if let Some(crop) = inherited_word_page_value(document, id, b"CropBox")?
            && word_pdf_box(document, crop)? != media
        {
            return Err("生成的 Word PDF 裁剪边界与纸张边界不一致".into());
        }
        // UserUnit is page-local, not an inheritable page-tree attribute.
        let dict = document.get_dictionary(id).map_err(|e| e.to_string())?;
        if let Ok(unit) = dict.get(b"UserUnit") {
            let unit = word_pdf_object(document, unit)?
                .as_float()
                .map_err(|e| e.to_string())?;
            if unit != 1.0 {
                return Err("生成的 Word PDF 使用非默认物理页面单位".into());
            }
        }
    }
    Ok(())
}

// Word pagination is content-dependent. Check every physical page, including
// inherited boxes, without borrowing presentation's fixed page-count contract.
fn verify_media_boxes(
    document: &lopdf::Document,
    width_inches: f64,
    height_inches: f64,
    mismatch: &str,
) -> Result<(), String> {
    let pages = document.get_pages();
    if pages.is_empty() {
        return Err("生成的 PDF 没有页面".into());
    }
    for mut id in pages.into_values() {
        let mut dimensions = None;
        for _ in 0..32 {
            let dict = document.get_dictionary(id).map_err(|e| e.to_string())?;
            if let Ok(object) = dict.get(b"MediaBox") {
                let object = document.dereference(object).map_err(|e| e.to_string())?.1;
                let values = object.as_array().map_err(|e| e.to_string())?;
                if values.len() != 4 {
                    return Err("PDF 页面边界无效".into());
                }
                let n = values
                    .iter()
                    .map(|v| v.as_float().map(f64::from))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())?;
                dimensions = Some((n[2] - n[0], n[3] - n[1]));
                break;
            }
            id = dict
                .get(b"Parent")
                .and_then(|v| v.as_reference())
                .map_err(|e| e.to_string())?;
        }
        let Some((w, h)) = dimensions else {
            return Err("PDF 缺少页面尺寸".into());
        };
        if !w.is_finite()
            || !h.is_finite()
            || w <= 0.0
            || h <= 0.0
            || (w - width_inches * 72.0).abs() > 2.0
            || (h - height_inches * 72.0).abs() > 2.0
        {
            return Err(mismatch.into());
        }
    }
    Ok(())
}
fn publish_pdf(
    source: PathBuf,
    output: PathBuf,
    page: Option<PdfPage>,
    word_page_geometry: Option<&PageGeometry>,
) -> Result<PdfExport, String> {
    use std::io::{Read, Write};
    if page.is_some() && word_page_geometry.is_some() {
        return Err("PDF 自定义页面与 Word 页面设置不能同时指定".into());
    }
    if let Some(layout) = word_page_geometry {
        validate_page_geometry(layout).map_err(|e| e.to_string())?;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(source)
        .map_err(|e| e.to_string())?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 64 * 1024 * 1024
        || !bytes.starts_with(b"%PDF-")
        || !bytes.windows(5).rev().take(1024).any(|w| w == b"%%EOF")
    {
        return Err("生成的 PDF 不完整或超出大小上限".into());
    }
    let document =
        lopdf::Document::load_mem(&bytes).map_err(|e| format!("生成的 PDF 无法重开：{e}"))?;
    if document.get_pages().is_empty() {
        return Err("生成的 PDF 没有页面".into());
    }
    if let Some(page) = page {
        verify_layout(&document, page)?;
    }
    if let Some(layout) = word_page_geometry {
        verify_word_layout(&document, layout)?;
    }
    let mut temporary =
        tempfile::NamedTempFile::new_in(output.parent().unwrap_or(std::path::Path::new(".")))
            .map_err(|e| e.to_string())?;
    temporary.write_all(&bytes).map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    temporary
        .persist_noclobber(&output)
        .map_err(|e| e.to_string())?;
    Ok(PdfExport {
        path: output.to_string_lossy().into(),
        bytes: bytes.len() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word_page() -> PageGeometry {
        PageGeometry {
            width_twips: 11906,
            height_twips: 16838,
            margin_top_twips: 719,
            margin_right_twips: 1441,
            margin_bottom_twips: 2162,
            margin_left_twips: 4320,
        }
    }

    fn pdf_with_boxes(boxes: &[Option<[f32; 4]>], inherited: Option<[f32; 4]>) -> lopdf::Document {
        use lopdf::{Document, Object, dictionary};
        let mut document = Document::with_version("1.7");
        let pages = document.new_object_id();
        let kids = boxes
            .iter()
            .map(|bounds| {
                let mut page = dictionary! { "Type" => "Page", "Parent" => pages };
                if let Some(bounds) = bounds {
                    page.set("MediaBox", bounds.map(Object::Real).to_vec());
                }
                document.add_object(page).into()
            })
            .collect::<Vec<Object>>();
        let mut page_tree = dictionary! {
            "Type" => "Pages", "Count" => kids.len() as i64, "Kids" => kids
        };
        if let Some(bounds) = inherited {
            // Exercise dereferencing as well as inheriting the MediaBox.
            let media_box = document.add_object(bounds.map(Object::Real).to_vec());
            page_tree.set("MediaBox", media_box);
        }
        document
            .objects
            .insert(pages, Object::Dictionary(page_tree));
        let root = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        document.trailer.set("Root", root);
        document
    }

    fn word_box(page: &PageGeometry) -> [f32; 4] {
        [
            0.0,
            0.0,
            page.width_twips as f32 / 20.0,
            page.height_twips as f32 / 20.0,
        ]
    }

    #[test]
    fn word_geometry_uses_exact_source_twips_and_each_margin() {
        for (width, height) in [(11906, 16838), (12240, 15840), (8391, 11906)] {
            for (width, height) in [(width, height), (height, width)] {
                let page = PageGeometry {
                    width_twips: width + 2,
                    height_twips: height - 2,
                    ..word_page()
                };
                let expected = PdfGeometry {
                    width_inches: (width + 2) as f64 / 1440.0,
                    height_inches: (height - 2) as f64 / 1440.0,
                    margin_top_inches: 719.0 / 1440.0,
                    margin_right_inches: 1441.0 / 1440.0,
                    margin_bottom_inches: 2162.0 / 1440.0,
                    margin_left_inches: 3.0,
                };
                assert_eq!(print_geometry(None, None, Some(&page)).unwrap(), expected);
                assert_eq!(
                    print_geometry(
                        None,
                        Some(rust_wps::document::STANDARD_STYLE_PROFILE),
                        Some(&page),
                    )
                    .unwrap(),
                    expected,
                );
            }
        }
        let page = PageGeometry {
            margin_top_twips: 0,
            ..word_page()
        };
        assert_eq!(
            print_geometry(None, None, Some(&page))
                .unwrap()
                .margin_top_inches,
            0.0
        );
    }

    #[test]
    fn invalid_word_geometry_and_presentation_combinations_fail_before_printing() {
        for page in [
            PageGeometry {
                width_twips: 11909,
                ..word_page()
            },
            PageGeometry {
                height_twips: 16841,
                ..word_page()
            },
            PageGeometry {
                margin_top_twips: 4321,
                ..word_page()
            },
            PageGeometry {
                margin_right_twips: 4321,
                ..word_page()
            },
            PageGeometry {
                margin_bottom_twips: 4321,
                ..word_page()
            },
            PageGeometry {
                margin_left_twips: 4321,
                ..word_page()
            },
            PageGeometry {
                width_twips: 8391,
                height_twips: 11906,
                margin_right_twips: 4000,
                margin_left_twips: 4000,
                ..word_page()
            },
            PageGeometry {
                width_twips: 11906,
                height_twips: 8391,
                margin_top_twips: 4000,
                margin_bottom_twips: 4000,
                ..word_page()
            },
        ] {
            assert!(print_geometry(None, None, Some(&page)).is_err());
        }
        let minimum_content = PageGeometry {
            width_twips: 8391,
            height_twips: 11906,
            margin_left_twips: 4320,
            margin_right_twips: 2631,
            ..word_page()
        };
        assert!(print_geometry(None, None, Some(&minimum_content)).is_ok());
        let too_narrow = PageGeometry {
            margin_right_twips: 2632,
            ..minimum_content
        };
        assert!(print_geometry(None, None, Some(&too_narrow)).is_err());
        let page = word_page();
        assert!(print_geometry(None, Some("invalid"), Some(&page)).is_err());
        let presentation = PdfPage {
            width_inches: 12.0,
            height_inches: 6.75,
            expected_pages: 1,
        };
        assert!(print_geometry(Some(presentation), None, Some(&page)).is_err());
        assert!(
            print_geometry(
                Some(presentation),
                Some(rust_wps::document::STANDARD_STYLE_PROFILE),
                Some(&page),
            )
            .is_err()
        );
    }

    #[test]
    fn native_word_geometry_deserialization_requires_exact_integer_shape() {
        let valid = serde_json::json!({
            "widthTwips": 11906, "heightTwips": 16838,
            "marginTopTwips": 0, "marginRightTwips": 1,
            "marginBottomTwips": 4320, "marginLeftTwips": 1440
        });
        let page: PageGeometry = serde_json::from_value(valid.clone()).unwrap();
        assert!(print_geometry(None, None, Some(&page)).is_ok());
        for value in [
            serde_json::json!(11906.5),
            serde_json::json!(-1),
            serde_json::json!("11906"),
            serde_json::Value::Null,
        ] {
            let mut invalid = valid.clone();
            invalid["widthTwips"] = value;
            assert!(serde_json::from_value::<PageGeometry>(invalid).is_err());
        }
        let mut extra = valid.clone();
        extra["orientation"] = serde_json::json!("portrait");
        assert!(serde_json::from_value::<PageGeometry>(extra).is_err());
        let mut missing = valid;
        missing.as_object_mut().unwrap().remove("marginLeftTwips");
        assert!(serde_json::from_value::<PageGeometry>(missing).is_err());
    }

    #[test]
    fn word_media_boxes_validate_all_pages_and_inheritance_without_fixed_count() {
        let page = word_page();
        let bounds = word_box(&page);
        for count in [1, 3, 27] {
            let document = pdf_with_boxes(&vec![Some(bounds); count], None);
            assert!(verify_word_layout(&document, &page).is_ok());
            let inherited = pdf_with_boxes(&vec![None; count], Some(bounds));
            assert!(verify_word_layout(&inherited, &page).is_ok());
        }
        let mut wrong = bounds;
        wrong[2] += 10.0;
        let document = pdf_with_boxes(&[Some(bounds), Some(wrong)], None);
        assert!(verify_word_layout(&document, &page).is_err());
        let inherited = pdf_with_boxes(&[None, Some(wrong)], Some(bounds));
        assert!(verify_word_layout(&inherited, &page).is_err());
        let landscape = PageGeometry {
            width_twips: 16840,
            height_twips: 11904,
            ..page
        };
        assert!(
            verify_word_layout(
                &pdf_with_boxes(&[Some(word_box(&landscape))], None),
                &landscape
            )
            .is_ok()
        );
        assert!(verify_word_layout(&document, &landscape).is_err());
    }

    #[test]
    fn word_media_boxes_reject_missing_nonfinite_and_nonpositive_dimensions() {
        let page = word_page();
        for bounds in [
            [0.0, 0.0, f32::NAN, 841.9],
            [0.0, 0.0, 595.3, f32::INFINITY],
            [0.0, 0.0, 0.0, 841.9],
            [0.0, 841.9, 595.3, 0.0],
        ] {
            assert!(verify_word_layout(&pdf_with_boxes(&[Some(bounds)], None), &page).is_err());
        }
        assert!(verify_word_layout(&pdf_with_boxes(&[None], None), &page).is_err());
        assert!(verify_word_layout(&pdf_with_boxes(&[], None), &page).is_err());
    }

    #[test]
    fn mismatched_word_pdf_is_rejected_before_any_output_is_published() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("render.pdf");
        let output = dir.path().join("word.pdf");
        let page = word_page();
        let bounds = word_box(&page);
        let mut wrong = bounds;
        wrong[3] += 10.0;
        pdf_with_boxes(&[Some(bounds), Some(wrong)], None)
            .save(&source)
            .unwrap();
        assert!(publish_pdf(source.clone(), output.clone(), None, Some(&page)).is_err());
        assert!(!output.exists());
        std::fs::write(&output, b"existing contents").unwrap();
        assert!(publish_pdf(source.clone(), output.clone(), None, Some(&page)).is_err());
        assert_eq!(std::fs::read(&output).unwrap(), b"existing contents");
        std::fs::remove_file(&output).unwrap();
        pdf_with_boxes(&[None, None, None], Some(bounds))
            .save(&source)
            .unwrap();
        let invalid = PageGeometry {
            margin_left_twips: 4321,
            ..page
        };
        assert!(publish_pdf(source.clone(), output.clone(), None, Some(&invalid)).is_err());
        assert!(!output.exists());
        let presentation = PdfPage {
            width_inches: page.width_twips as f64 / 1440.0,
            height_inches: page.height_twips as f64 / 1440.0,
            expected_pages: 3,
        };
        assert!(
            publish_pdf(
                source.clone(),
                output.clone(),
                Some(presentation),
                Some(&page)
            )
            .is_err()
        );
        assert!(!output.exists());
        assert!(
            publish_pdf(source, output.clone(), None, Some(&page))
                .unwrap()
                .bytes
                > 0
        );
        assert_eq!(lopdf::Document::load(output).unwrap().get_pages().len(), 3);
    }

    fn page_and_parent(document: &lopdf::Document) -> (lopdf::ObjectId, lopdf::ObjectId) {
        let page = *document.get_pages().values().next().unwrap();
        let parent = document
            .get_dictionary(page)
            .unwrap()
            .get(b"Parent")
            .unwrap()
            .as_reference()
            .unwrap();
        (page, parent)
    }

    #[test]
    fn explicit_word_rejects_direct_and_inherited_rotation_but_allows_effective_zero() {
        use lopdf::Object;
        let layout = word_page();
        for inherited in [false, true] {
            for value in [
                Object::Integer(90),
                Object::Integer(360),
                Object::Real(0.0),
                Object::Name(b"bad".to_vec()),
                Object::Null,
            ] {
                let mut document = pdf_with_boxes(&[Some(word_box(&layout))], None);
                let (page, parent) = page_and_parent(&document);
                let value = document.add_object(value);
                document
                    .get_dictionary_mut(if inherited { parent } else { page })
                    .unwrap()
                    .set("Rotate", value);
                assert!(verify_word_layout(&document, &layout).is_err());
                document.get_dictionary_mut(page).unwrap().set("Rotate", 0);
                assert!(verify_word_layout(&document, &layout).is_ok());
            }
        }
    }

    #[test]
    fn explicit_word_compares_all_crop_coordinates_including_inherited_references() {
        use lopdf::Object;
        let layout = word_page();
        let bounds = word_box(&layout);
        for inherited in [false, true] {
            for coordinate in 0..4 {
                let mut document = pdf_with_boxes(&[Some(bounds)], None);
                let (page, parent) = page_and_parent(&document);
                let mut cropped = bounds;
                cropped[coordinate] += 1.0;
                let value = document.add_object(cropped.map(Object::Real).to_vec());
                document
                    .get_dictionary_mut(if inherited { parent } else { page })
                    .unwrap()
                    .set("CropBox", value);
                assert!(verify_word_layout(&document, &layout).is_err());
                let exact = document.add_object(bounds.map(Object::Real).to_vec());
                document
                    .get_dictionary_mut(page)
                    .unwrap()
                    .set("CropBox", exact);
                assert!(verify_word_layout(&document, &layout).is_ok());
            }
        }
    }

    #[test]
    fn explicit_word_rejects_malformed_crop_and_nondefault_physical_units() {
        use lopdf::Object;
        let layout = word_page();
        for bad in [
            Object::Null,
            Object::Integer(0),
            Object::Array(vec![0.into(); 3]),
            Object::Array(vec![0.into(), 0.into(), Object::Real(f32::NAN), 800.into()]),
            Object::Array(vec![0.into(), 0.into(), 0.into(), 800.into()]),
        ] {
            let mut document = pdf_with_boxes(&[Some(word_box(&layout))], None);
            let (page, _) = page_and_parent(&document);
            document
                .get_dictionary_mut(page)
                .unwrap()
                .set("CropBox", bad);
            assert!(verify_word_layout(&document, &layout).is_err());
        }
        for value in [
            Object::Integer(0),
            Object::Real(2.0),
            Object::Real(f32::NAN),
            Object::Null,
        ] {
            let mut document = pdf_with_boxes(&[Some(word_box(&layout))], None);
            let (page, _) = page_and_parent(&document);
            let value = document.add_object(value);
            document
                .get_dictionary_mut(page)
                .unwrap()
                .set("UserUnit", value);
            assert!(verify_word_layout(&document, &layout).is_err());
            document
                .get_dictionary_mut(page)
                .unwrap()
                .set("UserUnit", 1);
            assert!(verify_word_layout(&document, &layout).is_ok());
        }
    }

    #[test]
    fn explicit_word_page_attribute_resolution_bounds_cycles_and_depth() {
        use lopdf::{Object, dictionary};
        let layout = word_page();
        for key in [
            b"Rotate".as_slice(),
            b"CropBox".as_slice(),
            b"UserUnit".as_slice(),
        ] {
            let mut document = pdf_with_boxes(&[Some(word_box(&layout))], None);
            let (page, _) = page_and_parent(&document);
            let circular = document.new_object_id();
            document
                .objects
                .insert(circular, Object::Reference(circular));
            document
                .get_dictionary_mut(page)
                .unwrap()
                .set(key, circular);
            assert!(verify_word_layout(&document, &layout).is_err());
            let mut reference = document.add_object(Object::Integer(0));
            for _ in 0..40 {
                reference = document.add_object(Object::Reference(reference));
            }
            document
                .get_dictionary_mut(page)
                .unwrap()
                .set(key, reference);
            assert!(verify_word_layout(&document, &layout).is_err());
        }
        let mut document = pdf_with_boxes(&[Some(word_box(&layout))], None);
        let (page, parent) = page_and_parent(&document);
        document
            .get_dictionary_mut(parent)
            .unwrap()
            .set("Parent", page);
        assert!(verify_word_layout(&document, &layout).is_err());
        document
            .get_dictionary_mut(parent)
            .unwrap()
            .remove(b"Parent");
        let mut next = parent;
        for _ in 0..40 {
            next = document.add_object(dictionary! {"Type" => "Pages", "Parent" => next});
        }
        document
            .get_dictionary_mut(page)
            .unwrap()
            .set("Parent", next);
        assert!(verify_word_layout(&document, &layout).is_err());
    }

    #[test]
    fn cropped_or_rotated_word_output_is_never_published_and_old_modes_stay_unchanged() {
        let layout = word_page();
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("render.pdf");
        let output = dir.path().join("word.pdf");
        for rotate in [true, false] {
            let mut document = pdf_with_boxes(&[Some(word_box(&layout))], None);
            let (page, _) = page_and_parent(&document);
            if rotate {
                document.get_dictionary_mut(page).unwrap().set("Rotate", 90);
            } else {
                document
                    .get_dictionary_mut(page)
                    .unwrap()
                    .set("CropBox", vec![0.into(), 0.into(), 300.into(), 400.into()]);
            }
            document.save(&source).unwrap();
            assert!(publish_pdf(source.clone(), output.clone(), None, Some(&layout)).is_err());
            assert!(!output.exists());
            let presentation = PdfPage {
                width_inches: layout.width_twips as f64 / 1440.0,
                height_inches: layout.height_twips as f64 / 1440.0,
                expected_pages: 1,
            };
            assert!(verify_layout(&document, presentation).is_ok());
            assert!(publish_pdf(source.clone(), output.clone(), None, None).is_ok());
            let original = std::fs::read(&output).unwrap();
            assert!(publish_pdf(source.clone(), output.clone(), None, Some(&layout)).is_err());
            assert_eq!(std::fs::read(&output).unwrap(), original);
            std::fs::remove_file(&output).unwrap();
        }
    }

    #[test]
    fn print_geometry_preserves_legacy_and_presentation_and_versions_word_margins() {
        let legacy = print_geometry(None, None, None).unwrap();
        assert_eq!(
            legacy,
            PdfGeometry {
                width_inches: 210.0 / 25.4,
                height_inches: 297.0 / 25.4,
                margin_top_inches: 20.0 / 25.4,
                margin_right_inches: 20.0 / 25.4,
                margin_bottom_inches: 20.0 / 25.4,
                margin_left_inches: 20.0 / 25.4,
            }
        );
        assert_eq!(
            print_geometry(None, Some(rust_wps::document::STANDARD_STYLE_PROFILE), None).unwrap(),
            PdfGeometry {
                margin_top_inches: 1.0,
                margin_right_inches: 1.0,
                margin_bottom_inches: 1.0,
                margin_left_inches: 1.0,
                ..legacy
            },
        );
        let presentation = PdfPage {
            width_inches: 12.0,
            height_inches: 6.75,
            expected_pages: 3,
        };
        assert_eq!(
            print_geometry(Some(presentation), None, None).unwrap(),
            PdfGeometry {
                width_inches: 12.0,
                height_inches: 6.75,
                margin_top_inches: 0.0,
                margin_right_inches: 0.0,
                margin_bottom_inches: 0.0,
                margin_left_inches: 0.0,
            }
        );
        assert!(
            print_geometry(
                Some(presentation),
                Some(rust_wps::document::STANDARD_STYLE_PROFILE),
                None,
            )
            .is_err()
        );
        for invalid in ["", "legacy", "standard-v1", "STANDARD-V2", "standard-v2 "] {
            assert!(
                print_geometry(None, Some(invalid), None).is_err(),
                "{invalid}"
            );
        }
        assert!(
            print_geometry(
                Some(PdfPage {
                    width_inches: f64::NAN,
                    ..presentation
                }),
                None,
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn invalid_render_never_replaces_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("render.pdf");
        let output = dir.path().join("existing.pdf");
        std::fs::write(&source, b"%PDF-1.7\njunk\n%%EOF").unwrap();
        std::fs::write(&output, b"existing contents").unwrap();
        assert!(publish_pdf(source, output.clone(), None, None).is_err());
        assert_eq!(std::fs::read(output).unwrap(), b"existing contents");
    }
    #[test]
    fn valid_pdf_is_published_once_and_racing_existing_output_is_preserved() {
        use lopdf::{Document, Object, dictionary};
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("render.pdf");
        let output = dir.path().join("中文.pdf");
        let mut document = Document::with_version("1.7");
        let pages = document.new_object_id();
        let page = document.add_object(dictionary! { "Type" => "Page", "Parent" => pages, "MediaBox" => vec![0.into(),0.into(),300.into(),400.into()] });
        document.objects.insert(
            pages,
            Object::Dictionary(
                dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 },
            ),
        );
        let root = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        document.trailer.set("Root", root);
        document.save(&source).unwrap();
        let layout = PdfPage {
            width_inches: 300.0 / 72.0,
            height_inches: 400.0 / 72.0,
            expected_pages: 1,
        };
        assert!(
            publish_pdf(
                source.clone(),
                output.clone(),
                Some(PdfPage {
                    expected_pages: 2,
                    ..layout
                }),
                None,
            )
            .is_err()
        );
        assert!(!output.exists());
        assert!(
            publish_pdf(
                source.clone(),
                output.clone(),
                Some(PdfPage {
                    width_inches: 12.0,
                    ..layout
                }),
                None,
            )
            .is_err()
        );
        assert!(!output.exists());
        let report = publish_pdf(source.clone(), output.clone(), Some(layout), None).unwrap();
        assert!(report.bytes > 0);
        let original = std::fs::read(&output).unwrap();
        assert!(publish_pdf(source, output.clone(), None, None).is_err());
        assert_eq!(std::fs::read(output).unwrap(), original);
    }
    #[test]
    fn custom_pdf_dimensions_are_finite_and_bounded() {
        for (width, height, count) in [
            (f64::NAN, 5.0, 1),
            (5.0, f64::INFINITY, 1),
            (0.0, 5.0, 1),
            (101.0, 5.0, 1),
            (5.0, 5.0, 0),
            (5.0, 5.0, 1001),
        ] {
            assert!(
                PdfPage {
                    width_inches: width,
                    height_inches: height,
                    expected_pages: count
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            PdfPage {
                width_inches: 12.0,
                height_inches: 6.75,
                expected_pages: 1000
            }
            .validate()
            .is_ok()
        );
    }
}
