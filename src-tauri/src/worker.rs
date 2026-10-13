//! Parser fault isolation. This is a process boundary with a deadline and
//! response limit. Windows also applies a commit-memory quota; this is not a
//! filesystem sandbox or a CPU-time quota.
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Seek, Write},
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const MAX_RESPONSE: u64 = 32 * 1024 * 1024;
const MAX_CALCULATION_REQUEST: u64 = 16 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
enum CalculationResponse {
    Calculated {
        report: rust_wps::calculation::Report,
    },
    Error {
        message: String,
    },
}

pub fn entry_calculation() -> Result<(), String> {
    let mut input = Vec::new();
    std::io::stdin()
        .take(MAX_CALCULATION_REQUEST + 1)
        .read_to_end(&mut input)
        .map_err(|e| e.to_string())?;
    if input.len() as u64 > MAX_CALCULATION_REQUEST {
        return Err("Calculation request exceeds 16 MiB".into());
    }
    let result = serde_json::from_slice::<rust_wps::spreadsheet_edit::Workbook>(&input)
        .map_err(|e| e.to_string())
        .and_then(|model| rust_wps::calculation::calculate(&model).map_err(|e| format!("{e:#}")));
    let response = match result {
        Ok(report) => CalculationResponse::Calculated { report },
        Err(message) => CalculationResponse::Error { message },
    };
    let bytes = serde_json::to_vec(&response).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_RESPONSE {
        return Err("Calculation response exceeds 32 MiB".into());
    }
    std::io::stdout()
        .write_all(&bytes)
        .map_err(|e| e.to_string())
}

pub fn calculate(
    model: rust_wps::spreadsheet_edit::Workbook,
) -> Result<rust_wps::calculation::Report, String> {
    let bytes = serde_json::to_vec(&model).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_CALCULATION_REQUEST {
        return Err("Calculation request exceeds 16 MiB".into());
    }
    let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command.arg("--calculation-worker");
    let bytes = run_process(command, &bytes, Duration::from_secs(30), MAX_RESPONSE)?;
    match serde_json::from_slice::<CalculationResponse>(&bytes)
        .map_err(|e| format!("计算进程返回无效结果：{e}"))?
    {
        CalculationResponse::Calculated { report } => Ok(report),
        CalculationResponse::Error { message } => Err(message),
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Open {
        path: PathBuf,
    },
    Import {
        path: PathBuf,
    },
    ImportWorkbook {
        path: PathBuf,
    },
    ImportPresentation {
        path: PathBuf,
    },
    ReplacePresentation {
        path: PathBuf,
        output: PathBuf,
        find: String,
        replacement: String,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum Response {
    PresentationSaved {
        saved: crate::SavedPresentation,
    },
    Opened {
        document: crate::OpenedDocument,
    },
    Imported {
        report: rust_wps::importing::ImportReport,
    },
    WorkbookImported {
        report: rust_wps::spreadsheet_import::ImportReport,
    },
    PresentationImported {
        report: rust_wps::presentation_import::ImportReport,
    },
    Error {
        message: String,
    },
}

pub fn entry() -> Result<(), String> {
    let mut input = Vec::new();
    std::io::stdin()
        .take(65537)
        .read_to_end(&mut input)
        .map_err(|e| e.to_string())?;
    if input.len() > 65536 {
        return Err("Worker request exceeds 64 KiB".into());
    }
    let result = serde_json::from_slice::<Request>(&input)
        .map_err(|e| e.to_string())
        .and_then(execute);
    let response = result.unwrap_or_else(|message| Response::Error { message });
    let bytes = serde_json::to_vec(&response).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_RESPONSE {
        return Err("Parser response exceeds 32 MiB".into());
    }
    std::io::stdout()
        .write_all(&bytes)
        .map_err(|e| e.to_string())
}

fn execute(request: Request) -> Result<Response, String> {
    match request {
        Request::ReplacePresentation {
            path,
            output,
            find,
            replacement,
        } => {
            let report =
                rust_wps::presentation_edit::replace_pptx(&path, &output, &find, &replacement)
                    .map_err(|e| format!("{e:#}"))?;
            let presentation = rust_wps::presentation::inspect_pptx(&output)
                .map_err(|e| format!("File saved but preview failed: {e:#}"))?;
            Ok(Response::PresentationSaved {
                saved: crate::SavedPresentation {
                    path: output.to_string_lossy().into(),
                    report,
                    presentation,
                },
            })
        }
        Request::ImportPresentation { path } => Ok(Response::PresentationImported {
            report: rust_wps::presentation_import::import_pptx(&path)
                .map_err(|e| format!("{e:#}"))?,
        }),
        Request::ImportWorkbook { path } => Ok(Response::WorkbookImported {
            report: rust_wps::spreadsheet_import::import_xlsx(&path)
                .map_err(|e| format!("{e:#}"))?,
        }),
        Request::Open { path } => {
            if path
                .extension()
                .is_some_and(|s| s.eq_ignore_ascii_case("pptx") || s.eq_ignore_ascii_case("ppt"))
            {
                let started = Instant::now();
                let bytes = rust_wps::spreadsheet::read_xlsx_snapshot(&path)
                    .map_err(|e| format!("{e:#}"))?;
                let legacy = path
                    .extension()
                    .is_some_and(|s| s.eq_ignore_ascii_case("ppt"));
                let mut report = if legacy {
                    rust_wps::presentation_ppt::inspect_ppt_bytes(&bytes)
                } else {
                    rust_wps::presentation::inspect_pptx_bytes(&bytes)
                }
                .map_err(|e| format!("{e:#}"))?;
                let presentation_model = if legacy {
                    None
                } else {
                    match rust_wps::presentation_model::load_authored_pptx_bytes(&bytes) {
                        Ok(model) => model.map(Box::new),
                        Err(error) => {
                            report
                                .warnings
                                .push(format!("Editable presentation unavailable: {error:#}"));
                            None
                        }
                    }
                };
                let inspection = rust_wps::Inspection {
                    format: report.format.clone(),
                    backend: "office_oxide bounded presentation inventory".into(),
                    bytes: bytes.len() as u64,
                    elapsed_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
                    text: String::new(),
                    warnings: report.warnings.clone(),
                    text_error: None,
                };
                return Ok(Response::Opened {
                    document: crate::OpenedDocument {
                        name: path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into(),
                        path: path.to_string_lossy().into(),
                        inspection,
                        authored: None,
                        workbook: None,
                        workbook_model: None,
                        presentation: Some(Box::new(report)),
                        presentation_model,
                    },
                });
            }

            if path
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("xlsx") || s.eq_ignore_ascii_case("xls"))
            {
                let started = std::time::Instant::now();
                let bytes = rust_wps::spreadsheet::read_xlsx_snapshot(&path)
                    .map_err(|e| format!("{e:#}"))?;
                let legacy = path
                    .extension()
                    .is_some_and(|s| s.eq_ignore_ascii_case("xls"));
                let mut workbook = if legacy {
                    rust_wps::spreadsheet_xls::inspect_xls_bytes(&bytes)
                } else {
                    rust_wps::spreadsheet::inspect_xlsx_bytes(&bytes)
                }
                .map_err(|e| format!("{e:#}"))?;
                let workbook_model = if legacy {
                    None
                } else {
                    match rust_wps::spreadsheet_edit::load_authored_xlsx_bytes(&bytes) {
                        Ok(model) => model.map(Box::new),
                        Err(error) => {
                            workbook
                                .warnings
                                .push(format!("Editable workbook model unavailable: {error:#}"));
                            None
                        }
                    }
                };
                let inspection = rust_wps::Inspection {
                    format: if legacy { "xls" } else { "xlsx" }.into(),
                    backend: if legacy {
                        "office_oxide XLS + bounded BIFF8 inventory"
                    } else {
                        "office_oxide sparse XLSX"
                    }
                    .into(),
                    bytes: bytes.len() as u64,
                    elapsed_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
                    text: String::new(),
                    warnings: workbook.warnings.clone(),
                    text_error: None,
                };
                return Ok(Response::Opened {
                    document: crate::OpenedDocument {
                        name: path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into(),
                        path: path.to_string_lossy().into(),
                        inspection,
                        authored: None,
                        workbook: Some(Box::new(workbook)),
                        workbook_model,
                        presentation: None,
                        presentation_model: None,
                    },
                });
            }
            let mut inspection =
                rust_wps::inspect_for_preview(&path).map_err(|e| format!("{e:#}"))?;
            let authored = if inspection.format == "docx" {
                match rust_wps::document::load_authored_docx(&path) {
                    Ok(doc) => doc,
                    Err(error) => {
                        inspection
                            .warnings
                            .push(format!("Authoring model unavailable: {error:#}"));
                        None
                    }
                }
            } else {
                None
            };
            Ok(Response::Opened {
                document: crate::OpenedDocument {
                    name: path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into(),
                    path: path.to_string_lossy().into(),
                    inspection,
                    authored,
                    workbook: None,
                    workbook_model: None,
                    presentation: None,
                    presentation_model: None,
                },
            })
        }
        Request::Import { path } => {
            let report = match path
                .extension()
                .and_then(|s| s.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref()
            {
                Some("doc") => rust_wps::importing::import_doc(&path),
                Some("docx") => rust_wps::importing::import_docx(&path),
                _ => return Err("Only DOC/DOCX can be imported".into()),
            }
            .map_err(|e| format!("{e:#}"))?;
            Ok(Response::Imported { report })
        }
    }
}

pub fn run(request: Request) -> Result<Response, String> {
    let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command.arg("--document-worker");
    let response = run_process(
        command,
        &serde_json::to_vec(&request).map_err(|e| e.to_string())?,
        Duration::from_secs(30),
        MAX_RESPONSE,
    )?;
    let response: Response =
        serde_json::from_slice(&response).map_err(|e| format!("解析进程返回无效结果：{e}"))?;
    match response {
        Response::Error { message } => Err(message),
        other => Ok(other),
    }
}

fn run_process(
    mut command: Command,
    request: &[u8],
    timeout: Duration,
    limit: u64,
) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut capture = tempfile::tempfile().map_err(|e| e.to_string())?;
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::from(capture.try_clone().map_err(|e| e.to_string())?))
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("无法启动解析进程：{e}"))?;
    let started = Instant::now();
    #[cfg(windows)]
    let _job = match crate::worker_job::WorkerJob::attach(&child, 512 * 1024 * 1024) {
        Ok(job) => job,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("无法设置解析进程资源限制：{error}"));
        }
    };
    let Some(mut input) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("Missing worker input".into());
    };
    // Do not let a child that stalls before reading stdin block the deadline.
    let request = request.to_vec();
    let writer = std::thread::spawn(move || input.write_all(&request).map_err(|e| e.to_string()));
    let result = (|| {
        loop {
            if capture.metadata().map_err(|e| e.to_string())?.len() > limit {
                return Err("解析结果超出大小上限".into());
            }
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                if !status.success() {
                    return Err(format!("解析进程异常退出（{status}），当前文档已保留"));
                }
                break;
            }
            if started.elapsed() >= timeout {
                return Err("解析超时，已终止解析进程，当前文档已保留".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        capture.rewind().map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        capture
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > limit {
            return Err("解析结果超出大小上限".into());
        }
        Ok(bytes)
    })();
    if result.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    let written = writer
        .join()
        .map_err(|_| "Worker input thread failed".to_string())
        .and_then(|r| r);
    result.and_then(|bytes| written.map(|()| bytes))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn opened_response_roundtrips_through_the_process_protocol() {
        let response = Response::Opened {
            document: crate::OpenedDocument {
                name: "test.docx".into(),
                path: "test.docx".into(),
                inspection: rust_wps::Inspection {
                    format: "docx".into(),
                    backend: "test".into(),
                    bytes: 10,
                    elapsed_ms: 1,
                    text: "正文".into(),
                    warnings: vec![],
                    text_error: None,
                },
                authored: None,
                workbook: None,
                workbook_model: None,
                presentation: None,
                presentation_model: None,
            },
        };
        let bytes = serde_json::to_vec(&response).unwrap();
        assert!(serde_json::from_slice::<Response>(&bytes).is_ok());
    }
    fn shell(script: &str) -> Command {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
        command
    }
    #[test]
    fn hung_child_is_terminated_at_the_deadline() {
        let started = Instant::now();
        let result = run_process(
            shell("$null=[Console]::In.ReadToEnd(); Start-Sleep -Seconds 10"),
            b"request",
            Duration::from_millis(150),
            1024,
        );
        assert!(result.unwrap_err().contains("解析超时"));
        assert!(started.elapsed() < Duration::from_secs(5));
    }
    #[test]
    fn stalled_input_pipe_cannot_bypass_the_deadline() {
        let started = Instant::now();
        let result = run_process(
            shell("Start-Sleep -Seconds 10"),
            &vec![b'x'; 131072],
            Duration::from_millis(150),
            1024,
        );
        assert!(result.unwrap_err().contains("解析超时"));
        assert!(started.elapsed() < Duration::from_secs(5));
    }
    #[test]
    fn child_failure_is_not_treated_as_a_document() {
        let result = run_process(
            shell("$null=[Console]::In.ReadToEnd(); exit 7"),
            b"request",
            Duration::from_secs(5),
            1024,
        );
        assert!(result.unwrap_err().contains("异常退出"));
    }
    #[test]
    fn response_size_is_bounded_and_normal_output_is_returned() {
        let result = run_process(
            shell("$null=[Console]::In.ReadToEnd(); [Console]::Out.Write(('x'*8192))"),
            b"request",
            Duration::from_secs(5),
            1024,
        );
        assert!(result.unwrap_err().contains("大小上限"));
        let result = run_process(
            shell("$null=[Console]::In.ReadToEnd(); [Console]::Out.Write('ok')"),
            b"request",
            Duration::from_secs(5),
            1024,
        )
        .unwrap();
        assert_eq!(result, b"ok");
    }
}
