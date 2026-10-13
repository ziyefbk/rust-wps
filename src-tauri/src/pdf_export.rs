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
) -> Result<Option<PdfExport>, String> {
    let geometry = print_geometry(page, word_style_profile.as_deref())?;
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
                        settings.SetMarginTop(geometry.margin_inches)?;
                        settings.SetMarginBottom(geometry.margin_inches)?;
                        settings.SetMarginLeft(geometry.margin_inches)?;
                        settings.SetMarginRight(geometry.margin_inches)?;
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
            publish_pdf(generated, output, page).map(Some)
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PdfGeometry {
    width_inches: f64,
    height_inches: f64,
    margin_inches: f64,
}

fn print_geometry(
    page: Option<PdfPage>,
    word_style_profile: Option<&str>,
) -> Result<PdfGeometry, String> {
    if page.is_some() && word_style_profile.is_some() {
        return Err("PDF 自定义页面与 Word 样式版本不能同时指定".into());
    }
    if let Some(page) = page {
        page.validate()?;
        return Ok(PdfGeometry {
            width_inches: page.width_inches,
            height_inches: page.height_inches,
            margin_inches: 0.0,
        });
    }
    let margin_inches = match word_style_profile {
        None => 20.0 / 25.4,
        Some(rust_wps::document::STANDARD_STYLE_PROFILE) => 1.0,
        Some(_) => return Err("不支持的 Word 样式版本".into()),
    };
    Ok(PdfGeometry {
        width_inches: 210.0 / 25.4,
        height_inches: 297.0 / 25.4,
        margin_inches,
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
            || (w - layout.width_inches * 72.0).abs() > 2.0
            || (h - layout.height_inches * 72.0).abs() > 2.0
        {
            return Err("生成的 PDF 尺寸与演示快照不一致".into());
        }
    }
    Ok(())
}
fn publish_pdf(
    source: PathBuf,
    output: PathBuf,
    page: Option<PdfPage>,
) -> Result<PdfExport, String> {
    use std::io::{Read, Write};
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

    #[test]
    fn print_geometry_preserves_legacy_and_presentation_and_versions_word_margins() {
        let legacy = print_geometry(None, None).unwrap();
        assert_eq!(
            legacy,
            PdfGeometry {
                width_inches: 210.0 / 25.4,
                height_inches: 297.0 / 25.4,
                margin_inches: 20.0 / 25.4,
            }
        );
        assert_eq!(
            print_geometry(None, Some(rust_wps::document::STANDARD_STYLE_PROFILE)).unwrap(),
            PdfGeometry {
                margin_inches: 1.0,
                ..legacy
            },
        );
        let presentation = PdfPage {
            width_inches: 12.0,
            height_inches: 6.75,
            expected_pages: 3,
        };
        assert_eq!(
            print_geometry(Some(presentation), None).unwrap(),
            PdfGeometry {
                width_inches: 12.0,
                height_inches: 6.75,
                margin_inches: 0.0,
            }
        );
        assert!(
            print_geometry(
                Some(presentation),
                Some(rust_wps::document::STANDARD_STYLE_PROFILE)
            )
            .is_err()
        );
        for invalid in ["", "legacy", "standard-v1", "STANDARD-V2", "standard-v2 "] {
            assert!(print_geometry(None, Some(invalid)).is_err(), "{invalid}");
        }
        assert!(
            print_geometry(
                Some(PdfPage {
                    width_inches: f64::NAN,
                    ..presentation
                }),
                None
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
        assert!(publish_pdf(source, output.clone(), None).is_err());
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
                })
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
                })
            )
            .is_err()
        );
        assert!(!output.exists());
        let report = publish_pdf(source.clone(), output.clone(), Some(layout)).unwrap();
        assert!(report.bytes > 0);
        let original = std::fs::read(&output).unwrap();
        assert!(publish_pdf(source, output.clone(), None).is_err());
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
