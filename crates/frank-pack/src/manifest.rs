//! Pack manifest schema (`pack.toml`). See the pack format section of the
//! project plan for the full rationale — in short: TOML for structure, plain
//! `.md` files for prose (unlike the archive's YAML-frontmatter skills,
//! which forced `>` folded scalars and banned `<`/`>` in descriptions —
//! `archive/tests/verify_repo.py:104`), levels compose from named fragments
//! instead of one document filtered at runtime, and a `[pack.budget]` table
//! makes "the injected prompt is small" a build-time invariant instead of a
//! claim.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PackManifest {
    pub schema: u32,
    pub pack: PackMeta,
    #[serde(default)]
    pub fragments: BTreeMap<String, Fragment>,
    #[serde(rename = "level", default)]
    pub levels: Vec<LevelDef>,
    #[serde(rename = "oneshot", default)]
    pub oneshots: Vec<OneshotDef>,
    #[serde(default)]
    pub activation: Option<ActivationDef>,
    #[serde(default)]
    pub benchmark: Option<BenchmarkDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PackMeta {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub default_level: String,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub budget: Option<Budget>,
}

/// Compiler-enforced hard limits. Absent = unbounded (a pack author who
/// skips this loses the invariant, but is never silently held to a made-up
/// default).
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct Budget {
    #[serde(default)]
    pub max_activation_bytes: Option<usize>,
    #[serde(default)]
    pub max_reinforce_bytes: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Fragment {
    File { file: String },
    Inline { inline: String },
}

#[derive(Debug, Clone, Deserialize)]
pub struct LevelDef {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub inherits: Option<String>,
    /// Ordered list of composition tokens: names into `[fragments]`, or the
    /// reserved tokens `@rules` (this level's `rules` file) and `@examples`
    /// (this level's folded `examples`). Inherited from the parent named in
    /// `inherits` when absent, so a level that only changes `rules` doesn't
    /// have to repeat its whole composition.
    #[serde(default)]
    pub compose: Option<Vec<String>>,
    #[serde(default)]
    pub rules: Option<String>,
    #[serde(default)]
    pub examples: Vec<Example>,
    #[serde(default)]
    pub reinforce: Option<String>,
    #[serde(default)]
    pub lang_hint: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Example {
    pub q: String,
    pub a: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OneshotDef {
    pub id: String,
    pub prompt: String,
    #[serde(default)]
    pub restores_previous: bool,
}

/// Pack-owned activation triggers. Moving these out of the engine and into
/// pack data is what lets a Spanish- or Japanese-language persona ship its
/// own natural-language triggers without patching Frank (contrast with the
/// archive, where the trigger regexes were hardcoded in
/// `caveman-mode-tracker.js` and diverged from a second, looser copy in the
/// opencode plugin — see `frank-state`'s port-fidelity notes).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ActivationDef {
    #[serde(default)]
    pub on: Vec<String>,
    #[serde(default)]
    pub off: Vec<String>,
    #[serde(default)]
    pub question_guard: Option<String>,
    #[serde(default)]
    pub command_prefix: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BenchmarkDef {
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub reduction: BTreeMap<String, ReductionStat>,
}

/// Per-level measured output reduction, with spread and provenance — what
/// replaces the archive's single hardcoded `COMPRESSION = { full: 0.65 }`
/// (`archive/src/hooks/caveman-stats.js:19`). A level absent from this map
/// means the ledger reports "unmeasured" rather than guessing.
#[derive(Debug, Clone, Deserialize)]
pub struct ReductionStat {
    pub mean: f64,
    #[serde(default)]
    pub p25: Option<f64>,
    #[serde(default)]
    pub p75: Option<f64>,
    #[serde(default)]
    pub n: Option<u32>,
    #[serde(default)]
    pub model: Option<String>,
}
