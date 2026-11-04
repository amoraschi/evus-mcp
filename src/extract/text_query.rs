//! grep-style query filtering over extracted text, plus the MAX_TEXT_CHARS
//! guard that forces callers to pass `query` instead of dumping huge text.
//! Direct port of server.mjs's grepText/textOrRequireQuery.

pub const MAX_TEXT_CHARS: usize = 8000;

/// Legacy .doc/.ppt extraction can return 100k+ chars of binary noise
/// alongside the real text, too big for a client's context window. When
/// `query` is given, return only matching lines (+ context) instead of the
/// full dump.
pub fn grep_text(text: &str, query: Option<&str>, context_lines: usize) -> String {
    let Some(query) = query else {
        return text.to_string();
    };
    let lines: Vec<&str> = text.split('\n').collect();
    let q = query.to_lowercase();
    let mut keep = std::collections::BTreeSet::new();
    for (i, line) in lines.iter().enumerate() {
        if line.to_lowercase().contains(&q) {
            let lo = i.saturating_sub(context_lines);
            let hi = (i + context_lines).min(lines.len().saturating_sub(1));
            for j in lo..=hi {
                keep.insert(j);
            }
        }
    }
    if keep.is_empty() {
        return format!("No lines matching \"{query}\" found.");
    }
    let sorted: Vec<usize> = keep.into_iter().collect();
    let mut out = Vec::new();
    for (k, &idx) in sorted.iter().enumerate() {
        if k > 0 && idx != sorted[k - 1] + 1 {
            out.push("--".to_string());
        }
        out.push(format!("{idx}: {}", lines[idx]));
    }
    out.join("\n")
}

/// Don't hand back a wall of text the caller's context can't hold, force a
/// query instead of silently dumping something that blows the client's limit.
pub fn text_or_require_query(text: &str, query: Option<&str>) -> anyhow::Result<String> {
    if query.is_none() && text.chars().count() > MAX_TEXT_CHARS {
        anyhow::bail!(
            "Extracted text is {} chars, too large to return directly. \
             Call again with `query` set to the keyword you're looking for.",
            text.chars().count()
        );
    }
    Ok(grep_text(text, query, 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_query_returns_full_text() {
        assert_eq!(grep_text("a\nb\nc", None, 2), "a\nb\nc");
    }

    #[test]
    fn query_with_context_and_gap_marker() {
        let text = "one\ntwo\nthree\nfour\nfive\nSIX\nseven";
        let out = grep_text(text, Some("six"), 1);
        assert_eq!(out, "4: five\n5: SIX\n6: seven");
    }

    #[test]
    fn two_distant_matches_get_gap_marker() {
        let text = "aaa\nbbb\nccc\nddd\neee\nfff\nggg\nHIT\niii";
        let out = grep_text(text, Some("hit"), 0);
        // only line 7 ("HIT") matches with zero context, single line, no gap marker possible here
        assert_eq!(out, "7: HIT");
        let text2 = "HIT\nbbb\nccc\nddd\neee\nHIT\nggg";
        let out2 = grep_text(text2, Some("hit"), 0);
        assert_eq!(out2, "0: HIT\n--\n5: HIT");
    }

    #[test]
    fn no_match_message() {
        assert_eq!(grep_text("a\nb", Some("zzz"), 2), "No lines matching \"zzz\" found.");
    }

    #[test]
    fn large_text_without_query_errors() {
        let text = "x".repeat(MAX_TEXT_CHARS + 1);
        assert!(text_or_require_query(&text, None).is_err());
        assert!(text_or_require_query(&text, Some("x")).is_ok());
    }
}
