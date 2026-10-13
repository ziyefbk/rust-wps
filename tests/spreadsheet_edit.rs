use rust_wps::spreadsheet_edit::load_authored_xlsx;
use rust_wps::spreadsheet_edit::{Cell, Content, Sheet, Workbook, export_xlsx, validate};
use std::io::{Cursor, Read, Write};

fn model() -> Workbook {
    Workbook {
        version: 1,
        date1904: false,
        sheets: vec![
            Sheet {
                name: "中文数据".into(),
                hidden: false,
                cells: vec![
                    Cell {
                        row: 0,
                        column: 0,
                        content: Content::Text(" 中文<&>😀\n第二行 ".into()),
                        number_format: None,
                    },
                    Cell {
                        row: 0,
                        column: 1,
                        content: Content::Number(12.5),
                        number_format: Some("0.00".into()),
                    },
                    Cell {
                        row: 0,
                        column: 2,
                        content: Content::Boolean(true),
                        number_format: None,
                    },
                    Cell {
                        row: 0,
                        column: 3,
                        content: Content::Formula("B1*2".into()),
                        number_format: None,
                    },
                    Cell {
                        row: 1,
                        column: 0,
                        content: Content::Empty,
                        number_format: Some("0.000".into()),
                    },
                    Cell {
                        row: 1048575,
                        column: 16383,
                        content: Content::Text("最远单元格".into()),
                        number_format: None,
                    },
                ],
            },
            Sheet {
                name: "隐藏".into(),
                hidden: true,
                cells: vec![],
            },
        ],
    }
}

#[test]
fn exports_sparse_typed_values_styles_and_uncached_formulas_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("新工作簿.xlsx");
    let report = export_xlsx(&model(), &output).unwrap();
    assert_eq!(report.sheets[0].cells.len(), 6);
    assert_eq!(
        report.sheets[0]
            .cells
            .iter()
            .find(|cell| cell.address == "D1")
            .unwrap()
            .formula_cache
            .as_deref(),
        Some("absent")
    );
    let original = std::fs::read(&output).unwrap();
    assert!(export_xlsx(&model(), &output).is_err());
    assert_eq!(std::fs::read(&output).unwrap(), original);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn invalid_models_do_not_publish_partial_files() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("bad.xlsx");
    let reject = |book: Workbook| {
        assert!(export_xlsx(&book, &output).is_err());
        assert!(!output.exists());
    };
    let mut book = model();
    let duplicate = book.sheets[0].cells[0].clone();
    book.sheets[0].cells.push(duplicate);
    reject(book);
    let mut book = model();
    book.sheets[0].cells[0].row = 1048576;
    reject(book);
    let mut book = model();
    book.sheets[0].cells[1].content = Content::Number(f64::NAN);
    reject(book);
    let mut book = model();
    book.sheets[0].cells[0].content = Content::Text("\0".into());
    reject(book);
    let mut book = model();
    book.sheets[0].cells[3].content = Content::Formula("=B1*2".into());
    reject(book);
    let mut book = model();
    book.sheets[0].hidden = true;
    reject(book);
    let mut book = model();
    book.sheets[1].name = "中文数据".into();
    reject(book);
    let mut book = model();
    book.sheets[0].name = "bad/name".into();
    reject(book);
    let mut book = model();
    book.version = 2;
    reject(book);
    let mut book = model();
    book.sheets[0].cells[0].content = Content::Text("a".repeat(32768));
    reject(book);
    assert!(
        validate(&Workbook {
            version: 1,
            date1904: false,
            sheets: vec![]
        })
        .is_err()
    );
    assert!(
        serde_json::from_str::<Workbook>(r#"{"version":1,"sheets":[],"unknown":true}"#).is_err()
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

fn rewrite(bytes: &[u8], target: &str, replacement: Option<&[u8]>, extra: bool) -> Vec<u8> {
    let mut source = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut result = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..source.len() {
        let mut part = source.by_index(index).unwrap();
        let mut data = Vec::new();
        part.read_to_end(&mut data).unwrap();
        let name = part.name();
        if name == target && replacement.is_none() {
            continue;
        }
        result
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        result
            .write_all(if name == target {
                replacement.unwrap()
            } else {
                &data
            })
            .unwrap();
    }
    if extra {
        result
            .start_file(
                "external-change.xml",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        result.write_all(b"<change/>").unwrap();
    }
    result.finish().unwrap().into_inner()
}

#[test]
fn authored_model_reopens_and_rejects_stale_content_metadata_and_model_changes() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("authored.xlsx");
    export_xlsx(&model(), &source).unwrap();
    let original = std::fs::read(&source).unwrap();
    assert_eq!(load_authored_xlsx(&source).unwrap(), Some(model()));
    // Changing only compression is harmless; compare uncompressed OPC parts.
    let repacked = rewrite(&original, "nonexistent", Some(b""), false);
    std::fs::write(&source, repacked).unwrap();
    assert_eq!(load_authored_xlsx(&source).unwrap(), Some(model()));
    for (target, replacement, extra) in [
        (
            "xl/worksheets/sheet1.xml",
            Some(b"<worksheet/>".as_slice()),
            false,
        ),
        ("xl/styles.xml", Some(b"<styleSheet/>".as_slice()), false),
        ("xl/workbook.xml", Some(b"<workbook/>".as_slice()), false),
        (
            "xl/_rels/workbook.xml.rels",
            Some(b"<Relationships/>".as_slice()),
            false,
        ),
        ("xl/styles.xml", None, false),
        ("unused", Some(b"".as_slice()), true),
    ] {
        let changed = rewrite(&original, target, replacement, extra);
        std::fs::write(&source, &changed).unwrap();
        assert!(
            load_authored_xlsx(&source).is_err(),
            "Accepted changed part: {target}"
        );
        assert_eq!(std::fs::read(&source).unwrap(), changed);
    }
    let mut changed_model = model();
    changed_model.sheets[0].cells[0].content = Content::Text("模型单独被修改".into());
    let changed = rewrite(
        &original,
        "rust-wps/workbook-v1.json",
        Some(&serde_json::to_vec(&changed_model).unwrap()),
        false,
    );
    std::fs::write(&source, changed).unwrap();
    assert!(load_authored_xlsx(&source).is_err());
    let external = rewrite(&original, "rust-wps/workbook-v1.json", None, false);
    std::fs::write(&source, external).unwrap();
    assert_eq!(load_authored_xlsx(&source).unwrap(), None);
}

#[test]
fn preview_and_editable_model_can_share_one_bounded_source_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot.xlsx");
    export_xlsx(&model(), &path).unwrap();
    let snapshot = rust_wps::spreadsheet::read_xlsx_snapshot(&path).unwrap();
    std::fs::write(&path, b"changed after snapshot").unwrap();
    assert_eq!(
        rust_wps::spreadsheet::inspect_xlsx_bytes(&snapshot)
            .unwrap()
            .sheets[0]
            .name,
        "中文数据"
    );
    assert_eq!(
        rust_wps::spreadsheet_edit::load_authored_xlsx_bytes(&snapshot).unwrap(),
        Some(model())
    );
    assert!(load_authored_xlsx(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"changed after snapshot");
}

#[test]
fn workbook_draft_replacement_recovers_latest_valid_model_and_preserves_on_failure() {
    use rust_wps::spreadsheet_edit::{load_draft, save_draft};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("drafts/workbook-v1.json");
    assert_eq!(load_draft(&path).unwrap(), None);
    save_draft(&model(), &path).unwrap();
    assert_eq!(load_draft(&path).unwrap(), Some(model()));
    let mut edited = model();
    edited.sheets[0].cells[1].content = Content::Number(99.5);
    save_draft(&edited, &path).unwrap();
    assert_eq!(load_draft(&path).unwrap(), Some(edited.clone()));
    let before = std::fs::read(&path).unwrap();
    edited.version = 999;
    assert!(save_draft(&edited, &path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    std::fs::write(&path, b"{truncated").unwrap();
    assert!(load_draft(&path).is_err());
    let large = std::fs::File::create(&path).unwrap();
    large.set_len(16 * 1024 * 1024 + 1).unwrap();
    assert!(load_draft(&path).is_err());
    assert_eq!(
        std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
        1
    );
}
