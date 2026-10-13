use std::collections::HashMap;

use quick_xml::events::Event;

use super::error::{Error, Result};
use super::xml;

/// The 12 named theme color slots in DrawingML.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThemeColorSlot {
    /// Dark 1 (text/foreground primary).
    Dk1,
    /// Light 1 (background primary).
    Lt1,
    /// Dark 2 (text/foreground secondary).
    Dk2,
    /// Light 2 (background secondary).
    Lt2,
    /// Accent color 1.
    Accent1,
    /// Accent color 2.
    Accent2,
    /// Accent color 3.
    Accent3,
    /// Accent color 4.
    Accent4,
    /// Accent color 5.
    Accent5,
    /// Accent color 6.
    Accent6,
    /// Hyperlink color.
    Hlink,
    /// Followed hyperlink color.
    FolHlink,
}

impl ThemeColorSlot {
    /// Parse a WordprocessingML `ST_ThemeColor` using the default color mapping.
    /// Word uses different names from DrawingML (for example `dark1` and
    /// `hyperlink`). Custom `w:clrSchemeMapping` is not represented here.
    pub fn from_word_val(s: &str) -> Option<Self> {
        match s {
            "dark1" | "text1" => Some(Self::Dk1),
            "light1" | "background1" => Some(Self::Lt1),
            "dark2" | "text2" => Some(Self::Dk2),
            "light2" | "background2" => Some(Self::Lt2),
            "accent1" => Some(Self::Accent1),
            "accent2" => Some(Self::Accent2),
            "accent3" => Some(Self::Accent3),
            "accent4" => Some(Self::Accent4),
            "accent5" => Some(Self::Accent5),
            "accent6" => Some(Self::Accent6),
            "hyperlink" => Some(Self::Hlink),
            "followedHyperlink" => Some(Self::FolHlink),
            _ => None,
        }
    }

    /// Parse from a scheme color value string (e.g. "accent1", "dk1", "tx1").
    pub fn from_scheme_val(s: &str) -> Option<Self> {
        match s {
            "dk1" | "tx1" => Some(Self::Dk1),
            "lt1" | "bg1" => Some(Self::Lt1),
            "dk2" | "tx2" => Some(Self::Dk2),
            "lt2" | "bg2" => Some(Self::Lt2),
            "accent1" => Some(Self::Accent1),
            "accent2" => Some(Self::Accent2),
            "accent3" => Some(Self::Accent3),
            "accent4" => Some(Self::Accent4),
            "accent5" => Some(Self::Accent5),
            "accent6" => Some(Self::Accent6),
            "hlink" => Some(Self::Hlink),
            "folHlink" => Some(Self::FolHlink),
            _ => None,
        }
    }

    fn from_element_name(name: &str) -> Option<Self> {
        match name {
            "dk1" => Some(Self::Dk1),
            "lt1" => Some(Self::Lt1),
            "dk2" => Some(Self::Dk2),
            "lt2" => Some(Self::Lt2),
            "accent1" => Some(Self::Accent1),
            "accent2" => Some(Self::Accent2),
            "accent3" => Some(Self::Accent3),
            "accent4" => Some(Self::Accent4),
            "accent5" => Some(Self::Accent5),
            "accent6" => Some(Self::Accent6),
            "hlink" => Some(Self::Hlink),
            "folHlink" => Some(Self::FolHlink),
            _ => None,
        }
    }
}

/// An sRGB color as 3 bytes (red, green, blue).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbColor(pub [u8; 3]);

impl RgbColor {
    /// Parse an sRGB color from a 6-character hex string (e.g., `"4472C4"`).
    ///
    /// # Errors
    /// Returns an error if `hex` is not exactly 6 hex characters.
    pub fn from_hex(hex: &str) -> Result<Self> {
        if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Error::MalformedXml(format!(
                "invalid hex color (expected 6 ASCII hex digits): {hex}"
            )));
        }
        let r = u8::from_str_radix(&hex[0..2], 16)?;
        let g = u8::from_str_radix(&hex[2..4], 16)?;
        let b = u8::from_str_radix(&hex[4..6], 16)?;
        Ok(Self([r, g, b]))
    }

    /// Format as a 6-character uppercase hex string (e.g., `"4472C4"`).
    pub fn to_hex(&self) -> String {
        format!("{:02X}{:02X}{:02X}", self.0[0], self.0[1], self.0[2])
    }

    /// Red component (0–255).
    pub fn red(&self) -> u8 {
        self.0[0]
    }

    /// Green component (0–255).
    pub fn green(&self) -> u8 {
        self.0[1]
    }

    /// Blue component (0–255).
    pub fn blue(&self) -> u8 {
        self.0[2]
    }
}

/// Color scheme: the 12 named colors in a theme.
#[derive(Debug, Clone)]
pub struct ColorScheme {
    /// Scheme name (e.g., "Office").
    pub name: String,
    /// Mapping from slot to resolved RGB color.
    pub colors: HashMap<ThemeColorSlot, RgbColor>,
}

/// Font scheme: major (heading) and minor (body) fonts.
#[derive(Debug, Clone)]
pub struct FontScheme {
    /// Scheme name.
    pub name: String,
    /// Latin typeface for major (heading) font.
    pub major_latin: String,
    /// Latin typeface for minor (body) font.
    pub minor_latin: String,
    /// East Asian typeface for major font (empty = inherit).
    pub major_ea: Option<String>,
    /// East Asian typeface for minor font (empty = inherit).
    pub minor_ea: Option<String>,
    /// Complex-script typeface for major font (empty = inherit).
    pub major_cs: Option<String>,
    /// Complex-script typeface for minor font (empty = inherit).
    pub minor_cs: Option<String>,
    /// Supplemental ISO 15924 script faces in the major font collection.
    pub major_scripts: std::collections::BTreeMap<String, String>,
    /// Supplemental ISO 15924 script faces in the minor font collection.
    pub minor_scripts: std::collections::BTreeMap<String, String>,
}

/// A parsed DrawingML theme (`a:theme`).
#[derive(Debug, Clone)]
pub struct Theme {
    /// Theme name.
    pub name: String,
    /// Color scheme.
    pub color_scheme: ColorScheme,
    /// Font scheme.
    pub font_scheme: FontScheme,
}

impl Theme {
    /// Parse a DrawingML theme from XML bytes.
    pub fn parse(xml_data: &[u8]) -> Result<Self> {
        let mut reader = xml::make_fast_reader(xml_data);

        let mut theme_name = String::new();
        let mut color_scheme = None;
        let mut font_scheme = None;

        loop {
            match reader.read_event()? {
                Event::Start(ref e) => {
                    let local = e.local_name();
                    let local_bytes = local.as_ref();

                    match local_bytes {
                        "theme" => {
                            if let Ok(Some(name)) = xml::optional_attr_str(e, "name") {
                                theme_name = name.into_owned();
                            }
                        },
                        "clrScheme" => {
                            color_scheme = Some(parse_color_scheme(&mut reader, e)?);
                        },
                        "fontScheme" => {
                            font_scheme = Some(parse_font_scheme(&mut reader, e)?);
                        },
                        _ => {},
                    }
                },
                Event::Eof => break,
                _ => {},
            }
        }

        let color_scheme = color_scheme
            .ok_or_else(|| Error::MalformedXml("theme missing clrScheme".to_string()))?;
        let font_scheme = font_scheme
            .ok_or_else(|| Error::MalformedXml("theme missing fontScheme".to_string()))?;

        Ok(Theme {
            name: theme_name,
            color_scheme,
            font_scheme,
        })
    }

    /// Resolve a theme color slot to its RGB value.
    pub fn resolve_color(&self, slot: ThemeColorSlot) -> Option<&RgbColor> {
        self.color_scheme.colors.get(&slot)
    }
}

/// Parse `<a:clrScheme>` element and its children.
fn parse_color_scheme(
    reader: &mut quick_xml::Reader<&[u8]>,
    start: &quick_xml::events::BytesStart,
) -> Result<ColorScheme> {
    let name = xml::optional_attr_str(start, "name")?
        .map(|c| c.into_owned())
        .unwrap_or_default();
    let mut colors = HashMap::new();
    let mut current_slot: Option<ThemeColorSlot> = None;

    loop {
        match reader.read_event()? {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let local = e.local_name();
                let local_bytes = local.as_ref();

                // Check if this is a color slot element (dk1, lt1, accent1, etc.)
                if let Some(slot) = ThemeColorSlot::from_element_name(local_bytes) {
                    current_slot = Some(slot);
                } else if local_bytes == "srgbClr" {
                    // <a:srgbClr val="4472C4"/>
                    if let Some(slot) = current_slot {
                        if let Ok(val) = xml::required_attr_str(e, "val") {
                            if let Ok(rgb) = RgbColor::from_hex(&val) {
                                colors.insert(slot, rgb);
                            }
                        }
                    }
                } else if local_bytes == "sysClr" {
                    // <a:sysClr val="windowText" lastClr="000000"/>
                    if let Some(slot) = current_slot {
                        // Use lastClr for the actual color value
                        if let Ok(Some(last_clr)) = xml::optional_attr_str(e, "lastClr") {
                            if let Ok(rgb) = RgbColor::from_hex(&last_clr) {
                                colors.insert(slot, rgb);
                            }
                        }
                    }
                }
            },
            Event::End(ref e) => {
                let local = e.local_name();
                let local_bytes = local.as_ref();

                if local_bytes == "clrScheme" {
                    break;
                }
                // Reset current_slot when we leave a color slot element
                if ThemeColorSlot::from_element_name(local_bytes).is_some() {
                    current_slot = None;
                }
            },
            Event::Eof => break,
            _ => {},
        }
    }

    Ok(ColorScheme { name, colors })
}

/// Parse `<a:fontScheme>` element and its children.
fn parse_font_scheme(
    reader: &mut quick_xml::Reader<&[u8]>,
    start: &quick_xml::events::BytesStart,
) -> Result<FontScheme> {
    let name = xml::optional_attr_str(start, "name")?
        .map(|c| c.into_owned())
        .unwrap_or_default();

    let mut major_latin = String::new();
    let mut minor_latin = String::new();
    let mut major_ea: Option<String> = None;
    let mut minor_ea: Option<String> = None;
    let mut major_cs: Option<String> = None;
    let mut minor_cs: Option<String> = None;
    let mut major_scripts = std::collections::BTreeMap::new();
    let mut minor_scripts = std::collections::BTreeMap::new();

    enum FontCtx {
        None,
        Major,
        Minor,
    }
    let mut ctx = FontCtx::None;

    loop {
        match reader.read_event()? {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let local = e.local_name();
                let local_bytes = local.as_ref();

                match local_bytes {
                    "majorFont" => ctx = FontCtx::Major,
                    "minorFont" => ctx = FontCtx::Minor,
                    "latin" => {
                        if let Ok(Some(tf)) = xml::optional_attr_str(e, "typeface") {
                            let typeface = tf.into_owned();
                            match ctx {
                                FontCtx::Major => major_latin = typeface,
                                FontCtx::Minor => minor_latin = typeface,
                                FontCtx::None => {},
                            }
                        }
                    },
                    "ea" => {
                        if let Ok(Some(tf)) = xml::optional_attr_str(e, "typeface") {
                            let typeface = tf.into_owned();
                            if !typeface.is_empty() {
                                match ctx {
                                    FontCtx::Major => major_ea = Some(typeface),
                                    FontCtx::Minor => minor_ea = Some(typeface),
                                    FontCtx::None => {},
                                }
                            }
                        }
                    },
                    "cs" => {
                        if let Ok(Some(tf)) = xml::optional_attr_str(e, "typeface") {
                            let typeface = tf.into_owned();
                            if !typeface.is_empty() {
                                match ctx {
                                    FontCtx::Major => major_cs = Some(typeface),
                                    FontCtx::Minor => minor_cs = Some(typeface),
                                    FontCtx::None => {},
                                }
                            }
                        }
                    },
                    "font" => {
                        if let (Some(script), Some(typeface)) = (
                            xml::optional_attr_str(e, "script")?,
                            xml::optional_attr_str(e, "typeface")?,
                        ) {
                            if !script.is_empty() && !typeface.is_empty() {
                                match ctx {
                                    FontCtx::Major => { major_scripts.insert(script.into_owned(), typeface.into_owned()); },
                                    FontCtx::Minor => { minor_scripts.insert(script.into_owned(), typeface.into_owned()); },
                                    FontCtx::None => {},
                                }
                            }
                        }
                    },
                    _ => {},
                }
            },
            Event::End(ref e) => {
                let local = e.local_name();
                let local_bytes = local.as_ref();

                match local_bytes {
                    "fontScheme" => break,
                    "majorFont" | "minorFont" => ctx = FontCtx::None,
                    _ => {},
                }
            },
            Event::Eof => break,
            _ => {},
        }
    }

    Ok(FontScheme {
        name,
        major_latin,
        minor_latin,
        major_ea,
        minor_ea,
        major_cs,
        minor_cs,
        major_scripts,
        minor_scripts,
    })
}

// ---------------------------------------------------------------------------
// ColorRef — used by format crates for color resolution
// ---------------------------------------------------------------------------

/// A color reference that may need theme resolution.
#[derive(Debug, Clone)]
pub enum ColorRef {
    /// Direct sRGB color.
    Rgb(RgbColor),
    /// Theme color with optional tint/shade transforms.
    Theme {
        /// Theme color slot index.
        slot: ThemeColorSlot,
        /// Format-specific tint adjustment. Word stores the hexadecimal
        /// fraction (0.0–1.0); resolve it with `resolve_word_opt`.
        tint: Option<f64>,
        /// Shade multiplier (0.0–1.0); Word applies it to HSL luminance.
        shade: Option<f64>,
        /// The literal `w:val` / `val` colour written alongside the theme
        /// reference. OOXML producers emit it as the last-resort value for
        /// consumers that cannot resolve the theme; keeping it means a
        /// document with a missing or unreadable theme part still renders
        /// its colours instead of falling back to black.
        fallback: Option<RgbColor>,
    },
    /// System color (e.g., "windowText", "window").
    System(String),
    /// Automatic color.
    Auto,
}

impl ColorRef {
    /// Resolve a Word run color to fixed RGB. Word transforms HSL luminance,
    /// with tint taking precedence over shade (MS-OI29500, section 2.1.72).
    /// The literal `w:val` is already the final color, so an unavailable theme
    /// must not apply its transforms a second time. Automatic colors and
    /// unresolved themes without a literal fallback stay unspecified.
    pub fn resolve_word_opt(&self, theme: Option<&Theme>) -> Option<RgbColor> {
        match self {
            Self::Theme { slot, tint, shade, fallback } => {
                match theme.and_then(|theme| theme.resolve_color(*slot)) {
                    Some(base) => Some(apply_word_tint_shade(base, *tint, *shade)),
                    None => fallback.clone(),
                }
            },
            _ => self.resolve_opt(theme),
        }
    }

    /// Resolve this color reference to an RGB value using the given theme.
    pub fn resolve(&self, theme: &Theme) -> RgbColor {
        match self {
            Self::Rgb(rgb) => rgb.clone(),
            Self::Theme {
                slot,
                tint,
                shade,
                fallback,
            } => {
                let base = theme
                    .resolve_color(*slot)
                    .cloned()
                    .or_else(|| fallback.clone())
                    .unwrap_or(RgbColor([0, 0, 0]));
                apply_tint_shade(&base, *tint, *shade)
            },
            Self::System(name) => {
                // Map common system colors to reasonable defaults
                match name.as_str() {
                    "windowText" => RgbColor([0, 0, 0]),
                    "window" => RgbColor([255, 255, 255]),
                    "highlightText" => RgbColor([255, 255, 255]),
                    "highlight" => RgbColor([0, 120, 215]),
                    _ => RgbColor([0, 0, 0]),
                }
            },
            Self::Auto => RgbColor([0, 0, 0]),
        }
    }

    /// Resolve to an explicit RGB value, or `None` when the reference
    /// carries no colour of its own (`auto`, or an unresolvable theme slot
    /// with no `w:val` fallback). Unlike [`Self::resolve`] this never
    /// invents black, so callers can leave the renderer's default in place.
    pub fn resolve_opt(&self, theme: Option<&Theme>) -> Option<RgbColor> {
        match self {
            Self::Rgb(rgb) => Some(rgb.clone()),
            Self::Theme {
                slot,
                tint,
                shade,
                fallback,
            } => {
                let base = theme
                    .and_then(|t| t.resolve_color(*slot).cloned())
                    .or_else(|| fallback.clone())?;
                Some(apply_tint_shade(&base, *tint, *shade))
            },
            Self::System(name) => Some(match name.as_str() {
                "window" | "highlightText" => RgbColor([255, 255, 255]),
                "highlight" => RgbColor([0, 120, 215]),
                _ => RgbColor([0, 0, 0]),
            }),
            Self::Auto => None,
        }
    }
}

/// Word's hexadecimal tint/shade values are fractions of 255. Preserve hue
/// and saturation while changing HSL luminance: shade L'=L*s; tint
/// L'=L*t+(1-t). Keep the generic resolver separate for other formats.
fn apply_word_tint_shade(base: &RgbColor, tint: Option<f64>, shade: Option<f64>) -> RgbColor {
    if tint.is_none() && shade.is_none() {
        return base.clone();
    }
    let rgb = base.0.map(|component| f64::from(component) / 255.0);
    let min = rgb.into_iter().fold(f64::INFINITY, f64::min);
    let max = rgb.into_iter().fold(f64::NEG_INFINITY, f64::max);
    let luminance = (min + max) / 2.0;
    let adjusted = match tint {
        Some(tint) => luminance * tint + (1.0 - tint),
        None => luminance * shade.unwrap_or(1.0),
    }.clamp(0.0, 1.0);
    let delta = max - min;
    let transformed = if delta == 0.0 {
        [adjusted; 3]
    } else {
        let saturation = delta / (1.0 - (2.0 * luminance - 1.0).abs());
        let chroma = (1.0 - (2.0 * adjusted - 1.0).abs()) * saturation;
        // Positions within the original RGB extrema encode the unchanged
        // hue; rescale that chroma around the new luminance.
        rgb.map(|component| adjusted + ((component - min) / delta - 0.5) * chroma)
    };
    RgbColor(transformed.map(|component| (component * 255.0).round().clamp(0.0, 255.0) as u8))
}

/// Apply tint and shade transformations to a base color.
fn apply_tint_shade(base: &RgbColor, tint: Option<f64>, shade: Option<f64>) -> RgbColor {
    let mut r = base.0[0] as f64;
    let mut g = base.0[1] as f64;
    let mut b = base.0[2] as f64;

    // Apply shade (darken): component = component * shade
    if let Some(shade_val) = shade {
        r *= shade_val;
        g *= shade_val;
        b *= shade_val;
    }

    // Apply tint (lighten): component = component + (255 - component) * tint
    if let Some(tint_val) = tint {
        r = r + (255.0 - r) * tint_val;
        g = g + (255.0 - g) * tint_val;
        b = b + (255.0 - b) * tint_val;
    }

    RgbColor([
        r.round().clamp(0.0, 255.0) as u8,
        g.round().clamp(0.0, 255.0) as u8,
        b.round().clamp(0.0, 255.0) as u8,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_THEME: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office Theme">
  <a:themeElements>
    <a:clrScheme name="Office">
      <a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1>
      <a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>
      <a:dk2><a:srgbClr val="44546A"/></a:dk2>
      <a:lt2><a:srgbClr val="E7E6E6"/></a:lt2>
      <a:accent1><a:srgbClr val="4472C4"/></a:accent1>
      <a:accent2><a:srgbClr val="ED7D31"/></a:accent2>
      <a:accent3><a:srgbClr val="A5A5A5"/></a:accent3>
      <a:accent4><a:srgbClr val="FFC000"/></a:accent4>
      <a:accent5><a:srgbClr val="5B9BD5"/></a:accent5>
      <a:accent6><a:srgbClr val="70AD47"/></a:accent6>
      <a:hlink><a:srgbClr val="0563C1"/></a:hlink>
      <a:folHlink><a:srgbClr val="954F72"/></a:folHlink>
    </a:clrScheme>
    <a:fontScheme name="Office">
      <a:majorFont>
        <a:latin typeface="Calibri Light"/>
        <a:ea typeface=""/>
        <a:cs typeface=""/>
      </a:majorFont>
      <a:minorFont>
        <a:latin typeface="Calibri"/>
        <a:ea typeface=""/>
        <a:cs typeface=""/>
      </a:minorFont>
    </a:fontScheme>
    <a:fmtScheme name="Office">
      <a:fillStyleLst>
        <a:solidFill><a:schemeClr val="phClr"/></a:solidFill>
      </a:fillStyleLst>
      <a:lnStyleLst>
        <a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln>
      </a:lnStyleLst>
      <a:effectStyleLst>
        <a:effectStyle><a:effectLst/></a:effectStyle>
      </a:effectStyleLst>
      <a:bgFillStyleLst>
        <a:solidFill><a:schemeClr val="phClr"/></a:solidFill>
      </a:bgFillStyleLst>
    </a:fmtScheme>
  </a:themeElements>
</a:theme>"#;

    #[test]
    fn test_parse_theme() {
        let theme = Theme::parse(SAMPLE_THEME).unwrap();
        assert_eq!(theme.name, "Office Theme");
        assert_eq!(theme.color_scheme.name, "Office");
        assert_eq!(theme.font_scheme.name, "Office");
    }

    #[test]
    fn test_parse_theme_colors() {
        let theme = Theme::parse(SAMPLE_THEME).unwrap();
        let cs = &theme.color_scheme;

        // System colors (via lastClr)
        assert_eq!(cs.colors.get(&ThemeColorSlot::Dk1), Some(&RgbColor([0, 0, 0])));
        assert_eq!(cs.colors.get(&ThemeColorSlot::Lt1), Some(&RgbColor([255, 255, 255])));

        // sRGB colors
        assert_eq!(cs.colors.get(&ThemeColorSlot::Accent1), Some(&RgbColor([0x44, 0x72, 0xC4])));
        assert_eq!(cs.colors.get(&ThemeColorSlot::Hlink), Some(&RgbColor([0x05, 0x63, 0xC1])));

        // All 12 slots present
        assert_eq!(cs.colors.len(), 12);
    }

    #[test]
    fn test_parse_theme_fonts() {
        let theme = Theme::parse(SAMPLE_THEME).unwrap();
        assert_eq!(theme.font_scheme.major_latin, "Calibri Light");
        assert_eq!(theme.font_scheme.minor_latin, "Calibri");
        assert!(theme.font_scheme.major_ea.is_none()); // empty string = None
        assert!(theme.font_scheme.minor_ea.is_none());
    }

    #[test]
    fn test_color_ref_resolve_rgb() {
        let theme = Theme::parse(SAMPLE_THEME).unwrap();
        let color = ColorRef::Rgb(RgbColor([128, 64, 32]));
        assert_eq!(color.resolve(&theme), RgbColor([128, 64, 32]));
    }

    #[test]
    fn test_color_ref_resolve_theme() {
        let theme = Theme::parse(SAMPLE_THEME).unwrap();
        let color = ColorRef::Theme {
            fallback: None,
            slot: ThemeColorSlot::Accent1,
            tint: None,
            shade: None,
        };
        assert_eq!(color.resolve(&theme), RgbColor([0x44, 0x72, 0xC4]));
    }

    #[test]
    fn test_color_ref_resolve_theme_with_tint() {
        let theme = Theme::parse(SAMPLE_THEME).unwrap();
        let color = ColorRef::Theme {
            fallback: None,
            slot: ThemeColorSlot::Dk1,
            tint: Some(0.5),
            shade: None,
        };
        let resolved = color.resolve(&theme);
        // Black (0,0,0) with 50% tint = (128, 128, 128) approximately
        assert_eq!(resolved, RgbColor([128, 128, 128]));
    }

    #[test]
    fn test_rgb_hex_round_trip() {
        let rgb = RgbColor::from_hex("4472C4").unwrap();
        assert_eq!(rgb.to_hex(), "4472C4");
        assert_eq!(rgb.red(), 0x44);
        assert_eq!(rgb.green(), 0x72);
        assert_eq!(rgb.blue(), 0xC4);
    }

    #[test]
    fn rgb_hex_rejects_non_ascii_without_slicing_inside_utf8() {
        for invalid in ["中ABC", "A中BC", "ABC中", "GG0000", "12345", "1234567", "+12345"] {
            assert!(RgbColor::from_hex(invalid).is_err(), "{invalid}");
        }
        assert_eq!(RgbColor::from_hex("aBcD09").unwrap(), RgbColor([0xAB, 0xCD, 0x09]));
    }

    #[test]
    fn word_tint_and_shade_change_hsl_luminance_with_tint_precedence() {
        // MS-OI29500 2.1.72: 4F81BD with themeTint="99" becomes 95B3D7.
        let blue = RgbColor([0x4F, 0x81, 0xBD]);
        assert_eq!(apply_word_tint_shade(&blue, Some(153.0 / 255.0), None), RgbColor([0x95, 0xB3, 0xD7]));
        // RGB-channel multiplication would give [128, 64, 64]; HSL shade
        // preserves the saturated red hue as luminance crosses 0.5.
        assert_eq!(apply_word_tint_shade(&RgbColor([255, 128, 128]), None, Some(0.5)), RgbColor([192, 0, 0]));
        assert_eq!(apply_word_tint_shade(&RgbColor([255, 0, 0]), Some(0.5), None), RgbColor([255, 128, 128]));
        assert_eq!(apply_word_tint_shade(&blue, Some(1.0), Some(0.0)), blue);
        assert_eq!(apply_word_tint_shade(&blue, Some(0.0), None), RgbColor([255, 255, 255]));
        assert_eq!(apply_word_tint_shade(&blue, None, Some(0.0)), RgbColor([0, 0, 0]));
        assert_eq!(apply_word_tint_shade(&blue, None, Some(1.0)), blue);
        assert_eq!(apply_word_tint_shade(&RgbColor([0, 0, 0]), Some(0.6), None), RgbColor([102, 102, 102]));
    }

    #[test]
    fn word_theme_resolution_does_not_transform_literal_fallback_again() {
        let mut theme = Theme::parse(SAMPLE_THEME).unwrap();
        theme.color_scheme.colors.insert(ThemeColorSlot::Accent1, RgbColor([0x4F, 0x81, 0xBD]));
        let color = ColorRef::Theme {
            slot: ThemeColorSlot::Accent1,
            tint: Some(153.0 / 255.0),
            shade: Some(0.0),
            fallback: Some(RgbColor([1, 2, 3])),
        };
        assert_eq!(color.resolve_word_opt(Some(&theme)), Some(RgbColor([0x95, 0xB3, 0xD7])));
        assert_eq!(color.resolve_word_opt(None), Some(RgbColor([1, 2, 3])));
        theme.color_scheme.colors.remove(&ThemeColorSlot::Accent1);
        assert_eq!(color.resolve_word_opt(Some(&theme)), Some(RgbColor([1, 2, 3])));
        let unresolved = ColorRef::Theme { slot: ThemeColorSlot::Accent1, tint: None, shade: None, fallback: None };
        assert_eq!(unresolved.resolve_word_opt(Some(&theme)), None);
        assert_eq!(unresolved.resolve_word_opt(None), None);
        assert_eq!(ColorRef::Auto.resolve_word_opt(Some(&theme)), None);
        assert_eq!(ColorRef::Rgb(RgbColor([1, 2, 3])).resolve_word_opt(None), Some(RgbColor([1, 2, 3])));
    }

    #[test]
    fn test_theme_color_slot_from_scheme_val() {
        assert_eq!(ThemeColorSlot::from_scheme_val("accent1"), Some(ThemeColorSlot::Accent1));
        assert_eq!(ThemeColorSlot::from_scheme_val("dk1"), Some(ThemeColorSlot::Dk1));
        // tx1 maps to dk1 (text1 = dark1)
        assert_eq!(ThemeColorSlot::from_scheme_val("tx1"), Some(ThemeColorSlot::Dk1));
        // bg1 maps to lt1 (background1 = light1)
        assert_eq!(ThemeColorSlot::from_scheme_val("bg1"), Some(ThemeColorSlot::Lt1));
    }
}
