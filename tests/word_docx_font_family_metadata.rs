//! External DOCX family declarations are a complete, source-bound metadata group.
//! These fixtures intentionally use invented literal names: installed-font/name
//! heuristics, source typography repair and rendering are outside this contract.
use rust_wps::{
    compatibility,
    document::{self, Node},
    importing::{self, ImportReport},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::Path,
};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const P: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const C: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
const SUCCESS: &str = "Source-declared Roman/Swiss classifications preserved for proved Latin text and empty-paragraph fonts; other font metadata, bare-break defaults and local substitution remain unproved";
const PREFIX: &str = "DOCX font-family metadata not preserved: ";
const PACKAGE: &str = "package identity is unproved";
const TABLE: &str = "font-table declarations are malformed or ambiguous";
const BODY: &str = "body content is outside the supported Latin subset";
const IDENTITY: &str = "effective font or style/theme identity is unproved";
const MISSING: &str = "a retained font has no supported source classification";
const ALIAS: &str = "a retained name has duplicate or ASCII-case-alias definitions";
const LIMIT: &str = "proof resource limits exceeded";
const BINDING: &str = "source and imported font ranges differ";
const COLOR_SCHEME: &str =
    r#"<a:clrScheme name="Proof"><a:dk1><a:srgbClr val="000000"/></a:dk1></a:clrScheme>"#;
const A: &str = "Absent Swiss 8Q";
const B: &str = "Absent Roman 9R";
const BREAK: &str = r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#;
type Parts = BTreeMap<String, Vec<u8>>;

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn fonts(name: &str) -> String {
    let name = escape(name);
    format!(r#"<w:rFonts w:ascii="{name}" w:hAnsi="{name}"/>"#)
}
fn run(name: &str, text: &str) -> String {
    format!(
        "<w:r><w:rPr>{}</w:rPr><w:t>{}</w:t></w:r>",
        fonts(name),
        escape(text)
    )
}
fn paragraph(name: &str, text: &str) -> String {
    format!("<w:p>{}</w:p>", run(name, text))
}
fn defaults(properties: &str) -> String {
    format!(
        "<w:docDefaults><w:rPrDefault><w:rPr>{properties}</w:rPr></w:rPrDefault></w:docDefaults>"
    )
}
fn definition(name: &str, family: Option<&str>) -> String {
    format!(
        r#"<w:font w:name="{}">{}</w:font>"#,
        escape(name),
        family
            .map(|family| format!(r#"<w:family w:val="{family}"/>"#))
            .unwrap_or_default()
    )
}
fn table(definitions: &str) -> String {
    format!(
        r#"<w:fonts xmlns:w="{W}" xmlns:x="urn:foreign" xmlns:same="{W}">{definitions}</w:fonts>"#
    )
}
fn pair() -> String {
    table(&format!(
        "{}{}",
        definition(B, Some("roman")),
        definition(A, Some("swiss"))
    ))
}
fn expected_pair() -> Value {
    json!([{"name":B,"family":"roman"},{"name":A,"family":"swiss"}])
}
fn relation(kind: &str, target: &str) -> String {
    format!(r#"<Relationship Id="{kind}" Type="{R}/{kind}" Target="{target}"/>"#)
}
fn content_type(kind: &str) -> &'static str {
    match kind {
        "fontTable" => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml"
        }
        "styles" => "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml",
        "theme" => "application/vnd.openxmlformats-officedocument.theme+xml",
        "settings" => "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml",
        _ => panic!("unknown fixture role"),
    }
}
fn replace(parts: &mut Parts, name: &str, before: &str, after: &str) {
    let xml = String::from_utf8(parts[name].clone()).unwrap();
    assert!(
        xml.contains(before),
        "fixture replacement missing {before:?} in {name}"
    );
    parts.insert(name.into(), xml.replace(before, after).into_bytes());
}
fn add_role(parts: &mut Parts, kind: &str, target: &str, physical: &str, xml: &str) {
    replace(
        parts,
        "word/_rels/document.xml.rels",
        "</Relationships>",
        &format!("{}</Relationships>", relation(kind, target)),
    );
    replace(
        parts,
        "[Content_Types].xml",
        "</Types>",
        &format!(
            r#"<Override PartName="/{physical}" ContentType="{}"/></Types>"#,
            content_type(kind)
        ),
    );
    assert!(
        parts
            .insert(physical.into(), xml.as_bytes().to_vec())
            .is_none()
    );
}
fn package(body: &str, styles: &str) -> Parts {
    let mut parts = BTreeMap::from([
        ("[Content_Types].xml".into(), format!(r#"<Types xmlns="{C}"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#).into_bytes()),
        ("_rels/.rels".into(), format!(r#"<Relationships xmlns="{P}">{}</Relationships>"#, relation("officeDocument", "word/document.xml")).into_bytes()),
        ("word/_rels/document.xml.rels".into(), format!(r#"<Relationships xmlns="{P}"></Relationships>"#).into_bytes()),
        ("word/document.xml".into(), format!(r#"<w:document xmlns:w="{W}" xmlns:x="urn:foreign" xmlns:same="{W}" xmlns:r="{R}"><w:body>{body}</w:body></w:document>"#).into_bytes()),
    ]);
    if !styles.is_empty() {
        add_role(
            &mut parts,
            "styles",
            "styles.xml",
            "word/styles.xml",
            &format!(
                r#"<w:styles xmlns:w="{W}" xmlns:x="urn:foreign" xmlns:same="{W}">{styles}</w:styles>"#
            ),
        );
    }
    parts
}
fn with_table(base: &Parts, xml: &str) -> Parts {
    let mut parts = base.clone();
    add_role(
        &mut parts,
        "fontTable",
        "fontTable.xml",
        "word/fontTable.xml",
        xml,
    );
    parts
}
fn write(path: &Path, parts: &Parts) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, bytes) in parts {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}
fn read(path: &Path) -> Parts {
    let mut zip = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut entry = zip.by_index(i).unwrap();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            (entry.name().to_owned(), bytes)
        })
        .collect()
}
fn import(parts: &Parts) -> ImportReport {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("external.docx");
    write(&path, parts);
    let before = fs::read(&path).unwrap();
    let result = importing::import_docx(&path).unwrap();
    assert_eq!(
        fs::read(&path).unwrap(),
        before,
        "source must remain untouched"
    );
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    assert!(!result.compatibility.fidelity_verified);
    result
}
fn stripped(mut report: ImportReport, diagnostic: Option<&str>) -> ImportReport {
    report.document.attrs.remove("fontFamilyMetadata");
    if let Some(diagnostic) = diagnostic {
        assert!(
            report.compatibility.risks.remove(diagnostic),
            "missing exact diagnostic {diagnostic:?}; actual {:?}",
            report.compatibility.risks
        );
    }
    assert!(!report.compatibility.risks.contains(SUCCESS));
    assert!(
        !report
            .compatibility
            .risks
            .iter()
            .any(|risk| risk.starts_with(PREFIX))
    );
    report
}
fn exact_diagnostic_delta(base: &ImportReport, actual: &ImportReport, diagnostic: Option<&str>) {
    assert!(!base.document.attrs.contains_key("fontFamilyMetadata"));
    assert!(!base.compatibility.risks.contains(SUCCESS));
    assert!(
        !base
            .compatibility
            .risks
            .iter()
            .any(|risk| risk.starts_with(PREFIX))
    );
    let mut expected = base.compatibility.risks.clone();
    if let Some(diagnostic) = diagnostic {
        assert!(
            expected.insert(diagnostic.to_owned()),
            "diagnostic must be genuinely new"
        );
    }
    // This also fixes the count and serialized sorted position, rather than
    // removing an arbitrary set of newly introduced warnings.
    assert_eq!(actual.compatibility.risks, expected);
}
fn exact_old_response(base: &ImportReport, actual: &ImportReport) {
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        serde_json::to_value(base).unwrap(),
        "only the approved field/diagnostic may differ"
    );
    let directory = tempfile::tempdir().unwrap();
    let old = directory.path().join("old.docx");
    let new = directory.path().join("new.docx");
    document::export_docx(&base.document, &old).unwrap();
    document::export_docx(&actual.document, &new).unwrap();
    assert_eq!(
        fs::read(old).unwrap(),
        fs::read(new).unwrap(),
        "unchanged imported models must retain exact exported package bytes"
    );
}
fn preserved(base: &Parts, source: &Parts, expected: Value) -> ImportReport {
    let baseline = import(base);
    assert!(!baseline.document.attrs.contains_key("fontFamilyMetadata"));
    let actual = import(source);
    assert_eq!(
        actual.document.attrs.get("fontFamilyMetadata"),
        Some(&expected)
    );
    assert_eq!(
        actual.document.attrs.len(),
        baseline.document.attrs.len() + 1
    );
    exact_diagnostic_delta(&baseline, &actual, Some(SUCCESS));
    let stripped_actual: ImportReport =
        serde_json::from_value(serde_json::to_value(&actual).unwrap()).unwrap();
    exact_old_response(&baseline, &stripped(stripped_actual, Some(SUCCESS)));
    let directory = tempfile::tempdir().unwrap();
    let old = directory.path().join("old.docx");
    let new = directory.path().join("new.docx");
    document::export_docx(&baseline.document, &old).unwrap();
    document::export_docx(&actual.document, &new).unwrap();
    let prior = read(&old);
    let current = read(&new);
    assert_eq!(current.len(), prior.len() + 1);
    for (name, bytes) in &prior {
        if !matches!(
            name.as_str(),
            "[Content_Types].xml" | "word/_rels/document.xml.rels" | "rust-wps/model-v2.json"
        ) {
            assert_eq!(&current[name], bytes, "non-font-table export part {name}");
        }
    }
    assert!(current.contains_key("word/fontTable.xml"));
    assert_eq!(
        serde_json::to_value(document::load_authored_docx(&new).unwrap().unwrap()).unwrap(),
        serde_json::to_value(&actual.document).unwrap()
    );
    actual
}
fn fallback(base: &Parts, source: &Parts, reason: &str) -> ImportReport {
    let baseline = import(base);
    let actual = import(source);
    assert!(!actual.document.attrs.contains_key("fontFamilyMetadata"));
    let diagnostic = format!("{PREFIX}{reason}; existing imported fonts and layout are unchanged");
    exact_diagnostic_delta(&baseline, &actual, Some(&diagnostic));
    let stripped_actual = serde_json::from_value(serde_json::to_value(&actual).unwrap()).unwrap();
    exact_old_response(&baseline, &stripped(stripped_actual, Some(&diagnostic)));
    actual
}
fn noop(base: &Parts, source: &Parts) {
    let baseline = import(base);
    let actual = import(source);
    assert!(!actual.document.attrs.contains_key("fontFamilyMetadata"));
    exact_diagnostic_delta(&baseline, &actual, None);
    exact_old_response(&baseline, &stripped(actual, None));
}
fn literal_base() -> Parts {
    package(
        &format!(
            "{}{}{}",
            paragraph(A, "One ’ & exact"),
            paragraph(B, "Two é"),
            paragraph(A, "Again")
        ),
        "",
    )
}

#[test]
fn complete_multiple_literal_names_are_sorted_deduplicated_and_only_add_metadata() {
    let base = literal_base();
    let actual = preserved(&base, &with_table(&base, &pair()), expected_pair());
    assert_eq!(actual.document.content.len(), 3);
    let mut reverse = with_table(&base, &pair());
    reverse.insert(
        "word/fontTable.xml".into(),
        table(&format!(
            "{}{}",
            definition(A, Some("swiss")),
            definition(B, Some("roman"))
        ))
        .into_bytes(),
    );
    assert_eq!(
        serde_json::to_value(import(&reverse)).unwrap(),
        serde_json::to_value(actual).unwrap()
    );
}

#[test]
fn absent_relationship_empty_table_and_no_retained_fonts_are_exact_noops() {
    let base = literal_base();
    noop(&base, &base);
    noop(&base, &with_table(&base, &table("")));
    let mut orphan = base.clone();
    orphan.insert("word/fontTable.xml".into(), pair().into_bytes());
    noop(&base, &orphan);
    let breaks = package(&format!("{BREAK}{BREAK}"), "");
    noop(&breaks, &with_table(&breaks, &pair()));
}

#[test]
fn source_escaped_names_text_and_formatting_splits_keep_exact_identity() {
    let name = "A & B <face> \"q\"";
    let body = format!(
        "<w:p>{}<w:r><w:rPr>{}<w:b/><w:color w:val=\"1A6B3C\"/><w:sz w:val=\"27\"/></w:rPr><w:t> ’ café </w:t><w:t>&lt;&amp;&gt;</w:t></w:r></w:p>",
        run(name, "first"),
        fonts(name)
    );
    let base = package(&body, "");
    let actual = preserved(
        &base,
        &with_table(&base, &table(&definition(name, Some("roman")))),
        json!([{"name":name,"family":"roman"}]),
    );
    let text: String = actual.document.content[0]
        .content
        .iter()
        .filter_map(|node| node.text.as_deref())
        .collect();
    assert_eq!(text, "first ’ café <&>");
    assert!(
        actual.document.content[0]
            .content
            .iter()
            .any(|node| node.marks.iter().any(|mark| mark.kind == "textColor"))
    );
}

#[test]
fn late_missing_auto_or_unsupported_family_discards_the_entire_group() {
    let base = literal_base();
    for family in [
        None,
        Some("auto"),
        Some("modern"),
        Some("script"),
        Some("decorative"),
    ] {
        let xml = table(&format!(
            "{}{}",
            definition(A, Some("swiss")),
            definition(B, family)
        ));
        fallback(&base, &with_table(&base, &xml), MISSING);
    }
    fallback(
        &base,
        &with_table(&base, &table(&definition(A, Some("swiss")))),
        MISSING,
    );
    let base = package(
        &paragraph("Arial", "Known names still need source evidence"),
        "",
    );
    fallback(&base, &with_table(&base, &pair()), MISSING);
}

#[test]
fn all_active_exact_duplicates_and_ascii_case_aliases_are_atomic_fallbacks() {
    let base = literal_base();
    for extra in [
        definition(A, Some("swiss")),
        definition(A, Some("roman")),
        definition(&A.to_ascii_uppercase(), Some("swiss")),
    ] {
        let xml = table(&format!(
            "{}{}{}",
            definition(A, Some("swiss")),
            definition(B, Some("roman")),
            extra
        ));
        fallback(&base, &with_table(&base, &xml), ALIAS);
    }
    let xml = table(&format!(
        "{}{}",
        definition(&A.to_ascii_lowercase(), Some("swiss")),
        definition(B, Some("roman"))
    ));
    fallback(&base, &with_table(&base, &xml), ALIAS);
}

#[test]
fn valid_inactive_nonroman_records_and_unrelated_duplicates_are_not_authority() {
    let base = literal_base();
    let mut records = format!(
        "{}{}",
        definition(A, Some("swiss")),
        definition(B, Some("roman"))
    );
    for (name, family) in [
        ("Dormant", Some("modern")),
        ("Dormant", Some("script")),
        ("NoFamily", None),
        ("UnusedAlias", Some("auto")),
        ("unusedalias", Some("decorative")),
    ] {
        records.push_str(&definition(name, family));
    }
    records.push_str(&definition(&"LongInactiveName".repeat(8), Some("roman")));
    preserved(&base, &with_table(&base, &table(&records)), expected_pair());
}

#[test]
fn defaults_paragraph_and_explicit_character_chains_follow_existing_precedence() {
    let styles = format!(
        r#"{}<w:style w:type="paragraph" w:styleId="Base"><w:rPr>{}</w:rPr></w:style><w:style w:type="paragraph" w:default="1" w:styleId="正文"><w:basedOn w:val="Base"/></w:style><w:style w:type="character" w:default="1" w:styleId="UnusedDefault"><w:rPr>{}</w:rPr></w:style><w:style w:type="character" w:styleId="CharacterBase"><w:rPr>{}</w:rPr></w:style><w:style w:type="character" w:styleId="强调"><w:basedOn w:val="CharacterBase"/></w:style>"#,
        defaults(&fonts("Unused Defaults")),
        fonts(A),
        fonts("Unused Character"),
        fonts(B)
    );
    let body = format!(
        r#"<w:p><w:pPr><w:rPr>{}</w:rPr></w:pPr><w:r><w:t>Default paragraph</w:t></w:r><w:r><w:rPr><w:rStyle w:val="强调"/></w:rPr><w:t>Character</w:t></w:r><w:r><w:rPr><w:rStyle w:val="强调"/>{}</w:rPr><w:t>Direct</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="正文"/></w:pPr><w:r><w:t>Explicit paragraph</w:t></w:r></w:p>"#,
        fonts("Unused Paragraph Mark"),
        fonts(A)
    );
    let base = package(&body, &styles);
    preserved(&base, &with_table(&base, &pair()), expected_pair());
}

fn theme() -> String {
    format!(
        r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements>{COLOR_SCHEME}<a:fontScheme name="Proof"><a:majorFont><a:latin typeface="{B}"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="{A}"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme></a:themeElements></a:theme>"#
    )
}
fn themed_base() -> Parts {
    let properties = r#"<w:rFonts w:ascii="Stale Literal" w:hAnsi="Stale Literal" w:asciiTheme="minorHAnsi" w:hAnsiTheme="minorHAnsi"/><w:sz w:val="22"/>"#;
    let mut base = package(
        &format!(
            "<w:p><w:r><w:t>Theme ’</w:t></w:r></w:p>{}",
            paragraph(B, "Later direct literal")
        ),
        &defaults(properties),
    );
    add_role(
        &mut base,
        "theme",
        "theme/theme.xml",
        "word/theme/theme.xml",
        &theme(),
    );
    base
}
#[test]
fn same_layer_theme_wins_and_later_literal_replaces_inherited_theme() {
    for language in [None, Some("en-US"), Some("fr-Latn-FR"), Some("pt-BR")] {
        eprintln!("theme positive language: {language:?}");
        let mut base = themed_base();
        if let Some(language) = language {
            add_role(
                &mut base,
                "settings",
                "settings.xml",
                "word/settings.xml",
                &format!(
                    r#"<w:settings xmlns:w="{W}"><w:themeFontLang w:val="{language}" w:eastAsia="ja-JP" w:bidi="ar-SA"/></w:settings>"#
                ),
            );
        }
        preserved(&base, &with_table(&base, &pair()), expected_pair());
    }
}

#[test]
fn unknown_nonlatin_and_supplemental_theme_language_mapping_never_guess() {
    for language in ["ja-JP", "xx-ZZ", "en-Cyrl-US", "en-Latn-Cyrl", "en_US"] {
        let mut base = themed_base();
        add_role(
            &mut base,
            "settings",
            "settings.xml",
            "word/settings.xml",
            &format!(
                r#"<w:settings xmlns:w="{W}"><w:themeFontLang w:val="{language}"/></w:settings>"#
            ),
        );
        fallback(&base, &with_table(&base, &pair()), IDENTITY);
    }
    let mut base = themed_base();
    replace(
        &mut base,
        "word/theme/theme.xml",
        "</a:minorFont>",
        r#"<a:font script="Latn" typeface="Another Face"/></a:minorFont>"#,
    );
    fallback(&base, &with_table(&base, &pair()), IDENTITY);
}

#[test]
fn independent_latin_slots_must_both_resolve_even_for_ascii_only_text() {
    for properties in [
        format!(r#"<w:rFonts w:ascii="{A}"/>"#),
        format!(r#"<w:rFonts w:hAnsi="{A}"/>"#),
        format!(r#"<w:rFonts w:ascii="{A}" w:hAnsi="{B}"/>"#),
        format!(r#"<w:rFonts w:ascii="{A}" w:hAnsiTheme="minorHAnsi"/>"#),
    ] {
        let base = package(
            &format!("<w:p><w:r><w:rPr>{properties}</w:rPr><w:t>ASCII only</w:t></w:r></w:p>"),
            "",
        );
        fallback(&base, &with_table(&base, &pair()), IDENTITY);
    }
}

#[test]
fn complete_empty_baselines_are_included_but_one_unproved_blank_fails_all() {
    let base = package(
        &format!("{}<w:p/>{}", paragraph(A, "Text"), paragraph(A, "After")),
        &defaults(&format!("{}<w:sz w:val=\"26\"/><w:b/>", fonts(B))),
    );
    let actual = preserved(&base, &with_table(&base, &pair()), expected_pair());
    assert_eq!(
        actual.document.content[1].attrs["emptyParagraphFont"],
        json!({"family":B,"halfPoints":26,"bold":true,"italic":false})
    );
    fallback(
        &base,
        &with_table(&base, &table(&definition(A, Some("swiss")))),
        MISSING,
    );
    let unproved = package(&format!("{}<w:p/>", paragraph(A, "Text")), "");
    fallback(&unproved, &with_table(&unproved, &pair()), IDENTITY);
}

#[test]
fn bare_break_positions_preserve_old_atoms_without_inventing_default_font_metadata() {
    let body = format!(
        "{BREAK}{BREAK}{}{BREAK}{}{BREAK}",
        paragraph(A, "Before"),
        paragraph(B, "After")
    );
    let base = package(
        &body,
        &defaults(&format!(
            "{}<w:sz w:val=\"22\"/>",
            fonts("Source Break Default")
        )),
    );
    let actual = preserved(&base, &with_table(&base, &pair()), expected_pair());
    assert_eq!(
        actual
            .document
            .content
            .iter()
            .map(|node| node.kind.as_str())
            .collect::<Vec<_>>(),
        [
            "pageBreak",
            "pageBreak",
            "paragraph",
            "pageBreak",
            "paragraph",
            "pageBreak"
        ]
    );
    assert!(
        actual
            .document
            .content
            .iter()
            .filter(|node| node.kind == "pageBreak")
            .all(|node| node.attrs.is_empty() && node.marks.is_empty() && node.content.is_empty())
    );
    assert!(
        !actual.document.attrs["fontFamilyMetadata"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["name"] == "Arial" || entry["name"] == "Source Break Default")
    );
}

#[test]
fn literal_w_identity_is_required_for_active_properties_but_table_prefix_is_arbitrary() {
    let base = literal_base();
    let alternate = pair()
        .replace("xmlns:w=", "xmlns:f=")
        .replace("<w:", "<f:")
        .replace("</w:", "</f:")
        .replace(" w:", " f:");
    preserved(&base, &with_table(&base, &alternate), expected_pair());
    for (before, after) in [
        ("<w:rFonts", "<same:rFonts"),
        ("w:ascii=", "same:ascii="),
        ("w:hAnsi=", "x:hAnsi="),
    ] {
        eprintln!("reader identity case: {before} -> {after}");
        let mut base = package(&paragraph(A, "Reader identity"), "");
        replace(&mut base, "word/document.xml", before, after);
        fallback(&base, &with_table(&base, &pair()), IDENTITY);
    }
}

#[test]
fn strict_wml_namespace_is_allowed_for_exact_literal_reader_properties() {
    let mut base = literal_base();
    replace(
        &mut base,
        "word/document.xml",
        W,
        "http://purl.oclc.org/ooxml/wordprocessingml/main",
    );
    let xml = pair().replace(W, "http://purl.oclc.org/ooxml/wordprocessingml/main");
    preserved(&base, &with_table(&base, &xml), expected_pair());
}

#[test]
fn related_nonstandard_font_table_location_has_authority_but_an_orphan_does_not() {
    let base = literal_base();
    let mut source = base.clone();
    add_role(
        &mut source,
        "fontTable",
        "../metadata/faces.xml",
        "metadata/faces.xml",
        &pair(),
    );
    source.insert(
        "word/fontTable.xml".into(),
        table(&definition(A, Some("decorative"))).into_bytes(),
    );
    preserved(&base, &source, expected_pair());
}

#[test]
fn wrong_missing_external_duplicate_and_lookalike_font_roles_cannot_add_metadata() {
    let base = literal_base();
    let clean = with_table(&base, &pair());
    for replacement in [
        relation("fontTable", "missing.xml"),
        relation("fontTable", "fontTable.xml").replace("/>", r#" TargetMode="External"/>"#),
        relation("fontTable", "fontTable.xml").replace("/>", r#" TargetMode="unknown"/>"#),
        format!(
            "{}{}",
            relation("fontTable", "fontTable.xml"),
            relation("fontTable", "fontTable.xml").replace("Id=\"fontTable\"", "Id=\"again\"")
        ),
        relation("fontTable", "fontTable.xml")
            .replace(&format!("{R}/fontTable"), "urn:unrelated/fontTable"),
        format!(
            "<Wrapper>{}</Wrapper>",
            relation("fontTable", "fontTable.xml")
        ),
    ] {
        let mut source = clean.clone();
        replace(
            &mut source,
            "word/_rels/document.xml.rels",
            &relation("fontTable", "fontTable.xml"),
            &replacement,
        );
        fallback(&base, &source, PACKAGE);
    }
}

#[test]
fn malformed_font_table_structure_namespaces_and_unknown_family_are_unavailable() {
    let base = literal_base();
    let valid = pair();
    let a = definition(A, Some("swiss"));
    let bad_tables = [
        valid.replace("w:val=\"swiss\"", "w:val=\"Swiss\""),
        valid.replace(
            "<w:family w:val=\"swiss\"/>",
            "<w:family w:val=\"swiss\"/><w:family w:val=\"swiss\"/>",
        ),
        valid.replace(&a, &format!("<w:wrapper>{a}</w:wrapper>")),
        valid.replace(
            "<w:family w:val=\"swiss\"/>",
            "<w:wrapper><w:family w:val=\"swiss\"/></w:wrapper>",
        ),
        valid.replace("<w:family w:val=\"swiss\"/>", "<x:family w:val=\"swiss\"/>"),
        valid.replace("w:val=\"swiss\"", "w:val=\"swiss\" same:val=\"swiss\""),
        valid.replace("w:val=\"swiss\"", "w:val=\"swiss\" w:val=\"swiss\""),
        valid.replace("</w:fonts>", "unexpected</w:fonts>"),
        valid
            .replace("<w:fonts ", "<x:fonts ")
            .replace("</w:fonts>", "</x:fonts>"),
        format!("{valid}{valid}"),
    ];
    for (index, xml) in bad_tables.iter().enumerate() {
        eprintln!("malformed table case: {index}");
        let source = with_table(&base, xml);
        fallback(&base, &source, TABLE);
    }
}

#[test]
fn raw_content_type_duplicates_wrong_types_and_percent_aliases_have_no_authority() {
    let base = literal_base();
    let original = with_table(&base, &pair());
    let declaration = format!(
        r#"<Override PartName="/word/fontTable.xml" ContentType="{}"/>"#,
        content_type("fontTable")
    );
    for replacement in [
        declaration.replace(content_type("fontTable"), content_type("styles")),
        format!("{declaration}{declaration}"),
        format!(
            "{declaration}{}",
            declaration.replace("fontTable.xml", "font%54able.xml")
        ),
        format!(
            "{declaration}{}",
            declaration.replace("fontTable.xml", "FONTTABLE.XML")
        ),
    ] {
        let mut source = original.clone();
        replace(
            &mut source,
            "[Content_Types].xml",
            &declaration,
            &replacement,
        );
        fallback(&base, &source, PACKAGE);
    }
    let mut source = original.clone();
    replace(
        &mut source,
        "[Content_Types].xml",
        C,
        "urn:foreign-content-types",
    );
    fallback(&base, &source, PACKAGE);
}

#[test]
fn active_unsupported_font_metadata_is_atomic_and_ancillary_fields_do_not_supply_family() {
    let base = literal_base();
    for extra in [
        r#"<w:altName w:val="Other Name"/>"#,
        r#"<w:charset w:val="02"/>"#,
        r#"<w:charset w:val="80"/>"#,
        r#"<w:embedRegular r:id="embedded"/>"#,
    ] {
        let xml = pair()
            .replace("xmlns:x=", &format!(r#"xmlns:r="{R}" xmlns:x="#))
            .replace(
                "<w:family w:val=\"swiss\"/>",
                &format!("<w:family w:val=\"swiss\"/>{extra}"),
            );
        fallback(&base, &with_table(&base, &xml), IDENTITY);
    }
    let ancillary = pair().replace("<w:family w:val=\"swiss\"/>", r#"<w:family w:val="swiss"/><w:panose1 w:val="020B0604020202020204"/><w:charset w:val="00"/><w:pitch w:val="variable"/><w:notTrueType w:val="0"/><w:sig w:usb0="00000001" w:usb1="00000000" w:usb2="00000000" w:usb3="00000000" w:csb0="00000001" w:csb1="00000000"/>"#);
    preserved(&base, &with_table(&base, &ancillary), expected_pair());
}

#[test]
fn unsupported_body_contexts_are_metadata_fallbacks_without_new_import_refusals() {
    for middle in [
        format!(
            "<w:p><w:r><w:rPr>{}</w:rPr><w:t>tab</w:t><w:tab/><w:t>after</w:t></w:r></w:p>",
            fonts(A)
        ),
        format!(
            "<w:p><w:r><w:rPr>{}</w:rPr><w:t>line</w:t><w:br/><w:t>after</w:t></w:r></w:p>",
            fonts(A)
        ),
        format!(
            "<w:p><w:r><w:rPr>{}</w:rPr><w:t>mixed</w:t><w:br w:type=\"page\"/></w:r></w:p>",
            fonts(A)
        ),
        format!(
            "<w:p><w:r><w:rPr>{}</w:rPr><w:br w:type=\"page\"/></w:r></w:p>",
            fonts(A)
        ),
    ] {
        let base = package(
            &format!("{}{middle}{}", paragraph(A, "First"), paragraph(B, "Last")),
            "",
        );
        fallback(&base, &with_table(&base, &pair()), BODY);
    }
}

#[test]
fn nonlatin_scalars_and_active_forcing_do_not_gain_classification_from_names() {
    for text in ["中", "سلام", "😀", "µ", "Ω", "a\u{301}"] {
        let base = package(&paragraph(A, text), "");
        fallback(&base, &with_table(&base, &pair()), BODY);
    }
    for properties in ["<w:cs/>", "<w:cs w:val=\"mystery\"/>"] {
        let base = package(
            &format!(
                "<w:p><w:r><w:rPr>{}{properties}</w:rPr><w:t>Latin</w:t></w:r></w:p>",
                fonts(A)
            ),
            "",
        );
        fallback(&base, &with_table(&base, &pair()), IDENTITY);
    }
}

#[test]
fn old_blocking_refusals_are_exact_and_cannot_be_replaced_by_family_diagnostics() {
    for body in [
        "<w:p><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r></w:p>",
        "<w:p><w:ins w:id=\"1\"><w:r><w:t>Tracked</w:t></w:r></w:ins></w:p>",
        "<w:p><w:r><w:rPr><w:vanish/></w:rPr><w:t>Hidden</w:t></w:r></w:p>",
        "<w:p><w:r><w:rPr><w:rtl/></w:rPr><w:t>Direction</w:t></w:r></w:p>",
    ] {
        let base = package(body, "");
        let directory = tempfile::tempdir().unwrap();
        let plain = directory.path().join("plain.docx");
        write(&plain, &base);
        let baseline = importing::import_docx(&plain).unwrap_err().to_string();
        let baseline_audit =
            serde_json::to_value(compatibility::audit_docx(&plain).unwrap()).unwrap();
        for xml in [pair(), "invalid font table XML".into()] {
            let source = directory.path().join("source.docx");
            write(&source, &with_table(&base, &xml));
            let before = fs::read(&source).unwrap();
            assert_eq!(
                importing::import_docx(&source).unwrap_err().to_string(),
                baseline
            );
            assert_eq!(
                serde_json::to_value(compatibility::audit_docx(&source).unwrap()).unwrap(),
                baseline_audit
            );
            assert_eq!(fs::read(source).unwrap(), before);
        }
    }
}

#[test]
fn font_definition_budget_excess_never_publishes_the_earlier_proved_subset() {
    let base = literal_base();
    let mut records = format!(
        "{}{}",
        definition(A, Some("swiss")),
        definition(B, Some("roman"))
    );
    for index in 0..254 {
        records.push_str(&definition(&format!("Unused{index}"), Some("roman")));
    }
    preserved(&base, &with_table(&base, &table(&records)), expected_pair());
    records.push_str(&definition("OneOver", Some("roman")));
    fallback(&base, &with_table(&base, &table(&records)), LIMIT);
}

#[test]
fn imported_metadata_survives_draft_and_edits_as_independent_document_state() {
    let base = literal_base();
    let actual = preserved(&base, &with_table(&base, &pair()), expected_pair());
    let directory = tempfile::tempdir().unwrap();
    let mut doc = actual.document;
    doc.content[0].content[0].text = Some("Edited".into());
    doc.content[1].content.clear();
    for node in &mut doc.content[2].content {
        node.marks.retain(|mark| mark.kind != "fontFamily");
    }
    document::validate(&doc).unwrap();
    assert_eq!(doc.attrs["fontFamilyMetadata"], expected_pair());
    let draft = directory.path().join("draft.json");
    document::save_draft(&doc, &draft).unwrap();
    let restored = document::load_draft(&draft).unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(restored).unwrap(),
        serde_json::to_value(&doc).unwrap()
    );
    let output = directory.path().join("edited.docx");
    document::export_docx(&doc, &output).unwrap();
    assert_eq!(
        serde_json::to_value(document::load_authored_docx(&output).unwrap().unwrap()).unwrap(),
        serde_json::to_value(&doc).unwrap()
    );
    let mut cleared: Node = doc;
    cleared.attrs.remove("fontFamilyMetadata");
    let output = directory.path().join("cleared.docx");
    document::export_docx(&cleared, &output).unwrap();
    assert!(!read(&output).contains_key("word/fontTable.xml"));
}

#[test]
fn theme_derived_blank_requires_its_own_settings_and_supplemental_mapping_proof() {
    for literal_text in [false, true] {
        let body = if literal_text {
            format!("{}<w:p/>", paragraph(B, "Literal text"))
        } else {
            "<w:p/>".into()
        };
        let properties =
            r#"<w:rFonts w:asciiTheme="minorHAnsi" w:hAnsiTheme="minorHAnsi"/><w:sz w:val="22"/>"#;
        for language in ["en-Latn-US", "ja-JP", "xx-ZZ", "en-Cyrl-US"] {
            let mut base = package(&body, &defaults(properties));
            add_role(
                &mut base,
                "theme",
                "theme/theme.xml",
                "word/theme/theme.xml",
                &theme(),
            );
            add_role(
                &mut base,
                "settings",
                "settings.xml",
                "word/settings.xml",
                &format!(
                    r#"<w:settings xmlns:w="{W}"><w:themeFontLang w:val="{language}"/></w:settings>"#
                ),
            );
            let source = with_table(&base, &pair());
            if language == "en-Latn-US" {
                preserved(
                    &base,
                    &source,
                    if literal_text {
                        expected_pair()
                    } else {
                        json!([{"name":A,"family":"swiss"}])
                    },
                );
            } else {
                fallback(&base, &source, IDENTITY);
            }
        }
        let mut base = package(&body, &defaults(properties));
        add_role(
            &mut base,
            "theme",
            "theme/theme.xml",
            "word/theme/theme.xml",
            &theme().replace(
                "</a:minorFont>",
                r#"<a:font script="Latn" typeface="Supplemental Face"/></a:minorFont>"#,
            ),
        );
        fallback(&base, &with_table(&base, &pair()), IDENTITY);
    }
}

#[test]
fn absent_main_relationship_part_is_noop_but_malformed_present_authority_is_not_absence() {
    let base = literal_base();
    let mut absent = base.clone();
    absent.remove("word/_rels/document.xml.rels");
    noop(&base, &absent);
    for xml in [
        format!(r#"<Relationships xmlns="{P}"><Wrapper/></Relationships>"#),
        format!(
            r#"<Relationships xmlns="{P}"><Relationship Id="ignored" Type="urn:unrelated" Target="other.xml" TargetMode="mystery"/></Relationships>"#
        ),
    ] {
        let mut malformed = base.clone();
        malformed.insert("word/_rels/document.xml.rels".into(), xml.into_bytes());
        fallback(&base, &malformed, PACKAGE);
    }
}

#[test]
fn selected_font_target_uri_encoding_and_physical_aliases_cannot_be_normalized_into_authority() {
    let base = literal_base();
    let source = with_table(&base, &pair());
    for target in [
        "fontTable.xml?ignored",
        "fontTable.xml#fragment",
        "fontTable.xml%",
        "fontTable.xml%ZZ",
        "font%2FTable.xml",
        "%2e%2e/word/fontTable.xml",
        "https://example.invalid/fontTable.xml",
        "//example.invalid/fontTable.xml",
    ] {
        let mut candidate = source.clone();
        replace(
            &mut candidate,
            "word/_rels/document.xml.rels",
            "Target=\"fontTable.xml\"",
            &format!("Target=\"{target}\""),
        );
        fallback(&base, &candidate, PACKAGE);
    }
    // The vendor's normalized lookup can pick one physical spelling. The new
    // proof must inventory both members before granting either one authority.
    let mut candidate = source.clone();
    candidate.insert("word/font%54able.xml".into(), pair().into_bytes());
    fallback(&base, &candidate, PACKAGE);
}

#[test]
fn font_program_relationship_is_outside_scope_without_reading_or_installing_any_font() {
    let base = literal_base();
    let mut source = with_table(&base, &pair());
    source.insert("word/_rels/fontTable.xml.rels".into(), format!(r#"<Relationships xmlns="{P}"><Relationship Id="embedded" Type="{R}/font" Target="../fonts/absent.odttf"/></Relationships>"#).into_bytes());
    fallback(&base, &source, TABLE);
    let mut unrelated = with_table(&base, &pair());
    unrelated.insert(
        "word/fonts/unrelated.odttf".into(),
        b"Not a font, never parsed".to_vec(),
    );
    preserved(&base, &unrelated, expected_pair());
}

#[test]
fn heading_typography_spacing_geometry_and_ordinary_text_are_never_repaired_by_family_proof() {
    let styles = format!(
        r#"{}<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:rPr>{}<w:sz w:val="31"/><w:color w:val="2A4B6C"/><w:b/></w:rPr></w:style>"#,
        defaults(&format!("{}<w:sz w:val=\"25\"/>", fonts(A))),
        fonts(B)
    );
    let body = r#"<w:p><w:pPr><w:pStyle w:val="Heading2"/><w:spacing w:before="100" w:after="140" w:line="300" w:lineRule="auto"/></w:pPr><w:r><w:t>Heading exact</w:t></w:r></w:p><w:p><w:r><w:t>Body exact</w:t></w:r></w:p><w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="720" w:right="1000" w:bottom="900" w:left="1200"/></w:sectPr>"#;
    let base = package(body, &styles);
    let actual = preserved(&base, &with_table(&base, &pair()), expected_pair());
    assert_eq!(actual.document.content[0].kind, "heading");
    assert_eq!(actual.document.content[0].attrs["level"], 2);
    assert_eq!(
        actual.document.attrs["pageGeometry"],
        json!({"widthTwips":12240,"heightTwips":15840,"marginTopTwips":720,"marginRightTwips":1000,"marginBottomTwips":900,"marginLeftTwips":1200})
    );
    assert!(
        actual.document.content[0].content[0]
            .marks
            .iter()
            .any(|mark| mark.kind == "fontSize" && mark.attrs["halfPoints"] == 31)
    );
    assert!(
        actual.document.content[0].content[0]
            .marks
            .iter()
            .any(|mark| mark.kind == "textColor" && mark.attrs["rgb"] == "2A4B6C")
    );
}

#[test]
fn preexisting_raw_package_refusals_keep_their_exact_errors() {
    let mut cases = Vec::new();
    let mut wrong_root = literal_base();
    wrong_root.insert(
        "word/_rels/document.xml.rels".into(),
        br#"<Relationships xmlns="urn:foreign"/>"#.to_vec(),
    );
    cases.push(wrong_root);
    let mut invalid_part = literal_base();
    replace(
        &mut invalid_part,
        "[Content_Types].xml",
        "</Types>",
        &format!(
            r#"<Override PartName="/word/fontTable.xml?query" ContentType="{}"/></Types>"#,
            content_type("fontTable")
        ),
    );
    cases.push(invalid_part);
    for base in cases {
        let directory = tempfile::tempdir().unwrap();
        let plain = directory.path().join("plain.docx");
        write(&plain, &base);
        let error = importing::import_docx(&plain).unwrap_err().to_string();
        let mut source = base.clone();
        // This malformed-root fixture cannot use the normal relationship helper.
        let rels = String::from_utf8(source["word/_rels/document.xml.rels"].clone()).unwrap();
        let rels = if rels.ends_with("/>") {
            rels.trim_end_matches("/>").to_owned()
                + ">"
                + &relation("fontTable", "fontTable.xml")
                + "</Relationships>"
        } else {
            rels.replace(
                "</Relationships>",
                &format!("{}</Relationships>", relation("fontTable", "fontTable.xml")),
            )
        };
        source.insert("word/_rels/document.xml.rels".into(), rels.into_bytes());
        source.insert("word/fontTable.xml".into(), pair().into_bytes());
        let input = directory.path().join("source.docx");
        write(&input, &source);
        let before = fs::read(&input).unwrap();
        assert_eq!(
            importing::import_docx(&input).unwrap_err().to_string(),
            error
        );
        assert_eq!(fs::read(input).unwrap(), before);
    }
}

#[test]
fn dense_single_paragraph_precharges_repeated_sibling_scans_before_resource_fallback() {
    // All ordinary count/byte/scalar limits fit, but 9,000 requests over one
    // wide paragraph exceed the declared conservative helper-visit budget.
    // This guards bounded proof work without an unreliable wall-clock test.
    let runs = "<w:r><w:t>a</w:t></w:r>".repeat(9_000);
    let base = package(&format!("<w:p>{runs}</w:p>"), &defaults(&fonts(A)));
    let source = with_table(&base, &table(&definition(A, Some("swiss"))));
    let actual = fallback(&base, &source, LIMIT);
    assert_eq!(
        actual.document.content[0]
            .content
            .iter()
            .filter_map(|node| node.text.as_deref())
            .collect::<String>(),
        "a".repeat(9_000)
    );
}

#[test]
fn whitespace_text_is_a_retained_font_range_and_never_an_empty_baseline() {
    let base = package(
        &format!(
            "{}{}{}",
            paragraph(A, "First"),
            paragraph(A, " "),
            paragraph(B, "Last")
        ),
        "",
    );
    let actual = preserved(&base, &with_table(&base, &pair()), expected_pair());
    let paragraph = &actual.document.content[1];
    assert!(!paragraph.attrs.contains_key("emptyParagraphFont"));
    assert_eq!(
        paragraph
            .content
            .iter()
            .filter_map(|node| node.text.as_deref())
            .collect::<String>(),
        " "
    );
}

#[test]
fn helper_parseable_but_vendor_unconsumed_theme_fails_late_exact_range_binding() {
    let mut base = themed_base();
    replace(&mut base, "word/theme/theme.xml", COLOR_SCHEME, "");
    fallback(&base, &with_table(&base, &pair()), BINDING);
}

#[test]
fn font_table_dtd_keeps_the_existing_global_xml_refusal_even_without_authority() {
    let mut base = literal_base();
    let dtd = format!("<!DOCTYPE w:fonts [<!ENTITY face 'swiss'>]>{}", pair());
    // The old inventory audits every XML part, including orphan font tables.
    // A relationship must not replace that refusal with the new family result.
    let source = with_table(&base, &dtd);
    base.insert("word/fontTable.xml".into(), dtd.into_bytes());
    let directory = tempfile::tempdir().unwrap();
    for (name, parts) in [("orphan.docx", base), ("related.docx", source)] {
        let path = directory.path().join(name);
        write(&path, &parts);
        let before = fs::read(&path).unwrap();
        assert_eq!(
            importing::import_docx(&path).unwrap_err().to_string(),
            "DTD is not supported in DOCX XML"
        );
        assert_eq!(fs::read(path).unwrap(), before);
    }
}

#[test]
fn decoded_invalid_utf8_and_control_targets_cannot_gain_authority_from_matching_members() {
    let base = literal_base();
    for (target, physical) in [
        ("font%FFTable.xml", "word/font%FFTable.xml"),
        ("font%C2%85Table.xml", "word/font\u{85}Table.xml"),
    ] {
        let mut source = base.clone();
        let encoded_path = format!("word/{target}");
        add_role(&mut source, "fontTable", target, &encoded_path, &pair());
        // Invalid UTF-8 has a literal matching ZIP spelling, which PartName
        // tolerantly retains. The valid UTF-8 control has its decoded matching
        // ZIP spelling. Both Overrides use the exact encoded relationship path,
        // so absence of a physical target cannot explain the expected fallback.
        let bytes = source.remove(&encoded_path).unwrap();
        source.insert(physical.into(), bytes);
        fallback(&base, &source, PACKAGE);
    }
}
