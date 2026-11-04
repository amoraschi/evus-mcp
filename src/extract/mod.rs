//! Text extraction dispatch by mime type. Direct port of server.mjs's
//! extractText, split by format into sibling modules instead of one big
//! function.

pub mod legacy_office;
pub mod office;
pub mod pdf;
pub mod text_query;

use anyhow::{bail, Result};

const OFFICE_MIME_TYPES: &[&str] = &[
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document", // .docx
    "application/vnd.openxmlformats-officedocument.presentationml.presentation", // .pptx
];
const LEGACY_OFFICE_MIME_TYPES: &[&str] = &[
    "application/vnd.ms-powerpoint", // .ppt
    "application/msword",            // .doc
];

pub async fn extract_text(buf: Vec<u8>, mime_type: Option<&str>) -> Result<String> {
    let mime_type = mime_type.unwrap_or_default().to_string();
    tokio::task::spawn_blocking(move || extract_text_blocking(&buf, &mime_type)).await?
}

fn extract_text_blocking(buf: &[u8], mime_type: &str) -> Result<String> {
    if mime_type == "application/pdf" {
        return pdf::extract_text(buf);
    }
    if mime_type.starts_with("text/") {
        return Ok(String::from_utf8_lossy(buf).into_owned());
    }
    if OFFICE_MIME_TYPES.contains(&mime_type) {
        return office::extract_text(buf, mime_type);
    }
    if LEGACY_OFFICE_MIME_TYPES.contains(&mime_type) {
        return Ok(legacy_office::extract_legacy_office_text(buf));
    }
    bail!("Text extraction not supported for mimeType \"{mime_type}\".")
}
