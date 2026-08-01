//! Claude Code session JSONL scan.
//!
//! Ported from `archive/src/hooks/caveman-stats.js`'s `parseSession` /
//! `findRecentSession`, with the gap this project exists to close: the
//! archive's `parseSession` reads only `output_tokens` and
//! `cache_read_input_tokens` from `message.usage` — never
//! `input_tokens` or `cache_creation_input_tokens`, despite both sitting
//! in the same object it already parses. That's the data needed to check
//! the archive's own headline caveat (`docs/HONEST-NUMBERS.md`: "the skill
//! itself adds ~1-1.5k input tokens per turn"), and the project never read
//! it. See `crate::verdict` for where these four numbers actually get used.

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Default)]
pub struct SessionTurn {
    /// Milliseconds since epoch, if the entry had a parseable timestamp.
    pub ts: Option<i64>,
    pub output_tokens: u64,
    pub input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    /// Subagent traffic — kept separate from the user-facing conversation.
    /// The archive never distinguished these (`caveman-stats.js` has no
    /// `isSidechain` handling at all), so subagent tokens were silently
    /// folded into the same totals as the main conversation.
    pub is_sidechain: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SessionScan {
    pub turns: Vec<SessionTurn>,
    pub model: Option<String>,
}

impl SessionScan {
    pub fn turn_count(&self) -> usize {
        self.turns.iter().filter(|t| !t.is_sidechain).count()
    }
}

#[derive(Debug, Deserialize)]
struct RawEntry {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default, rename = "isSidechain")]
    is_sidechain: bool,
    #[serde(default)]
    message: Option<RawMessage>,
}

#[derive(Debug, Deserialize)]
struct RawMessage {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<RawUsage>,
}

#[derive(Debug, Deserialize)]
struct RawUsage {
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
}

/// Parse an ISO-8601 timestamp (`"2026-08-01T03:13:39.489Z"`) into
/// milliseconds since epoch, by hand — pulling in a full date/time crate
/// for one fixed-shape field the source always writes the same way isn't
/// worth the dependency weight on a path that only runs inside `frank
/// stats`, never on a hook's hot path.
fn parse_iso8601_ms(s: &str) -> Option<i64> {
    let s = s.strip_suffix('Z')?;
    let (date, time) = s.split_once('T')?;
    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;

    let (time, millis) = match time.split_once('.') {
        Some((t, ms)) => (t, ms.parse::<i64>().ok()?),
        None => (time, 0),
    };
    let mut time_parts = time.split(':');
    let hour: i64 = time_parts.next()?.parse().ok()?;
    let min: i64 = time_parts.next()?.parse().ok()?;
    let sec: i64 = time_parts.next()?.parse().ok()?;

    // Days since epoch via a standard civil-from-days inverse (Howard
    // Hinnant's algorithm) — no calendar crate needed for a UTC-only,
    // Gregorian-only, always-well-formed input.
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days_since_epoch = era * 146097 + doe - 719468;

    let total_seconds = days_since_epoch * 86400 + hour * 3600 + min * 60 + sec;
    Some(total_seconds * 1000 + millis)
}

pub fn parse_session(path: &Path) -> SessionScan {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return SessionScan::default();
    };

    let mut turns = Vec::new();
    let mut model = None;

    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<RawEntry>(line) else {
            continue;
        };
        if entry.kind != "assistant" {
            continue;
        }
        let Some(message) = entry.message else { continue };
        let Some(usage) = message.usage else { continue };

        if model.is_none() {
            model = message.model;
        }
        let ts = entry.timestamp.as_deref().and_then(parse_iso8601_ms);

        turns.push(SessionTurn {
            ts,
            output_tokens: usage.output_tokens,
            input_tokens: usage.input_tokens,
            cache_creation_input_tokens: usage.cache_creation_input_tokens,
            cache_read_input_tokens: usage.cache_read_input_tokens,
            is_sidechain: entry.is_sidechain,
        });
    }

    SessionScan { turns, model }
}

/// Recursive-descent search under `<config_dir>/projects/` for the
/// most-recently-modified `.jsonl` file. Iterative (explicit stack), not
/// truly recursive, so a deep or cyclic tree can't blow the stack.
pub fn find_recent_session(config_dir: &Path) -> Option<PathBuf> {
    let projects_dir = config_dir.join("projects");
    let mut stack: Vec<PathBuf> = std::fs::read_dir(&projects_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();

    let mut best: Option<(PathBuf, std::time::SystemTime)> = None;
    while let Some(p) = stack.pop() {
        let Ok(meta) = std::fs::metadata(&p) else { continue };
        if meta.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&p) {
                stack.extend(rd.filter_map(|e| e.ok()).map(|e| e.path()));
            }
        } else if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
            if best.as_ref().map(|(_, best_mtime)| mtime > *best_mtime).unwrap_or(true) {
                best = Some((p, mtime));
            }
        }
    }
    best.map(|(p, _)| p)
}
