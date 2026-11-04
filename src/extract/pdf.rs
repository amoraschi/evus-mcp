//! PDF text extraction. pdf-extract was picked over lopdf because it exposes
//! a single "give me plain text" entry point (extract_text_from_mem), and lopdf
//! is a lower-level PDF object model that would need its own text-run
//! assembly on top, which is exactly what pdf-extract already does.

use anyhow::{Context, Result};

pub fn extract_text(buf: &[u8]) -> Result<String> {
    pdf_extract::extract_text_from_mem(buf).context("extracting PDF text")
}
