//! Declarative target manifest schema (`targets/*.toml`).
//!
//! Replaces the archive's `||`-joined string detection DSL
//! (`archive/bin/install.js:206-367`), which had a real, quiet bug:
//! `detectMatch`'s `switch` has no `default` arm, so a typo'd probe kind
//! (`detect: 'commnad:claude'`) silently evaluates false forever — the
//! agent is simply never detected, with no error anywhere. Every field
//! here uses `deny_unknown_fields`, so an unrecognized probe key is a
//! manifest **parse error** (caught by `xtask lint-targets`), not a
//! silent no-op.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct TargetManifest {
    pub schema: u32,
    pub target: TargetMeta,
    #[serde(rename = "detect", default)]
    pub detect: Vec<DetectClause>,
    pub install: InstallSpec,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TargetMeta {
    pub id: String,
    pub label: String,
    /// `"native:<id>"` for a hand-built `NativeTarget` impl, `"generic"`
    /// for a manifest fully described by `install`.
    pub kind: String,
    /// Matches documented, archive-confirmed behavior for this agent —
    /// not merely "looks plausible". See AGENTS.md: "Caveman claimed 35
    /// agents; Frank claims N verified + M unverified and is right rather
    /// than impressive."
    #[serde(default)]
    pub verified: bool,
    /// Excluded from auto-detect; only installs via `--only <id>`. Ported
    /// from the archive's `soft: true` — the one part of its detection
    /// model that was unambiguously a good idea.
    #[serde(default)]
    pub soft: bool,
}

/// One clause; every key present must match (AND). Clauses in the parent
/// `detect` array OR together — any one matching clause detects the
/// target.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DetectClause {
    pub command: Option<String>,
    pub dir: Option<String>,
    pub file: Option<String>,
    pub macapp: Option<String>,
    pub vscode_ext: Option<String>,
    pub cursor_ext: Option<String>,
    pub jetbrains_config: Option<bool>,
    pub jetbrains_plugin: Option<String>,
    pub command_version: Option<CommandVersionProbe>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommandVersionProbe {
    pub bin: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub matches: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "strategy", rename_all = "kebab-case")]
pub enum InstallSpec {
    Spawn {
        #[serde(rename = "step", default)]
        steps: Vec<SpawnStep>,
        #[serde(default)]
        uninstall: Option<SpawnStep>,
    },
    MarkdownBlock {
        markdown: MarkdownBlockSpec,
    },
    SettingsMerge {
        settings: SettingsMergeSpec,
    },
    Files {
        #[serde(default)]
        file: Vec<FileSpec>,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpawnStep {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub win_shell: bool,
    #[serde(default = "default_success")]
    pub success: String,
}

fn default_success() -> String {
    "status_zero".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct MarkdownBlockSpec {
    pub path: String,
    pub begin: String,
    pub end: String,
    /// A reference into the active pack's compiled output — e.g.
    /// `"pack:static_digest"`. Resolved by the caller, not this crate
    /// (`frank-target` doesn't depend on `frank-pack` output beyond the
    /// `CompiledPack` type it already uses for level/oneshot ids).
    pub body: String,
    #[serde(default)]
    pub create_if_missing: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SettingsMergeSpec {
    pub path: String,
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(rename = "hook", default)]
    pub hooks: Vec<SettingsHookSpec>,
}

fn default_format() -> String {
    "json".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct SettingsHookSpec {
    pub event: String,
    pub command: String,
    #[serde(default)]
    pub timeout: Option<u64>,
    pub owned_marker: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileSpec {
    pub path: String,
    pub render: String,
}
