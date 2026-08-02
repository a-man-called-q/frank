//! Marker-fenced markdown block injection — append/strip a Frank-owned
//! block inside a larger file the user also edits by hand (`AGENTS.md`,
//! `SOUL.md`, ...). Ported from `archive/bin/lib/openclaw.js`, written
//! there after a real data-loss bug (#596): pairing the *first* begin
//! marker with the *first* end marker anywhere in the file meant a stray
//! or truncated marker made every append add a second block, and a strip
//! then spanned all user content between the two.
//!
//! The fix, ported verbatim: each begin marker pairs with the nearest end
//! marker that occurs *before the next begin marker*. An unpaired begin
//! (no such end before the next begin, or before EOF) is treated as an
//! orphan and only the marker text itself is removed — never the content
//! after it, which is presumed to be the user's.

pub struct Block {
    pub begin: String,
    pub end: String,
}

/// Remove every well-formed `begin..end` block (inclusive) and any orphan
/// markers, returning the cleaned text.
pub fn strip_all(text: &str, block: &Block) -> String {
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    loop {
        let Some(rel_b) = text[i..].find(&block.begin) else {
            out.push_str(&text[i..]);
            break;
        };
        let b = i + rel_b;
        out.push_str(&text[i..b]);

        let after_begin = b + block.begin.len();
        let next_b = text[after_begin..]
            .find(&block.begin)
            .map(|p| after_begin + p);
        let e = text[after_begin..]
            .find(&block.end)
            .map(|p| after_begin + p);

        match e {
            Some(e) if next_b.is_none_or(|nb| e < nb) => {
                // Well-formed block: drop begin..end inclusive.
                i = e + block.end.len();
            }
            _ => {
                // Orphan begin: drop only the marker itself.
                i = after_begin;
            }
        }
    }

    // A trailing loop removes any remaining orphan `end` markers
    // (marker-only, matching the archive).
    out.replace(&block.end, "")
}

/// Result of appending: distinguishes "already present, no-op" from
/// "repaired a malformed marker state" from a plain fresh append, so
/// callers can report accurately.
pub enum AppendOutcome {
    AlreadyPresent,
    Appended(String),
    Repaired(String),
}

pub fn append(text: &str, block: &Block, body: &str) -> AppendOutcome {
    let begin_count = text.matches(&block.begin).count();
    let end_count = text.matches(&block.end).count();

    let one_clean_pair = begin_count == 1
        && end_count == 1
        && text.find(&block.begin).unwrap() < text.find(&block.end).unwrap();

    if one_clean_pair {
        return AppendOutcome::AlreadyPresent;
    }

    let stripped = if begin_count == 0 && end_count == 0 {
        text.to_string()
    } else {
        strip_all(text, block)
    };

    let trimmed = stripped.trim_end();
    let separator = if trimmed.is_empty() { "" } else { "\n\n" };
    let new_text = format!(
        "{trimmed}{separator}{}\n{}\n{}\n",
        block.begin,
        body.trim(),
        block.end
    );

    if begin_count == 0 && end_count == 0 {
        AppendOutcome::Appended(new_text)
    } else {
        AppendOutcome::Repaired(new_text)
    }
}

/// Strip the block; if the result is empty (whitespace-only), returns
/// `None` so the caller can delete the file entirely rather than leave a
/// blank file the host re-reads every turn for nothing.
pub fn remove(text: &str, block: &Block) -> Option<String> {
    let stripped = strip_all(text, block).trim_end().to_string();
    if stripped.is_empty() {
        None
    } else {
        Some(format!("{stripped}\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block() -> Block {
        Block {
            begin: "<!-- frank:begin -->".to_string(),
            end: "<!-- frank:end -->".to_string(),
        }
    }

    #[test]
    fn append_to_empty_file() {
        let out = append("", &block(), "hello");
        let AppendOutcome::Appended(text) = out else {
            panic!()
        };
        assert!(text.contains("<!-- frank:begin -->\nhello\n<!-- frank:end -->\n"));
    }

    #[test]
    fn append_classifies_clean_and_repaired_marker_states() {
        assert!(matches!(
            append("plain user text", &block(), "hello"),
            AppendOutcome::Appended(_)
        ));
        let AppendOutcome::Repaired(text) =
            append("plain user text\n<!-- frank:end -->", &block(), "hello")
        else {
            panic!()
        };
        assert_eq!(text.matches("frank:end").count(), 1);

        let AppendOutcome::Repaired(text) =
            append("<!-- frank:begin -->\nuser text", &block(), "hello")
        else {
            panic!()
        };
        assert_eq!(text.matches("frank:begin").count(), 1);
        assert!(text.contains("user text"));
    }

    #[test]
    fn append_preserves_existing_user_content() {
        let existing = "# My AGENTS.md\n\nSome rules I wrote.\n";
        let out = append(existing, &block(), "frank rules");
        let AppendOutcome::Appended(text) = out else {
            panic!()
        };
        assert!(text.starts_with("# My AGENTS.md\n\nSome rules I wrote."));
        assert!(text.contains("frank rules"));
    }

    #[test]
    fn append_reports_repair_for_orphan_markers() {
        let existing = "user text\n<!-- frank:begin -->\nuser content\n";
        let out = append(existing, &block(), "frank rules");
        let AppendOutcome::Repaired(text) = out else {
            panic!("orphan markers must be repaired, not reported as a fresh append");
        };
        assert!(text.contains("user content"));
        assert!(text.contains("frank rules"));
    }

    #[test]
    fn well_formed_block_is_a_noop_on_reappend() {
        let existing = "prefix\n\n<!-- frank:begin -->\nbody\n<!-- frank:end -->\n";
        let out = append(existing, &block(), "body");
        assert!(matches!(out, AppendOutcome::AlreadyPresent));
    }

    #[test]
    fn strip_removes_block_and_keeps_surrounding_content() {
        let text = "before\n\n<!-- frank:begin -->\nbody\n<!-- frank:end -->\n\nafter\n";
        let stripped = strip_all(text, &block());
        assert!(stripped.contains("before"));
        assert!(stripped.contains("after"));
        assert!(!stripped.contains("body"));
        assert!(!stripped.contains("frank:begin"));
    }

    #[test]
    fn remove_deletes_content_when_nothing_else_remains() {
        let text = "<!-- frank:begin -->\nbody\n<!-- frank:end -->\n";
        assert_eq!(remove(text, &block()), None);
    }

    #[test]
    fn remove_keeps_surrounding_user_content() {
        let text = "user text\n\n<!-- frank:begin -->\nbody\n<!-- frank:end -->\n";
        let result = remove(text, &block()).unwrap();
        assert!(result.contains("user text"));
        assert!(!result.contains("frank:begin"));
    }

    /// #596: a truncated/stray begin marker with no matching end before
    /// EOF must not eat everything after it — only the marker itself.
    #[test]
    fn orphan_begin_removes_only_the_marker_not_trailing_content() {
        let text = "keep this\n<!-- frank:begin -->\nthis is the user's own content, not ours\n";
        let stripped = strip_all(text, &block());
        assert!(stripped.contains("keep this"));
        assert!(stripped.contains("this is the user's own content"));
        assert!(!stripped.contains("frank:begin"));
    }

    #[test]
    fn each_begin_pairs_with_nearest_end_before_next_begin() {
        let text =
            "<!-- frank:begin -->\nfirst\n<!-- frank:begin -->\nsecond\n<!-- frank:end -->\ntail";
        // First begin has no end before the SECOND begin -> orphan, only marker removed.
        // Second begin pairs with the end that follows it.
        let stripped = strip_all(text, &block());
        assert!(stripped.contains("first"), "{stripped}");
        assert!(!stripped.contains("second"), "{stripped}");
        assert!(stripped.contains("tail"), "{stripped}");
    }

    #[test]
    fn multiple_well_formed_blocks_all_removed() {
        let text = "a\n<!-- frank:begin -->\nx\n<!-- frank:end -->\nb\n<!-- frank:begin -->\ny\n<!-- frank:end -->\nc";
        let stripped = strip_all(text, &block());
        assert!(stripped.contains('a') && stripped.contains('b') && stripped.contains('c'));
        assert!(!stripped.contains('x') && !stripped.contains('y'));
    }

    #[test]
    fn orphan_end_marker_alone_is_removed() {
        let text = "hello\n<!-- frank:end -->\nworld";
        let stripped = strip_all(text, &block());
        assert!(stripped.contains("hello") && stripped.contains("world"));
        assert!(!stripped.contains("frank:end"));
    }

    #[test]
    fn equal_marker_positions_are_not_a_clean_pair() {
        let same = Block {
            begin: "X".into(),
            end: "X".into(),
        };
        assert!(!matches!(
            append("X", &same, "body"),
            AppendOutcome::AlreadyPresent
        ));

        let stripped = strip_all("X user text X", &same);
        assert!(stripped.contains("user text"), "{stripped:?}");
    }

    // ---------- property: strip(append(text)) round-trips to text (mod whitespace) ----------
    proptest::proptest! {
        #[test]
        fn strip_after_append_recovers_original_modulo_whitespace(
            prefix in "[a-zA-Z0-9 \n]{0,60}",
            body in "[a-zA-Z0-9 ]{1,30}",
        ) {
            let out = append(&prefix, &block(), &body);
            let appended = match out {
                AppendOutcome::Appended(t) | AppendOutcome::Repaired(t) => t,
                AppendOutcome::AlreadyPresent => prefix.clone(),
            };
            let stripped = strip_all(&appended, &block());
            proptest::prop_assert_eq!(stripped.trim(), prefix.trim());
        }
    }
}
