//! Read-only acceptance adapter using the production preview/snapshot API.
//! No UI mock, source rewrite, output file, or alternative parser is involved.
use anyhow::{Result, ensure};
use std::path::PathBuf;
fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("Missing PDF path"))?,
    );
    ensure!(args.next().is_none(), "Unexpected extra arguments");
    let report = rust_wps::inspect_for_preview_with_identity(&path)?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}
