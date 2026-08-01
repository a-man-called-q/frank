//! Session JSONL scan, mode attribution, and net-token accounting.
//!
//! This crate is why Frank exists rather than being a faster version of
//! the same unverified claim caveman made — see `AGENTS.md`: "never cut
//! the ledger." Read `stats.rs` first; it's where the pieces below
//! (session scan, mode-log join, pricing) come together into what `frank
//! stats` actually prints, and where the "never sum measured and
//! estimated into one unlabeled number" rule is enforced.

pub mod attribution;
pub mod injection_ledger;
pub mod mode_log;
pub mod pricing;
pub mod session;
pub mod stats;

#[cfg(test)]
mod attribution_tests;
#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod stats_tests;

pub use attribution::{attribute_by_mode, Attribution, AttributionBasis, TokenBucket};
pub use injection_ledger::InjectionEntry;
pub use mode_log::{read_mode_log, ModeLogRow};
pub use pricing::{format_usd, price_for_model};
pub use session::{find_recent_session, parse_session, SessionScan, SessionTurn};
pub use stats::{build_session_report, render_json, render_text, HistoryRow, SessionReport};
