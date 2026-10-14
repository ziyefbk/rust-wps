//! Empty paragraph geometry, not named styles or typing-style inheritance.
//! Only source top-level true blanks receive metadata. Bind every surrounding
//! body block and table cell text to the converted model before attaching it.
use crate::{
    document::Node,
    image_dimensions::ImageParagraphFont,
    paragraph_indentation::{Xml, root, xml},
};
use anyhow::{Context, Result, ensure};
use office_oxide::core::{
    opc::OpcReader,
    relationships::{TargetMode, rel_types},
};
use std::io::Cursor;

pub(crate) const WARNING: &str = "Empty paragraph Latin baseline fonts preserved as fixed names; typing ends the empty-line override and unavailable fonts may be substituted";
pub(crate) const UNRESOLVED: &str = "Some empty paragraph baseline fonts are unproved; editor defaults may change blank line heights";
#[derive(Default)]
pub(crate) struct Audit {
    blocks: Vec<Block>,
    fonts: Vec<(usize, ImageParagraphFont)>,
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Block {
    Text(String),
    PageBreak,
    Image,
    Table(Vec<Vec<Vec<String>>>),
}

fn inline(n: &Xml, out: &mut String, breaks: &mut usize) -> Result<()> {
    match n.tag.as_str() {
        "pPr" | "rPr" => return Ok(()),
        "p" | "r" | "hyperlink" => n.identity()?,
        "t" => {
            n.identity()?;
            ensure!(n.children.is_empty(), "Nested text");
            out.push_str(&n.text);
            return Ok(());
        }
        "tab" => {
            n.schema(&[], true)?;
            out.push('\t');
            return Ok(());
        }
        "lastRenderedPageBreak" => {
            n.schema(&[], true)?;
            return Ok(());
        }
        "br" => {
            n.schema(&["type", "clear"], true)?;
            ensure!(
                n.attr("clear")?.is_none_or(|v| v == "none"),
                "Clearing breaks are unsupported"
            );
            match n.attr("type")? {
                None | Some("textWrapping") => out.push('\n'),
                Some("page") => *breaks += 1,
                _ => anyhow::bail!("Unsupported empty-font paragraph break"),
            }
            return Ok(());
        }
        "bookmarkStart" | "bookmarkEnd" | "proofErr" => {
            n.identity()?;
            ensure!(n.children.is_empty(), "Nested text marker");
            return Ok(());
        }
        _ => anyhow::bail!("Unsupported empty-font body content: {}", n.tag),
    }
    ensure!(n.text.trim().is_empty(), "Uncontained paragraph text");
    for c in &n.children {
        inline(c, out, breaks)?;
    }
    Ok(())
}
fn paragraph(n: &Xml) -> Result<Block> {
    let mut text = String::new();
    let mut breaks = 0;
    inline(n, &mut text, &mut breaks)?;
    if breaks != 0 {
        ensure!(
            breaks == 1 && text.is_empty(),
            "Mixed page-break paragraph has no empty-font identity proof"
        );
        Ok(Block::PageBreak)
    } else {
        Ok(Block::Text(text))
    }
}
pub(crate) fn true_blank(n: &Xml) -> Result<bool> {
    // A text node containing even whitespace, a tab, break or drawing is
    // not empty. Bare zero-content runs have no metrics of their own.
    n.identity()?;
    for child in &n.children {
        if child.tag == "r"
            && child.children.is_empty()
            && child.attrs.is_empty()
            && child.text.is_empty()
        {
            child.schema(&[], true)?;
            continue;
        }
        if !matches!(
            child.tag.as_str(),
            "pPr" | "bookmarkStart" | "bookmarkEnd" | "proofErr"
        ) {
            return Ok(false);
        }
        child.identity()?;
        if child.tag != "pPr" {
            ensure!(child.children.is_empty(), "Nested blank paragraph marker");
        }
    }
    ensure!(n.text.trim().is_empty(), "Uncontained blank paragraph text");
    Ok(true)
}
fn source_table(n: &Xml) -> Result<Vec<Vec<Vec<String>>>> {
    n.identity()?;
    let mut rows = Vec::new();
    for row in &n.children {
        if matches!(row.tag.as_str(), "tblPr" | "tblGrid") {
            row.identity()?;
            continue;
        }
        ensure!(row.tag == "tr", "Unsupported body table wrapper");
        row.identity()?;
        let mut cells = Vec::new();
        for cell in &row.children {
            if row.tag == "tr" && cell.tag == "trPr" {
                cell.identity()?;
                continue;
            }
            ensure!(cell.tag == "tc", "Unsupported body table cell wrapper");
            cell.identity()?;
            let mut paragraphs = Vec::new();
            for p in &cell.children {
                if p.tag == "tcPr" {
                    p.identity()?;
                    continue;
                }
                ensure!(p.tag == "p", "Unsupported body table paragraph wrapper");
                let Block::Text(text) = paragraph(p)? else {
                    anyhow::bail!("Unsupported table break");
                };
                paragraphs.push(text);
            }
            ensure!(!paragraphs.is_empty(), "Missing table paragraph identity");
            cells.push(paragraphs);
        }
        ensure!(!cells.is_empty(), "Missing table cell identity");
        rows.push(cells);
    }
    ensure!(!rows.is_empty(), "Missing table row identity");
    Ok(rows)
}
type SourceBlocks = (Vec<Block>, Vec<(usize, usize)>, bool);
fn blocks(main: &[u8]) -> Result<SourceBlocks> {
    let tree = xml(main)?;
    let body = root(&tree, "document")?
        .one("body")?
        .context("Missing body")?;
    let mut out = Vec::new();
    let mut candidates = Vec::new();
    let mut nested_blank = false;
    for (index, n) in body.children.iter().enumerate() {
        match n.tag.as_str() {
            "sectPr" => {
                n.identity()?;
            }
            "p" => {
                if !n.all("drawing").is_empty() {
                    out.push(Block::Image);
                    continue;
                }
                let block = paragraph(n)?;
                if matches!(&block, Block::Text(s) if s.is_empty()) {
                    if true_blank(n)? {
                        candidates.push((out.len(), index));
                    } else {
                        nested_blank = true;
                    }
                }
                out.push(block);
            }
            "tbl" => {
                nested_blank |= n.all("p").iter().any(|p| true_blank(p).unwrap_or(false));
                out.push(Block::Table(source_table(n)?));
            }
            _ => anyhow::bail!("Unsupported body block for empty-font proof: {}", n.tag),
        }
    }
    Ok((out, candidates, nested_blank))
}

pub(crate) fn analyze(bytes: &[u8]) -> Audit {
    fn inner(bytes: &[u8]) -> Result<Audit> {
        let mut package = OpcReader::new(Cursor::new(bytes))?;
        let main = package.main_document_part()?;
        ensure!(
            main.as_str() == "/word/document.xml",
            "Unsupported empty-font main part"
        );
        let data = package.read_part(&main)?;
        let tree = xml(&data)?;
        // Avoid new diagnostics or extra style/theme processing when no actual
        // empty candidate exists. Whitespace and breaks are not empty lines.
        if !tree
            .all("p")
            .iter()
            .any(|p| matches!(paragraph(p), Ok(Block::Text(s)) if s.is_empty()))
        {
            return Ok(Audit::default());
        }
        let (blocks, candidates, nested_blank) = blocks(&data)?;
        let mut audit = Audit {
            blocks,
            ..Audit::default()
        };
        if nested_blank {
            audit.warnings.push(UNRESOLVED.into());
        }
        if candidates.is_empty() {
            return Ok(audit);
        }
        let rels = package.read_rels_for(&main)?;
        ensure!(
            rels.duplicate_ids().is_empty(),
            "Ambiguous empty-font relationships"
        );
        let mut read = |kind| -> Result<Option<Vec<u8>>> {
            let found = rels.get_by_type(kind);
            ensure!(
                found.len() <= 1,
                "Ambiguous empty-font style/theme relationship"
            );
            found
                .first()
                .map(|r| {
                    ensure!(
                        r.target_mode == TargetMode::Internal,
                        "External empty-font style/theme relationship"
                    );
                    let target = main.resolve_relative(&r.target)?;
                    Ok(package.read_part(&target)?)
                })
                .transpose()
        };
        let styles = read(rel_types::STYLES)?.unwrap_or_else(|| br#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#.to_vec());
        let theme = read(rel_types::THEME)?;
        let indexes: Vec<_> = candidates.iter().map(|(_, source)| *source).collect();
        let fonts = crate::image_dimensions::resolve_empty_paragraph_fonts(
            &data,
            &styles,
            theme.as_deref(),
            &indexes,
        )?;
        for ((target, _), font) in candidates.into_iter().zip(fonts) {
            if let Some(font) = font {
                audit.fonts.push((target, font));
            } else {
                audit.warnings.push(UNRESOLVED.into());
            }
        }
        if !audit.fonts.is_empty() {
            audit.warnings.push(WARNING.into());
        }
        Ok(audit)
    }
    match inner(bytes) {
        Ok(audit) => audit,
        Err(error) => Audit {
            warnings: vec![
                UNRESOLVED.into(),
                format!("Empty paragraph font proof unavailable: {error}"),
            ],
            ..Audit::default()
        },
    }
}
fn model_text(n: &Node) -> Result<String> {
    ensure!(
        matches!(n.kind.as_str(), "paragraph" | "heading"),
        "Converted empty-font paragraph type differs"
    );
    let mut out = String::new();
    for c in &n.content {
        match c.kind.as_str() {
            "text" => out.push_str(c.text.as_deref().unwrap_or("")),
            "hardBreak" => out.push('\n'),
            _ => anyhow::bail!("Unsupported converted empty-font inline node"),
        }
    }
    Ok(out)
}
pub(crate) fn apply(audit: &Audit, document: &mut Node) -> Result<()> {
    if audit.fonts.is_empty() {
        return Ok(());
    }
    verify_body_identity(&audit.blocks, document)?;
    for (index, _) in &audit.fonts {
        ensure!(
            document.content[*index].content.is_empty(),
            "True blank source became a nonempty paragraph"
        );
    }
    for (index, font) in &audit.fonts {
        let node = &mut document.content[*index];
        node.attrs
            .insert("emptyParagraphFont".into(), serde_json::to_value(font)?);
    }
    Ok(())
}

/// Exact body identity shared by additive source proofs; does not attach state.
pub(crate) fn source_body_identity(main: &[u8]) -> Result<Vec<Block>> {
    Ok(blocks(main)?.0)
}
pub(crate) fn verify_body_identity(blocks: &[Block], document: &Node) -> Result<()> {
    let mut actual = Vec::new();
    for n in &document.content {
        actual.push(match n.kind.as_str() {
            "paragraph" | "heading" => Block::Text(model_text(n)?),
            "pageBreak" => Block::PageBreak,
            "image" => Block::Image,
            "table" => {
                let mut rows = Vec::new();
                for row in &n.content {
                    let mut cells = Vec::new();
                    for cell in &row.content {
                        cells.push(cell.content.iter().map(model_text).collect::<Result<_>>()?);
                    }
                    rows.push(cells);
                }
                Block::Table(rows)
            }
            _ => anyhow::bail!("Converted body layout differs from empty-font source proof"),
        });
    }
    ensure!(
        actual == blocks,
        "Converted body/table text or block order differs from empty-font source proof"
    );
    Ok(())
}
