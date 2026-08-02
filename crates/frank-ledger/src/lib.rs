//! Session JSONL scan, mode attribution, and net-token accounting.
//!
//! This crate is why Frank exists rather than being a faster version of
//! the same unverified claim caveman made — see `AGENTS.md`: "never cut
//! the ledger." Read `stats.rs` first; it's where the pieces below
//! (session scan, mode-log join, pricing) come together into what `frank
//! stats` actually prints, and where the "never sum measured and
//! estimated into one unlabeled number" rule is enforced.

mod attribution;
mod injection_ledger;
mod mode_log;
mod pricing;
mod session;
mod stats;

#[cfg(test)]
mod attribution_tests;
#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod stats_tests;

pub use attribution::{Attribution, AttributionBasis, TokenBucket, attribute_by_mode};
pub use injection_ledger::{
    InjectionEntry, append as append_injection, read_all as read_injections,
};
pub use mode_log::{ModeLogRow, read_mode_log};
pub use pricing::{MODEL_OUTPUT_PRICE_PER_M, format_usd, price_for_model};
pub use session::{SessionScan, SessionTurn, find_recent_session, parse_session};
pub use stats::{
    HistoryRow, MIN_SESSIONS_FOR_LIFETIME_VERDICT, MIN_TURNS_FOR_LIFETIME_VERDICT, SavingsEstimate,
    SessionReport, aggregate_history, append_history, build_session_report,
    lifetime_verdict_has_enough_data, measured_input_total, measured_output_total, read_history,
    render_json, render_text, savings_estimate,
};
