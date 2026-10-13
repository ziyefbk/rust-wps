// Standalone transport for actual production core/worker/session modules.
// It does not compile or exercise Tauri commands, dialogs, IPC or a WebView.
#![allow(dead_code)]
#[path = "../src-tauri/src/document_protocol.rs"]
mod document_protocol;
#[path = "../src-tauri/src/document_session.rs"]
mod document_session;
#[path = "../src-tauri/src/worker.rs"]
mod worker;
#[cfg(windows)]
#[path = "../src-tauri/src/worker_job.rs"]
mod worker_job;
use document_protocol::{OpenedDocument, SavedPresentation};
use std::{io::Write, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.as_slice() == ["--document-worker"] {
        return worker::entry().map_err(Into::into);
    }
    let (command, args) = args.split_first().ok_or("Missing command")?;
    match (command.as_str(), args) {
        ("preview", [path]) => println!(
            "{}",
            serde_json::to_string(&rust_wps::inspect_for_preview_with_identity(Path::new(
                path
            ))?)?
        ),
        ("inspect-preview", [path]) => println!(
            "{}",
            serde_json::to_string(&rust_wps::inspect_for_preview(Path::new(path))?)?
        ),
        ("open", [path]) => println!(
            "{}",
            serde_json::to_string(&worker::run(worker::Request::Open { path: path.into() })?)?
        ),
        ("verify", [path, identity]) => {
            let identity = serde_json::from_str(identity)?;
            let bytes = rust_wps::read_verified_pdf(Path::new(path), &identity)?;
            std::io::stdout().write_all(&bytes)?;
        }
        ("validate-open", [path, envelope]) => {
            let worker::Response::Opened {
                document,
                pdf_identity,
            } = serde_json::from_str(envelope)?
            else {
                return Err("Expected opened response".into());
            };
            let mut session = document_session::SessionState::default();
            let ticket = session.reserve()?;
            let token = session.commit_open(ticket, path.into(), &document, pdf_identity)?;
            println!("{}", serde_json::json!({"session_token":token}));
        }
        _ => return Err("Unexpected command or arguments".into()),
    }
    Ok(())
}
