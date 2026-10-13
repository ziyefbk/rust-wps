#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::Manager;
mod document_protocol;
mod document_session;
mod pdf_export;
use document_protocol::{OpenedDocument, SavedPresentation};
use document_session::SessionState;
mod worker;
#[cfg(windows)]
mod worker_job;

#[derive(Default)]
struct Session(Arc<Mutex<SessionState>>);

#[tauri::command]
async fn replace_presentation_current(
    state: tauri::State<'_, Session>,
    find: String,
    replacement: String,
) -> Result<Option<SavedPresentation>, String> {
    let (ticket, path) = {
        let mut session = state.0.lock().map_err(|e| e.to_string())?;
        let path = session.current_path().ok_or("请先打开 PPTX")?;
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("pptx"))
        {
            return Err("当前文件不是 PPTX".into());
        }
        let ticket = session.reserve()?;
        (ticket, path)
    };
    let mut saved = tauri::async_runtime::spawn_blocking(
        move || -> Result<Option<SavedPresentation>, String> {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            let Some(output) = rfd::FileDialog::new()
                .add_filter("演示文稿", &["pptx"])
                .set_file_name(format!("{stem}-修改.pptx"))
                .save_file()
            else {
                return Ok(None);
            };
            if !output
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pptx"))
            {
                return Err("请使用 .pptx 扩展名".into());
            }
            match worker::run(worker::Request::ReplacePresentation {
                path,
                output,
                find,
                replacement,
            })? {
                worker::Response::PresentationSaved { saved } => Ok(Some(saved)),
                _ => Err("替换进程返回意外结果".into()),
            }
        },
    )
    .await
    .map_err(|e| e.to_string())??;
    if let Some(saved) = &mut saved {
        let token = state
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .commit_non_pdf(ticket, PathBuf::from(&saved.path))
            .map_err(|error| format!("文件已另存至 {}，但未切换当前文档：{error}", saved.path))?;
        saved.session_token = Some(token);
    }
    Ok(saved)
}

#[derive(Serialize)]
struct SavedDocument {
    path: String,
    report: rust_wps::editing::EditReport,
}

#[tauri::command]
async fn import_workbook_current(
    state: tauri::State<'_, Session>,
) -> Result<rust_wps::spreadsheet_import::ImportReport, String> {
    let path = state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .current_path()
        .ok_or("请先打开 XLSX")?;
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("xlsx"))
    {
        return Err("当前文件不是 XLSX".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        match worker::run(worker::Request::ImportWorkbook { path })? {
            worker::Response::WorkbookImported { report } => Ok(report),
            _ => Err("解析进程返回了错误类型".into()),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn import_presentation_current(
    state: tauri::State<'_, Session>,
) -> Result<rust_wps::presentation_import::ImportReport, String> {
    let path = state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .current_path()
        .ok_or("请先打开 PPTX")?;
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pptx"))
    {
        return Err("当前文件不是 PPTX".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        match worker::run(worker::Request::ImportPresentation { path })? {
            worker::Response::PresentationImported { report } => Ok(report),
            _ => Err("解析进程返回了错误类型".into()),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn import_current(
    state: tauri::State<'_, Session>,
) -> Result<rust_wps::importing::ImportReport, String> {
    let path = state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .current_path()
        .ok_or("请先打开 DOC 或 DOCX")?;
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("docx") || e.eq_ignore_ascii_case("doc"))
    {
        return Err("当前文件不是 DOC 或 DOCX".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        match worker::run(worker::Request::Import { path })? {
            worker::Response::Imported { report } => Ok(report),
            _ => Err("解析进程返回了错误类型".into()),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn open_document(state: tauri::State<'_, Session>) -> Result<Option<OpenedDocument>, String> {
    let ticket = state.0.lock().map_err(|e| e.to_string())?.reserve()?;
    let result = tauri::async_runtime::spawn_blocking(|| {
        #[cfg(feature = "native-smoke")]
        let test_path = std::env::var_os("RUST_WPS_SMOKE_OPEN_INPUT").map(PathBuf::from);
        #[cfg(not(feature = "native-smoke"))]
        let test_path: Option<PathBuf> = None;
        let Some(path) = test_path.or_else(|| {
            rfd::FileDialog::new()
                .add_filter(
                    "办公文档",
                    &["doc", "docx", "pdf", "xlsx", "xls", "pptx", "ppt"],
                )
                .pick_file()
        }) else {
            return Ok(None);
        };
        let (opened, pdf_identity) =
            match worker::run(worker::Request::Open { path: path.clone() })? {
                worker::Response::Opened {
                    document,
                    pdf_identity,
                } => (document, pdf_identity),
                _ => return Err("解析进程返回了错误类型".into()),
            };
        Ok::<_, String>(Some((path, opened, pdf_identity)))
    })
    .await
    .map_err(|e| e.to_string())??;
    if let Some((path, mut opened, pdf_identity)) = result {
        let token = state.0.lock().map_err(|e| e.to_string())?.commit_open(
            ticket,
            path,
            &opened,
            pdf_identity,
        )?;
        opened.session_token = Some(token);
        Ok(Some(opened))
    } else {
        Ok(None)
    }
}

#[tauri::command]
async fn replace_current(
    state: tauri::State<'_, Session>,
    find: String,
    replacement: String,
) -> Result<Option<SavedDocument>, String> {
    let path = state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .current_path()
        .ok_or("请先打开文档")?;
    tauri::async_runtime::spawn_blocking(move || {
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let Some(output) = rfd::FileDialog::new()
            .add_filter("Word 文档", &["docx"])
            .set_file_name(format!("{stem}-修改.docx"))
            .save_file()
        else {
            return Ok(None);
        };
        let report = rust_wps::editing::replace_docx(&path, &output, &find, &replacement)
            .map_err(|e| format!("{e:#}"))?;
        Ok(Some(SavedDocument {
            path: output.to_string_lossy().into(),
            report,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn read_current_pdf(
    state: tauri::State<'_, Session>,
    session_token: String,
) -> Result<tauri::ipc::Response, String> {
    let session = Arc::clone(&state.0);
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        document_session::read_pdf(&session, &session_token)
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(bytes))
}

fn main() {
    if std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "--calculation-worker")
    {
        if worker::entry_calculation().is_err() {
            std::process::exit(2);
        }
        return;
    }
    if std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "--document-worker")
    {
        if worker::entry().is_err() {
            std::process::exit(2);
        }
        return;
    }
    tauri::Builder::default()
        .manage(Session::default())
        .setup(|_app| {
            #[cfg(feature = "native-smoke")]
            if std::env::var_os("RUST_WPS_SMOKE_PDF_OUTPUT").is_some()
                && let Some(window) = _app.get_webview_window("main")
            {
                window.hide()?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_document,
            pdf_export::export_current_pdf,
            import_current,
            import_workbook_current,
            import_presentation_current,
            replace_current,
            replace_presentation_current,
            save_new_presentation,
            inspect_presentation_picture,
            validate_presentation,
            save_presentation_draft,
            load_presentation_draft,
            read_current_pdf,
            save_new_document,
            save_draft,
            load_draft,
            save_workbook_draft,
            load_workbook_draft,
            save_new_workbook,
            calculate_workbook
        ])
        .run(tauri::generate_context!())
        .expect("Desktop application failed");
}

#[derive(Serialize)]
struct ExportedDocument {
    path: String,
    report: rust_wps::document::ExportReport,
}

#[tauri::command]
async fn save_new_document(
    doc: rust_wps::document::Node,
) -> Result<Option<ExportedDocument>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        rust_wps::document::validate(&doc).map_err(|e| format!("{e:#}"))?;
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Word 文档", &["docx"])
            .set_file_name("新文档.docx")
            .save_file()
        else {
            return Ok(None);
        };
        let report = rust_wps::document::export_docx(&doc, &path).map_err(|e| format!("{e:#}"))?;
        Ok(Some(ExportedDocument {
            path: path.to_string_lossy().into(),
            report,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_draft(app: tauri::AppHandle, doc: rust_wps::document::Node) -> Result<(), String> {
    let path = draft_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        rust_wps::document::save_draft(&doc, &path).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn load_draft(app: tauri::AppHandle) -> Result<Option<rust_wps::document::Node>, String> {
    let path = draft_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        rust_wps::document::load_draft(&path).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

fn draft_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    #[cfg(feature = "native-smoke")]
    if let Some(path) = std::env::var_os("RUST_WPS_SMOKE_PDF_OUTPUT") {
        let path = PathBuf::from(path);
        return Ok(path
            .parent()
            .ok_or("Missing native test directory")?
            .join("draft-v1.json"));
    }
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("draft-v1.json"))
}

#[derive(Serialize)]
struct ExportedWorkbook {
    path: String,
    report: rust_wps::spreadsheet::WorkbookReport,
}

#[tauri::command]
async fn calculate_workbook(
    model: rust_wps::spreadsheet_edit::Workbook,
) -> Result<rust_wps::calculation::Report, String> {
    tauri::async_runtime::spawn_blocking(move || worker::calculate(model))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_new_workbook(
    model: rust_wps::spreadsheet_edit::Workbook,
) -> Result<Option<ExportedWorkbook>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        rust_wps::spreadsheet_edit::validate(&model).map_err(|e| format!("{e:#}"))?;
        #[cfg(feature = "native-smoke")]
        let test_path = std::env::var_os("RUST_WPS_SMOKE_XLSX_OUTPUT").map(PathBuf::from);
        #[cfg(not(feature = "native-smoke"))]
        let test_path: Option<PathBuf> = None;
        let Some(path) = test_path.or_else(|| {
            rfd::FileDialog::new()
                .add_filter("Excel 工作簿", &["xlsx"])
                .set_file_name("新工作簿.xlsx")
                .save_file()
        }) else {
            return Ok(None);
        };
        let report =
            rust_wps::spreadsheet_edit::export_xlsx(&model, &path).map_err(|e| format!("{e:#}"))?;
        Ok(Some(ExportedWorkbook {
            path: path.to_string_lossy().into(),
            report,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_workbook_draft(
    app: tauri::AppHandle,
    model: rust_wps::spreadsheet_edit::Workbook,
) -> Result<(), String> {
    let path = draft_path(&app)?.with_file_name("workbook-draft-v1.json");
    tauri::async_runtime::spawn_blocking(move || {
        rust_wps::spreadsheet_edit::save_draft(&model, &path).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn load_workbook_draft(
    app: tauri::AppHandle,
) -> Result<Option<rust_wps::spreadsheet_edit::Workbook>, String> {
    let path = draft_path(&app)?.with_file_name("workbook-draft-v1.json");
    tauri::async_runtime::spawn_blocking(move || {
        rust_wps::spreadsheet_edit::load_draft(&path).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn validate_presentation(
    model: rust_wps::presentation_model::Presentation,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || model.validate().map_err(|e| format!("{e:#}")))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn inspect_presentation_picture(data_url: String) -> Result<[u32; 2], String> {
    tauri::async_runtime::spawn_blocking(move || {
        rust_wps::presentation_model::inspect_picture(&data_url).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_new_presentation(
    model: rust_wps::presentation_model::Presentation,
) -> Result<Option<SavedPresentation>, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Option<SavedPresentation>, String> {
        model.validate().map_err(|e| format!("{e:#}"))?;
        #[cfg(feature = "native-smoke")]
        let test_path = std::env::var_os("RUST_WPS_SMOKE_PPTX_OUTPUT").map(PathBuf::from);
        #[cfg(not(feature = "native-smoke"))]
        let test_path: Option<PathBuf> = None;
        let Some(path) = test_path.or_else(|| {
            rfd::FileDialog::new()
                .add_filter("PPTX", &["pptx"])
                .set_file_name("presentation.pptx")
                .save_file()
        }) else {
            return Ok(None);
        };
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("pptx"))
        {
            return Err("请使用 .pptx 扩展名".into());
        }
        let presentation = rust_wps::presentation_model::export_pptx(&model, &path)
            .map_err(|e| format!("{e:#}"))?;
        Ok(Some(SavedPresentation {
            session_token: None,
            path: path.to_string_lossy().into(),
            presentation,
            report: rust_wps::editing::EditReport {
                replacements: 0,
                changed_parts: vec![],
                preserved_parts: 0,
                warnings: vec![],
            },
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_presentation_draft(
    app: tauri::AppHandle,
    model: rust_wps::presentation_model::Presentation,
) -> Result<(), String> {
    let path = draft_path(&app)?.with_file_name("presentation-draft-v1.json");
    tauri::async_runtime::spawn_blocking(move || {
        rust_wps::presentation_model::save_draft(&model, &path).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn load_presentation_draft(
    app: tauri::AppHandle,
) -> Result<Option<rust_wps::presentation_model::Presentation>, String> {
    let path = draft_path(&app)?.with_file_name("presentation-draft-v1.json");
    tauri::async_runtime::spawn_blocking(move || {
        rust_wps::presentation_model::load_draft(&path).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}
