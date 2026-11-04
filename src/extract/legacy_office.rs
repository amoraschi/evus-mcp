//! ponytail: legacy OLE binary .doc/.ppt have no pure-Rust parser worth
//! adding as a dependency either. Heuristic: text runs are stored either as
//! UTF-16LE (TextCharsAtom) or single-byte Latin-1 (TextBytesAtom), depending
//! on whether the run needed double-byte chars, so scan both step sizes and
//! merge. Garbled on complex docs (tables, embedded objects) but recovers
//! plain text/schedules fine. Upgrade path: shell out to a LibreOffice
//! headless conversion if that ever matters. Direct port of server.mjs's
//! scanTextRuns/isTextByte/extractLegacyOfficeText.

fn is_text_byte(code: u16) -> bool {
    (0x20..0x7f).contains(&code) || (0xa0..=0xff).contains(&code) || code == 0x0a || code == 0x0d
}

fn scan_text_runs(buf: &[u8], step: usize) -> Vec<String> {
    let mut runs = Vec::new();
    let mut i = 0usize;
    while i + step <= buf.len() {
        let mut j = i;
        let mut chars = String::new();
        while j + step <= buf.len() {
            let code: u16 = if step == 2 {
                u16::from_le_bytes([buf[j], buf[j + 1]])
            } else {
                buf[j] as u16
            };
            if is_text_byte(code) {
                chars.push(char::from_u32(code as u32).unwrap_or('\u{FFFD}'));
                j += step;
            } else {
                break;
            }
        }
        if chars.chars().count() >= 4 {
            runs.push(chars);
            i = j;
        } else {
            i += step;
        }
    }
    runs
}

pub fn extract_legacy_office_text(buf: &[u8]) -> String {
    let mut runs = scan_text_runs(buf, 2);
    runs.extend(scan_text_runs(buf, 1));
    runs.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_utf16le_run() {
        let mut buf = vec![0u8, 0u8]; // leading junk (even length, keeps the step-2 scan aligned)
        for ch in "hello".encode_utf16() {
            buf.extend_from_slice(&ch.to_le_bytes());
        }
        buf.extend_from_slice(&[0xff, 0xff, 0xff]); // non-text separator
        let text = extract_legacy_office_text(&buf);
        assert!(text.contains("hello"), "expected 'hello' in {text:?}");
    }

    #[test]
    fn extracts_latin1_run_when_no_utf16_run_present() {
        // Bytes that don't form a valid 4+ char UTF-16LE text run, but do as
        // single-byte Latin-1.
        let buf = b"\x01notes\x02".to_vec();
        let text = extract_legacy_office_text(&buf);
        assert!(text.contains("notes"), "expected 'notes' in {text:?}");
    }

    #[test]
    fn short_runs_under_four_chars_are_dropped() {
        let buf = b"ab".to_vec();
        assert_eq!(extract_legacy_office_text(&buf), "");
    }
}
