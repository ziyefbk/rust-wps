// Standalone transport through actual production worker/session modules.
// No Tauri commands, IPC, save dialogs, browser or native WebView are exercised.
#![allow(dead_code)]
#[path = "../src-tauri/src/document_protocol.rs"]
mod document_protocol;
#[path = "../src-tauri/src/document_session.rs"]
mod document_session;
#[path = "../src-tauri/src/rotation_session.rs"]
mod rotation_session;
#[path = "../src-tauri/src/worker.rs"]
mod worker;
#[cfg(windows)]
#[path = "../src-tauri/src/worker_job.rs"]
mod worker_job;
use document_protocol::{OpenedDocument, SavedPresentation};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

fn publish(
    mode: &str,
    path: &str,
    output: &str,
    selection: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let worker::Response::Opened {
        document,
        pdf_identity,
    } = worker::run(worker::Request::Open { path: path.into() })?
    else {
        return Err("Expected opened response".into());
    };
    let mut state = document_session::SessionState::default();
    let ticket = state.reserve()?;
    let token = state.commit_open(ticket, path.into(), &document, pdf_identity)?;
    let session = Mutex::new(state);
    let selection = serde_json::from_str(selection)?;
    let saved = rotation_session::rotate_current(&session, &token, selection, |_| {
        if mode == "cancel" {
            return None;
        }
        if mode == "publish-stale" {
            let mut state = session.lock().expect("session mutex");
            let ticket = state.reserve().expect("reserve newer selection");
            state
                .commit_non_pdf(ticket, PathBuf::from("newer-document.docx"))
                .expect("commit newer selection");
        }
        Some(output.into())
    })?;
    let source_unchanged =
        session.lock().map_err(|e| e.to_string())?.current_path() == Some(path.into());
    println!(
        "{}",
        serde_json::json!({"saved":saved,"session_token":token,"session_source_unchanged":source_unchanged})
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.as_slice() == ["--document-worker"] {
        return worker::entry().map_err(Into::into);
    }
    let (command, args) = args.split_first().ok_or("Missing command")?;
    match (command.as_str(), args) {
        ("publish" | "publish-stale", [path, output, selection]) => {
            publish(command, path, output, selection)?
        }
        ("cancel", [path, selection]) => publish(command, path, "", selection)?,
        ("request", [request]) => {
            let request = serde_json::from_str::<worker::Request>(request)?;
            println!("{}", serde_json::to_string(&worker::run(request)?)?);
        }
        ("preview", [path]) => println!(
            "{}",
            serde_json::to_string(&rust_wps::inspect_for_preview_with_identity(Path::new(
                path
            ))?)?
        ),
        ("verify", [path, identity]) => {
            let identity = serde_json::from_str(identity)?;
            let bytes = rust_wps::read_verified_pdf(Path::new(path), &identity)?;
            std::io::stdout().write_all(&bytes)?;
        }
        ("rotate", [path, output, selection]) => {
            let source = std::fs::read(path)?;
            let selection = serde_json::from_str::<rust_wps::pdf_rotation::Selection>(selection)?;
            let bytes = rust_wps::pdf_rotation::rotate(&source, &selection)?;
            // The probe never overwrites any file, even after a successful core call.
            let mut destination = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)?;
            destination.write_all(&bytes)?;
            destination.sync_all()?;
        }
        _ => return Err("Unexpected command or arguments".into()),
    }
    Ok(())
}
