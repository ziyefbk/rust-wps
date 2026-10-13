//! Side stories must not silently disappear when creating an editable copy.
use rust_wps::{compatibility, importing};
use std::{collections::BTreeMap, fs, io::Write, path::Path};

const TRANSITIONAL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const STRICT: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships/";
const WML: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn fixture(path: &Path, story: &str, target: &str, relation: &str, reference: &str) {
    let target_path = office_oxide::core::opc::PartName::new("/word/document.xml")
        .unwrap()
        .resolve_relative(target)
        .unwrap();
    let (body_reference, section_reference) = if reference.contains("footnoteReference")
        || reference.contains("endnoteReference")
        || reference.contains("commentReference")
    {
        (format!("<w:p><w:r>{reference}</w:r></w:p>"), String::new())
    } else if reference.contains("commentRange") {
        (
            format!("<w:p>{reference}<w:r><w:t>Commented text</w:t></w:r></w:p>"),
            String::new(),
        )
    } else {
        (String::new(), reference.to_owned())
    };
    let mut parts = BTreeMap::from([
        (
            "[Content_Types].xml".to_owned(),
            format!(
                r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="{}" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.{story}+xml"/></Types>"#,
                target_path.as_str()
            ),
        ),
        (
            "_rels/.rels".into(),
            format!(
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="office" Type="{TRANSITIONAL}officeDocument" Target="word/document.xml"/></Relationships>"#
            ),
        ),
        (
            "word/_rels/document.xml.rels".into(),
            format!(
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{relation}</Relationships>"#
            ),
        ),
        (
            "word/document.xml".into(),
            format!(
                r#"<w:document xmlns:w="{WML}" xmlns:r="{TRANSITIONAL}"><w:body><w:p><w:r><w:t>BodyProbe</w:t></w:r></w:p>{body_reference}<w:sectPr>{section_reference}</w:sectPr></w:body></w:document>"#
            ),
        ),
    ]);
    let root = match story {
        "header" => "hdr",
        "footer" => "ftr",
        other => other,
    };
    parts.insert(target_path.as_str().trim_start_matches('/').into(), format!(r#"<w:{root} xmlns:w="{WML}"><w:p><w:r><w:t>SideStoryProbe</w:t></w:r></w:p></w:{root}>"#));
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, xml) in parts {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

fn relation(prefix: &str, story: &str, target: &str, mode: &str) -> String {
    format!(r#"<Relationship Id="side" Type="{prefix}{story}" Target="{target}"{mode}/>"#)
}

fn assert_refused(path: &Path) {
    let before = fs::read(path).unwrap();
    let audit = compatibility::audit_docx(path).unwrap();
    assert!(
        audit
            .risks
            .iter()
            .any(|risk| risk.starts_with("Additional document content:")),
        "{audit:?}"
    );
    let error = importing::import_docx(path).unwrap_err().to_string();
    assert!(error.contains("Additional document content:"), "{error}");
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn renamed_headers_and_footers_are_refused_for_all_reference_variants() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("renamed.docx");
    for story in ["header", "footer"] {
        for variant in ["default", "first", "even"] {
            for target in ["side.xml", "../stories/part.xml", "/stories/part.bin"] {
                fixture(
                    &path,
                    story,
                    target,
                    &relation(TRANSITIONAL, story, target, ""),
                    &format!(r#"<w:{story}Reference w:type="{variant}" r:id="side"/>"#),
                );
                assert_refused(&path);
            }
        }
    }
}

#[test]
fn normalized_strict_relationships_and_unused_stories_are_refused() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("strict.docx");
    for prefix in [TRANSITIONAL, STRICT] {
        for story in ["header", "footer", "footnotes", "endnotes", "comments"] {
            fixture(
                &path,
                story,
                "../stories/part.bin",
                &relation(prefix, story, "../stories/part.bin", ""),
                "",
            );
            assert_refused(&path);
        }
    }
}

#[test]
fn missing_external_and_wrong_type_relationships_do_not_bypass_references() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("unresolved.docx");
    for reference in [
        "headerReference",
        "footerReference",
        "footnoteReference",
        "endnoteReference",
        "commentReference",
        "commentRangeStart",
        "commentRangeEnd",
    ] {
        for rel in [
            String::new(),
            relation(TRANSITIONAL, "image", "side.xml", ""),
            relation(TRANSITIONAL, "header", "missing.xml", ""),
            relation(
                TRANSITIONAL,
                "footer",
                "https://example.invalid/story",
                r#" TargetMode="External""#,
            ),
        ] {
            fixture(
                &path,
                "header",
                "side.xml",
                &rel,
                &format!(r#"<w:{reference} r:id="side"/>"#),
            );
            assert_refused(&path);
        }
    }
}

#[test]
fn malformed_reference_without_id_also_fails_closed() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("missing-id.docx");
    for reference in ["headerReference", "footerReference"] {
        fixture(
            &path,
            "header",
            "side.xml",
            "",
            &format!("<w:{reference}/>"),
        );
        assert_refused(&path);
    }
}

#[test]
fn downstream_ir_really_keeps_content_that_body_only_conversion_would_drop() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ir.docx");
    for story in ["header", "footer"] {
        fixture(
            &path,
            story,
            "side.xml",
            &relation(TRANSITIONAL, story, "side.xml", ""),
            &format!(r#"<w:{story}Reference w:type="default" r:id="side"/>"#),
        );
        let parsed = office_oxide::Document::from_reader(
            std::io::Cursor::new(fs::read(&path).unwrap()),
            office_oxide::DocumentFormat::Docx,
        )
        .unwrap();
        let ir = parsed.to_ir();
        assert!(
            serde_json::to_string(&ir)
                .unwrap()
                .contains("SideStoryProbe")
        );
        assert!(
            !serde_json::to_string(&ir.sections[0].elements)
                .unwrap()
                .contains("SideStoryProbe")
        );
        assert_refused(&path);
    }
}

#[test]
fn unrelated_relationships_and_simple_body_are_still_eligible() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("simple.docx");
    // An unrelated URI suffix is not a standard side-story relationship.
    fixture(
        &path,
        "header",
        "side.xml",
        &relation("https://example.invalid/custom/", "header", "side.xml", ""),
        "",
    );
    let before = fs::read(&path).unwrap();
    let report = importing::import_docx(&path).unwrap();
    assert!(
        serde_json::to_string(&report.document)
            .unwrap()
            .contains("BodyProbe")
    );
    assert!(
        !report
            .compatibility
            .risks
            .iter()
            .any(|risk| risk.starts_with("Additional document content:"))
    );
    assert_eq!(fs::read(&path).unwrap(), before);
}
