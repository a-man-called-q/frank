//! Builds and renders the `frank stats` report.
//!
//! The rule this module exists to enforce, stated in `AGENTS.md`: **never
//! sum a measured token count and an estimated one into one unlabeled
//! number.** Every quantity below carries its own epistemic status —
//! measured (from the session JSONL, or from Frank's own injection
//! ledger), or estimated (from the pack's benchmark table, always shown as
//! a range, never a single point value dressed up as precision the data
//! doesn't have).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use frank_pack::CompiledPack;
use serde::Serialize;

use crate::attribution::{Attribution, AttributionBasis, TokenBucket, attribute_by_mode};
use crate::injection_ledger;
use crate::mode_log::read_mode_log;
use crate::pricing::{format_usd, price_for_model};
use crate::session::{SessionScan, parse_session};

pub struct SessionReport {
    pub session_path: Option<PathBuf>,
    pub session_id: Option<String>,
    pub turns: usize,
    pub model: Option<String>,
    pub attribution: Attribution,
    pub injection_activate_bytes: usize,
    pub injection_reinforce_bytes: usize,
}

/// Low/mean/high estimate of output tokens saved for one mode, derived
/// from the pack's `[benchmark.reduction]` table. `model_matches` is
/// `false` when the session's model doesn't share the benchmark's prefix —
/// rendered as a downgraded label ("measured on a different model") rather
/// than silently reused.
pub struct SavingsEstimate {
    pub low_tokens: u64,
    pub mean_tokens: u64,
    pub high_tokens: u64,
    pub n: Option<u32>,
    pub benchmark_model: Option<String>,
    pub model_matches: bool,
}

fn estimate_component(tokens: u64, ratio: f64) -> u64 {
    if !(0.0..1.0).contains(&ratio) || tokens == 0 {
        return 0;
    }
    let est_without = (tokens as f64 / (1.0 - ratio)).round();
    (est_without - tokens as f64).max(0.0) as u64
}

pub fn savings_estimate(
    tokens: u64,
    stat: &frank_pack::ReductionStat,
    session_model: Option<&str>,
) -> SavingsEstimate {
    let model_matches = match (session_model, &stat.model) {
        (Some(sm), Some(bm)) => sm == bm || sm.starts_with(bm.as_str()) || bm.starts_with(sm),
        _ => true, // nothing to compare against; don't manufacture a mismatch
    };
    SavingsEstimate {
        low_tokens: estimate_component(tokens, stat.p25.unwrap_or(stat.mean)),
        mean_tokens: estimate_component(tokens, stat.mean),
        high_tokens: estimate_component(tokens, stat.p75.unwrap_or(stat.mean)),
        n: stat.n,
        benchmark_model: stat.model.clone(),
        model_matches,
    }
}

pub fn build_session_report(
    session_path: &Path,
    mode_log_path: &Path,
    injection_ledger_path: &Path,
    pack: &CompiledPack,
    current_mode: Option<&str>,
    flag_mtime_ms: Option<i64>,
) -> SessionReport {
    let scan: SessionScan = parse_session(session_path);
    let mut valid: Vec<&str> = pack.levels.keys().map(String::as_str).collect();
    valid.extend(pack.oneshots.keys().map(String::as_str));
    valid.push("off");

    let mode_log = read_mode_log(mode_log_path, &valid);
    let attribution = attribute_by_mode(&scan.turns, &mode_log, current_mode, flag_mtime_ms);

    let session_id = session_path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_string);
    let injections = injection_ledger::read_all(injection_ledger_path);
    let (activate_bytes, reinforce_bytes) = session_id
        .as_deref()
        .map(|id| injection_ledger::totals_for_session(&injections, id))
        .unwrap_or((0, 0));

    SessionReport {
        session_path: Some(session_path.to_path_buf()),
        session_id,
        turns: scan.turn_count(),
        model: scan.model,
        attribution,
        injection_activate_bytes: activate_bytes,
        injection_reinforce_bytes: reinforce_bytes,
    }
}

/// Whole-session measured total: `by_mode` (attributed) plus `unknown`
/// (real usage that just couldn't be pinned to a mode) — everything
/// except `sidechain`, which is deliberately excluded as non-user-facing.
/// This must NOT be just `by_mode`'s sum: under `flag-mtime` basis, tokens
/// from before the flag write correctly land entirely in `unknown` rather
/// than being guessed into a mode, and a top-line total that only counted
/// `by_mode` would then misreport a session that spent real tokens as
/// "0 tokens" — technically consistent with "nothing was attributed" but
/// actively misleading as a headline number.
pub fn measured_output_total(attr: &Attribution) -> u64 {
    attr.by_mode.values().map(|b| b.output_tokens).sum::<u64>() + attr.unknown.output_tokens
}

pub fn measured_input_total(attr: &Attribution) -> u64 {
    attr.by_mode
        .values()
        .map(|b| b.input_tokens + b.cache_creation_input_tokens)
        .sum::<u64>()
        + attr.unknown.input_tokens
        + attr.unknown.cache_creation_input_tokens
}

/// Reading time at ~200 wpm, ~0.75 words/token — a rough estimate, always
/// labelled as one. This is the one benefit that survives every pricing
/// model (per-request billing, a flat subscription, whatever): the model
/// really did write fewer words.
fn reading_minutes(output_tokens: u64) -> f64 {
    (output_tokens as f64 * 0.75) / 200.0
}

pub fn render_text(report: &SessionReport, pack: &CompiledPack) -> String {
    let sep = "\u{2500}".repeat(34);
    let mut out = String::new();
    out.push_str("\nFrank Stats\n");
    out.push_str(&sep);
    out.push('\n');

    if let Some(p) = &report.session_path {
        let s = p.display().to_string();
        let tail = if s.len() > 45 {
            format!("...{}", &s[s.len() - 42..])
        } else {
            s
        };
        out.push_str(&format!("Session:  {tail}\n"));
    }
    out.push_str(&format!("Turns:    {}\n", report.turns));
    out.push_str(&sep);
    out.push('\n');

    if report.turns == 0 {
        out.push_str("No conversation yet — stats available after first response.\n");
        out.push_str(&sep);
        out.push('\n');
        return out;
    }

    let output_total = measured_output_total(&report.attribution);
    let input_total = measured_input_total(&report.attribution);
    out.push_str(&format!("Output tokens (measured):   {output_total}\n"));
    out.push_str(&format!("Input tokens (measured):    {input_total}\n"));
    out.push_str(&format!(
        "Frank injected (measured):  {} activation + {} reinforcement byte(s)\n",
        report.injection_activate_bytes, report.injection_reinforce_bytes
    ));
    out.push_str(&sep);
    out.push('\n');

    let basis_note = match report.attribution.basis {
        AttributionBasis::Log => "attributed per mode via the transition log",
        AttributionBasis::FlagMtime => {
            "mode was set mid-session — only output after the change is attributed"
        }
        AttributionBasis::WholeSession => "attributed to the mode active for the whole session",
    };
    out.push_str(&format!("{basis_note}\n"));

    for (mode, bucket) in &report.attribution.by_mode {
        if bucket.output_tokens == 0 {
            continue;
        }
        let label = if mode == "none" {
            "frank off".to_string()
        } else {
            mode.clone()
        };
        match pack.benchmark.get(mode) {
            Some(stat) => {
                let est = savings_estimate(bucket.output_tokens, stat, report.model.as_deref());
                let model_note = if est.model_matches {
                    String::new()
                } else {
                    " (measured on a different model)".to_string()
                };
                out.push_str(&format!(
                    "  {label}: {} output tok — est. saved {}-{} (pack benchmark, n={}{model_note})\n",
                    bucket.output_tokens,
                    est.low_tokens,
                    est.high_tokens,
                    est.n.map(|n| n.to_string()).unwrap_or_else(|| "?".to_string()),
                ));
            }
            None => {
                out.push_str(&format!(
                    "  {label}: {} output tok (no benchmark estimate — unmeasured)\n",
                    bucket.output_tokens
                ));
            }
        }
    }
    if report.attribution.unknown.output_tokens > 0 {
        out.push_str(&format!(
            "  unattributed: {} tok (mode unknown — excluded from estimate)\n",
            report.attribution.unknown.output_tokens
        ));
    }
    if report.attribution.sidechain.output_tokens > 0 {
        out.push_str(&format!(
            "  subagent (sidechain): {} tok (tracked separately, not user-facing prose)\n",
            report.attribution.sidechain.output_tokens
        ));
    }
    out.push_str(&sep);
    out.push('\n');

    let injected_total = report.injection_activate_bytes + report.injection_reinforce_bytes;
    if let Some(price) = price_for_model(report.model.as_deref()) {
        // Approximate injected bytes -> tokens at ~4 bytes/token for the
        // cost-of-Frank side of the ledger — the same rough conversion the
        // archive used for compressed-memory estimates, applied here to
        // Frank's own overhead so it's visible in the same currency as the
        // savings estimate above.
        let injected_tokens_est = (injected_total as f64 / 4.0).round();
        let injected_cost = (injected_tokens_est / 1_000_000.0) * price;
        out.push_str(&format!(
            "Frank's own cost (est. from injected bytes): ~{}\n",
            format_usd(injected_cost)
        ));
    }
    let minutes = reading_minutes(output_total);
    out.push_str(&format!(
        "Reading time (est., ~200 wpm): ~{minutes:.1} min for what the model wrote\n"
    ));
    out.push_str(&sep);
    out.push('\n');
    out
}

#[derive(Serialize)]
pub struct JsonBucket {
    pub output_tokens: u64,
    pub input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}
impl From<&TokenBucket> for JsonBucket {
    fn from(b: &TokenBucket) -> Self {
        JsonBucket {
            output_tokens: b.output_tokens,
            input_tokens: b.input_tokens,
            cache_creation_input_tokens: b.cache_creation_input_tokens,
            cache_read_input_tokens: b.cache_read_input_tokens,
        }
    }
}

pub fn render_json(report: &SessionReport, pack: &CompiledPack) -> serde_json::Value {
    let by_mode: BTreeMap<String, serde_json::Value> = report
        .attribution
        .by_mode
        .iter()
        .map(|(mode, bucket)| {
            let est = pack.benchmark.get(mode).map(|stat| {
                let e = savings_estimate(bucket.output_tokens, stat, report.model.as_deref());
                serde_json::json!({
                    "low_tokens": e.low_tokens,
                    "mean_tokens": e.mean_tokens,
                    "high_tokens": e.high_tokens,
                    "n": e.n,
                    "benchmark_model": e.benchmark_model,
                    "model_matches": e.model_matches,
                })
            });
            (
                mode.clone(),
                serde_json::json!({ "measured": JsonBucket::from(bucket), "estimate": est }),
            )
        })
        .collect();

    serde_json::json!({
        "session_path": report.session_path.as_ref().map(|p| p.display().to_string()),
        "turns": report.turns,
        "model": report.model,
        "basis": match report.attribution.basis {
            AttributionBasis::Log => "log",
            AttributionBasis::FlagMtime => "flag-mtime",
            AttributionBasis::WholeSession => "whole-session",
        },
        "by_mode": by_mode,
        "unknown": JsonBucket::from(&report.attribution.unknown),
        "sidechain": JsonBucket::from(&report.attribution.sidechain),
        "frank_injected_bytes": {
            "activate": report.injection_activate_bytes,
            "reinforce": report.injection_reinforce_bytes,
        },
    })
}

/// One row per session, appended for lifetime aggregation — analogous to
/// the archive's `.caveman-history.jsonl`, extended with input tokens.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct HistoryRow {
    pub ts: i64,
    pub session_id: String,
    pub model: Option<String>,
    pub output_tokens: u64,
    pub input_tokens: u64,
    /// Number of non-sidechain turns observed when this row was written.
    /// Missing in pre-M3 history files, so serde conservatively reads those
    /// rows as zero rather than inventing a turn count.
    #[serde(default)]
    pub turns: usize,
}

pub fn append_history(path: &Path, row: &HistoryRow) {
    if let Ok(line) = serde_json::to_string(row) {
        let _ = frank_safeio::append_line(path, &line);
    }
}

pub fn read_history(path: &Path) -> Vec<HistoryRow> {
    frank_safeio::read_lines(path)
        .iter()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// Dedup by `session_id`, keeping the latest row (a session may be
/// re-reported across multiple `frank stats` invocations as it grows).
pub fn aggregate_history(rows: &[HistoryRow]) -> Vec<HistoryRow> {
    let mut latest: BTreeMap<String, HistoryRow> = BTreeMap::new();
    for r in rows {
        latest
            .entry(r.session_id.clone())
            .and_modify(|existing| {
                if r.ts > existing.ts {
                    *existing = r.clone();
                }
            })
            .or_insert_with(|| r.clone());
    }
    latest.into_values().collect()
}

pub const MIN_SESSIONS_FOR_LIFETIME_VERDICT: usize = 20;
pub const MIN_TURNS_FOR_LIFETIME_VERDICT: usize = 200;

/// A lifetime verdict is useful only when both dimensions of the sample are
/// large enough. Older history rows deserialize with `turns == 0`, which is
/// intentionally conservative: the CLI must not turn an unknown count into
/// a measured one.
pub fn lifetime_verdict_has_enough_data(rows: &[HistoryRow]) -> bool {
    rows.len() >= MIN_SESSIONS_FOR_LIFETIME_VERDICT
        && rows.iter().map(|row| row.turns).sum::<usize>() >= MIN_TURNS_FOR_LIFETIME_VERDICT
}
