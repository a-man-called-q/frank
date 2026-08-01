//! Agent target manifests, detection, and install planning.
//!
//! M2 shipped one hand-built target ([`claude_code`]) via the `plan`/
//! `apply` architecture. M5 adds the declarative half: [`manifest`] is the
//! `targets/*.toml` schema, [`detect`] evaluates it against a
//! [`ProbeEnv`], and [`generic`] compiles a manifest down to the exact
//! same `Action` list a hand-built target produces — see `plan.rs` for why
//! plans are pure data instead of side-effecting functions.

pub mod claude_code;
pub mod detect;
pub mod generic;
pub mod manifest;
pub mod markdown_block;
pub mod plan;
pub mod settings;

#[cfg(test)]
mod manifest_tests;
#[cfg(test)]
mod settings_tests;

pub use plan::{
    apply, Action, ApplyError, Detection, Diagnosis, InstallCtx, InstallPlan, ProbeEnv,
    ResolvedSpawnStep,
};
