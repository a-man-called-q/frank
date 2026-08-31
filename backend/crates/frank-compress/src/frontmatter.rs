//! YAML frontmatter split-off and re-prepend. Ported from `compress.py`'s
//! `split_frontmatter` — memory files often start with a `---`-delimited
//! frontmatter block; compression (LLM or deterministic) has no business
//! touching it, so it's surgically removed before compression and
//! prepended back verbatim.

use regex::Regex;

fn frontmatter_regex() -> Regex {
    Regex::new(r"(?s)\A(---\r?\n.*?\r?\n---\r?\n)(.*)").expect("static pattern")
}

/// Returns `(frontmatter, body)`. `frontmatter` is empty when the text
/// doesn't start with a `---` block — the whole input is then `body`.
pub fn split_frontmatter(text: &str) -> (&str, &str) {
    match frontmatter_regex().captures(text) {
        Some(caps) => {
            let fm = caps.get(1).unwrap();
            (fm.as_str(), &text[fm.end()..])
        }
        None => ("", text),
    }
}
