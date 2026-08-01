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

use std::path::{Path, PathBuf};

use crate::settings::HookSpec;

#[derive(Debug, Clone)]
pub enum Action {
    EnsureDir(PathBuf),
    /// Copy `path` to `backup_path` iff `backup_path` doesn't already
    /// exist — the archive's "exactly-once backup" policy, so a second
    /// install run can't overwrite the only known-good pre-install copy
    /// with an already-merged file.
    BackupIfAbsent {
        path: PathBuf,
        backup_path: PathBuf,
    },
    /// Read-modify-write `settings_path`: validate hook fields, then add
    /// each hook in `add` (skipping any whose marker is already present).
    MergeSettingsHooks {
        settings_path: PathBuf,
        add: Vec<HookSpec>,
    },
    /// Read-modify-write `settings_path`: remove any hook whose command
    /// contains one of `markers`, then validate and prune orphans.
    RemoveSettingsHooks {
        settings_path: PathBuf,
        markers: Vec<String>,
    },
    RemoveFileIfManaged {
        path: PathBuf,
        must_contain: String,
    },
    /// Run each step in order; a step failing does not abort later steps
    /// (matches the archive's best-effort multi-agent install loop —
    /// one agent's broken CLI shouldn't block another's).
    SpawnSteps {
        steps: Vec<ResolvedSpawnStep>,
    },
    MarkdownBlockAppend {
        path: PathBuf,
        begin: String,
        end: String,
        body: String,
        create_if_missing: bool,
    },
    MarkdownBlockRemove {
        path: PathBuf,
        begin: String,
        end: String,
    },
    Noop {
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub struct ResolvedSpawnStep {
    pub program: String,
    pub args: Vec<String>,
}

pub struct InstallPlan {
    pub target_id: String,
    pub actions: Vec<Action>,
    scope_roots: Vec<PathBuf>,
}

impl InstallPlan {
    pub fn new(target_id: impl Into<String>) -> Self {
        InstallPlan {
            target_id: target_id.into(),
            actions: Vec::new(),
            scope_roots: Vec::new(),
        }
    }

    /// Construct a plan whose filesystem actions must remain below one of the
    /// supplied roots. Generic manifests use this at plan construction time;
    /// `apply` checks it again so a caller cannot accidentally bypass the
    /// boundary by holding onto a plan and applying it later.
    pub fn scoped(target_id: impl Into<String>, roots: Vec<PathBuf>) -> Self {
        let mut plan = Self::new(target_id);
        plan.scope_roots = roots;
        plan
    }

    pub fn set_scope(&mut self, roots: Vec<PathBuf>) {
        self.scope_roots = roots;
    }

    pub fn validate_scope(&self) -> Result<(), ApplyError> {
        if self.scope_roots.is_empty() {
            return Ok(());
        }
        for path in self.action_paths() {
            if !self
                .scope_roots
                .iter()
                .any(|root| path_is_within(path, root))
            {
                return Err(ApplyError::OutOfScope(path.clone()));
            }
        }
        Ok(())
    }

    fn action_paths(&self) -> Vec<&PathBuf> {
        self.actions
            .iter()
            .flat_map(|action| match action {
                Action::EnsureDir(path) => vec![path],
                Action::BackupIfAbsent { path, backup_path } => vec![path, backup_path],
                Action::MergeSettingsHooks { settings_path, .. }
                | Action::RemoveSettingsHooks { settings_path, .. } => vec![settings_path],
                Action::RemoveFileIfManaged { path, .. }
                | Action::MarkdownBlockAppend { path, .. }
                | Action::MarkdownBlockRemove { path, .. } => vec![path],
                Action::SpawnSteps { .. } | Action::Noop { .. } => Vec::new(),
            })
            .collect()
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
                    format!(
                        "back up {} -> {} (if not already backed up)",
                        path.display(),
                        backup_path.display()
                    )
                }
                Action::MergeSettingsHooks { settings_path, add } => format!(
                    "merge {} hook(s) into {}: {}",
                    add.len(),
                    settings_path.display(),
                    add.iter()
                        .map(|h| h.event.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                Action::RemoveSettingsHooks {
                    settings_path,
                    markers,
                } => format!(
                    "remove hook(s) matching [{}] from {}",
                    markers.join(", "),
                    settings_path.display()
                ),
                Action::RemoveFileIfManaged { path, .. } => {
                    format!("remove {} (if Frank-managed)", path.display())
                }
                Action::SpawnSteps { steps } => steps
                    .iter()
                    .map(|s| format!("run: {} {}", s.program, s.args.join(" ")))
                    .collect::<Vec<_>>()
                    .join("; "),
                Action::MarkdownBlockAppend { path, .. } => {
                    format!("update marker-fenced block in {}", path.display())
                }
                Action::MarkdownBlockRemove { path, .. } => {
                    format!("remove marker-fenced block from {}", path.display())
                }
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
    #[error("refusing out-of-scope target write: {0}")]
    OutOfScope(PathBuf),
}

/// Execute a plan's actions in order, returning one human-readable log line
/// per action actually taken. Every filesystem write goes through
/// `frank-safeio`.
pub fn apply(plan: &InstallPlan) -> Result<Vec<String>, ApplyError> {
    plan.validate_scope()?;
    let mut log = Vec::new();
    for action in &plan.actions {
        match action {
            Action::EnsureDir(p) => {
                frank_safeio::ensure_dir(p)?;
                log.push(format!("ensured directory {}", p.display()));
            }
            Action::BackupIfAbsent { path, backup_path } => {
                let backup_state = std::fs::symlink_metadata(backup_path);
                if backup_state
                    .as_ref()
                    .is_ok_and(|m| m.file_type().is_symlink())
                {
                    return Err(ApplyError::SafeIo(frank_safeio::SafeIoError::IsSymlink));
                }
                if backup_state.as_ref().is_ok_and(|m| !m.is_file()) {
                    return Err(ApplyError::SafeIo(frank_safeio::SafeIoError::NotAFile));
                }
                if backup_state.is_ok() {
                    // Already handled on a prior install — never re-derive
                    // this decision from `path`'s *current* contents, which
                    // by now include Frank's own merged hooks. Re-deriving
                    // it was a real bug: a settings.json that didn't exist
                    // before the first install (so nothing to back up then)
                    // would get "backed up" on the *second* install instead
                    // — capturing the already-merged file and mislabeling
                    // it as pristine pre-install state.
                } else if std::fs::symlink_metadata(path)
                    .map(|m| m.is_file() && !m.file_type().is_symlink())
                    .unwrap_or(false)
                {
                    let contents =
                        frank_safeio::read_text_capped(path, frank_safeio::MAX_CONFIG_BYTES)?;
                    frank_safeio::write_text_atomic(
                        backup_path,
                        &contents,
                        frank_safeio::MAX_CONFIG_BYTES,
                    )?;
                    log.push(format!(
                        "backed up {} -> {}",
                        path.display(),
                        backup_path.display()
                    ));
                } else {
                    // Nothing existed before install. Still write a marker
                    // so a later install (once settings.json exists, now
                    // containing our own hooks) can't mistake "no prior
                    // file" for "haven't backed up yet".
                    if std::fs::symlink_metadata(path).is_ok() {
                        return Err(ApplyError::SafeIo(frank_safeio::SafeIoError::NotAFile));
                    }
                    frank_safeio::write_text_atomic(
                        backup_path,
                        "{}\n",
                        frank_safeio::MAX_CONFIG_BYTES,
                    )?;
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
                        log.push(format!(
                            "added {} hook to {}",
                            spec.event,
                            settings_path.display()
                        ));
                    }
                }
                if changed {
                    crate::settings::write_settings(settings_path, &settings)?;
                } else {
                    log.push(format!(
                        "{}: hooks already present, nothing to do",
                        settings_path.display()
                    ));
                }
            }
            Action::RemoveSettingsHooks {
                settings_path,
                markers,
            } => {
                let Some(mut settings) = crate::settings::read_settings(settings_path) else {
                    log.push(format!("{}: unparseable, skipped", settings_path.display()));
                    continue;
                };
                let marker_refs: Vec<&str> = markers.iter().map(String::as_str).collect();
                let removed = crate::settings::remove_owned_hooks(&mut settings, &marker_refs);
                if removed > 0 {
                    crate::settings::write_settings(settings_path, &settings)?;
                    log.push(format!(
                        "removed {removed} hook(s) from {}",
                        settings_path.display()
                    ));
                }
            }
            Action::RemoveFileIfManaged { path, must_contain } => {
                let metadata = std::fs::symlink_metadata(path);
                if metadata.as_ref().is_ok_and(|m| m.file_type().is_symlink()) {
                    return Err(ApplyError::SafeIo(frank_safeio::SafeIoError::IsSymlink));
                }
                if metadata.as_ref().is_ok_and(|m| !m.is_file()) {
                    return Err(ApplyError::SafeIo(frank_safeio::SafeIoError::NotAFile));
                }
                if frank_safeio::remove_file_if_contains(path, must_contain)? {
                    log.push(format!("removed {}", path.display()));
                }
            }
            Action::SpawnSteps { steps } => {
                for step in steps {
                    match std::process::Command::new(&step.program)
                        .args(&step.args)
                        .status()
                    {
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
            Action::MarkdownBlockAppend {
                path,
                begin,
                end,
                body,
                create_if_missing,
            } => {
                if !path.exists() && !*create_if_missing {
                    log.push(format!("{}: does not exist, skipped", path.display()));
                    continue;
                }
                let existing =
                    match frank_safeio::read_text_capped(path, frank_safeio::MAX_CONFIG_BYTES) {
                        Ok(existing) => existing,
                        Err(frank_safeio::SafeIoError::Io(e))
                            if e.kind() == std::io::ErrorKind::NotFound && *create_if_missing =>
                        {
                            String::new()
                        }
                        Err(e) => return Err(ApplyError::SafeIo(e)),
                    };
                let block = crate::markdown_block::Block {
                    begin: begin.clone(),
                    end: end.clone(),
                };
                use crate::markdown_block::AppendOutcome;
                match crate::markdown_block::append(&existing, &block, body) {
                    AppendOutcome::AlreadyPresent => {
                        log.push(format!("{}: block already present", path.display()));
                    }
                    AppendOutcome::Appended(text) | AppendOutcome::Repaired(text) => {
                        frank_safeio::write_text_atomic(
                            path,
                            &text,
                            frank_safeio::MAX_CONFIG_BYTES,
                        )?;
                        log.push(format!("updated {}", path.display()));
                    }
                }
            }
            Action::MarkdownBlockRemove { path, begin, end } => {
                let existing =
                    match frank_safeio::read_text_capped(path, frank_safeio::MAX_CONFIG_BYTES) {
                        Ok(existing) => existing,
                        Err(frank_safeio::SafeIoError::Io(error))
                            if error.kind() == std::io::ErrorKind::NotFound =>
                        {
                            continue;
                        }
                        Err(error) => return Err(ApplyError::SafeIo(error)),
                    };
                let block = crate::markdown_block::Block {
                    begin: begin.clone(),
                    end: end.clone(),
                };
                match crate::markdown_block::remove(&existing, &block) {
                    Some(text) => {
                        frank_safeio::write_text_atomic(
                            path,
                            &text,
                            frank_safeio::MAX_CONFIG_BYTES,
                        )?;
                        log.push(format!("updated {}", path.display()));
                    }
                    None => {
                        let metadata = std::fs::symlink_metadata(path)?;
                        if metadata.file_type().is_symlink() || !metadata.is_file() {
                            return Err(ApplyError::SafeIo(frank_safeio::SafeIoError::NotAFile));
                        }
                        // The file is removed through the same anchored,
                        // no-follow primitive as every other Frank-owned
                        // write.  A direct path-based unlink here would
                        // reintroduce a symlink/TOCTOU escape precisely on
                        // the uninstall path where user data is most at
                        // risk.
                        let _ = frank_safeio::remove_file_if_contains(path, begin)?;
                        log.push(format!(
                            "removed {} (block was the only content)",
                            path.display()
                        ));
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
        if let Some(rest) = raw
            .strip_prefix("$HOME")
            .filter(|rest| rest.is_empty() || rest.starts_with(['/', '\\']))
        {
            let rest = rest.strip_prefix(['/', '\\']).unwrap_or(rest);
            return frank_safeio::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(rest);
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

    pub fn scope_roots(&self) -> Vec<PathBuf> {
        let mut roots = vec![self.config_dir.clone(), self.cwd.clone()];
        if let Some(home) = frank_safeio::home_dir() {
            roots.push(home);
        }
        roots
    }
}

fn path_is_within(path: &Path, root: &Path) -> bool {
    let path = normalize_path(path);
    let root = normalize_path(root);
    #[cfg(windows)]
    {
        // Windows drive letters, junctions, and ordinary path components are
        // case-insensitive.  Keep the component-boundary check from
        // `Path::starts_with`, but compare a normalized lowercase view so a
        // preview made with `C:\Users\...` cannot be rejected (or bypassed)
        // merely because an external writer used a different case.
        return PathBuf::from(path.to_string_lossy().to_ascii_lowercase())
            .starts_with(PathBuf::from(root.to_string_lossy().to_ascii_lowercase()));
    }
    #[cfg(not(windows))]
    {
        path.starts_with(root)
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
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
        if let Some(rest) = raw
            .strip_prefix("$HOME")
            .filter(|rest| rest.is_empty() || rest.starts_with(['/', '\\']))
        {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    fn hook(event: &str, marker: &str) -> HookSpec {
        HookSpec {
            event: event.to_string(),
            command: format!("/bin/frank {marker}"),
            timeout: Some(5),
            status_message: Some("testing".to_string()),
            owned_marker: marker.to_string(),
        }
    }

    #[test]
    fn describe_covers_every_action_without_executing_it() {
        let root = PathBuf::from("/tmp/frank-plan");
        let mut plan = InstallPlan::new("test");
        plan.push(Action::EnsureDir(root.join("config")));
        plan.push(Action::BackupIfAbsent {
            path: root.join("settings.json"),
            backup_path: root.join("settings.json.bak"),
        });
        plan.push(Action::MergeSettingsHooks {
            settings_path: root.join("settings.json"),
            add: vec![hook("SessionStart", "hook session-start")],
        });
        plan.push(Action::RemoveSettingsHooks {
            settings_path: root.join("settings.json"),
            markers: vec!["hook session-start".to_string()],
        });
        plan.push(Action::RemoveFileIfManaged {
            path: root.join("generated.md"),
            must_contain: "frank".to_string(),
        });
        plan.push(Action::SpawnSteps {
            steps: vec![ResolvedSpawnStep {
                program: "true".to_string(),
                args: vec!["--dry-run".to_string()],
            }],
        });
        plan.push(Action::MarkdownBlockAppend {
            path: root.join("AGENTS.md"),
            begin: "<!-- begin -->".to_string(),
            end: "<!-- end -->".to_string(),
            body: "rules".to_string(),
            create_if_missing: true,
        });
        plan.push(Action::MarkdownBlockRemove {
            path: root.join("AGENTS.md"),
            begin: "<!-- begin -->".to_string(),
            end: "<!-- end -->".to_string(),
        });
        plan.push(Action::Noop {
            reason: "test".to_string(),
        });

        let descriptions = plan.describe();
        assert_eq!(descriptions.len(), 9);
        assert!(descriptions.iter().any(|d| d.contains("ensure directory")));
        assert!(descriptions.iter().any(|d| d.contains("back up")));
        assert!(descriptions.iter().any(|d| d.contains("merge 1 hook")));
        assert!(descriptions.iter().any(|d| d.contains("remove hook")));
        assert!(descriptions.iter().any(|d| d.contains("Frank-managed")));
        assert!(descriptions.iter().any(|d| d.contains("run: true")));
        assert!(descriptions.iter().any(|d| d.contains("marker-fenced")));
        assert!(descriptions.iter().any(|d| d.contains("no-op")));
    }

    #[test]
    fn scope_uses_component_boundaries_and_normalizes_dot_segments() {
        let root = PathBuf::from("/tmp/frank-scope");
        let mut inside = InstallPlan::scoped("test", vec![root.clone()]);
        inside.push(Action::EnsureDir(root.join("./nested/../config")));
        assert!(inside.validate_scope().is_ok());

        let mut prefix_spoof = InstallPlan::scoped("test", vec![root.clone()]);
        prefix_spoof.push(Action::EnsureDir(PathBuf::from("/tmp/frank-scope-escape")));
        assert!(matches!(
            prefix_spoof.validate_scope(),
            Err(ApplyError::OutOfScope(_))
        ));
    }

    #[test]
    fn apply_backup_is_exactly_once_and_records_missing_source() {
        let tmp = tempdir().unwrap();
        let source = tmp.path().join("settings.json");
        let backup = tmp.path().join("settings.json.frank-backup");
        fs::write(&source, "{\"user\":true}\n").unwrap();

        let mut plan = InstallPlan::new("test");
        plan.push(Action::BackupIfAbsent {
            path: source.clone(),
            backup_path: backup.clone(),
        });
        let first = apply(&plan).unwrap();
        assert!(first[0].contains("backed up"));
        assert_eq!(fs::read_to_string(&backup).unwrap(), "{\"user\":true}\n");

        fs::write(&source, "{\"frank\":true}\n").unwrap();
        let second = apply(&plan).unwrap();
        assert!(second.is_empty());
        assert_eq!(fs::read_to_string(&backup).unwrap(), "{\"user\":true}\n");

        let fresh = tempdir().unwrap();
        let mut missing = InstallPlan::new("test");
        let missing_source = fresh.path().join("settings.json");
        let missing_backup = fresh.path().join("settings.json.frank-backup");
        missing.push(Action::BackupIfAbsent {
            path: missing_source,
            backup_path: missing_backup.clone(),
        });
        let log = apply(&missing).unwrap();
        assert!(log[0].contains("did not exist"));
        assert_eq!(fs::read_to_string(missing_backup).unwrap(), "{}\n");
    }

    #[test]
    fn apply_rejects_symlink_or_directory_backup_and_source() {
        let tmp = tempdir().unwrap();
        let source = tmp.path().join("settings.json");
        let backup = tmp.path().join("settings.json.frank-backup");
        fs::write(&source, "{}\n").unwrap();

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(tmp.path().join("decoy"), &backup).unwrap();
            let mut plan = InstallPlan::new("test");
            plan.push(Action::BackupIfAbsent {
                path: source.clone(),
                backup_path: backup.clone(),
            });
            assert!(matches!(
                apply(&plan),
                Err(ApplyError::SafeIo(frank_safeio::SafeIoError::IsSymlink))
            ));
            fs::remove_file(&backup).unwrap();
        }

        fs::create_dir(&backup).unwrap();
        let mut backup_dir = InstallPlan::new("test");
        backup_dir.push(Action::BackupIfAbsent {
            path: source.clone(),
            backup_path: backup,
        });
        assert!(matches!(
            apply(&backup_dir),
            Err(ApplyError::SafeIo(frank_safeio::SafeIoError::NotAFile))
        ));

        let source_dir = tmp.path().join("source-dir");
        fs::create_dir(&source_dir).unwrap();
        let backup_file = tmp.path().join("backup.json");
        let mut source_plan = InstallPlan::new("test");
        source_plan.push(Action::BackupIfAbsent {
            path: source_dir,
            backup_path: backup_file,
        });
        assert!(matches!(
            apply(&source_plan),
            Err(ApplyError::SafeIo(frank_safeio::SafeIoError::NotAFile))
        ));
    }

    #[test]
    fn apply_merges_and_removes_hooks_without_touching_user_data() {
        let tmp = tempdir().unwrap();
        let settings_path = tmp.path().join("settings.json");
        fs::write(
            &settings_path,
            serde_json::to_string(&json!({
                "custom": {"keep": true},
                "hooks": {"SessionStart": [{"hooks": [{"type": "command", "command": "echo user"}]}]}
            })).unwrap(),
        ).unwrap();

        let mut install = InstallPlan::new("test");
        install.push(Action::MergeSettingsHooks {
            settings_path: settings_path.clone(),
            add: vec![hook("SessionStart", "hook session-start")],
        });
        let log = apply(&install).unwrap();
        assert!(log.iter().any(|line| line.contains("added SessionStart")));
        let value = crate::settings::read_settings(&settings_path).unwrap();
        assert_eq!(value["custom"]["keep"], true);
        assert_eq!(value["hooks"]["SessionStart"].as_array().unwrap().len(), 2);

        let mut remove = InstallPlan::new("test");
        remove.push(Action::RemoveSettingsHooks {
            settings_path: settings_path.clone(),
            markers: vec!["hook session-start".to_string()],
        });
        let log = apply(&remove).unwrap();
        assert!(log.iter().any(|line| line.contains("removed 1")));
        let value = crate::settings::read_settings(&settings_path).unwrap();
        assert_eq!(value["custom"]["keep"], true);
        assert_eq!(
            value["hooks"]["SessionStart"][0]["hooks"][0]["command"],
            "echo user"
        );

        fs::write(&settings_path, "not-json").unwrap();
        let mut skip = InstallPlan::new("test");
        skip.push(Action::RemoveSettingsHooks {
            settings_path,
            markers: vec!["hook session-start".to_string()],
        });
        let log = apply(&skip).unwrap();
        assert!(log.iter().any(|line| line.contains("unparseable")));
    }

    #[test]
    fn apply_managed_file_and_markdown_actions_are_idempotent() {
        let tmp = tempdir().unwrap();
        let managed = tmp.path().join("managed.txt");
        fs::write(&managed, "header\nfrank-managed\n").unwrap();
        let mut remove = InstallPlan::new("test");
        remove.push(Action::RemoveFileIfManaged {
            path: managed.clone(),
            must_contain: "frank-managed".to_string(),
        });
        assert!(apply(&remove).unwrap()[0].contains("removed"));
        assert!(!managed.exists());
        assert!(apply(&remove).unwrap().is_empty());

        let markdown = tmp.path().join("nested").join("AGENTS.md");
        let mut append = InstallPlan::new("test");
        append.push(Action::MarkdownBlockAppend {
            path: markdown.clone(),
            begin: "<!-- begin -->".to_string(),
            end: "<!-- end -->".to_string(),
            body: "rules".to_string(),
            create_if_missing: true,
        });
        assert!(apply(&append).unwrap()[0].contains("updated"));
        assert!(apply(&append).unwrap()[0].contains("already present"));

        let mut remove_block = InstallPlan::new("test");
        remove_block.push(Action::MarkdownBlockRemove {
            path: markdown.clone(),
            begin: "<!-- begin -->".to_string(),
            end: "<!-- end -->".to_string(),
        });
        assert!(apply(&remove_block).unwrap()[0].contains("removed"));
        assert!(!markdown.exists());
        assert!(apply(&remove_block).unwrap().is_empty());

        let absent = tmp.path().join("absent.md");
        let mut skip = InstallPlan::new("test");
        skip.push(Action::MarkdownBlockAppend {
            path: absent,
            begin: "b".into(),
            end: "e".into(),
            body: "body".into(),
            create_if_missing: false,
        });
        assert!(apply(&skip).unwrap()[0].contains("skipped"));
    }

    #[test]
    fn spawn_steps_continue_after_failure_and_noop_is_logged() {
        let mut plan = InstallPlan::new("test");
        plan.push(Action::SpawnSteps {
            steps: vec![
                ResolvedSpawnStep {
                    program: "frank-command-that-does-not-exist".into(),
                    args: vec![],
                },
                ResolvedSpawnStep {
                    program: if cfg!(windows) { "cmd" } else { "false" }.into(),
                    args: if cfg!(windows) {
                        vec!["/C".into(), "exit 1".into()]
                    } else {
                        vec![]
                    },
                },
            ],
        });
        plan.push(Action::Noop {
            reason: "manual".into(),
        });
        let log = apply(&plan).unwrap();
        assert_eq!(log.len(), 3);
        assert!(log[0].contains("failed to run"));
        assert!(log[1].contains("exited") || log[1].contains("failed"));
        assert!(log[2].contains("manual"));
    }
}
