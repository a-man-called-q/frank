//! Command reduction, one module per domain.
//!
//! `Orchestrator::reduce` in the crate root is a pure dispatcher; every arm
//! body lives here. Splitting it this way is what makes a single command
//! reachable in a test without standing up the whole command surface.

mod agent;
mod approval;
mod artifact;
mod memory;
mod message;
mod mission;
mod operation;
mod project;
mod settings;
mod task;
mod terminal;
mod update;

/// The fallback every domain reducer needs and none of them can reach.
///
/// `Orchestrator::reduce` dispatches exhaustively over all 41 `Command`
/// variants, so a reducer never receives one it does not handle. Each still
/// needs a catch-all arm to satisfy match exhaustiveness; routing them through
/// one function keeps twelve identical dead arms from each carrying their own
/// error construction.
pub(crate) fn misrouted<T>() -> crate::Result<T> {
    Err(crate::OrchestratorError::Validation(
        "command was routed to the wrong reducer".into(),
    ))
}
