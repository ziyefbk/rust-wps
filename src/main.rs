use anyhow::{Result, bail};
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let command = args.next();
    if command.as_deref() == Some(std::ffi::OsStr::new("rotate-pdf")) {
        let input = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing input PDF"))?,
        );
        let output = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing output PDF"))?,
        );
        let page = args
            .next()
            .ok_or_else(|| anyhow::anyhow!("Missing page"))?
            .to_string_lossy()
            .parse::<u32>()?;
        if args.next().is_some() {
            bail!("Unexpected extra arguments");
        }
        println!(
            "{}",
            serde_json::to_string(&rust_wps::pdf_rotation::rotate_file(&input, &output, page)?)?
        );
        return Ok(());
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("import-pptx")) {
        let path = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing PPTX path"))?,
        );
        if args.next().is_some() {
            bail!("Unexpected extra arguments");
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&rust_wps::presentation_import::import_pptx(&path)?)?
        );
        return Ok(());
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("inspect-ppt")) {
        let path = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing PPT path"))?,
        );
        if args.next().is_some() {
            bail!("Unexpected extra arguments");
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&rust_wps::presentation_ppt::inspect_ppt(&path)?)?
        );
        return Ok(());
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("read-pptx-model")) {
        let path = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing PPTX path"))?,
        );
        if args.next().is_some() {
            bail!("Unexpected extra arguments");
        }
        let model = rust_wps::presentation_model::load_authored_pptx(&path)?
            .ok_or_else(|| anyhow::anyhow!("No verified editable presentation model"))?;
        println!("{}", serde_json::to_string_pretty(&model)?);
        return Ok(());
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("create-pptx")) {
        use std::io::Read;
        let input = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing model JSON"))?,
        );
        let output = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing output PPTX"))?,
        );
        if args.next().is_some() {
            bail!("Unexpected extra arguments");
        }
        let mut bytes = Vec::new();
        std::fs::File::open(input)?
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        anyhow::ensure!(
            bytes.len() <= 4 * 1024 * 1024,
            "Presentation JSON exceeds 4 MiB"
        );
        let model = serde_json::from_slice(&bytes)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&rust_wps::presentation_model::export_pptx(
                &model, &output
            )?)?
        );
        return Ok(());
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("replace-pptx")) {
        let input = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing input"))?,
        );
        let output = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing output"))?,
        );
        let find = args
            .next()
            .ok_or_else(|| anyhow::anyhow!("Missing find text"))?;
        let replacement = args
            .next()
            .ok_or_else(|| anyhow::anyhow!("Missing replacement"))?;
        if args.next().is_some() {
            bail!("Unexpected extra arguments");
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&rust_wps::presentation_edit::replace_pptx(
                &input,
                &output,
                &find.to_string_lossy(),
                &replacement.to_string_lossy()
            )?)?
        );
        return Ok(());
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("inspect-pptx")) {
        let path = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing PPTX path"))?,
        );
        if args.next().is_some() {
            bail!("Unexpected extra arguments");
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&rust_wps::presentation::inspect_pptx(&path)?)?
        );
        return Ok(());
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("inspect-xls")) {
        let path = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing XLS path"))?,
        );
        if args.next().is_some() {
            bail!("Unexpected extra arguments");
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&rust_wps::spreadsheet_xls::inspect_xls(&path)?)?
        );
        return Ok(());
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("import-xlsx")) {
        let path = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing XLSX path"))?,
        );
        if args.next().is_some() {
            bail!("Unexpected extra arguments");
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&rust_wps::spreadsheet_import::import_xlsx(&path)?)?
        );
        return Ok(());
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("calculate-workbook")) {
        use std::io::Read;
        let input = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing workbook model JSON"))?,
        );
        if args.next().is_some() {
            bail!("Unexpected extra arguments");
        }
        let mut bytes = Vec::new();
        std::fs::File::open(input)?
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        anyhow::ensure!(
            bytes.len() <= 16 * 1024 * 1024,
            "Workbook JSON exceeds 16 MiB"
        );
        let model = serde_json::from_slice(&bytes)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&rust_wps::calculation::calculate(&model)?)?
        );
        return Ok(());
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("read-xlsx-model")) {
        let input = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing XLSX path"))?,
        );
        if args.next().is_some() {
            bail!("Unexpected extra arguments");
        }
        let model = rust_wps::spreadsheet_edit::load_authored_xlsx(&input)?.ok_or_else(|| {
            anyhow::anyhow!("Workbook has no verified editable model; use read-only inspection")
        })?;
        println!("{}", serde_json::to_string_pretty(&model)?);
        return Ok(());
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("create-xlsx")) {
        use std::io::Read;
        let input = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing workbook model JSON"))?,
        );
        let output = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing XLSX output path"))?,
        );
        if args.next().is_some() {
            bail!("Unexpected extra arguments");
        }
        let mut bytes = Vec::new();
        std::fs::File::open(input)?
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        anyhow::ensure!(
            bytes.len() <= 16 * 1024 * 1024,
            "Workbook JSON exceeds 16 MiB"
        );
        let model = serde_json::from_slice(&bytes)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&rust_wps::spreadsheet_edit::export_xlsx(
                &model, &output
            )?)?
        );
        return Ok(());
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("inspect-xlsx")) {
        let path = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing XLSX path"))?,
        );
        if args.next().is_some() {
            bail!("Unexpected extra arguments");
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&rust_wps::spreadsheet::inspect_xlsx(&path)?)?
        );
        return Ok(());
    }
    if matches!(
        command.as_deref().and_then(|s| s.to_str()),
        Some("import-docx" | "import-doc")
    ) {
        let path = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing DOCX path"))?,
        );
        if args.next().is_some() {
            bail!("Unexpected extra arguments");
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&if command.as_deref()
                == Some(std::ffi::OsStr::new("import-doc"))
            {
                rust_wps::importing::import_doc(&path)?
            } else {
                rust_wps::importing::import_docx(&path)?
            })?
        );
        return Ok(());
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("audit-docx")) {
        let path = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing DOCX path"))?,
        );
        if args.next().is_some() {
            bail!("Unexpected extra arguments");
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&rust_wps::compatibility::audit_docx(&path)?)?
        );
        return Ok(());
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("read-docx-model")) {
        let path = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing DOCX path"))?,
        );
        if args.next().is_some() {
            bail!("Unexpected extra arguments");
        }
        let model = rust_wps::document::load_authored_docx(&path)?
            .ok_or_else(|| anyhow::anyhow!("No verified editable document model"))?;
        println!("{}", serde_json::to_string_pretty(&model)?);
        return Ok(());
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("create-docx")) {
        let model = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing model JSON path"))?,
        );
        let output = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing output DOCX path"))?,
        );
        if args.next().is_some() {
            bail!("Unexpected extra arguments");
        }
        let bytes = std::fs::read(model)?;
        if bytes.len() > 32 * 1024 * 1024 {
            bail!("Model JSON exceeds 32 MiB");
        }
        let doc: rust_wps::document::Node = serde_json::from_slice(&bytes)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&rust_wps::document::export_docx(&doc, &output)?)?
        );
        return Ok(());
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("--help")) || command.is_none() {
        println!(
            "rust-wps rotate-pdf <input.pdf> <new-output.pdf> <page> (bounded clockwise 90-degree page rotation; source unchanged)"
        );
        println!("rust-wps inspect-xlsx <file.xlsx> (read-only; no formula recalculation)");
        println!("rust-wps inspect-xls <file.xls> (OLE BIFF8; read-only formula caches)");
        println!("rust-wps inspect-pptx <file.pptx> (read-only slide inventory; not rendering)");
        println!("rust-wps read-pptx-model <file.pptx> (verified self-authored model only)");
        println!(
            "rust-wps create-pptx <model.json> <new-output.pptx> (versioned authored text boxes)"
        );
        println!(
            "rust-wps replace-pptx <input.pptx> <new-output.pptx> <find> <replacement> (individual slide text nodes)"
        );
        println!(
            "rust-wps import-xlsx <file.xlsx> (audited basic copy; model is null when blocked)"
        );
        println!(
            "rust-wps calculate-workbook <model.json> (bounded numeric prototype; JSON results only)"
        );
        println!("rust-wps read-docx-model <authored.docx> (verified editable model only)");
        println!("rust-wps read-xlsx-model <authored.xlsx> (verified editable model only)");
        println!(
            "rust-wps create-xlsx <model.json> <new.xlsx> (typed sparse model; no formula recalculation)"
        );
        println!(
            "rust-wps inspect <file.doc|file.docx|file.pdf>\nrust-wps audit-docx <file.docx>\nrust-wps import-docx <file.docx>\nrust-wps import-doc <file.doc>\nrust-wps replace <input.docx> <new-output.docx> <find> <replacement>\nrust-wps create-docx <model.json> <new-output.docx>\nOutputs JSON reports. Existing files are never overwritten."
        );
        return Ok(());
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("replace")) {
        let input = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing input"))?,
        );
        let output = PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing output"))?,
        );
        let find = args
            .next()
            .ok_or_else(|| anyhow::anyhow!("Missing search text"))?
            .into_string()
            .map_err(|_| anyhow::anyhow!("Search must be Unicode"))?;
        let replacement = args
            .next()
            .ok_or_else(|| anyhow::anyhow!("Missing replacement"))?
            .into_string()
            .map_err(|_| anyhow::anyhow!("Replacement must be Unicode"))?;
        if args.next().is_some() {
            bail!("Unexpected extra arguments");
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&rust_wps::editing::replace_docx(
                &input,
                &output,
                &find,
                &replacement
            )?)?
        );
        return Ok(());
    }
    if command.as_deref() != Some(std::ffi::OsStr::new("inspect")) {
        bail!("Expected: inspect or replace; use --help");
    }
    let path = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("Missing input path"))?,
    );
    if args.next().is_some() {
        bail!("Unexpected extra arguments");
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&rust_wps::inspect(&path)?)?
    );
    Ok(())
}
