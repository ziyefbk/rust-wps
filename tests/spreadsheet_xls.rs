use rust_wps::spreadsheet::Value;
use rust_wps::spreadsheet_xls::inspect_xls_bytes;
const SOURCE: &[u8] = include_bytes!("fixtures/basic-biff8.xls");

#[test]
fn biff8_inventory_preserves_types_formats_visibility_and_marks_formula_caches() {
    let report = inspect_xls_bytes(SOURCE).unwrap();
    assert_eq!(report.format, "xls");
    assert!(!report.recalculated);
    assert!(!report.date1904);
    assert_eq!(report.sheets.len(), 2);
    assert_eq!(report.sheets[1].visibility, "VeryHidden");
    let cells = &report.sheets[0].cells;
    let at = |address: &str| cells.iter().find(|cell| cell.address == address).unwrap();
    assert!(matches!(at("A1").value, Value::Number(12.5)));
    assert_eq!(at("A1").number_format.as_deref(), Some("0.00"));
    assert!(matches!(at("B1").value, Value::Number(25.)));
    assert!(at("B1").formula_unavailable);
    assert!(at("B1").formula.is_none());
    assert_eq!(at("B1").formula_cache.as_deref(), Some("present"));
    assert!(at("C2").formula_unavailable);
    assert_eq!(at("C2").formula_cache.as_deref(), Some("absent"));
    assert!(matches!(at("C1").value, Value::Boolean(true)));
    assert!(matches!(&at("E1").value,Value::Error(error) if error=="#DIV/0!"));
    assert!(matches!(&at("D2").value,Value::Text(text) if text=="中文"));
    assert_eq!(at("D3").number_format.as_deref(), Some("0.00\"元\""));
    assert_eq!(at("IV65536").row, 65535);
    assert_eq!(at("IV65536").column, 255);
    assert_eq!(report.sheets[0].merged_ranges, vec!["A3:B3"]);
    let json = serde_json::to_value(&report).unwrap();
    assert!(
        json["sheets"][0]["cells"][0]
            .get("formula_unavailable")
            .is_none()
    );
}
#[test]
fn rejects_disguised_raw_biff_truncated_and_oversized_inputs() {
    assert!(inspect_xls_bytes(b"<html>old.xls</html>").is_err());
    assert!(inspect_xls_bytes(&[9, 8, 16, 0, 0, 6, 5, 0]).is_err());
    assert!(inspect_xls_bytes(&SOURCE[..512]).is_err());
    assert!(inspect_xls_bytes(&vec![0; 64 * 1024 * 1024 + 1]).is_err());
}
