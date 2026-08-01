//! Checks that a compression didn't lose (or corrupt) anything it
//! shouldn't have. Ported verbatim from
//! `archive/skills/caveman-compress/scripts/validate.py` — six checks,
//! kept exactly as designed: code-block list equality (order-sensitive,
//! byte-exact) and URL set equality and inline-code **multiset** equality
//! are errors; heading *count* is an error but heading *text/order* is
//! only a warning; path and bullet-count drift are warnings. The multiset
//! choice for inline code is the subtle one worth preserving — plain set
//! equality would miss a *dropped duplicate* (three occurrences of `` `x`
//! `` becoming two still reads as "the set of inline codes is unchanged").
//!
//! With a deterministic compressor (unlike the archive's LLM-backed
//! `compress.py`), this stops being a runtime safety net and becomes a CI
//! regression assertion run over a fixed corpus — see the differential
//! test in `rules_tests.rs`.

use std::collections::{HashMap, HashSet};

use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    pub findings: Vec<Finding>,
}

impl ValidationResult {
    pub fn is_valid(&self) -> bool {
        !self.findings.iter().any(|f| f.severity == Severity::Error)
    }
    pub fn errors(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(|f| f.severity == Severity::Error)
    }
    pub fn warnings(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(|f| f.severity == Severity::Warning)
    }
    fn error(&mut self, message: impl Into<String>) {
        self.findings.push(Finding { severity: Severity::Error, message: message.into() });
    }
    fn warning(&mut self, message: impl Into<String>) {
        self.findings.push(Finding { severity: Severity::Warning, message: message.into() });
    }
}

fn url_regex() -> Regex {
    Regex::new(r"https?://[^\s)]+").unwrap()
}
fn heading_regex() -> Regex {
    Regex::new(r"(?m)^(#{1,6})\s+(.*)").unwrap()
}
fn bullet_regex() -> Regex {
    Regex::new(r"(?m)^\s*[-*+]\s+").unwrap()
}
fn path_regex() -> Regex {
    Regex::new(r"(?:\./|\.\./|/|[A-Za-z]:\\)[\w\-/\\.]+|[\w\-.]+[/\\][\w\-/\\.]+").unwrap()
}
fn fence_open_regex() -> Regex {
    Regex::new(r"^(\s{0,3})(`{3,}|~{3,})(.*)$").unwrap()
}

fn extract_headings(text: &str) -> Vec<(usize, String)> {
    heading_regex()
        .captures_iter(text)
        .map(|c| (c[1].len(), c[2].trim().to_string()))
        .collect()
}

/// Line-based fenced code block extractor. Handles ``` and ~~~ fences with
/// variable length (CommonMark: closing fence must use the same character
/// and be at least as long as the opening one); nested fences (e.g. a
/// 4-backtick block wrapping inner 3-backtick content) work because the
/// closer must be at least `fence_len` long. Unclosed fences are silently
/// dropped, matching the original — including them would flag malformed
/// markdown as a compression regression, which it isn't.
pub fn extract_code_blocks(text: &str) -> Vec<String> {
    let open_re = fence_open_regex();
    let lines: Vec<&str> = text.split('\n').collect();
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some(open_caps) = open_re.captures(lines[i]) else {
            i += 1;
            continue;
        };
        let fence_str = &open_caps[2];
        let fence_char = fence_str.chars().next().unwrap();
        let fence_len = fence_str.len();
        let mut block_lines = vec![lines[i]];
        i += 1;
        let mut closed = false;
        while i < lines.len() {
            if let Some(close_caps) = open_re.captures(lines[i]) {
                let close_fence = &close_caps[2];
                if close_fence.starts_with(fence_char)
                    && close_fence.len() >= fence_len
                    && close_caps[3].trim().is_empty()
                {
                    block_lines.push(lines[i]);
                    closed = true;
                    i += 1;
                    break;
                }
            }
            block_lines.push(lines[i]);
            i += 1;
        }
        if closed {
            blocks.push(block_lines.join("\n"));
        }
    }
    blocks
}

fn extract_urls(text: &str) -> HashSet<String> {
    url_regex().find_iter(text).map(|m| m.as_str().to_string()).collect()
}

fn extract_paths(text: &str) -> HashSet<String> {
    path_regex().find_iter(text).map(|m| m.as_str().to_string()).collect()
}

fn count_bullets(text: &str) -> usize {
    bullet_regex().find_iter(text).count()
}

/// A cruder, separate fence-stripper than `extract_code_blocks` — ported
/// as-is: the original uses two different fence-handling strategies in
/// different functions (this one requires the closing fence at column 0
/// with nothing else on the line), and unifying them would change which
/// inline-code occurrences this specific check catches on edge-case input
/// (indented fences, fences with trailing content).
fn extract_inline_codes(text: &str) -> Vec<String> {
    let strip_backtick = Regex::new(r"(?m)^```[\s\S]*?^```").unwrap();
    let strip_tilde = Regex::new(r"(?m)^~~~[\s\S]*?^~~~").unwrap();
    let without_backtick_fences = strip_backtick.replace_all(text, "");
    let no_fences = strip_tilde.replace_all(&without_backtick_fences, "");
    let inline = Regex::new(r"`([^`]+)`").unwrap();
    inline.captures_iter(&no_fences).map(|c| c[1].to_string()).collect()
}

fn counter(items: &[String]) -> HashMap<String, usize> {
    let mut m = HashMap::new();
    for i in items {
        *m.entry(i.clone()).or_insert(0) += 1;
    }
    m
}

pub fn validate(orig: &str, comp: &str) -> ValidationResult {
    let mut result = ValidationResult::default();

    let h1 = extract_headings(orig);
    let h2 = extract_headings(comp);
    if h1.len() != h2.len() {
        result.error(format!("Heading count mismatch: {} vs {}", h1.len(), h2.len()));
    }
    if h1 != h2 {
        result.warning("Heading text/order changed");
    }

    let c1 = extract_code_blocks(orig);
    let c2 = extract_code_blocks(comp);
    if c1 != c2 {
        result.error("Code blocks not preserved exactly");
    }

    let u1 = extract_urls(orig);
    let u2 = extract_urls(comp);
    if u1 != u2 {
        let lost: Vec<_> = u1.difference(&u2).cloned().collect();
        let added: Vec<_> = u2.difference(&u1).cloned().collect();
        result.error(format!("URL mismatch: lost={lost:?}, added={added:?}"));
    }

    let p1 = extract_paths(orig);
    let p2 = extract_paths(comp);
    if p1 != p2 {
        let lost: Vec<_> = p1.difference(&p2).cloned().collect();
        let added: Vec<_> = p2.difference(&p1).cloned().collect();
        result.warning(format!("Path mismatch: lost={lost:?}, added={added:?}"));
    }

    let b1 = count_bullets(orig);
    let b2 = count_bullets(comp);
    if b1 > 0 {
        let diff = (b1 as f64 - b2 as f64).abs() / b1 as f64;
        if diff > 0.15 {
            result.warning(format!("Bullet count changed too much: {b1} -> {b2}"));
        }
    }

    let ic1 = counter(&extract_inline_codes(orig));
    let ic2 = counter(&extract_inline_codes(comp));
    if ic1 != ic2 {
        let mut lost = Vec::new();
        for (code, count) in &ic1 {
            match ic2.get(code) {
                None => lost.push(code.clone()),
                Some(c2) if c2 < count => {
                    lost.push(format!("{code} (lost {} of {count} occurrences)", count - c2))
                }
                _ => {}
            }
        }
        let added: Vec<_> = ic2.keys().filter(|c| !ic1.contains_key(*c)).cloned().collect();
        if !lost.is_empty() {
            result.error(format!("Inline code lost: {lost:?}"));
        }
        if !added.is_empty() {
            result.warning(format!("Inline code added: {added:?}"));
        }
    }

    result
}
