use rust_wps::presentation_ppt::inspect_ppt;
use sha2::{Digest, Sha256};
use std::path::Path;

#[test]
fn real_textbox_master_and_encrypted_decks_have_explicit_results() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/apache-poi");
    let report = inspect_ppt(&root.join("with_textbox.ppt")).unwrap();
    assert_eq!(report.slides.len(), 1);
    assert_eq!(
        report.slides[0].text,
        "Hello, World!!!\nI am just a poor boy\nThis is Times New Roman\nPlain Text \n"
    );
    let report = inspect_ppt(&root.join("WithMaster.ppt")).unwrap();
    assert_eq!(report.slides.len(), 2);
    assert_eq!(
        report.master_text,
        "This text comes from the Master Slide\n"
    );
    assert!(!report.master_text.contains("This is the Master Title"));
    assert!(!report.slides[0].text.contains("Master Slide"));
    let error = inspect_ppt(&root.join("Password_Protected-hello.ppt")).unwrap_err();
    assert!(error.to_string().contains("Encrypted"));
}

#[test]
fn real_poi_deck_matches_upstream_page_content_without_mutating_source() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/apache-poi/basic_test_ppt_file.ppt");
    let before = std::fs::read(&path).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&before)),
        "e8fd4c38ad72ce3735acbca0b60dd5870fb60f819c169e3947d166f4f1bcc79d"
    );
    let report = inspect_ppt(&path).unwrap();
    assert_eq!(report.format, "ppt");
    assert_eq!(report.slides[0].notes, "These are the notes for page 1\n");
    assert_eq!(
        report.slides[1].notes,
        "These are the notes on page two, again lacking formatting\n"
    );
    assert_eq!(report.size_emu, None);
    assert_eq!(report.slides.len(), 2);
    // Apache POI TestExtractor EXPECTED_PAGE1/2; blank paragraph spacing
    // is deliberately normalized by this text inventory.
    assert_eq!(
        report.slides[0].text.lines().collect::<Vec<_>>(),
        [
            "This is a test title",
            "This is a test subtitle",
            "This is on page 1"
        ]
    );
    assert_eq!(
        report.slides[1].text.lines().collect::<Vec<_>>(),
        [
            "This is the title on page 2",
            "This is page two",
            "It has several blocks of text",
            "None of them have formatting"
        ]
    );
    assert!(
        report
            .slides
            .iter()
            .all(|s| !s.hidden && s.shapes.is_empty())
    );
    assert!(!report.has_macros);
    assert!(!report.warnings.is_empty());
    assert_eq!(std::fs::read(path).unwrap(), before);
}
