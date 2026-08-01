//! Per-mode token attribution — never credit a whole session's tokens to
//! whatever mode the flag happens to hold at report time, since a
//! mid-session mode change would otherwise inflate the estimate (verbose
//! tokens counted as compressed) or zero it out (compressed tokens counted
//! as verbose).
//!
//! Ported from `archive/src/hooks/caveman-stats.js`'s `attributeByMode`
//! (#601) — the three-basis model (`log` / `flag-mtime` / `whole-session`)
//! and the "unattributed tokens are excluded, never guessed" rule are
//! exactly right in the original; this is a structural port, not a
//! rewrite. Extended to track all four usage fields per bucket (the
//! archive's version only ever accumulated `output_tokens`), and to split
//! out subagent (`isSidechain`) traffic into its own bucket instead of
//! silently folding it into the main conversation's totals.

use std::collections::BTreeMap;

use crate::mode_log::ModeLogRow;
use crate::session::SessionTurn;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenBucket {
    pub output_tokens: u64,
    pub input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}

impl TokenBucket {
    fn add(&mut self, t: &SessionTurn) {
        self.output_tokens = self.output_tokens.saturating_add(t.output_tokens);
        self.input_tokens = self.input_tokens.saturating_add(t.input_tokens);
        self.cache_creation_input_tokens = self
            .cache_creation_input_tokens
            .saturating_add(t.cache_creation_input_tokens);
        self.cache_read_input_tokens = self
            .cache_read_input_tokens
            .saturating_add(t.cache_read_input_tokens);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributionBasis {
    /// Every message could be matched against a logged transition.
    Log,
    /// No transition log, but the flag's mtime falls inside the session —
    /// only the span from that write onward is attributable.
    FlagMtime,
    /// No log, no mid-session evidence: the whole session is attributed to
    /// whatever mode is currently active.
    WholeSession,
}

pub struct Attribution {
    /// Key is a mode id, or `"none"` for spans where Frank was off.
    pub by_mode: BTreeMap<String, TokenBucket>,
    /// Spans that could not be attributed to any mode — excluded from any
    /// savings estimate, never guessed into a bucket.
    pub unknown: TokenBucket,
    /// Subagent turns, tracked but not attributed to a mode.
    pub sidechain: TokenBucket,
    pub basis: AttributionBasis,
}

struct Event {
    ts: i64,
    mode: Option<String>,
}

pub fn attribute_by_mode(
    turns: &[SessionTurn],
    mode_log: &[ModeLogRow],
    current_mode: Option<&str>,
    flag_mtime_ms: Option<i64>,
) -> Attribution {
    let mut sidechain = TokenBucket::default();
    let main_turns: Vec<&SessionTurn> = turns
        .iter()
        .filter(|t| {
            if t.is_sidechain {
                sidechain.add(t);
                false
            } else {
                true
            }
        })
        .collect();

    let first_ts = main_turns.iter().filter_map(|t| t.ts).min();
    let current_key = || current_mode.unwrap_or("none").to_string();

    let use_flag_mtime = mode_log.is_empty()
        && flag_mtime_ms
            .zip(first_ts)
            .map(|(mtime, fts)| mtime > fts)
            .unwrap_or(false);

    if mode_log.is_empty() && !use_flag_mtime {
        let mut bucket = TokenBucket::default();
        for t in &main_turns {
            bucket.add(t);
        }
        let mut by_mode = BTreeMap::new();
        by_mode.insert(current_key(), bucket);
        return Attribution {
            by_mode,
            unknown: TokenBucket::default(),
            sidechain,
            basis: AttributionBasis::WholeSession,
        };
    }

    // `read_mode_log` already sorts rows, but this function is public and is
    // also used directly by property/state-machine tests. Normalize here as
    // well so a caller supplying duplicated or out-of-order events cannot
    // make attribution depend on JSONL append order. `sort_by_key` is stable,
    // which gives equal-timestamp rows deterministic last-write-wins behavior
    // below without discarding evidence.
    let mut ordered_log = mode_log.to_vec();
    ordered_log.sort_by_key(|row| row.ts);

    let (events, basis, prefix_mode): (Vec<Event>, AttributionBasis, Option<Option<String>>) =
        if use_flag_mtime {
            (
                vec![Event {
                    ts: flag_mtime_ms.unwrap(),
                    mode: current_mode.map(String::from),
                }],
                AttributionBasis::FlagMtime,
                None,
            )
        } else {
            (
                ordered_log
                    .iter()
                    .map(|r| Event {
                        ts: r.ts,
                        mode: r.mode.clone(),
                    })
                    .collect(),
                AttributionBasis::Log,
                Some(ordered_log[0].prev.clone()),
            )
        };

    let mut by_mode: BTreeMap<String, TokenBucket> = BTreeMap::new();
    let mut unknown = TokenBucket::default();

    for t in &main_turns {
        let Some(ts) = t.ts else {
            unknown.add(t);
            continue;
        };
        let mut active: Option<&Event> = None;
        for ev in &events {
            if ev.ts <= ts {
                active = Some(ev);
            } else {
                break;
            }
        }
        match active {
            Some(ev) => {
                let key = ev.mode.clone().unwrap_or_else(|| "none".to_string());
                by_mode.entry(key).or_default().add(t);
            }
            None => match &prefix_mode {
                Some(pm) => {
                    let key = pm.clone().unwrap_or_else(|| "none".to_string());
                    by_mode.entry(key).or_default().add(t);
                }
                None => unknown.add(t),
            },
        }
    }

    Attribution {
        by_mode,
        unknown,
        sidechain,
        basis,
    }
}
