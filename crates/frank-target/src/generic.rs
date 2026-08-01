//! Converts a declarative [`TargetManifest`] into an [`InstallPlan`] —
//! the same `Action` list a hand-built [`crate::claude_code::ClaudeCodeTarget`]
//! produces, just assembled from manifest data instead of Rust code. This
//! is what makes adding a new `npx skills add`-style or `AGENTS.md`-style
//! agent a TOML file, not a PR against this crate.

use crate::manifest::{InstallSpec, TargetManifest};
use crate::plan::{Action, InstallCtx, InstallPlan, ResolvedSpawnStep};
use crate::settings::HookSpec;

/// `resolve_body` turns a manifest's `body` reference (e.g.
/// `"pack:static_digest"`) into literal text. This crate doesn't depend on
/// `frank-pack` for compiled prompt content, so the caller (which does)
/// supplies the resolver.
pub fn build_install_plan(
    manifest: &TargetManifest,
    ctx: &InstallCtx,
    resolve_body: impl Fn(&str) -> Option<String>,
) -> InstallPlan {
    let mut plan = InstallPlan::new(manifest.target.id.clone());

    match &manifest.install {
        InstallSpec::Spawn { steps, .. } => {
            let resolved = steps
                .iter()
                .map(|s| ResolvedSpawnStep {
                    program: s.program.clone(),
                    args: s.args.iter().map(|a| ctx.expand_template(a)).collect(),
                })
                .collect();
            plan.push(Action::SpawnSteps { steps: resolved });
        }
        InstallSpec::MarkdownBlock { markdown } => {
            let path = ctx.resolve_path(&markdown.path);
            match resolve_body(&markdown.body) {
                Some(body) => plan.push(Action::MarkdownBlockAppend {
                    path,
                    begin: markdown.begin.clone(),
                    end: markdown.end.clone(),
                    body,
                    create_if_missing: markdown.create_if_missing,
                }),
                None => plan.push(Action::Noop {
                    reason: format!("could not resolve body reference '{}'", markdown.body),
                }),
            }
        }
        InstallSpec::SettingsMerge { settings } => {
            let path = ctx.resolve_path(&settings.path);
            let hooks = settings
                .hooks
                .iter()
                .map(|h| HookSpec {
                    event: h.event.clone(),
                    command: ctx.expand_template(&h.command),
                    timeout: h.timeout,
                    status_message: None,
                    owned_marker: h.owned_marker.clone(),
                })
                .collect();
            plan.push(Action::MergeSettingsHooks { settings_path: path, add: hooks });
        }
        InstallSpec::Files { .. } => {
            // No concrete manifest needs this yet — the `files` strategy
            // is schema-complete but not wired to an executor. Reporting
            // a clear no-op beats silently doing nothing.
            plan.push(Action::Noop { reason: "'files' install strategy not yet implemented".to_string() });
        }
    }

    plan
}

pub fn build_uninstall_plan(manifest: &TargetManifest, ctx: &InstallCtx) -> InstallPlan {
    let mut plan = InstallPlan::new(manifest.target.id.clone());
    match &manifest.install {
        InstallSpec::Spawn { uninstall, .. } => match uninstall {
            Some(step) => plan.push(Action::SpawnSteps {
                steps: vec![ResolvedSpawnStep {
                    program: step.program.clone(),
                    args: step.args.iter().map(|a| ctx.expand_template(a)).collect(),
                }],
            }),
            // Matches the archive's own honesty here: a generic
            // `npx skills add`-installed agent has no automated uninstall
            // either — it told users to remove those by hand.
            None => plan.push(Action::Noop {
                reason: format!(
                    "no automated uninstall for '{}' — installed via an external tool, remove manually",
                    manifest.target.id
                ),
            }),
        },
        InstallSpec::MarkdownBlock { markdown } => {
            let path = ctx.resolve_path(&markdown.path);
            plan.push(Action::MarkdownBlockRemove { path, begin: markdown.begin.clone(), end: markdown.end.clone() });
        }
        InstallSpec::SettingsMerge { settings } => {
            let path = ctx.resolve_path(&settings.path);
            let markers = settings.hooks.iter().map(|h| h.owned_marker.clone()).collect();
            plan.push(Action::RemoveSettingsHooks { settings_path: path, markers });
        }
        InstallSpec::Files { .. } => {
            plan.push(Action::Noop { reason: "'files' install strategy not yet implemented".to_string() });
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::*;
    use std::path::PathBuf;

    fn ctx() -> InstallCtx {
        InstallCtx {
            config_dir: PathBuf::from("/home/user/.claude"),
            frank_bin: PathBuf::from("/usr/local/bin/frank"),
            cwd: PathBuf::from("/repo"),
        }
    }

    #[test]
    fn spawn_strategy_expands_frank_bin_template() {
        let manifest = TargetManifest {
            schema: 1,
            target: TargetMeta { id: "x".into(), label: "X".into(), kind: "generic".into(), verified: true, soft: false },
            detect: vec![],
            install: InstallSpec::Spawn {
                steps: vec![SpawnStep {
                    program: "echo".into(),
                    args: vec!["{frank_bin}".into()],
                    win_shell: false,
                    success: "status_zero".into(),
                }],
                uninstall: None,
            },
        };
        let plan = build_install_plan(&manifest, &ctx(), |_| None);
        let Action::SpawnSteps { steps } = &plan.actions[0] else { panic!() };
        assert_eq!(steps[0].args[0], "/usr/local/bin/frank");
    }

    #[test]
    fn markdown_block_resolves_project_relative_path() {
        let manifest = TargetManifest {
            schema: 1,
            target: TargetMeta { id: "x".into(), label: "X".into(), kind: "generic".into(), verified: true, soft: false },
            detect: vec![],
            install: InstallSpec::MarkdownBlock {
                markdown: MarkdownBlockSpec {
                    path: "./AGENTS.md".into(),
                    begin: "<!-- b -->".into(),
                    end: "<!-- e -->".into(),
                    body: "pack:static_digest".into(),
                    create_if_missing: true,
                },
            },
        };
        let plan = build_install_plan(&manifest, &ctx(), |r| (r == "pack:static_digest").then(|| "hi".to_string()));
        let Action::MarkdownBlockAppend { path, body, .. } = &plan.actions[0] else { panic!() };
        assert_eq!(path, &PathBuf::from("/repo/AGENTS.md"));
        assert_eq!(body, "hi");
    }

    #[test]
    fn uninstall_without_a_configured_step_reports_manual_removal() {
        let manifest = TargetManifest {
            schema: 1,
            target: TargetMeta { id: "goose".into(), label: "Goose".into(), kind: "generic".into(), verified: false, soft: false },
            detect: vec![],
            install: InstallSpec::Spawn {
                steps: vec![SpawnStep { program: "npx".into(), args: vec![], win_shell: false, success: "status_zero".into() }],
                uninstall: None,
            },
        };
        let plan = build_uninstall_plan(&manifest, &ctx());
        let Action::Noop { reason } = &plan.actions[0] else { panic!() };
        assert!(reason.contains("manually"));
    }
}
