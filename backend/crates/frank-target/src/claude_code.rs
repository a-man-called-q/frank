//! The Claude Code native target: wires `hook session-start` and
//! `hook user-prompt-submit` into `settings.json`, the same "standalone
//! hooks" mechanism the archive falls back to
//! (`archive/bin/install.js:installHooks`) when its Claude Code plugin
//! install doesn't apply. Frank ships as a single binary with no plugin
//! marketplace of its own yet, so this standalone path is the *only*
//! Claude Code integration for now — not a fallback.

use std::path::{Path, PathBuf};

use crate::plan::{Action, Detection, Diagnosis, InstallCtx, InstallPlan, ProbeEnv};
use crate::settings::HookSpec;

pub const SESSION_START_MARKER: &str = "hook session-start";
pub const USER_PROMPT_SUBMIT_MARKER: &str = "hook user-prompt-submit";

pub struct ClaudeCodeTarget;

impl ClaudeCodeTarget {
    pub fn id() -> &'static str {
        "claude-code"
    }

    pub fn detect(env: &ProbeEnv) -> Detection {
        if env.has_command("claude") || env.dir_exists(".claude") {
            Detection::Detected
        } else {
            Detection::NotDetected
        }
    }

    fn settings_path(ctx: &InstallCtx) -> PathBuf {
        ctx.config_dir.join("settings.json")
    }

    fn backup_path(ctx: &InstallCtx) -> PathBuf {
        ctx.config_dir.join("settings.json.frank-backup")
    }

    fn command(ctx: &InstallCtx, subcommand: &str) -> String {
        format!("{} hook {}", quote(&ctx.frank_bin), subcommand)
    }

    pub fn plan_install(ctx: &InstallCtx) -> InstallPlan {
        let mut plan = InstallPlan::scoped(Self::id(), ctx.scope_roots());
        let settings_path = Self::settings_path(ctx);

        plan.push(Action::EnsureDir(ctx.config_dir.clone()));
        plan.push(Action::BackupIfAbsent {
            path: settings_path.clone(),
            backup_path: Self::backup_path(ctx),
        });
        plan.push(Action::MergeSettingsHooks {
            settings_path,
            add: vec![
                HookSpec {
                    event: "SessionStart".to_string(),
                    command: Self::command(ctx, "session-start"),
                    timeout: Some(5),
                    status_message: Some("Loading frank mode...".to_string()),
                    owned_marker: SESSION_START_MARKER.to_string(),
                },
                HookSpec {
                    event: "UserPromptSubmit".to_string(),
                    command: Self::command(ctx, "user-prompt-submit"),
                    timeout: Some(5),
                    status_message: Some("Tracking frank mode...".to_string()),
                    owned_marker: USER_PROMPT_SUBMIT_MARKER.to_string(),
                },
            ],
        });
        plan
    }

    pub fn plan_uninstall(ctx: &InstallCtx) -> InstallPlan {
        let mut plan = InstallPlan::scoped(Self::id(), ctx.scope_roots());
        plan.push(Action::RemoveSettingsHooks {
            settings_path: Self::settings_path(ctx),
            markers: vec![
                SESSION_START_MARKER.to_string(),
                USER_PROMPT_SUBMIT_MARKER.to_string(),
            ],
        });
        plan
    }

    pub fn doctor(ctx: &InstallCtx) -> Vec<Diagnosis> {
        let mut out = Vec::new();
        let settings_path = Self::settings_path(ctx);
        let Some(settings) = crate::settings::read_settings(&settings_path) else {
            out.push(Diagnosis {
                ok: false,
                message: format!("{} exists but could not be parsed", settings_path.display()),
            });
            return out;
        };

        for (event, marker) in [
            ("SessionStart", SESSION_START_MARKER),
            ("UserPromptSubmit", USER_PROMPT_SUBMIT_MARKER),
        ] {
            let present = settings["hooks"][event]
                .as_array()
                .map(|arr| {
                    arr.iter().any(|e| {
                        e["hooks"]
                            .as_array()
                            .map(|hs| {
                                hs.iter().any(|h| {
                                    h["command"].as_str().is_some_and(|c| c.contains(marker))
                                })
                            })
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            out.push(Diagnosis {
                ok: present,
                message: if present {
                    format!("{event} hook installed")
                } else {
                    format!("{event} hook missing — run `frank install --only claude-code`")
                },
            });
        }
        out
    }
}

/// Windows argv quoting is a real concern for M6; for now this only needs
/// to survive a path containing spaces on POSIX shells, which
/// double-quoting handles.
fn quote(path: &Path) -> String {
    format!("\"{}\"", path.display())
}
