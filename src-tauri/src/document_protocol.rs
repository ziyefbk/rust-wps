// Shared production wire types; no Tauri runtime required.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct SavedPresentation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
    pub path: String,
    pub report: rust_wps::editing::EditReport,
    pub presentation: rust_wps::presentation::PresentationReport,
}

#[derive(Serialize, Deserialize)]
pub struct OpenedDocument {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
    pub name: String,
    pub path: String,
    pub inspection: rust_wps::Inspection,
    pub authored: Option<rust_wps::document::Node>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workbook: Option<Box<rust_wps::spreadsheet::WorkbookReport>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workbook_model: Option<Box<rust_wps::spreadsheet_edit::Workbook>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<Box<rust_wps::presentation::PresentationReport>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation_model: Option<Box<rust_wps::presentation_model::Presentation>>,
}
