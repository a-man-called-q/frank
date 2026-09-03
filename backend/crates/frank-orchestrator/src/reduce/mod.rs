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
