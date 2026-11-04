//! docx/pptx text extraction. Both formats are zip archives of XML, and officeparser's
//! job here is just "give me the text", so instead of pulling in a heavy
//! office-parsing crate we unzip with `zip` and pull text runs out of the XML
//! with `quick-xml` directly: w:t elements for docx (word/document.xml), a:t
//! elements for pptx (ppt/slides/slideN.xml, in slide order).

use anyhow::{Context, Result};
use quick_xml::escape::unescape;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::io::{Cursor, Read};
use zip::ZipArchive;

/// Pull all text inside `tag` elements out of one XML document, inserting a
/// newline at each paragraph-start tag (`para_tag`, e.g. "w:p" or "a:p") so
/// output roughly follows the original paragraph structure.
fn extract_runs(xml: &str, text_tag: &str, para_tag: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut out = String::new();
    let mut in_text = false;
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = e.name().as_ref().to_string();
                if local == text_tag {
                    in_text = true;
                } else if local == para_tag {
                    out.push('\n');
                }
            }
            Ok(Event::End(e)) => {
                if e.name().as_ref() == text_tag {
                    in_text = false;
                }
            }
            Ok(Event::Text(t)) if in_text => {
                if let Ok(unescaped) = unescape(&t) {
                    out.push_str(&unescaped);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

fn read_entry(archive: &mut ZipArchive<Cursor<&[u8]>>, path: &str) -> Option<String> {
    let mut file = archive.by_name(path).ok()?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).ok()?;
    Some(contents)
}

pub fn extract_text(buf: &[u8], mime_type: &str) -> Result<String> {
    let mut archive = ZipArchive::new(Cursor::new(buf)).context("opening office file as zip")?;

    const DOCX: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
    const PPTX: &str = "application/vnd.openxmlformats-officedocument.presentationml.presentation";

    if mime_type == DOCX {
        let xml = read_entry(&mut archive, "word/document.xml")
            .context("docx missing word/document.xml")?;
        return Ok(extract_runs(&xml, "w:t", "w:p").trim().to_string());
    }

    if mime_type == PPTX {
        let mut slide_names: Vec<String> = archive
            .file_names()
            .filter_map(|n| n.ok())
            .filter(|n| n.starts_with("ppt/slides/slide") && n.ends_with(".xml"))
            .map(|n| n.into_owned())
            .collect();
        // Sort by the numeric suffix so slides come out in presentation order
        // (slide2.xml must sort before slide10.xml).
        slide_names.sort_by_key(|n| {
            n.trim_start_matches("ppt/slides/slide")
                .trim_end_matches(".xml")
                .parse::<u32>()
                .unwrap_or(u32::MAX)
        });
        let mut out = String::new();
        for name in slide_names {
            if let Some(xml) = read_entry(&mut archive, &name) {
                out.push_str(&extract_runs(&xml, "a:t", "a:p"));
                out.push('\n');
            }
        }
        return Ok(out.trim().to_string());
    }

    anyhow::bail!("unsupported office mimeType \"{mime_type}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_runs_pulls_text_and_paragraph_breaks() {
        let xml = r#"<w:p><w:r><w:t>Hello</w:t></w:r></w:p><w:p><w:r><w:t>World</w:t></w:r></w:p>"#;
        let out = extract_runs(xml, "w:t", "w:p");
        assert_eq!(out.trim(), "Hello\nWorld");
    }
}
