//! Literal replacement inside individual DrawingML text nodes on slide parts.
use anyhow::{Result, ensure};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
    path::Path,
};

pub fn replace_pptx(
    input: &Path,
    output: &Path,
    find: &str,
    replacement: &str,
) -> Result<crate::editing::EditReport> {
    ensure!(
        !find.is_empty() && find.len() <= 65536 && replacement.len() <= 65536,
        "Invalid replacement length"
    );
    ensure!(find != replacement, "Replacement is unchanged");
    ensure!(replacement.chars().all(|c| matches!(c, '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')), "Replacement contains unsupported control characters");
    ensure!(
        !replacement.starts_with(char::is_whitespace)
            && !replacement.ends_with(char::is_whitespace),
        "Leading/trailing whitespace replacement is unsupported"
    );
    ensure!(!output.try_exists()?, "Output exists; choose a new file");
    let source = crate::spreadsheet::read_xlsx_snapshot(input)?;
    let report = crate::presentation::inspect_pptx_bytes(&source)?;
    ensure!(
        report.unreadable_parts.is_empty() && !report.has_macros,
        "Unreadable or macro-enabled presentations cannot be edited"
    );
    let mut parts = read_parts(&source)?;
    ensure!(
        !parts
            .get("[Content_Types].xml")
            .is_some_and(|data| String::from_utf8_lossy(data).contains("macroEnabled"))
            && !parts
                .keys()
                .any(|name| name.to_ascii_lowercase().contains("vbaproject")),
        "Macro-enabled package cannot be edited"
    );
    ensure!(
        !parts
            .keys()
            .any(|n| n.to_ascii_lowercase().starts_with("_xmlsignatures/")),
        "Signed presentation cannot be edited"
    );
    let mut replacements = 0;
    let mut changed_parts = Vec::new();
    let mut growth_budget = 8 * 1024 * 1024;
    for (name, data) in &mut parts {
        if !(name.starts_with("ppt/slides/") && !name[11..].contains('/') && name.ends_with(".xml"))
        {
            continue;
        }
        let xml = std::str::from_utf8(data)?;
        let (edited, count) = replace_nodes(xml, find, replacement, &mut growth_budget)?;
        if count > 0 {
            *data = edited.into_bytes();
            replacements += count;
            changed_parts.push(name.clone());
        }
    }
    ensure!(
        replacements > 0,
        "No matches within individual slide text nodes; cross-run replacement is unsupported"
    );
    let mut bytes = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut bytes);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in &parts {
            if name.ends_with('/') {
                writer.add_directory(name, options)?;
            } else {
                writer.start_file(name, options)?;
                writer.write_all(data)?;
            }
        }
        writer.finish()?;
    }
    ensure!(
        bytes.get_ref().len() <= 64 * 1024 * 1024,
        "Edited PPTX exceeds 64 MiB"
    );
    ensure!(
        read_parts(bytes.get_ref())? == parts,
        "Outgoing package verification failed"
    );
    let reopened = crate::presentation::inspect_pptx_bytes(bytes.get_ref())?;
    ensure!(
        reopened.unreadable_parts.is_empty() && reopened.slides.len() == report.slides.len(),
        "Edited presentation cannot be reopened completely"
    );
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes.get_ref())?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(output)
        .map_err(|e| anyhow::anyhow!("Cannot publish without overwriting: {}", e.error))?;
    Ok(crate::editing::EditReport { replacements, preserved_parts: parts.len()-changed_parts.len(), changed_parts,
        warnings: vec!["Only individual text nodes in standard ppt/slides/*.xml parts are replaced; notes, masters and cross-run matches are excluded".into(),
            "Unchanged part payloads and surrounding slide XML are preserved; ZIP metadata is regenerated; layout requires review".into()] })
}

pub(crate) fn read_parts(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
    crate::editing::preflight_ooxml(bytes, "PPTX")?;
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut parts = BTreeMap::new();
    for i in 0..zip.len() {
        let mut part = zip.by_index(i)?;
        let mut data = Vec::new();
        part.read_to_end(&mut data)?;
        parts.insert(part.name().into(), data);
    }
    Ok(parts)
}

fn replace_nodes(
    xml: &str,
    find: &str,
    replacement: &str,
    growth_budget: &mut usize,
) -> Result<(String, usize)> {
    use quick_xml::{events::Event, name::ResolveResult};
    let mut reader = quick_xml::NsReader::from_reader(xml.as_bytes());
    let mut start = None;
    let mut edits = Vec::new();
    let mut field_depth = 0usize;
    loop {
        let before = reader.buffer_position() as usize;
        let (namespace, event) = reader.read_resolved_event()?;
        let drawing = matches!(namespace, ResolveResult::Bound(ns) if ns.as_ref() == "http://schemas.openxmlformats.org/drawingml/2006/main");
        match event {
            Event::Start(e) if drawing && e.local_name().as_ref() == "fld" => field_depth += 1,
            Event::End(e) if drawing && e.local_name().as_ref() == "fld" => {
                field_depth = field_depth.saturating_sub(1)
            }
            Event::Start(e) if drawing && field_depth == 0 && e.local_name().as_ref() == "t" => {
                ensure!(start.is_none(), "Nested DrawingML text");
                start = Some(reader.buffer_position() as usize);
            }
            Event::End(e) if drawing && e.local_name().as_ref() == "t" => {
                if let Some(begin) = start.take() {
                    let raw = &xml[begin..before];
                    ensure!(!raw.contains('<'), "Complex text node is unsupported");
                    let decoded = quick_xml::escape::unescape(raw)?;
                    let count = decoded.matches(find).count();
                    if count > 0 {
                        let growth = replacement
                            .len()
                            .saturating_sub(find.len())
                            .checked_mul(count)
                            .ok_or_else(|| anyhow::anyhow!("Replacement size overflow"))?;
                        ensure!(
                            growth <= *growth_budget,
                            "Replacement exceeds 8 MiB growth budget"
                        );
                        *growth_budget -= growth;
                        let next = decoded.replace(find, replacement);
                        let escaped = quick_xml::escape::escape(&next).into_owned();
                        let escape_growth = escaped.len().saturating_sub(next.len());
                        ensure!(
                            escape_growth <= *growth_budget,
                            "Escaped replacement exceeds growth budget"
                        );
                        *growth_budget -= escape_growth;
                        edits.push((begin, before, escaped, count));
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let count = edits.iter().map(|e| e.3).sum();
    let mut output = xml.to_owned();
    for (begin, end, text, _) in edits.into_iter().rev() {
        output.replace_range(begin..end, &text);
    }
    Ok((output, count))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn namespace_entities_field_exclusion_and_node_boundaries() {
        let xml = r#"<p xmlns:d="http://schemas.openxmlformats.org/drawingml/2006/main"><d:r><d:t>A&amp;B</d:t></d:r><d:r><d:t>A</d:t></d:r><d:r><d:t>B</d:t></d:r><d:fld><d:t>A&amp;B</d:t></d:fld><t>A&amp;B</t></p>"#;
        let (output, count) = replace_nodes(xml, "A&B", "C<D", &mut 10000).unwrap();
        assert_eq!(count, 1);
        assert!(output.contains("<d:t>C&lt;D</d:t>"));
        assert!(output.contains("<d:fld><d:t>A&amp;B</d:t></d:fld>"));
        assert_eq!(replace_nodes(xml, "AB", "x", &mut 10000).unwrap().1, 0);
        assert!(replace_nodes(xml, "A&B", &"x".repeat(100), &mut 1).is_err());
    }
}
