use std::collections::HashMap;

use quick_xml::events::Event;

use crate::core::xml;

use super::formatting::TableBorders;
use super::formatting::{
    ParagraphProperties, RunProperties, parse_paragraph_properties_fast, parse_run_properties_fast,
};
use super::table::{TableCellProperties, TableProperties};

/// Parsed stylesheet from `word/styles.xml`.
#[derive(Debug, Clone, Default)]
pub struct StyleSheet {
    /// Document-wide default formatting.
    pub doc_defaults: Option<DocDefaults>,
    /// Map from style ID to style definition.
    pub styles: HashMap<String, Style>,
    /// Last paragraph style explicitly marked `w:default` in XML order.
    /// A style's identifier or display name never makes it the default.
    pub default_paragraph_style_id: Option<String>,
}

/// Document-wide default properties.
#[derive(Debug, Clone, Default)]
pub struct DocDefaults {
    /// Default run properties applied to all text.
    pub run_properties: Option<RunProperties>,
    /// Default paragraph properties.
    pub paragraph_properties: Option<ParagraphProperties>,
}

/// A single style definition.
#[derive(Debug, Clone)]
pub struct Style {
    /// Unique style identifier (e.g., `"Heading1"`).
    pub style_id: String,
    /// Kind of style (paragraph, character, table, or numbering).
    pub style_type: StyleType,
    /// Whether `w:default` is explicitly enabled for this style's type.
    pub is_default: bool,
    /// Human-readable style name.
    pub name: Option<String>,
    /// ID of the parent style this style inherits from.
    pub based_on: Option<String>,
    /// Run-level overrides for this style.
    pub run_properties: Option<RunProperties>,
    /// Paragraph-level overrides for this style.
    pub paragraph_properties: Option<ParagraphProperties>,
    /// Table-level overrides for this style (`w:tblPr`).
    pub table_properties: Option<TableProperties>,
    /// Conditional formats of a table style (`w:tblStylePr`), applied to
    /// the parts of a table its `w:tblLook` switches on.
    pub table_conditionals: Vec<TableStyleConditional>,
}

/// One `w:tblStylePr` (ECMA-376 §17.7.6): formatting a table style applies
/// to one region of the table.
#[derive(Debug, Clone)]
pub struct TableStyleConditional {
    /// The region (`w:type`): `wholeTable`, `firstRow`, `lastRow`,
    /// `firstCol`, `lastCol`, `band1Horz`, `band2Horz`, `band1Vert`,
    /// `band2Vert`, `nwCell`, `neCell`, `swCell` or `seCell`.
    pub kind: String,
    /// Cell formatting for the region (`w:tcPr`).
    pub cell_properties: Option<TableCellProperties>,
    /// Run formatting for the region's text (`w:rPr`), e.g. a bold header
    /// row.
    pub run_properties: Option<RunProperties>,
    /// Paragraph formatting for the region's paragraphs (`w:pPr`).
    pub paragraph_properties: Option<ParagraphProperties>,
}

/// The kind of style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleType {
    /// Paragraph style.
    Paragraph,
    /// Character (inline) style.
    Character,
    /// Table style.
    Table,
    /// Numbering style.
    Numbering,
}

impl StyleSheet {
    /// Parse `word/styles.xml` content.
    pub fn parse(xml_data: &[u8]) -> crate::core::Result<Self> {
        let mut reader = xml::make_fast_reader(xml_data);
        let mut sheet = StyleSheet::default();

        loop {
            match reader.read_event()? {
                Event::Start(ref e) => match e.local_name().as_ref() {
                    "docDefaults" => {
                        sheet.doc_defaults = Some(parse_doc_defaults(&mut reader)?);
                    },
                    "style" => {
                        if let Some(style) = parse_style(&mut reader, e, false)? {
                            sheet.add_style(style)?;
                        }
                    },
                    _ => {},
                },
                Event::Empty(ref e) if e.local_name().as_ref() == "style" => {
                    if let Some(style) = parse_style(&mut reader, e, true)? {
                        sheet.add_style(style)?;
                    }
                },
                Event::Eof => break,
                _ => {},
            }
        }
        Ok(sheet)
    }

    fn add_style(&mut self, style: Style) -> crate::core::Result<()> {
        // Duplicate identifiers cannot preserve an unambiguous basedOn/default
        // reference in this map. Reject instead of accidentally applying the
        // formatting of a different definition or style type.
        if self.styles.contains_key(&style.style_id) {
            return Err(crate::core::Error::MalformedXml(format!(
                "duplicate DOCX style identifier: {}", style.style_id
            )));
        }
        // ECMA-376 17.7.4.17: when several styles of a type have the
        // default attribute enabled, the last in document order wins.
        if style.style_type == StyleType::Paragraph && style.is_default {
            self.default_paragraph_style_id = Some(style.style_id.clone());
        }
        self.styles.insert(style.style_id.clone(), style);
        Ok(())
    }

    /// Resolve the implicit paragraph style only when pStyle is absent.
    /// An explicit unknown or non-paragraph reference contributes no style.
    fn paragraph_style_id(&self, explicit: Option<&str>) -> Option<&str> {
        let id = explicit.or(self.default_paragraph_style_id.as_deref())?;
        self.styles.get_key_value(id)
            .filter(|(_, style)| style.style_type == StyleType::Paragraph)
            .map(|(key, _)| key.as_str())
    }

    /// Return the inheritance chain for `style_id`, **root ancestor first**,
    /// so callers can overlay each style in turn and have the most specific
    /// one win. Cycles and pathological `w:basedOn` chains are cut off at 20
    /// links.
    ///
    /// Collected on the stack: this runs for every run and paragraph of a
    /// document, and a `Vec` per call was a heap allocation each time.
    fn chain(&self, style_id: &str) -> impl DoubleEndedIterator<Item = &Style> {
        const MAX_CHAIN: usize = 20;
        let mut buf: [Option<&Style>; MAX_CHAIN] = [None; MAX_CHAIN];
        let mut len = 0;
        let mut current = self.styles.get(style_id);
        while let Some(style) = current {
            if len >= MAX_CHAIN {
                break;
            }
            buf[len] = Some(style);
            len += 1;
            current = style.based_on.as_deref().and_then(|id| self.styles.get(id));
        }
        buf.into_iter().take(len).rev().flatten()
    }

    /// Fold the effective run formatting for a run: document defaults, then
    /// the paragraph style chain's run properties, then the character style
    /// chain, then the run's own `w:rPr`. Later stages win field by field.
    ///
    /// Without this, only direct `w:rPr` reached the IR — so a document that
    /// puts all of its formatting in styles (which is what Word's built-in
    /// styles and every template do) read back as unformatted text.
    pub fn effective_run_properties(
        &self,
        paragraph_style_id: Option<&str>,
        direct: Option<&RunProperties>,
    ) -> RunProperties {
        self.effective_run_properties_in(None, paragraph_style_id, direct)
    }

    /// [`Self::effective_run_properties`] for a run inside a table cell:
    /// the table style's layer for the cell (`table`) sits between the
    /// document defaults and the paragraph style.
    pub(crate) fn effective_run_properties_in(
        &self,
        table: Option<&RunProperties>,
        paragraph_style_id: Option<&str>,
        direct: Option<&RunProperties>,
    ) -> RunProperties {
        let mut out = self.run_style_base(
            table,
            paragraph_style_id,
            direct.and_then(|d| d.style_id.as_deref()),
        );
        if let Some(d) = direct {
            out.overlay(d);
        }
        out
    }

    /// Everything a run inherits before its own `w:rPr`: document
    /// defaults, the table layer, the paragraph style chain, then the
    /// character style chain. Depends only on its three inputs, so a
    /// converter can fold it once per distinct combination.
    pub(crate) fn run_style_base(
        &self,
        table: Option<&RunProperties>,
        paragraph_style_id: Option<&str>,
        character_style_id: Option<&str>,
    ) -> RunProperties {
        let mut out = self
            .doc_defaults
            .as_ref()
            .and_then(|d| d.run_properties.clone())
            .unwrap_or_default();
        if let Some(t) = table {
            out.overlay(t);
        }
        if let Some(pid) = self.paragraph_style_id(paragraph_style_id) {
            for style in self.chain(pid) {
                if let Some(rp) = style.run_properties.as_ref() {
                    out.overlay(rp);
                }
            }
        }
        // `w:rStyle` on the run names a character style, which sits above
        // the paragraph style but below the run's own direct formatting.
        if let Some(cid) = character_style_id {
            for style in self.chain(cid) {
                if let Some(rp) = style.run_properties.as_ref() {
                    out.overlay(rp);
                }
            }
        }
        out
    }

    /// Whether a run is hidden (`<w:vanish/>`) once document defaults, the
    /// paragraph style chain, the run's character style and its own
    /// `w:rPr` are folded together — the same precedence as
    /// [`Self::effective_run_properties`], without materialising the whole
    /// property set for renderers that only need this one bit.
    pub fn effective_hidden(
        &self,
        paragraph_style_id: Option<&str>,
        direct: Option<&RunProperties>,
    ) -> bool {
        self.effective_hidden_in(None, paragraph_style_id, direct)
    }

    /// [`Self::effective_hidden`] inside a table cell whose table style
    /// layer is `table`.
    pub(crate) fn effective_hidden_in(
        &self,
        table: Option<&RunProperties>,
        paragraph_style_id: Option<&str>,
        direct: Option<&RunProperties>,
    ) -> bool {
        let mut hidden = self
            .doc_defaults
            .as_ref()
            .and_then(|d| d.run_properties.as_ref())
            .and_then(|rp| rp.hidden);
        if let Some(h) = table.and_then(|t| t.hidden) {
            hidden = Some(h);
        }
        let chain_hidden = |sid: &str| {
            self.chain(sid)
                .rev()
                .find_map(|style| style.run_properties.as_ref().and_then(|rp| rp.hidden))
        };
        if let Some(h) = self.paragraph_style_id(paragraph_style_id).and_then(chain_hidden) {
            hidden = Some(h);
        }
        if let Some(h) = direct
            .and_then(|d| d.style_id.as_deref())
            .and_then(chain_hidden)
        {
            hidden = Some(h);
        }
        if let Some(h) = direct.and_then(|d| d.hidden) {
            hidden = Some(h);
        }
        hidden.unwrap_or(false)
    }

    /// Fold the effective paragraph formatting: document defaults, then the
    /// style chain, then the paragraph's own `w:pPr`.
    pub fn effective_paragraph_properties(
        &self,
        direct: Option<&ParagraphProperties>,
    ) -> ParagraphProperties {
        self.effective_paragraph_properties_in(None, direct)
    }

    /// [`Self::effective_paragraph_properties`] inside a table cell: the
    /// table style's layer sits between the document defaults and the
    /// paragraph style.
    pub(crate) fn effective_paragraph_properties_in(
        &self,
        table: Option<&ParagraphProperties>,
        direct: Option<&ParagraphProperties>,
    ) -> ParagraphProperties {
        let mut out = self.paragraph_style_base(table, direct.and_then(|d| d.style_id.as_deref()));
        if let Some(d) = direct {
            out.overlay(d);
        }
        out
    }

    /// Everything a paragraph inherits before its own `w:pPr`: document
    /// defaults, the table layer, then the paragraph style chain.
    pub(crate) fn paragraph_style_base(
        &self,
        table: Option<&ParagraphProperties>,
        paragraph_style_id: Option<&str>,
    ) -> ParagraphProperties {
        let mut out = self
            .doc_defaults
            .as_ref()
            .and_then(|d| d.paragraph_properties.clone())
            .unwrap_or_default();
        if let Some(t) = table {
            out.overlay(t);
        }
        if let Some(pid) = self.paragraph_style_id(paragraph_style_id) {
            for style in self.chain(pid) {
                if let Some(pp) = style.paragraph_properties.as_ref() {
                    out.overlay(pp);
                }
            }
        }
        out
    }

    /// Resolve the effective outline level for a given style ID, walking the inheritance chain.
    pub fn resolve_outline_level(&self, style_id: &str) -> Option<u8> {
        let mut current = self.styles.get(style_id);
        let mut depth = 0;
        while let Some(style) = current {
            if depth > 20 {
                break; // prevent infinite loops
            }
            if let Some(ref pp) = style.paragraph_properties {
                if let Some(lvl) = pp.outline_level {
                    return Some(lvl);
                }
            }
            current = style.based_on.as_ref().and_then(|id| self.styles.get(id));
            depth += 1;
        }
        None
    }
}

fn parse_doc_defaults(reader: &mut quick_xml::Reader<&[u8]>) -> crate::core::Result<DocDefaults> {
    let mut defaults = DocDefaults::default();

    loop {
        match reader.read_event()? {
            Event::Start(ref e) => {
                match e.local_name().as_ref() {
                    "rPrDefault" => {
                        // contains w:rPr
                        defaults.run_properties = parse_nested_rpr(reader)?;
                    },
                    "pPrDefault" => {
                        defaults.paragraph_properties = parse_nested_ppr(reader)?;
                    },
                    _ => {
                        xml::skip_element_fast(reader)?;
                    },
                }
            },
            Event::End(ref e) if e.local_name().as_ref() == "docDefaults" => {
                break;
            },
            Event::Eof => break,
            _ => {},
        }
    }
    Ok(defaults)
}

/// Parse the `w:rPr` nested inside `w:rPrDefault` (or similar wrapper).
fn parse_nested_rpr(
    reader: &mut quick_xml::Reader<&[u8]>,
) -> crate::core::Result<Option<RunProperties>> {
    let mut result = None;

    loop {
        match reader.read_event()? {
            Event::Start(ref e) => {
                if e.local_name().as_ref() == "rPr" {
                    result = Some(parse_run_properties_fast(reader)?);
                } else {
                    xml::skip_element_fast(reader)?;
                }
            },
            Event::End(ref e) if e.local_name().as_ref() == "rPrDefault" => {
                break;
            },
            Event::Eof => break,
            _ => {},
        }
    }
    Ok(result)
}

/// Parse the `w:pPr` nested inside `w:pPrDefault`.
fn parse_nested_ppr(
    reader: &mut quick_xml::Reader<&[u8]>,
) -> crate::core::Result<Option<ParagraphProperties>> {
    let mut result = None;

    loop {
        match reader.read_event()? {
            Event::Start(ref e) => {
                if e.local_name().as_ref() == "pPr" {
                    result = Some(parse_paragraph_properties_fast(reader)?);
                } else {
                    xml::skip_element_fast(reader)?;
                }
            },
            Event::End(ref e) if e.local_name().as_ref() == "pPrDefault" => {
                break;
            },
            Event::Eof => break,
            _ => {},
        }
    }
    Ok(result)
}

fn parse_style(
    reader: &mut quick_xml::Reader<&[u8]>,
    start: &quick_xml::events::BytesStart,
    empty: bool,
) -> crate::core::Result<Option<Style>> {
    let is_default = match xml::optional_attr_str(start, "w:default")?.as_deref() {
        None | Some("0" | "false" | "off") => false,
        Some("1" | "true" | "on") => true,
        Some(value) => return Err(crate::core::Error::MalformedXml(format!(
            "invalid DOCX style default value: {value}"
        ))),
    };
    let style_id = match xml::optional_attr_str(start, "w:styleId")? {
        Some(id) => id.into_owned(),
        None if is_default => return Err(crate::core::Error::MalformedXml(
            "default DOCX style has no identifier".into()
        )),
        None => return Ok(None),
    };
    let style_type = match xml::optional_attr_str(start, "w:type")? {
        Some(ref t) => match t.as_ref() {
            "paragraph" => StyleType::Paragraph,
            "character" => StyleType::Character,
            "table" => StyleType::Table,
            "numbering" => StyleType::Numbering,
            _ => return Err(crate::core::Error::MalformedXml(format!(
                "invalid DOCX style type: {t}"
            ))),
        },
        None => StyleType::Paragraph,
    };

    let mut name = None;
    let mut based_on = None;
    let mut run_properties = None;
    let mut paragraph_properties = None;
    let mut table_properties = None;
    let mut table_conditionals = Vec::new();

    if !empty {
        loop {
            match reader.read_event()? {
                Event::Start(ref e) => match e.local_name().as_ref() {
                    "name" => {
                        if let Ok(Some(val)) = xml::optional_attr_str(e, "w:val") {
                            name = Some(val.into_owned());
                        }
                        xml::skip_element_fast(reader)?;
                    },
                    "basedOn" => {
                        if let Ok(Some(val)) = xml::optional_attr_str(e, "w:val") {
                            based_on = Some(val.into_owned());
                        }
                        xml::skip_element_fast(reader)?;
                    },
                    "rPr" => {
                        run_properties = Some(parse_run_properties_fast(reader)?);
                    },
                    "pPr" => {
                        paragraph_properties = Some(parse_paragraph_properties_fast(reader)?);
                    },
                    "tblPr" => {
                        table_properties = Some(super::parse_table_properties(reader)?);
                    },
                    "tblStylePr" => {
                        let kind = xml::optional_attr_str(e, "w:type")?
                            .map(|v| v.into_owned())
                            .unwrap_or_default();
                        table_conditionals.push(parse_table_style_conditional(reader, kind)?);
                    },
                    _ => {
                        xml::skip_element_fast(reader)?;
                    },
                },
                Event::Empty(ref e) => match e.local_name().as_ref() {
                    "name" => {
                        if let Ok(Some(val)) = xml::optional_attr_str(e, "w:val") {
                            name = Some(val.into_owned());
                        }
                    },
                    "basedOn" => {
                        if let Ok(Some(val)) = xml::optional_attr_str(e, "w:val") {
                            based_on = Some(val.into_owned());
                        }
                    },
                    _ => {},
                },
                Event::End(ref e) if e.local_name().as_ref() == "style" => {
                    break;
                },
                Event::Eof => break,
                _ => {},
            }
        }
    }

    Ok(Some(Style {
        style_id,
        style_type,
        is_default,
        name,
        based_on,
        run_properties,
        paragraph_properties,
        table_properties,
        table_conditionals,
    }))
}

/// The body of one `w:tblStylePr` (its start already consumed).
fn parse_table_style_conditional(
    reader: &mut quick_xml::Reader<&[u8]>,
    kind: String,
) -> crate::core::Result<TableStyleConditional> {
    let mut cond = TableStyleConditional {
        kind,
        cell_properties: None,
        run_properties: None,
        paragraph_properties: None,
    };
    loop {
        match reader.read_event()? {
            Event::Start(ref e) => match e.local_name().as_ref() {
                "tcPr" => cond.cell_properties = Some(super::parse_table_cell_properties(reader)?),
                "rPr" => cond.run_properties = Some(parse_run_properties_fast(reader)?),
                "pPr" => cond.paragraph_properties = Some(parse_paragraph_properties_fast(reader)?),
                _ => xml::skip_element_fast(reader)?,
            },
            Event::End(ref e) if e.local_name().as_ref() == "tblStylePr" => break,
            Event::Eof => break,
            _ => {},
        }
    }
    Ok(cond)
}

/// A table style resolved through its `w:basedOn` chain.
#[derive(Debug, Default)]
pub(crate) struct ResolvedTableStyle<'a> {
    /// Borders from the nearest style in the chain that sets them.
    pub borders: Option<&'a TableBorders>,
    /// Rows / columns per band.
    pub row_band_size: Option<u32>,
    pub col_band_size: Option<u32>,
    /// Conditional formats by region; a derived style's entry replaces
    /// its base style's for the same region.
    pub conditionals: Vec<&'a TableStyleConditional>,
    /// The chain's own `w:rPr` / `w:pPr` (formatting for the whole table),
    /// root ancestor first.
    pub run_properties: Vec<&'a RunProperties>,
    pub paragraph_properties: Vec<&'a ParagraphProperties>,
}

/// The run and paragraph formatting a table style gives the content of one
/// cell: its own `w:rPr`/`w:pPr`, then each region's `w:tblStylePr` that
/// covers the cell, in increasing precedence. It sits between the document
/// defaults and the paragraph style (ECMA-376 Part 1 §17.7.2: defaults,
/// table style, numbering, paragraph style, run style, direct formatting).
#[derive(Debug, Clone, Default)]
pub(crate) struct CellStyleLayer {
    pub run: RunProperties,
    pub paragraph: ParagraphProperties,
}

impl ResolvedTableStyle<'_> {
    /// The layer for a cell covered by `regions` (from
    /// `TableStyleLayout::regions`, lowest precedence first), or `None`
    /// when the style formats no text at all there.
    pub(crate) fn cell_layer(&self, regions: &[&str]) -> Option<CellStyleLayer> {
        let conds = regions
            .iter()
            .filter_map(|kind| self.conditionals.iter().find(|c| c.kind == *kind));
        let any = !self.run_properties.is_empty()
            || !self.paragraph_properties.is_empty()
            || conds
                .clone()
                .any(|c| c.run_properties.is_some() || c.paragraph_properties.is_some());
        if !any {
            return None;
        }
        let mut layer = CellStyleLayer::default();
        for rp in &self.run_properties {
            layer.run.overlay(rp);
        }
        for pp in &self.paragraph_properties {
            layer.paragraph.overlay(pp);
        }
        for c in conds {
            if let Some(rp) = c.run_properties.as_ref() {
                layer.run.overlay(rp);
            }
            if let Some(pp) = c.paragraph_properties.as_ref() {
                layer.paragraph.overlay(pp);
            }
        }
        Some(layer)
    }
}

impl StyleSheet {
    /// Resolve table style `style_id` through its inheritance chain.
    pub(crate) fn resolve_table_style(&self, style_id: &str) -> ResolvedTableStyle<'_> {
        let mut out = ResolvedTableStyle::default();
        for style in self.chain(style_id) {
            if let Some(tp) = style.table_properties.as_ref() {
                if tp.borders.is_some() {
                    out.borders = tp.borders.as_ref();
                }
                out.row_band_size = tp.row_band_size.or(out.row_band_size);
                out.col_band_size = tp.col_band_size.or(out.col_band_size);
            }
            for c in &style.table_conditionals {
                out.conditionals.retain(|prev| prev.kind != c.kind);
                out.conditionals.push(c);
            }
            out.run_properties.extend(style.run_properties.as_ref());
            out.paragraph_properties
                .extend(style.paragraph_properties.as_ref());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_STYLES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults>
    <w:rPrDefault>
      <w:rPr>
        <w:sz w:val="22"/>
      </w:rPr>
    </w:rPrDefault>
  </w:docDefaults>
  <w:style w:type="paragraph" w:styleId="Normal">
    <w:name w:val="Normal"/>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading1">
    <w:name w:val="heading 1"/>
    <w:basedOn w:val="Normal"/>
    <w:pPr>
      <w:outlineLvl w:val="0"/>
    </w:pPr>
    <w:rPr>
      <w:b/>
      <w:sz w:val="32"/>
    </w:rPr>
  </w:style>
  <w:style w:type="character" w:styleId="Strong">
    <w:name w:val="Strong"/>
    <w:rPr>
      <w:b/>
    </w:rPr>
  </w:style>
</w:styles>"#;

    #[test]
    fn test_parse_stylesheet() {
        let sheet = StyleSheet::parse(SAMPLE_STYLES).unwrap();
        assert_eq!(sheet.styles.len(), 3);
        assert!(sheet.doc_defaults.is_some());
    }

    #[test]
    fn test_parse_doc_defaults_font_size() {
        let sheet = StyleSheet::parse(SAMPLE_STYLES).unwrap();
        let defaults = sheet.doc_defaults.as_ref().unwrap();
        let rp = defaults.run_properties.as_ref().unwrap();
        assert_eq!(rp.font_size, Some(crate::core::units::HalfPoint(22)));
    }

    #[test]
    fn test_parse_heading1_style() {
        let sheet = StyleSheet::parse(SAMPLE_STYLES).unwrap();
        let h1 = sheet.styles.get("Heading1").unwrap();
        assert_eq!(h1.name.as_deref(), Some("heading 1"));
        assert_eq!(h1.based_on.as_deref(), Some("Normal"));
        assert_eq!(h1.style_type, StyleType::Paragraph);

        let pp = h1.paragraph_properties.as_ref().unwrap();
        assert_eq!(pp.outline_level, Some(0));

        let rp = h1.run_properties.as_ref().unwrap();
        assert_eq!(rp.bold, Some(true));
        assert_eq!(rp.font_size, Some(crate::core::units::HalfPoint(32)));
    }

    #[test]
    fn test_resolve_outline_level() {
        let sheet = StyleSheet::parse(SAMPLE_STYLES).unwrap();
        assert_eq!(sheet.resolve_outline_level("Heading1"), Some(0));
        assert_eq!(sheet.resolve_outline_level("Normal"), None);
    }

    fn default_style_sheet(body: &str) -> StyleSheet {
        StyleSheet::parse(format!(
            r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">{body}</w:styles>"#
        ).as_bytes()).unwrap()
    }

    #[test]
    fn default_paragraph_style_supplies_run_paragraph_and_hidden_properties() {
        use crate::core::units::HalfPoint;
        let sheet = default_style_sheet(r#"
          <w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="20"/></w:rPr></w:rPrDefault></w:docDefaults>
          <w:style w:type="paragraph" w:styleId="Normal"><w:rPr><w:sz w:val="90"/></w:rPr></w:style>
          <w:style w:type="paragraph" w:default="1" w:styleId="BodyInAnyLanguage">
            <w:pPr><w:jc w:val="center"/></w:pPr>
            <w:rPr><w:sz w:val="28"/><w:vanish/></w:rPr>
          </w:style>"#);
        assert_eq!(sheet.default_paragraph_style_id.as_deref(), Some("BodyInAnyLanguage"));
        assert_eq!(sheet.effective_run_properties(None, None).font_size, Some(HalfPoint(28)));
        assert!(sheet.effective_hidden(None, None));
        assert_eq!(sheet.effective_paragraph_properties(None).justification,
            Some(super::super::Justification::Center));
        let direct = RunProperties { hidden: Some(false), ..Default::default() };
        assert!(!sheet.effective_hidden(None, Some(&direct)));
        // A pPr with direct formatting but no pStyle still inherits the default.
        let direct = ParagraphProperties { keep_next: Some(true), ..Default::default() };
        let effective = sheet.effective_paragraph_properties(Some(&direct));
        assert_eq!(effective.justification, Some(super::super::Justification::Center));
        assert_eq!(effective.keep_next, Some(true));
    }

    #[test]
    fn default_paragraph_style_respects_based_on_and_explicit_style_selection() {
        use crate::core::units::HalfPoint;
        let sheet = default_style_sheet(r#"
          <w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="20"/></w:rPr></w:rPrDefault></w:docDefaults>
          <w:style w:type="paragraph" w:styleId="Ancestor"><w:rPr><w:sz w:val="24"/><w:i/></w:rPr></w:style>
          <w:style w:type="paragraph" w:default="true" w:styleId="Body"><w:basedOn w:val="Ancestor"/><w:rPr><w:b/></w:rPr></w:style>
          <w:style w:type="paragraph" w:styleId="Independent"><w:rPr><w:sz w:val="36"/></w:rPr></w:style>
          <w:style w:type="paragraph" w:styleId="Derived"><w:basedOn w:val="Body"/><w:rPr><w:sz w:val="32"/></w:rPr></w:style>"#);
        let table = RunProperties { font_size: Some(HalfPoint(22)), ..Default::default() };
        let inherited = sheet.effective_run_properties_in(Some(&table), None, None);
        assert_eq!(inherited.font_size, Some(HalfPoint(24)));
        assert_eq!((inherited.bold, inherited.italic), (Some(true), Some(true)));
        let explicit = sheet.effective_run_properties(Some("Independent"), None);
        assert_eq!(explicit.font_size, Some(HalfPoint(36)));
        assert_eq!((explicit.bold, explicit.italic), (None, None), "default is not a base for every style");
        let derived = sheet.effective_run_properties(Some("Derived"), None);
        assert_eq!(derived.font_size, Some(HalfPoint(32)));
        assert_eq!((derived.bold, derived.italic), (Some(true), Some(true)));
        assert_eq!(sheet.effective_run_properties(Some("Missing"), None).font_size, Some(HalfPoint(20)));
    }

    #[test]
    fn default_paragraph_font_size_keeps_character_direct_and_script_precedence() {
        use crate::core::units::HalfPoint;
        use super::super::formatting::ScriptClass;
        let sheet = default_style_sheet(r#"
          <w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="20"/><w:szCs w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults>
          <w:style w:type="paragraph" w:default="on" w:styleId="Body"><w:rPr><w:sz w:val="28"/><w:szCs w:val="30"/></w:rPr></w:style>
          <w:style w:type="character" w:styleId="CharacterBase"><w:rPr><w:szCs w:val="34"/></w:rPr></w:style>
          <w:style w:type="character" w:styleId="Emphasis"><w:basedOn w:val="CharacterBase"/><w:rPr><w:sz w:val="32"/></w:rPr></w:style>"#);
        let inherited = sheet.effective_run_properties(None, None);
        assert_eq!(inherited.face_for(ScriptClass::EastAsia).font_size, Some(HalfPoint(28)));
        assert_eq!(inherited.face_for(ScriptClass::Complex).font_size, Some(HalfPoint(30)));
        let mut direct = RunProperties { style_id: Some("Emphasis".into()), ..Default::default() };
        let character = sheet.effective_run_properties(None, Some(&direct));
        assert_eq!(character.face_for(ScriptClass::Ascii).font_size, Some(HalfPoint(32)));
        assert_eq!(character.face_for(ScriptClass::Complex).font_size, Some(HalfPoint(34)));
        direct.font_size = Some(HalfPoint(40));
        let direct_size = sheet.effective_run_properties(None, Some(&direct));
        assert_eq!(direct_size.face_for(ScriptClass::Ascii).font_size, Some(HalfPoint(40)));
        assert_eq!(direct_size.face_for(ScriptClass::Complex).font_size, Some(HalfPoint(34)), "direct sz does not erase inherited szCs");
        direct.script = Some(Box::new(super::super::formatting::ScriptRunProperties {
            font_size_cs: Some(HalfPoint(42)), ..Default::default()
        }));
        let direct_both = sheet.effective_run_properties(None, Some(&direct));
        assert_eq!(direct_both.face_for(ScriptClass::Complex).font_size, Some(HalfPoint(42)));
    }

    #[test]
    fn default_paragraph_style_does_not_infer_default_from_name_or_false_attribute() {
        use crate::core::units::HalfPoint;
        for attribute in ["", r#"w:default="0""#, r#"w:default="false""#, r#"w:default="off""#] {
            let sheet = default_style_sheet(&format!(r#"
              <w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="20"/></w:rPr></w:rPrDefault></w:docDefaults>
              <w:style w:type="paragraph" w:styleId="Normal" {attribute}><w:name w:val="Normal"/><w:rPr><w:sz w:val="48"/></w:rPr></w:style>"#));
            assert_eq!(sheet.default_paragraph_style_id, None, "{attribute}");
            assert_eq!(sheet.effective_run_properties(None, None).font_size, Some(HalfPoint(20)));
            assert_eq!(sheet.effective_run_properties(Some("Normal"), None).font_size, Some(HalfPoint(48)));
        }
    }

    #[test]
    fn default_paragraph_style_uses_last_enabled_of_its_own_type() {
        use crate::core::units::HalfPoint;
        let sheet = default_style_sheet(r#"
          <w:style w:type="paragraph" w:default="1" w:styleId="First"><w:rPr><w:sz w:val="24"/></w:rPr></w:style>
          <w:style w:type="paragraph" w:default="true" w:styleId="Last"><w:rPr><w:sz w:val="28"/></w:rPr></w:style>
          <w:style w:type="paragraph" w:default="false" w:styleId="Disabled"><w:rPr><w:sz w:val="30"/></w:rPr></w:style>
          <w:style w:type="character" w:default="1" w:styleId="Character"><w:rPr><w:sz w:val="40"/></w:rPr></w:style>
          <w:style w:type="table" w:default="1" w:styleId="Table"><w:rPr><w:sz w:val="50"/></w:rPr></w:style>
          <w:style w:type="numbering" w:default="1" w:styleId="Numbering"><w:rPr><w:sz w:val="60"/></w:rPr></w:style>"#);
        assert_eq!(sheet.default_paragraph_style_id.as_deref(), Some("Last"));
        assert_eq!(sheet.effective_run_properties(None, None).font_size, Some(HalfPoint(28)));
        assert_eq!(sheet.effective_run_properties(Some("Character"), None).font_size, None,
            "a character style is not a paragraph style even when referenced by pStyle");
    }

    #[test]
    fn default_paragraph_style_accepts_empty_last_definition() {
        use crate::core::units::HalfPoint;
        let sheet = default_style_sheet(r#"
          <w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="20"/></w:rPr></w:rPrDefault></w:docDefaults>
          <w:style w:type="paragraph" w:default="1" w:styleId="First"><w:rPr><w:sz w:val="24"/></w:rPr></w:style>
          <w:style w:type="paragraph" w:default="1" w:styleId="Empty"/>"#);
        assert_eq!(sheet.default_paragraph_style_id.as_deref(), Some("Empty"));
        assert_eq!(sheet.effective_run_properties(None, None).font_size, Some(HalfPoint(20)));
    }

    #[test]
    fn default_paragraph_style_rejects_ambiguous_or_invalid_definitions() {
        for body in [
            r#"<w:style w:type="paragraph" w:default="1" w:styleId="Same"/><w:style w:type="character" w:styleId="Same"/>"#,
            r#"<w:style w:type="unknown" w:default="1" w:styleId="Bad"/>"#,
            r#"<w:style w:type="paragraph" w:default="yes" w:styleId="Bad"/>"#,
            r#"<w:style w:type="paragraph" w:default="1"/>"#,
        ] {
            let xml = format!(r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">{body}</w:styles>"#);
            assert!(StyleSheet::parse(xml.as_bytes()).is_err(), "{body}");
        }
    }

    #[test]
    fn default_paragraph_style_reaches_ir_without_aliasing_unknown_style_cache_entries() {
        use crate::core::opc::{OpcWriter, PartName};
        use crate::core::relationships::rel_types;
        use crate::ir::{Element, InlineContent, ParagraphAlignment};
        use std::io::Cursor;

        let styles = br#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="20"/></w:rPr></w:rPrDefault></w:docDefaults>
          <w:style w:type="paragraph" w:default="1" w:styleId="Body">
            <w:pPr><w:jc w:val="center"/></w:pPr><w:rPr><w:sz w:val="28"/></w:rPr>
          </w:style>
        </w:styles>"#;
        // The omitted and explicit-unknown pStyle used to intern to the same
        // cache key. Exercise both insertion orders and repeated cache hits.
        for first_unknown in [false, true] {
            let mut body = String::new();
            for index in 0..4 {
                let unknown = (index % 2 == 0) == first_unknown;
                let ppr = if unknown { r#"<w:pPr><w:pStyle w:val="Missing"/></w:pPr>"# } else { "" };
                body.push_str(&format!("<w:p>{ppr}<w:r><w:t>正文{index}</w:t></w:r></w:p>"));
            }
            let document_xml = format!(r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#);
            let mut writer = OpcWriter::new(Cursor::new(Vec::new())).unwrap();
            let doc_part = PartName::new("/word/document.xml").unwrap();
            writer.add_part(&doc_part,
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
                document_xml.as_bytes()).unwrap();
            writer.add_package_rel(rel_types::OFFICE_DOCUMENT, "word/document.xml");
            writer.add_part(&PartName::new("/word/styles.xml").unwrap(),
                "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml", styles).unwrap();
            writer.add_part_rel(&doc_part, rel_types::STYLES, "styles.xml");
            let doc = super::super::DocxDocument::from_reader(Cursor::new(writer.finish().unwrap().into_inner())).unwrap();
            let ir = crate::convert_docx::docx_to_ir(&doc);
            assert_eq!(ir.sections[0].elements.len(), 4);
            for (index, element) in ir.sections[0].elements.iter().enumerate() {
                let Element::Paragraph(paragraph) = element else { panic!("expected paragraph") };
                let unknown = (index % 2 == 0) == first_unknown;
                assert_eq!(paragraph.alignment, if unknown { None } else { Some(ParagraphAlignment::Center) });
                let InlineContent::Text(span) = &paragraph.content[0] else { panic!("expected text") };
                assert_eq!(span.text, format!("正文{index}"));
                assert_eq!(span.font_size_half_pt, Some(if unknown { 20 } else { 28 }));
            }
        }
    }
}
