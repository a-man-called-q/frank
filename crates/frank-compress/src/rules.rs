//! The deterministic prose compressor.
//!
//! Ported from the historical Caveman `compress.js`, with
//! one structural change: protection is span-based, not sentinel-splicing.
//! The original replaces each protected match with a `\0<index>\0` marker
//! (confirmed via `od -c` — its own comment claiming a space delimiter is
//! stale), transforms the mutated string, then restores the markers over
//! up to `MAX_RESTORE_PASSES = 8` passes because protect-time matches can
//! nest (its own example: the path rule swallows `STARTER/BUSINESS`, then
//! the function-call rule swallows the resulting `type(\0N\0)`).
//!
//! Matching all 8 patterns against the *original*, never-mutated text and
//! merging overlapping byte ranges handles that same nesting case for
//! free: the function-call pattern's match on unmutated text already
//! spans the inner path match, so merging naturally produces one
//! encompassing protected region — no restore passes, no sentinel
//! collision possible by construction.
//!
//! Two adaptations forced by using the `regex` crate (deliberately
//! lookaround-free — see `AGENTS.md`): the ARTICLES rule's `(?=[a-z])`
//! lookahead becomes a match-then-inspect-next-char loop; and the
//! whitespace-collapse / sentence-recapitalization cleanup runs per
//! protected-span *gap* rather than once over the fully reassembled
//! string. That second point is a documented, narrow divergence: a
//! sentence that starts immediately after a protected span (e.g. `` `code`.
//! Next sentence`` where "Next" directly follows the span with no
//! whitespace before the period) won't be recapitalized, because the gap
//! before the span and the gap after it are cleaned up independently.
//! Whitespace-collapse itself is unaffected, since a run of newlines never
//! spans *across* a protected match either way.

use std::ops::Range;
use std::sync::LazyLock;

use regex::{Captures, Regex};

pub struct CompressResult {
    pub compressed: String,
    pub before: usize,
    pub after: usize,
}

static PROTECT_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"```[\s\S]*?```",
        r"`[^`\n]+`",
        r"(?i)\bhttps?://\S+",
        r"\b[\w.-]*[/\\][\w./\\-]+",
        r"\b[A-Z][A-Za-z0-9]*(?:_[A-Z][A-Za-z0-9]*)+\b",
        r"\b\w+\.\w+(?:\.\w+)*\(\)?",
        r"[A-Za-z_][A-Za-z0-9_]*\s*\([^)]*\)",
        r"\b\d+\.\d+\.\d+\b",
    ]
    .iter()
    .map(|p| Regex::new(p).expect("static pattern"))
    .collect()
});

fn protect_patterns() -> &'static [Regex] {
    &PROTECT_PATTERNS
}

/// Every byte range matched by any protect pattern, sorted and merged into
/// non-overlapping spans.
pub fn protected_spans(text: &str) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for re in protect_patterns() {
        for m in re.find_iter(text) {
            ranges.push(m.start()..m.end());
        }
    }
    ranges.sort_by_key(|r| r.start);

    let mut merged: Vec<Range<usize>> = Vec::new();
    for r in ranges {
        match merged.last_mut() {
            Some(last) if r.start <= last.end => {
                last.end = last.end.max(r.end);
            }
            _ => merged.push(r),
        }
    }
    merged
}

static LEADERS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?im)^(?:i'?ll|i will|i can|i'?d|you can|we will|we can|let me|let'?s)\s+")
        .expect("static pattern")
});

fn leaders() -> &'static Regex {
    &LEADERS
}

static PLEASANTRIES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:please|kindly|thank you|thanks|sure|certainly|of course|happy to|i'?d be happy)\b[,.]?\s*")
        .expect("static pattern")
});

fn pleasantries() -> &'static Regex {
    &PLEASANTRIES
}

static HEDGES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:perhaps|maybe|might|could potentially|would like to|i think|in my opinion|it seems|it appears)\b\s*")
        .expect("static pattern")
});

fn hedges() -> &'static Regex {
    &HEDGES
}

static FILLERS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:just|really|basically|actually|simply|quite|very|essentially|literally)\b",
    )
    .expect("static pattern")
});

fn fillers() -> &'static Regex {
    &FILLERS
}

static ARTICLES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(?:a|an|the)\s+").expect("static pattern"));

fn articles() -> &'static Regex {
    &ARTICLES
}

/// `regex` has no lookaround, so the original's `(?=[a-z])` becomes a
/// match-then-inspect loop: keep a match that isn't followed by a letter,
/// drop one that is. Confirmed against the real archive compressor (not
/// assumed): the original regex carries the `/i` flag, and in JS that
/// flag applies to the *entire* pattern including the lookahead, so
/// `(?=[a-z])` under `/gi` actually means "followed by any letter,
/// either case" — `"The API"` does **not** survive (verified:
/// `compress("The API returned an error")` → `"API returned error"`).
/// Articles only survive before a non-letter — a digit, punctuation, or
/// the end of the text/gap (`"a 5-minute task"`, `"an $100 fee"`).
fn strip_articles(s: &str) -> String {
    let re = articles();
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    for m in re.find_iter(s) {
        let next_is_letter = s[m.end()..]
            .chars()
            .next()
            .map(|c| c.is_alphabetic())
            .unwrap_or(false);
        if next_is_letter {
            out.push_str(&s[last..m.start()]);
            last = m.end();
        }
    }
    out.push_str(&s[last..]);
    out
}

fn collapse_spaces(s: &str) -> String {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"[ \t]{2,}").expect("static pattern"));
    RE.replace_all(s, " ").into_owned()
}
fn tighten_punctuation(s: &str) -> String {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\s+([,.;:!?])").expect("static pattern"));
    RE.replace_all(s, "$1").into_owned()
}
fn collapse_blank_runs(s: &str) -> String {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{3,}").expect("static pattern"));
    RE.replace_all(s, "\n\n").into_owned()
}
fn recapitalize(s: &str) -> String {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(^|[.!?]\s+)([a-z])").expect("static pattern"));
    RE.replace_all(s, |caps: &Captures| {
        format!("{}{}", &caps[1], caps[2].to_uppercase())
    })
    .into_owned()
}

/// Applied to the text *between* protected spans — never to a span's
/// contents, which are copied verbatim by `compress`. Deliberately does
/// *not* trim leading/trailing whitespace: it runs once per gap, and
/// trimming each gap independently would eat the space on either side of
/// every protected span, running words together (`` Hello there,`code`world. ``
/// instead of `` Hello there, `code` world. ``). The original only ever
/// trims once, over the fully assembled string — `compress` does the same
/// via `final_trim`.
pub fn compress_prose(text: &str) -> String {
    let mut s = leaders().replace_all(text, "").into_owned();
    s = pleasantries().replace_all(&s, "").into_owned();
    s = hedges().replace_all(&s, "").into_owned();
    s = fillers().replace_all(&s, "").into_owned();
    s = strip_articles(&s);
    s = collapse_spaces(&s);
    s = tighten_punctuation(&s);
    s = collapse_blank_runs(&s);
    recapitalize(&s)
}

pub fn compress(text: &str) -> CompressResult {
    if text.is_empty() {
        return CompressResult {
            compressed: String::new(),
            before: 0,
            after: 0,
        };
    }

    // Simple concatenation, matching the original: its prose regexes only
    // ever consume *trailing* whitespace after a matched word, never the
    // whitespace preceding it, so the separator before a protected span is
    // always whatever was already in the source — nothing to reinsert.
    // `compress` trims the final result. Trim leading whitespace before the
    // gap pass as well so recapitalization sees the actual first character;
    // otherwise `" a"` becomes `"a"` on the first pass and `"A"` on the
    // second, violating the deterministic/idempotent compressor contract.
    // Protected spans are still copied byte-for-byte from this trimmed source.
    let source = text.trim_start();
    let spans = protected_spans(source);
    let mut out = String::with_capacity(source.len());
    let mut last = 0;
    for span in &spans {
        out.push_str(&compress_prose(&source[last..span.start]));
        out.push_str(&source[span.start..span.end]);
        last = span.end;
    }
    out.push_str(&compress_prose(&source[last..]));
    let out = out.trim().to_string();

    CompressResult {
        before: text.len(),
        after: out.len(),
        compressed: out,
    }
}
