//! Install plans: a target's `plan()` returns a list of [`Action`]s instead
//! of performing writes. This is what makes `--dry-run` exact by
//! construction — the CLI prints the same plan it would otherwise execute,
//! rather than a hand-maintained parallel code path.
//!
//! The archive threaded `opts.dryRun` by hand through every `runSpawn` /
//! `fs.writeFileSync` call site (`archive/bin/install.js`), so dry-run
//! accuracy was only as good as the least careful call. Here there is
//! structurally only one way to make a change: build an `Action`, then
//! either print it or hand it to [`apply`].

use std::path::PathBuf;

use crate::settings::HookSpec;

#[derive(Debug, Clone)]
pub enum Action {
    EnsureDir(PathBuf),
    /// Copy `path` to `backup_path` iff `backup_path` doesn't already
    /// exist — the archive's "exactly-once backup" policy, so a second
    /// install run can't overwrite the only known-good pre-install copy
    /// with an already-merged file.
    BackupIfAbsent { path: PathBuf, backup_path: PathBuf },
    /// Read-modify-write `settings_path`: validate hook fields, then add
    /// each hook in `add` (skipping any whose marker is already present).
    MergeSettingsHooks { settings_path: PathBuf, add: Vec<HookSpec> },
    /// Read-modify-write `settings_path`: remove any hook whose command
    /// contains one of `markers`, then validate and prune orphans.
    RemoveSettingsHooks { settings_path: PathBuf, markers: Vec<String> },
    RemoveFileIfManaged { path: PathBuf, must_contain: String },
    /// Run each step in order; a step failing does not abort later steps
    /// (matches the archive's best-effort multi-agent install loop —
    /// one agent's broken CLI shouldn't block another's).
    SpawnSteps { steps: Vec<ResolvedSpawnStep> },
    MarkdownBlockAppend {
        path: PathBuf,
        begin: String,
        end: String,
        body: String,
        create_if_missing: bool,
    },
    MarkdownBlockRemove { path: PathBuf, begin: String, end: String },
    Noop { reason: String },
}

#[derive(Debug, Clone)]
pub struct ResolvedSpawnStep {
    pub program: String,
    pub args: Vec<String>,
}

pub struct InstallPlan {
    pub target_id: String,
    pub actions: Vec<Action>,
}

impl InstallPlan {
    pub fn new(target_id: impl Into<String>) -> Self {
        InstallPlan { target_id: target_id.into(), actions: Vec::new() }
    }

    pub fn push(&mut self, action: Action) {
        self.actions.push(action);
    }

    /// Human-readable description of each action, for `--dry-run` and
    /// `frank install --dry-run`'s printed plan — this is the *only*
    /// rendering of a plan; there's no separate "preview text" to drift
    /// from what `apply` actually does.
    pub fn describe(&self) -> Vec<String> {
        self.actions
            .iter()
            .map(|a| match a {
                Action::EnsureDir(p) => format!("ensure directory {}", p.display()),
                Action::BackupIfAbsent { path, backup_path } => {
                    format!("back up {} -> {} (if not already backed up)", path.display(), backup_path.display())
                }
                Action::MergeSettingsHooks { settings_path, add } => format!(
                    "merge {} hook(s) into {}: {}",
                    add.len(),
                    settings_path.display(),
                    add.iter().map(|h| h.event.as_str()).collect::<Vec<_>>().join(", ")
                ),
                Action::RemoveSettingsHooks { settings_path, markers } => format!(
                    "remove hook(s) matching [{}] from {}",
                    markers.join(", "),
                    settings_path.display()
                ),
                Action::RemoveFileIfManaged { path, .. } => format!("remove {} (if Frank-managed)", path.display()),
                Action::SpawnSteps { steps } => steps
                    .iter()
                    .map(|s| format!("run: {} {}", s.program, s.args.join(" ")))
                    .collect::<Vec<_>>()
                    .join("; "),
                Action::MarkdownBlockAppend { path, .. } => format!("update marker-fenced block in {}", path.display()),
                Action::MarkdownBlockRemove { path, .. } => format!("remove marker-fenced block from {}", path.display()),
                Action::Noop { reason } => format!("(no-op: {reason})"),
            })
            .collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error("refusing to modify {0}: existing file could not be parsed as JSON/JSONC")]
    UnparseableSettings(PathBuf),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    SafeIo(#[from] frank_safeio::SafeIoError),
}

/// Execute a plan's actions in order, returning one human-readable log line
/// per action actually taken. Every filesystem write goes through
/// `frank-safeio`.
pub fn apply(plan: &InstallPlan) -> Result<Vec<String>, ApplyError> {
    let mut log = Vec::new();
    for action in &plan.actions {
        match action {
            Action::EnsureDir(p) => {
                std::fs::create_dir_all(p)?;
                log.push(format!("ensured directory {}", p.display()));
            }
            Action::BackupIfAbsent { path, backup_path } => {
                if backup_path.exists() {
                    // Already handled on a prior install — never re-derive
                    // this decision from `path`'s *current* contents, which
                    // by now include Frank's own merged hooks. Re-deriving
                    // it was a real bug: a settings.json that didn't exist
                    // before the first install (so nothing to back up then)
                    // would get "backed up" on the *second* install instead
                    // — capturing the already-merged file and mislabeling
                    // it as pristine pre-install state.
                } else if path.exists() {
                    std::fs::copy(path, backup_path)?;
                    log.push(format!("backed up {} -> {}", path.display(), backup_path.display()));
                } else {
                    // Nothing existed before install. Still write a marker
                    // so a later install (once settings.json exists, now
                    // containing our own hooks) can't mistake "no prior
                    // file" for "haven't backed up yet".
                    std::fs::write(backup_path, "{}\n")?;
                    log.push(format!(
                        "recorded that {} did not exist before install",
                        path.display()
                    ));
                }
            }
            Action::MergeSettingsHooks { settings_path, add } => {
                let mut settings = crate::settings::read_settings(settings_path)
                    .ok_or_else(|| ApplyError::UnparseableSettings(settings_path.clone()))?;
                crate::settings::validate_hook_fields(&mut settings);
                let mut changed = false;
                for spec in add {
                    if crate::settings::add_command_hook(&mut settings, spec) {
                        changed = true;
                        log.push(format!("added {} hook to {}", spec.event, settings_path.display()));
                    }
                }
                if changed {
                    crate::settings::write_settings(settings_path, &settings)?;
                } else {
                    log.push(format!("{}: hooks already present, nothing to do", settings_path.display()));
                }
            }
            Action::RemoveSettingsHooks { settings_path, markers } => {
                let Some(mut settings) = crate::settings::read_settings(settings_path) else {
                    log.push(format!("{}: unparseable, skipped", settings_path.display()));
                    continue;
                };
                let marker_refs: Vec<&str> = markers.iter().map(String::as_str).collect();
                let removed = crate::settings::remove_owned_hooks(&mut settings, &marker_refs);
                crate::settings::validate_hook_fields(&mut settings);
                if removed > 0 {
                    crate::settings::write_settings(settings_path, &settings)?;
                    log.push(format!("removed {removed} hook(s) from {}", settings_path.display()));
                }
            }
            Action::RemoveFileIfManaged { path, must_contain } => {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if content.contains(must_contain.as_str()) {
                        std::fs::remove_file(path)?;
                        log.push(format!("removed {}", path.display()));
                    }
                }
            }
            Action::SpawnSteps { steps } => {
                for step in steps {
                    match std::process::Command::new(&step.program).args(&step.args).status() {
                        Ok(status) if status.success() => {
                            log.push(format!("ran: {} {}", step.program, step.args.join(" ")));
                        }
                        Ok(status) => log.push(format!(
                            "{} {} exited with {:?}",
                            step.program,
                            step.args.join(" "),
                            status.code()
                        )),
                        Err(e) => log.push(format!("failed to run {}: {e}", step.program)),
                    }
                }
            }
            Action::MarkdownBlockAppend { path, begin, end, body, create_if_missing } => {
                if !path.exists() && !*create_if_missing {
                    log.push(format!("{}: does not exist, skipped", path.display()));
                    continue;
                }
                let existing = std::fs::read_to_string(path).unwrap_or_default();
                let block = crate::markdown_block::Block { begin: begin.clone(), end: end.clone() };
                use crate::markdown_block::AppendOutcome;
                match crate::markdown_block::append(&existing, &block, body) {
                    AppendOutcome::AlreadyPresent => {
                        log.push(format!("{}: block already present", path.display()));
                    }
                    AppendOutcome::Appended(text) | AppendOutcome::Repaired(text) => {
                        if let Some(dir) = path.parent() {
                            std::fs::create_dir_all(dir)?;
                        }
                        frank_safeio::write_flag_atomic(path, &text)?;
                        log.push(format!("updated {}", path.display()));
                    }
                }
            }
            Action::MarkdownBlockRemove { path, begin, end } => {
                if let Ok(existing) = std::fs::read_to_string(path) {
                    let block = crate::markdown_block::Block { begin: begin.clone(), end: end.clone() };
                    match crate::markdown_block::remove(&existing, &block) {
                        Some(text) => {
                            frank_safeio::write_flag_atomic(path, &text)?;
                            log.push(format!("updated {}", path.display()));
                        }
                        None => {
                            std::fs::remove_file(path)?;
                            log.push(format!("removed {} (block was the only content)", path.display()));
                        }
                    }
                }
            }
            Action::Noop { reason } => log.push(format!("skipped: {reason}")),
        }
    }
    Ok(log)
}

/// Context a target needs to build a plan. Deliberately narrow — a target
/// gets exactly what it needs to decide what to do, nothing that would let
/// it reach for ambient global state.
pub struct InstallCtx {
    pub config_dir: PathBuf,
    pub frank_bin: PathBuf,
    /// Project directory a project-scoped target (an `AGENTS.md` at repo
    /// root, say) resolves relative paths against.
    pub cwd: PathBuf,
}

impl InstallCtx {
    /// Expand a manifest path string: `$HOME/...` against the real home
    /// dir, `./...` against `cwd`, anything else passed through as-is.
    pub fn resolve_path(&self, raw: &str) -> PathBuf {
        if let Some(rest) = raw.strip_prefix("$HOME") {
            let rest = rest.strip_prefix(['/', '\\']).unwrap_or(rest);
            return frank_safeio::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(rest);
        }
        if let Some(rest) = raw.strip_prefix("./") {
            return self.cwd.join(rest);
        }
        PathBuf::from(raw)
    }

    /// Substitute `{frank_bin}` in a manifest string template. More
    /// placeholders (`{repo}`, ...) are added as real manifests need them
    /// — kept minimal rather than speculative.
    pub fn expand_template(&self, raw: &str) -> String {
        raw.replace("{frank_bin}", &self.frank_bin.display().to_string())
    }
}

pub struct ProbeEnv {
    pub path_dirs: Vec<PathBuf>,
    pub home: Option<PathBuf>,
    pub extra_dirs: Vec<PathBuf>,
    pub is_macos: bool,
}

impl ProbeEnv {
    /// Build from the real process environment.
    pub fn from_process() -> Self {
        let path_dirs = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        ProbeEnv {
            path_dirs,
            home: frank_safeio::home_dir(),
            extra_dirs: Vec::new(),
            is_macos: cfg!(target_os = "macos"),
        }
    }

    pub fn has_command(&self, name: &str) -> bool {
        self.path_dirs.iter().any(|dir| {
            let candidate = dir.join(name);
            candidate.is_file() || dir.join(format!("{name}.exe")).is_file()
        })
    }

    pub fn dir_exists(&self, rel_to_home: &str) -> bool {
        self.home
            .as_ref()
            .map(|h| h.join(rel_to_home).is_dir())
            .unwrap_or(false)
    }

    /// Expand a manifest path string's leading `$HOME` against this env's
    /// home dir. Manifests never see an absolute path outside `$HOME` —
    /// enforced by `xtask lint-targets`, not at expansion time.
    pub fn expand(&self, raw: &str) -> Option<PathBuf> {
        if let Some(rest) = raw.strip_prefix("$HOME") {
            let rest = rest.strip_prefix(['/', '\\']).unwrap_or(rest);
            return self.home.as_ref().map(|h| h.join(rest));
        }
        Some(PathBuf::from(raw))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Detection {
    Detected,
    NotDetected,
}

pub struct Diagnosis {
    pub ok: bool,
    pub message: String,
}
