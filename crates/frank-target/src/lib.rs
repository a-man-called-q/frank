//! Agent target manifests, detection, and install planning.
//!
//! M2 shipped one hand-built target ([`claude_code`]) via the `plan`/
//! `apply` architecture. M5 adds the declarative half: [`manifest`] is the
//! `targets/*.toml` schema, [`detect`] evaluates it against a
//! [`ProbeEnv`], and [`generic`] compiles a manifest down to the exact
//! same `Action` list a hand-built target produces — see `plan.rs` for why
//! plans are pure data instead of side-effecting functions.

mod claude_code;
mod detect;
mod generic;
mod manifest;
mod markdown_block;
mod plan;
mod settings;

#[cfg(test)]
mod manifest_tests;
#[cfg(test)]
mod settings_tests;

pub use claude_code::ClaudeCodeTarget;
pub use detect::detect;
pub use generic::{build_install_plan, build_uninstall_plan};
pub use manifest::{
    CommandVersionProbe, DetectClause, FileSpec, InstallSpec, MarkdownBlockSpec, SettingsHookSpec,
    SettingsMergeSpec, SpawnStep, TargetManifest, TargetMeta,
};
pub use plan::{
    Action, ApplyError, Detection, Diagnosis, InstallCtx, InstallPlan, ProbeEnv, ResolvedSpawnStep,
    apply,
};
pub use settings::read_settings;
