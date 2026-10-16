use rust_wps::PdfActualTextStatus;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

fn cases() -> Vec<Value> {
    serde_json::from_str::<Value>(include_str!("fixtures/pdf-actualtext/manifest.json"))
        .unwrap()["cases"].as_array().unwrap().clone()
}
fn fixture(case: &Value) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pdf-actualtext")
        .join(case["path"].as_str().unwrap())
}
fn fold(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

#[test]
fn real_and_independent_actual_text_preserves_exact_authored_unicode() {
    for case in cases().iter().filter(|c| c["expected_status"] == "applied") {
        let path = fixture(case);
        let before = fs::read(&path).unwrap();
        let report = rust_wps::inspect_for_preview_with_identity(&path).unwrap();
        assert_eq!(
            report.inspection.pdf_actual_text,
            Some(PdfActualTextStatus::Applied),
            "{}: {:?}",
            case["name"],
            report.inspection
        );
        assert!(report.inspection.text_error.is_none(), "{}", case["name"]);
        if case["expected_text"] == "" {
            assert_eq!(report.inspection.text, "", "{}", case["name"]);
        }
        assert_eq!(
            fold(&report.inspection.text),
            fold(case["expected_text"].as_str().unwrap()),
            "{}",
            case["name"]
        );
        for literal in case["exact_contains"].as_array().unwrap() {
            assert!(
                report.inspection.text.contains(literal.as_str().unwrap()),
                "{}: {:?}",
                case["name"],
                report.inspection.text
            );
        }
        assert_eq!(
            rust_wps::read_verified_pdf(&path, report.pdf_identity.as_ref().unwrap()).unwrap(),
            before
        );
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[test]
fn absent_actual_text_preserves_legacy_output_exactly_including_forms_and_type3() {
    for case in cases()
        .iter()
        .filter(|c| c["expected_status"] == "not_detected")
    {
        let path = fixture(case);
        let source = fs::read(&path).unwrap();
        let legacy = pdf_extract::extract_text_from_mem(&source).unwrap();
        let report = rust_wps::inspect(&path).unwrap();
        assert_eq!(
            report.pdf_actual_text,
            Some(PdfActualTextStatus::NotDetected),
            "{}: {:?}",
            case["name"],
            report
        );
        assert_eq!(report.text, legacy, "{} changed legacy text", case["name"]);
        assert_eq!(
            fold(&report.text),
            fold(case["expected_text"].as_str().unwrap()),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn known_unsupported_actual_text_has_no_glyph_fallback_and_keeps_original_preview() {
    for case in cases()
        .iter()
        .filter(|c| c["expected_status"] == "unsupported")
    {
        let path = fixture(case);
        let source = fs::read(&path).unwrap();
        assert!(rust_wps::inspect(&path).is_err(), "{}", case["name"]);
        let report = rust_wps::inspect_for_preview_with_identity(&path).unwrap();
        assert_eq!(
            report.inspection.pdf_actual_text,
            Some(PdfActualTextStatus::Unsupported),
            "{}: {:?}",
            case["name"],
            report.inspection
        );
        assert!(report.inspection.text.is_empty(), "{}", case["name"]);
        assert!(report.inspection.text_error.is_some(), "{}", case["name"]);
        assert_eq!(
            rust_wps::read_verified_pdf(&path, report.pdf_identity.as_ref().unwrap()).unwrap(),
            source
        );
    }
}

#[test]
fn unresolved_semantics_is_unknown_never_a_claim_of_absence() {
    for case in cases().iter().filter(|c| c["expected_status"] == "unknown") {
        let path = fixture(case);
        let report = rust_wps::inspect_for_preview_with_identity(&path).unwrap();
        assert_eq!(
            report.inspection.pdf_actual_text,
            Some(PdfActualTextStatus::Unknown),
            "{}: {:?}",
            case["name"],
            report.inspection
        );
        assert!(report.inspection.text_error.is_some(), "{}", case["name"]);
        if case["preserve_legacy_text"] == true {
            let legacy = pdf_extract::extract_text_from_mem(&fs::read(&path).unwrap()).unwrap();
            assert_eq!(report.inspection.text, legacy, "{}", case["name"]);
            assert_eq!(
                fold(&report.inspection.text),
                fold(case["expected_text"].as_str().unwrap()),
                "{}",
                case["name"]
            );
        }
        assert!(
            report
                .inspection
                .warnings
                .iter()
                .any(|s| s.contains("provisional"))
        );
    }
}

#[test]
fn frozen_inputs_match_source_receipts_and_are_not_synthetic_roundtrips() {
    let cases = cases();
    assert!(
        cases
            .iter()
            .filter(|c| c["name"].as_str().unwrap().starts_with("chromium-"))
            .count()
            >= 3
    );
    for case in cases {
        let source = fs::read(fixture(&case)).unwrap();
        assert_eq!(source.len() as u64, case["bytes"].as_u64().unwrap());
        assert_eq!(
            format!("{:x}", Sha256::digest(&source)),
            case["sha256"].as_str().unwrap(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn actual_text_evidence_serializes_strictly_and_is_optional_for_older_reports() {
    for (status, expected) in [
        (PdfActualTextStatus::NotDetected, "not_detected"),
        (PdfActualTextStatus::Applied, "applied"),
        (PdfActualTextStatus::Unsupported, "unsupported"),
        (PdfActualTextStatus::Unknown, "unknown"),
    ] {
        assert_eq!(
            serde_json::to_string(&status).unwrap(),
            format!("\"{expected}\"")
        );
    }
    assert!(serde_json::from_str::<PdfActualTextStatus>("\"verified\"").is_err());
    let old = r#"{"format":"pdf","backend":"old","bytes":5,"elapsed_ms":0,"text":"text","warnings":[],"text_error":null}"#;
    assert!(
        serde_json::from_str::<rust_wps::Inspection>(old)
            .unwrap()
            .pdf_actual_text
            .is_none()
    );
}
