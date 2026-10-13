use rust_wps::calculation::{Value, calculate};
use rust_wps::spreadsheet_edit::{Cell, Content, Sheet, Workbook};

fn cell(row: u32, column: u32, content: Content) -> Cell {
    Cell {
        row,
        column,
        content,
        number_format: None,
    }
}
fn formula(row: u32, text: &str) -> Cell {
    cell(row, 1, Content::Formula(text.into()))
}
fn book(cells: Vec<Cell>) -> Workbook {
    Workbook {
        version: 1,
        date1904: false,
        sheets: vec![Sheet {
            name: "Data".into(),
            hidden: false,
            cells,
        }],
    }
}

#[test]
fn scalar_math_handles_negative_rounding_zero_dependencies_and_errors() {
    let samples = [
        ("SQRT(16)", 4.),
        ("SQRT(0)", 0.),
        ("SQRT(ABS(-9))", 3.),
        ("INT(8.9)", 8.),
        ("INT(-8.9)", -9.),
        ("INT(-8)", -8.),
        ("SIGN(-0)", 0.),
        ("SIGN(0)", 0.),
        ("SIGN(-0.00001)", -1.),
        ("SIGN(12)", 1.),
        ("sqrt(A1)", 5.),
        ("INT(FALSE)", 0.),
        ("SQRT(A100)", 0.),
        ("IFERROR(SQRT(-1),42)", 42.),
        ("IF(FALSE,SQRT(-1),SIGN(A1))", 1.),
    ];
    let mut cells = vec![cell(0, 0, Content::Number(25.))];
    cells.extend(
        samples
            .iter()
            .enumerate()
            .map(|(i, (text, _))| formula(i as u32, text)),
    );
    let report = calculate(&book(cells)).unwrap();
    assert!(report.complete);
    for (result, (_, expected)) in report.cells.iter().zip(samples) {
        assert_eq!(
            result.value,
            Some(Value::Number(expected)),
            "{}",
            result.formula
        );
    }
    for text in ["SQRT(-1)", "SQRT(-1e300)"] {
        let report = calculate(&book(vec![formula(0, text)])).unwrap();
        assert!(report.complete);
        assert_eq!(report.cells[0].failure.as_ref().unwrap().code, "#NUM!");
    }
    for text in [
        "SQRT()",
        "INT(1,2)",
        "SIGN(1,2)",
        "INT(\"2\")",
        "SQRT(A1:A2)",
    ] {
        let report = calculate(&book(vec![formula(0, text)])).unwrap();
        assert!(!report.complete);
        assert_eq!(
            report.cells[0].failure.as_ref().unwrap().kind,
            "unsupported"
        );
    }
    for text in ["SQRT(1/0)", "INT(1/0)", "SIGN(1/0)"] {
        let report = calculate(&book(vec![formula(0, text)])).unwrap();
        assert_eq!(report.cells[0].failure.as_ref().unwrap().code, "#DIV/0!");
    }
    let report = calculate(&book(vec![formula(0, "SQRT(B1)")])).unwrap();
    assert_eq!(report.cells[0].failure.as_ref().unwrap().kind, "cycle");
}

#[test]
fn operators_precedence_and_constants() {
    let samples = [
        ("1+2*3", 7.),
        ("(1+2)*3", 9.),
        ("8/4/2", 1.),
        ("-2^2", 4.),
        ("2^3^2", 64.),
        ("20%*50", 10.),
        ("-20%", -0.2),
        ("1.2e2+3E-1", 120.3),
        ("TRUE+FALSE", 1.),
        ("ABS(-3)", 3.),
        ("SUM(1,TRUE,\"5\")", 7.),
        ("AVERAGE(1,3)", 2.),
        ("MIN(9,2,3)", 2.),
        ("MAX(9,2,3)", 9.),
        ("COUNT(1,TRUE,\"5\")", 3.),
    ];
    let model = book(
        samples
            .iter()
            .enumerate()
            .map(|(i, (formula_text, _))| formula(i as u32, formula_text))
            .collect(),
    );
    let report = calculate(&model).unwrap();
    assert!(report.complete);
    for (result, (_, expected)) in report.cells.iter().zip(samples) {
        assert_eq!(
            result.value,
            Some(Value::Number(expected)),
            "{}",
            result.formula
        );
    }
}

#[test]
fn dependencies_cross_sheet_ranges_and_types() {
    let mut model = book(vec![
        cell(0, 0, Content::Number(10.)),
        cell(1, 0, Content::Text("5".into())),
        cell(2, 0, Content::Boolean(true)),
        formula(0, "SUM(A1:A4)"),
        formula(1, "B1*2"),
        formula(2, "AVERAGE(A1:A4)"),
        formula(3, "COUNT(A1:A4)"),
        formula(4, "A4+1"),
        formula(5, "SUM(A3:A1)"),
    ]);
    model.sheets.push(Sheet {
        name: "中'文".into(),
        hidden: true,
        cells: vec![formula(0, "Data!$B$2+1"), formula(1, "'中''文'!B1*2")],
    });
    let original = model.clone();
    let report = calculate(&model).unwrap();
    assert!(report.complete);
    assert_eq!(model, original);
    assert_eq!(
        report
            .cells
            .iter()
            .map(|cell| cell.value.clone())
            .collect::<Vec<_>>(),
        [10., 20., 10., 1., 1., 10., 21., 42.].map(|n| Some(Value::Number(n)))
    );
}

#[test]
fn errors_propagate_but_count_ignores_error_cells() {
    let model = book(vec![
        formula(0, "1/0"),
        formula(1, "B1+1"),
        formula(2, "SUM(B1:B2)"),
        formula(3, "COUNT(B1:B2)"),
        formula(4, "Missing!A1"),
        formula(5, "0^0"),
        formula(6, "1e308*10"),
        formula(7, "AVERAGE(A1:A2)"),
        formula(8, "#REF!A1+1"),
    ]);
    let report = calculate(&model).unwrap();
    assert!(report.complete);
    for index in [0, 1, 2, 7] {
        assert_eq!(
            report.cells[index].failure.as_ref().unwrap().code,
            "#DIV/0!"
        );
    }
    assert_eq!(report.cells[3].value, Some(Value::Number(0.)));
    for index in [4, 8] {
        assert_eq!(report.cells[index].failure.as_ref().unwrap().code, "#REF!");
    }
    for index in [5, 6] {
        assert_eq!(report.cells[index].failure.as_ref().unwrap().code, "#NUM!");
    }
}

#[test]
fn unsupported_syntax_never_uses_fake_results() {
    let formulas = [
        "UNKNOWN(1,0)",
        "Table[Amount]",
        "[Book.xlsx]Data!A1",
        "'[Book.xlsx]Data'!A1",
        "SUM(Data:Other!A1)",
        "SUM(A:A)",
        "R1C1",
        "Revenue",
        "A1 A2",
        "SUM(A1;A2)",
        "A$$1",
        "A1$",
        "'TRUE'",
        "\"5\"+1",
        "1+",
        "SUM()",
        "1&2",
    ];
    let report = calculate(&book(
        formulas
            .iter()
            .enumerate()
            .map(|(i, text)| formula(i as u32, text))
            .collect(),
    ))
    .unwrap();
    assert!(!report.complete);
    for cell in &report.cells {
        assert!(cell.value.is_none(), "{}", cell.formula);
        assert_eq!(
            cell.failure.as_ref().unwrap().kind,
            "unsupported",
            "{}",
            cell.formula
        );
    }
}

#[test]
fn cycles_and_dependent_cells_are_explicit_and_do_not_hide_independent_results() {
    let report = calculate(&book(vec![
        formula(0, "B2+1"),
        formula(1, "B1+1"),
        formula(2, "B1*2"),
        formula(3, "2+3"),
    ]))
    .unwrap();
    assert!(!report.complete);
    for index in 0..3 {
        assert_eq!(report.cells[index].failure.as_ref().unwrap().kind, "cycle");
    }
    assert_eq!(report.cells[3].value, Some(Value::Number(5.)));
}

#[test]
fn deep_formulas_dependencies_and_large_ranges_are_bounded() {
    let deep = format!("{}1{}", "(".repeat(70), ")".repeat(70));
    let report = calculate(&book(vec![
        formula(0, &deep),
        formula(1, "SUM(A1:A100001)"),
    ]))
    .unwrap();
    assert!(!report.complete);
    for cell in report.cells {
        assert_eq!(cell.failure.unwrap().kind, "limit");
    }
    let chain = book(
        (0..100)
            .map(|i| formula(i, &format!("B{}+1", i + 2)))
            .collect(),
    );
    assert!(!calculate(&chain).unwrap().complete);
    let wide = book(vec![
        formula(0, "SUM(A1:A100000)"),
        formula(1, "SUM(A1:A100000)"),
        formula(2, "SUM(A1:A100000)"),
        formula(3, "SUM(A1:A100000)"),
        formula(4, "SUM(A1:A100000)"),
        formula(5, "SUM(A1:A100000)"),
        formula(6, "SUM(A1:A100000)"),
        formula(7, "SUM(A1:A100000)"),
        formula(8, "SUM(A1:A100000)"),
        formula(9, "SUM(A1:A100000)"),
    ]);
    let report = calculate(&wide).unwrap();
    assert!(!report.complete);
    assert!(report.operations <= 1_000_000);
}

#[test]
fn cli_calculates_budget_example_without_mutating_input() {
    let input =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/examples/workbook-v1.json");
    let before = std::fs::read(&input).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rust-wps"))
        .arg("calculate-workbook")
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["complete"], true);
    assert_eq!(report["cells"][0]["value"]["value"], 1280.);
    assert_eq!(std::fs::read(input).unwrap(), before);
}

#[test]
fn formula_references_to_empty_cells_produce_numeric_zero() {
    let report = calculate(&book(vec![
        formula(0, "A1"),
        formula(1, "COUNT(B1)"),
        formula(2, "SUM(A1)"),
        formula(3, "SUM(Unknown(XFE1))"),
    ]))
    .unwrap();
    assert_eq!(report.cells[0].value, Some(Value::Number(0.)));
    assert_eq!(report.cells[1].value, Some(Value::Number(1.)));
    assert_eq!(report.cells[2].value, Some(Value::Number(0.)));
    assert_eq!(
        report.cells[3].failure.as_ref().unwrap().kind,
        "unsupported"
    );
}

#[test]
fn combined_expression_and_dependency_depth_is_bounded() {
    let model = book(
        (0..60)
            .map(|i| {
                formula(
                    i,
                    &format!("{}B{}{}", "ABS(".repeat(20), i + 2, ")".repeat(20)),
                )
            })
            .collect(),
    );
    let report = calculate(&model).unwrap();
    assert!(!report.complete);
    assert!(report.cells.iter().any(|cell| {
        cell.failure
            .as_ref()
            .is_some_and(|failure| failure.kind == "limit")
    }));
}

#[test]
fn invalid_models_are_rejected_before_calculation() {
    let model = book(vec![
        cell(0, 0, Content::Number(f64::NAN)),
        formula(0, "A1+1"),
    ]);
    assert!(calculate(&model).is_err());
    let model = book(vec![formula(0, "1+1"), formula(0, "2+2")]);
    assert!(calculate(&model).is_err());
}

#[test]
fn if_selects_one_branch_and_preserves_result_types() {
    let report = calculate(&book(vec![
        formula(0, "IF(TRUE,12,1/0)"),
        formula(1, "IF(FALSE,1/0,\"否\")"),
        formula(2, "IF(0,1)"),
        formula(3, "IF(-2,TRUE,FALSE)"),
        formula(4, "IF(TRUE,5,B5)"),
        formula(5, "IF(FALSE,B6,7)"),
        formula(6, "IF(TRUE,IF(FALSE,1,2),3)"),
        formula(7, "IF(TRUE,1/0,2)"),
        formula(8, "IF(1/0,1,2)"),
    ]))
    .unwrap();
    assert!(report.complete);
    assert_eq!(report.cells[0].value, Some(Value::Number(12.)));
    assert_eq!(report.cells[1].value, Some(Value::Text("否".into())));
    assert_eq!(report.cells[2].value, Some(Value::Boolean(false)));
    assert_eq!(report.cells[3].value, Some(Value::Boolean(true)));
    assert_eq!(report.cells[4].value, Some(Value::Number(5.)));
    assert_eq!(report.cells[5].value, Some(Value::Number(7.)));
    assert_eq!(report.cells[6].value, Some(Value::Number(2.)));
    for index in [7, 8] {
        assert_eq!(
            report.cells[index].failure.as_ref().unwrap().code,
            "#DIV/0!"
        );
    }
}

#[test]
fn numeric_comparisons_feed_lazy_if_branches() {
    let formulas = [
        "IF(1+2*3>=7,10,1/0)",
        "IF(2<>3,11,0)",
        "IF(2<=2,12,0)",
        "IF(3<2,0,13)",
        "IF(3>2,14,0)",
        "IF(2=2,15,0)",
        "IF(FALSE=FALSE,16,0)",
        "IF(A1=0,17,0)",
        "IF(TRUE,18,SUM(A1:A100001))",
    ];
    let report = calculate(&book(
        formulas
            .iter()
            .enumerate()
            .map(|(i, s)| formula(i as u32, s))
            .collect(),
    ))
    .unwrap();
    assert!(report.complete);
    for (i, cell) in report.cells.iter().enumerate() {
        assert_eq!(
            cell.value,
            Some(Value::Number(10. + i as f64)),
            "{}",
            cell.formula
        );
    }
    let report = calculate(&book(vec![
        formula(0, "IF(\"a\"=\"A\",1,0)"),
        formula(1, "TRUE=1"),
    ]))
    .unwrap();
    assert!(!report.complete);
    assert!(
        report
            .cells
            .iter()
            .all(|c| c.failure.as_ref().unwrap().kind == "unsupported")
    );
}

#[test]
fn logical_functions_and_error_fallback() {
    let samples = [
        ("AND(TRUE,2,1<3)", Value::Boolean(true)),
        ("OR(FALSE,0,NOT(FALSE))", Value::Boolean(true)),
        ("NOT(2)", Value::Boolean(false)),
        ("IFERROR(1/0,42)", Value::Number(42.)),
        ("IFERROR(3,1/0)", Value::Number(3.)),
        ("IFERROR(A5,42)", Value::Text(String::new())),
        ("IFERROR(1/0,A5)", Value::Text(String::new())),
        ("AND(A1:A4)", Value::Boolean(true)),
        ("OR(A1:A4)", Value::Boolean(true)),
    ];
    let mut cells = vec![
        cell(0, 0, Content::Number(2.)),
        cell(1, 0, Content::Boolean(true)),
        cell(2, 0, Content::Text("ignored".into())),
    ];
    cells.extend(
        samples
            .iter()
            .enumerate()
            .map(|(i, (s, _))| formula(i as u32, s)),
    );
    let report = calculate(&book(cells)).unwrap();
    assert!(report.complete);
    for (result, (_, expected)) in report.cells.iter().zip(samples) {
        assert_eq!(result.value, Some(expected), "{}", result.formula);
    }
}
#[test]
fn logical_errors_remain_visible() {
    for (text, kind, code) in [
        ("AND(FALSE,1/0)", "error", "#DIV/0!"),
        ("OR(TRUE,1/0)", "error", "#DIV/0!"),
        ("AND(A1:A4)", "error", "#VALUE!"),
        ("OR(A1:A4)", "error", "#VALUE!"),
        ("IFERROR(B1,0)", "cycle", "CIRCULAR_REFERENCE"),
        ("IFERROR(SUM(A1:A100001),0)", "limit", "CALCULATION_LIMIT"),
        (
            "IFERROR(UNKNOWN(1),0)",
            "unsupported",
            "UNSUPPORTED_FORMULA",
        ),
        ("NOT(1,2)", "unsupported", "UNSUPPORTED_FORMULA"),
        ("IFERROR(1)", "unsupported", "UNSUPPORTED_FORMULA"),
    ] {
        let report = calculate(&book(vec![formula(0, text)])).unwrap();
        let failure = report.cells[0].failure.as_ref().unwrap();
        assert_eq!((&*failure.kind, &*failure.code), (kind, code), "{text}");
    }
}
