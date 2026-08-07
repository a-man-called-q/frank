use std::time::Duration;

use frank_app::{PackOperation, TargetOperation, TargetSummary, UserSettingsPatch};

use crate::backend::Backend;
use crate::message::{Message, TrayMessage};
use crate::model::{Confirm, Model, PendingKey, PlanRef};

/// The exact seven `FrankService` entry points a mutating `Effect::Call` can
/// invoke. Kept separate from `Message` so `reduce` never has to match on
/// "did we already get a response" -- a `Request` is always a thing to go
/// *do*, a `Message` is always a thing that *happened*.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Snapshot {
        generation: u64,
    },
    SetActiveLevel(Option<String>),
    UpdateSettings(UserSettingsPatch),
    PrepareTarget {
        target_id: String,
        operation: TargetOperation,
    },
    ApplyPlan(String),
    PreparePack(PackOperation),
    ApplyPack(String),
}

/// What `reduce` wants the outside world to do. `reduce` itself never
/// touches the filesystem, the tray, or an event loop -- it only describes
/// intent. An `interpret(Effect, &dyn Platform, &impl Backend) ->
/// Task<Message>` (M-3/M-4) turns this into real async work; `perform_request`
/// below is the synchronous, directly-testable core of that mapping for the
/// `Call` case.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    None,
    Batch(Vec<Effect>),
    Call(Request),
    PickDirectory,
    SetAutostart(bool),
    OpenWindow,
    HideWindow,
    UpdateTrayStatus(Option<String>),
    InstallTray,
    Quit,
}

fn refresh(model: &mut Model) -> Effect {
    model.snapshot_generation += 1;
    Effect::Call(Request::Snapshot {
        generation: model.snapshot_generation,
    })
}

fn operation_word(operation: TargetOperation) -> &'static str {
    match operation {
        TargetOperation::Install => "install",
        TargetOperation::Uninstall => "uninstall",
    }
}

/// The core state machine: no iced, no filesystem IO, no async runtime.
/// Every branch here is meant to be trivially unit-testable and, per the
/// plan's mutation-gate discussion, trivially mutation-testable to 100%.
pub fn reduce(model: &mut Model, message: Message) -> Effect {
    match message {
        Message::Navigate(page) => {
            // Deliberately NOT gated on `model.pending` -- the React nav
            // buttons aren't disabled while an action is in flight either,
            // and switching pages mid-action must keep working.
            model.page = page;
            Effect::None
        }

        Message::RefreshRequested => refresh(model),

        Message::DismissError => {
            model.error = None;
            Effect::None
        }

        Message::SnapshotLoaded { generation, result } => {
            if generation != model.snapshot_generation {
                // Stale response, superseded by a newer refresh. Direct
                // port of App.tsx's `latestRequest` guard.
                return Effect::None;
            }
            match result {
                Ok(snapshot) => {
                    let active_level = snapshot.active_level.clone();
                    model.snapshot = Some(snapshot);
                    model.error = None;
                    Effect::UpdateTrayStatus(active_level)
                }
                Err(e) => {
                    model.error = Some(e);
                    Effect::None
                }
            }
        }

        Message::ToggleActive => {
            if model.pending.is_some() {
                return Effect::None;
            }
            let Some(snapshot) = &model.snapshot else {
                return Effect::None;
            };
            let level = if snapshot.active_level.is_some() {
                None
            } else {
                Some(snapshot.default_level.clone())
            };
            model.pending = Some(PendingKey::Toggle);
            Effect::Call(Request::SetActiveLevel(level))
        }

        Message::LevelSet(result) => {
            model.pending = None;
            match result {
                Ok(_) => refresh(model),
                Err(e) => {
                    model.error = Some(e);
                    Effect::None
                }
            }
        }

        Message::AddPackRequested => {
            if model.pending.is_some() {
                return Effect::None;
            }
            model.pending = Some(PendingKey::PackAdd);
            Effect::PickDirectory
        }

        Message::DirectoryPicked(None) => {
            // Cancelled picker: silently clear pending, matching the JS
            // early `return` when `open()` resolves to `null`.
            model.pending = None;
            Effect::None
        }
        Message::DirectoryPicked(Some(path)) => {
            Effect::Call(Request::PreparePack(PackOperation::Add {
                source: path,
                expected_sha256: None,
            }))
        }

        Message::PackUseRequested { selector } => {
            if model.pending.is_some() {
                return Effect::None;
            }
            model.pending = Some(PendingKey::PackUse {
                selector: selector.clone(),
            });
            Effect::Call(Request::PreparePack(PackOperation::Use { selector }))
        }
        Message::PackRemoveRequested { selector } => {
            if model.pending.is_some() {
                return Effect::None;
            }
            model.pending = Some(PendingKey::PackRemove {
                selector: selector.clone(),
            });
            Effect::Call(Request::PreparePack(PackOperation::Remove { selector }))
        }

        Message::PackPrepared(Ok(preview)) => {
            let prompt = format!("{}\n\nApply this pack plan?", preview.actions.join("\n"));
            model.confirm = Some(Confirm {
                prompt,
                actions: preview.actions,
                plan: PlanRef::Pack(preview.plan_id),
            });
            Effect::None
        }
        Message::PackPrepared(Err(e)) => {
            model.pending = None;
            model.error = Some(e);
            Effect::None
        }
        Message::PackApplied(result) => {
            model.pending = None;
            match result {
                Ok(_) => refresh(model),
                Err(e) => {
                    model.error = Some(e);
                    Effect::None
                }
            }
        }

        Message::TargetChangeRequested {
            target_id,
            operation,
        } => {
            if model.pending.is_some() {
                return Effect::None;
            }
            model.pending = Some(PendingKey::Target {
                target_id: target_id.clone(),
                operation,
            });
            Effect::Call(Request::PrepareTarget {
                target_id,
                operation,
            })
        }
        Message::TargetPrepared(Ok(preview)) => {
            let prompt = format!(
                "{}\n\nApply this {} plan?",
                preview.actions.join("\n"),
                operation_word(preview.operation)
            );
            model.confirm = Some(Confirm {
                prompt,
                actions: preview.actions,
                plan: PlanRef::Target(preview.plan_id),
            });
            Effect::None
        }
        Message::TargetPrepared(Err(e)) => {
            model.pending = None;
            model.error = Some(e);
            Effect::None
        }
        Message::TargetApplied(result) => {
            model.pending = None;
            match result {
                Ok(_) => refresh(model),
                Err(e) => {
                    model.error = Some(e);
                    Effect::None
                }
            }
        }

        Message::ConfirmAccepted => {
            let Some(confirm) = model.confirm.take() else {
                return Effect::None;
            };
            match confirm.plan {
                PlanRef::Target(plan_id) => Effect::Call(Request::ApplyPlan(plan_id)),
                PlanRef::Pack(plan_id) => Effect::Call(Request::ApplyPack(plan_id)),
            }
        }
        Message::ConfirmDismissed => {
            // No apply, no refresh -- matches the JS early `return` when
            // `window.confirm` resolves `false`. The prepared plan is left
            // to expire server-side rather than cancelled explicitly.
            model.confirm = None;
            model.pending = None;
            Effect::None
        }

        Message::DefaultLevelSelected(level) => {
            if model.pending.is_some() {
                return Effect::None;
            }
            model.pending = Some(PendingKey::DefaultLevel);
            // The UI never sends an explicit `null` (clear-override); "off"
            // is a real domain sentinel the backend special-cases, not a
            // serialization artifact. See `UserSettingsPatch::default_level`.
            Effect::Call(Request::UpdateSettings(UserSettingsPatch {
                default_level: Some(Some(level)),
                ..Default::default()
            }))
        }
        Message::LaunchAtLoginToggled(enabled) => {
            if model.pending.is_some() {
                return Effect::None;
            }
            model.pending = Some(PendingKey::LaunchAtLogin(enabled));
            Effect::Call(Request::UpdateSettings(UserSettingsPatch {
                launch_at_login: Some(enabled),
                ..Default::default()
            }))
        }
        Message::CloseToTrayToggled(enabled) => {
            if model.pending.is_some() {
                return Effect::None;
            }
            model.pending = Some(PendingKey::CloseToTray);
            Effect::Call(Request::UpdateSettings(UserSettingsPatch {
                close_to_tray: Some(enabled),
                ..Default::default()
            }))
        }
        Message::SettingsSaved(result) => {
            let pending = model.pending.take();
            match result {
                Ok(_) => {
                    let refresh_effect = refresh(model);
                    match pending {
                        Some(PendingKey::LaunchAtLogin(enabled)) => {
                            Effect::Batch(vec![refresh_effect, Effect::SetAutostart(enabled)])
                        }
                        _ => refresh_effect,
                    }
                }
                Err(e) => {
                    model.error = Some(e);
                    Effect::None
                }
            }
        }
        Message::AutostartApplied { enabled, result } => {
            if let Err(e) = result {
                let verb = if enabled { "enabled" } else { "disabled" };
                model.error = Some(format!(
                    "settings saved, but autostart could not be {verb}: {e}"
                ));
            }
            Effect::None
        }
        Message::AutostartStateLoaded(actual) => {
            model.autostart_actual = Some(actual);
            Effect::None
        }

        Message::Tray(TrayMessage::Open) | Message::ShowRequested => {
            model.window_open = true;
            Effect::OpenWindow
        }
        Message::Tray(TrayMessage::Quit) => Effect::Quit,
        Message::Tray(TrayMessage::SetLevel(level)) => Effect::Call(Request::SetActiveLevel(level)),

        Message::RuntimeReady => Effect::InstallTray,

        Message::WindowOpened => {
            model.window_open = true;
            Effect::None
        }
        Message::CloseRequested => {
            let close_to_tray = model
                .snapshot
                .as_ref()
                .map(|s| s.settings.gui.close_to_tray)
                .unwrap_or(true);
            if close_to_tray {
                model.window_open = false;
                Effect::HideWindow
            } else {
                Effect::Quit
            }
        }

        Message::Tick => Effect::None,
    }
}

/// The synchronous core of turning a `Request` into its resulting
/// `Message` by calling the backend and converting `AppError` to `String`
/// at this boundary -- see the doc comment on `Message` for why that
/// conversion exists here rather than being avoidable. `Task::perform`
/// (M-3/M-4) wraps this in an async closure; nothing about the mapping
/// itself is async, which is what makes it unit-testable without a runtime.
pub fn perform_request(backend: &impl Backend, request: Request) -> Message {
    match request {
        Request::Snapshot { generation } => Message::SnapshotLoaded {
            generation,
            result: backend.snapshot().map_err(|e| e.to_string()),
        },
        Request::SetActiveLevel(level) => Message::LevelSet(
            backend
                .set_active_level(level.as_deref())
                .map_err(|e| e.to_string()),
        ),
        Request::UpdateSettings(patch) => {
            Message::SettingsSaved(backend.update_settings(patch).map_err(|e| e.to_string()))
        }
        Request::PrepareTarget {
            target_id,
            operation,
        } => Message::TargetPrepared(
            backend
                .prepare_target_change(&target_id, operation)
                .map_err(|e| e.to_string()),
        ),
        Request::ApplyPlan(plan_id) => Message::TargetApplied(
            backend
                .apply_prepared_plan(&plan_id)
                .map_err(|e| e.to_string()),
        ),
        Request::PreparePack(operation) => Message::PackPrepared(
            backend
                .prepare_pack_change(operation)
                .map_err(|e| e.to_string()),
        ),
        Request::ApplyPack(plan_id) => Message::PackApplied(
            backend
                .apply_prepared_pack(&plan_id)
                .map_err(|e| e.to_string()),
        ),
    }
}

/// `None` while tray-only (no point polling a snapshot nobody is looking
/// at); `Some(2s)` once a window is open. Stricter than the Tauri app,
/// which polls unconditionally even while hidden. The CLI is still a
/// second process that can flip the active flag underneath the GUI, so the
/// poll itself is not removed entirely -- only gated on visibility. See
/// `frank_app`'s `randomized_cli_gui_restart_sequences_never_panic` proptest
/// for why that coexistence has to keep working.
pub fn poll_interval(model: &Model) -> Option<Duration> {
    model.window_open.then_some(Duration::from_secs(2))
}

/// Whether the Integrations page's Preview install/Uninstall buttons should
/// be enabled for a given target. Extracted as a pure predicate because
/// asserting a disabled `iced` button in a headless test is not guaranteed
/// to be possible (see the plan's test-strategy notes on `iced_test`); this
/// function is testable regardless of what the view layer can assert.
pub fn target_actions_enabled(target: &TargetSummary) -> bool {
    target.verified
}

/// The button label to show while a given action owns the pending slot.
pub fn working_label(key: &PendingKey) -> &'static str {
    match key {
        PendingKey::Toggle => "Working…",
        PendingKey::PackAdd => "Adding…",
        PendingKey::DefaultLevel
        | PendingKey::LaunchAtLogin(_)
        | PendingKey::CloseToTray
        | PendingKey::PackUse { .. }
        | PendingKey::PackRemove { .. }
        | PendingKey::Target { .. } => "Preparing…",
    }
}

#[cfg(test)]
// Test fixtures read as `let mut model = Model::default(); model.field =
// ...;` on purpose -- it names which field each test actually cares about,
// which a `Model { field: ..., ..Default::default() }` literal buries.
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::model::Page;
    use frank_app::{
        DashboardSnapshot, DiagnosisView, GuiSettings, PackOperationKind, PackPlanPreview,
        PackSummary, PlanPreview, TargetSummary, UserSettings,
    };

    fn snapshot(active_level: Option<&str>) -> DashboardSnapshot {
        DashboardSnapshot {
            active_level: active_level.map(str::to_string),
            active_pack: "caveman".into(),
            active_pack_version: "1.0.0".into(),
            default_level: "full".into(),
            settings: UserSettings {
                default_level: None,
                gui: GuiSettings {
                    launch_at_login: false,
                    close_to_tray: true,
                },
            },
            packs: vec![PackSummary {
                id: "caveman".into(),
                version: "1.0.0".into(),
                active: true,
                builtin: true,
                levels: vec![],
            }],
            targets: vec![
                target_summary("claude-code", true, true),
                target_summary("codex", false, true),
            ],
            target_errors: vec![],
            diagnoses: vec![DiagnosisView {
                ok: true,
                message: "SessionStart hook installed".into(),
            }],
        }
    }

    fn target_summary(id: &str, verified: bool, detected: bool) -> TargetSummary {
        TargetSummary {
            id: id.into(),
            label: id.into(),
            kind: "generic".into(),
            verified,
            soft: false,
            detected,
            source: "manifest".into(),
        }
    }

    fn model_with_snapshot(active_level: Option<&str>) -> Model {
        let mut model = Model::default();
        model.snapshot = Some(snapshot(active_level));
        model
    }

    // --- Overview: toggle active level ---

    #[test]
    fn toggle_active_turns_off_an_active_level() {
        let mut model = model_with_snapshot(Some("full"));
        let effect = reduce(&mut model, Message::ToggleActive);
        assert_eq!(effect, Effect::Call(Request::SetActiveLevel(None)));
        assert_eq!(model.pending, Some(PendingKey::Toggle));
    }

    #[test]
    fn toggle_active_turns_on_the_default_level_when_inactive() {
        let mut model = model_with_snapshot(None);
        let effect = reduce(&mut model, Message::ToggleActive);
        assert_eq!(
            effect,
            Effect::Call(Request::SetActiveLevel(Some("full".to_string())))
        );
    }

    #[test]
    fn level_set_failure_surfaces_the_error_and_clears_pending() {
        let mut model = model_with_snapshot(Some("full"));
        model.pending = Some(PendingKey::Toggle);
        let effect = reduce(
            &mut model,
            Message::LevelSet(Err("backend timeout".to_string())),
        );
        assert_eq!(effect, Effect::None);
        assert_eq!(model.error.as_deref(), Some("backend timeout"));
        assert_eq!(model.pending, None);
    }

    #[test]
    fn level_set_success_refreshes() {
        let mut model = model_with_snapshot(Some("full"));
        model.pending = Some(PendingKey::Toggle);
        let effect = reduce(&mut model, Message::LevelSet(Ok(None)));
        assert_eq!(model.snapshot_generation, 1);
        assert_eq!(effect, Effect::Call(Request::Snapshot { generation: 1 }));
    }

    // --- Initial load / refresh ---

    #[test]
    fn refresh_requested_bumps_generation_and_calls_snapshot() {
        let mut model = Model::default();
        let effect = reduce(&mut model, Message::RefreshRequested);
        assert_eq!(model.snapshot_generation, 1);
        assert_eq!(effect, Effect::Call(Request::Snapshot { generation: 1 }));
    }

    #[test]
    fn initial_load_rejection_surfaces_the_error() {
        let mut model = Model::default();
        reduce(&mut model, Message::RefreshRequested);
        let effect = reduce(
            &mut model,
            Message::SnapshotLoaded {
                generation: 1,
                result: Err("permission denied".to_string()),
            },
        );
        assert_eq!(effect, Effect::None);
        assert_eq!(model.error.as_deref(), Some("permission denied"));
        assert!(model.snapshot.is_none());
    }

    #[test]
    fn successful_snapshot_updates_tray_status() {
        let mut model = Model::default();
        reduce(&mut model, Message::RefreshRequested);
        let effect = reduce(
            &mut model,
            Message::SnapshotLoaded {
                generation: 1,
                result: Ok(snapshot(Some("full"))),
            },
        );
        assert_eq!(effect, Effect::UpdateTrayStatus(Some("full".to_string())));
        assert!(model.snapshot.is_some());
        assert!(model.error.is_none());
    }

    #[test]
    fn newest_snapshot_wins_when_an_older_refresh_lands_late() {
        let mut model = Model::default();
        reduce(&mut model, Message::RefreshRequested); // generation 1
        reduce(&mut model, Message::RefreshRequested); // generation 2
        // The stale generation-1 response arrives after generation 2 was
        // already requested.
        let effect = reduce(
            &mut model,
            Message::SnapshotLoaded {
                generation: 1,
                result: Ok(snapshot(Some("full"))),
            },
        );
        assert_eq!(effect, Effect::None);
        assert!(model.snapshot.is_none(), "stale response must be dropped");
    }

    // --- Navigation ---

    #[test]
    fn navigation_is_never_gated_on_pending() {
        let mut model = Model::default();
        model.pending = Some(PendingKey::Toggle);
        let effect = reduce(&mut model, Message::Navigate(Page::Settings));
        assert_eq!(model.page, Page::Settings);
        assert_eq!(effect, Effect::None);
    }

    // --- Personas: add / use / remove pack ---

    #[test]
    fn cancelled_directory_picker_does_not_prepare_a_pack() {
        let mut model = Model::default();
        model.pending = Some(PendingKey::PackAdd);
        let effect = reduce(&mut model, Message::DirectoryPicked(None));
        assert_eq!(effect, Effect::None);
        assert_eq!(model.pending, None);
    }

    #[test]
    fn picking_a_directory_prepares_an_add_pack_operation() {
        let mut model = Model::default();
        let path = std::path::PathBuf::from("/tmp/my pack");
        let effect = reduce(&mut model, Message::DirectoryPicked(Some(path.clone())));
        assert_eq!(
            effect,
            Effect::Call(Request::PreparePack(PackOperation::Add {
                source: path,
                expected_sha256: None,
            }))
        );
    }

    #[test]
    fn pack_prepared_opens_a_confirmation_with_the_plan_id() {
        let mut model = Model::default();
        let effect = reduce(
            &mut model,
            Message::PackPrepared(Ok(PackPlanPreview {
                plan_id: "add-plan".into(),
                operation: PackOperationKind::Add,
                selector: "local".into(),
                actions: vec!["install local@1.0.0".into()],
                expires_in_seconds: 300,
            })),
        );
        assert_eq!(effect, Effect::None);
        let confirm = model.confirm.expect("confirm should be set");
        assert_eq!(confirm.plan, PlanRef::Pack("add-plan".into()));
    }

    #[test]
    fn removing_a_pack_asks_for_confirmation_naming_the_pack() {
        let mut model = Model::default();
        reduce(
            &mut model,
            Message::PackRemoveRequested {
                selector: "local@1.0.0".into(),
            },
        );
        reduce(
            &mut model,
            Message::PackPrepared(Ok(PackPlanPreview {
                plan_id: "remove-plan".into(),
                operation: PackOperationKind::Remove,
                selector: "local@1.0.0".into(),
                actions: vec!["remove pack local@1.0.0".into()],
                expires_in_seconds: 300,
            })),
        );
        let confirm = model.confirm.expect("confirm should be set");
        assert!(confirm.prompt.contains("remove pack local@1.0.0"));
    }

    #[test]
    fn confirm_accepted_applies_the_pending_pack_plan() {
        let mut model = Model::default();
        model.confirm = Some(Confirm {
            prompt: "...".into(),
            actions: vec![],
            plan: PlanRef::Pack("add-plan".into()),
        });
        let effect = reduce(&mut model, Message::ConfirmAccepted);
        assert_eq!(
            effect,
            Effect::Call(Request::ApplyPack("add-plan".to_string()))
        );
        assert!(model.confirm.is_none());
    }

    #[test]
    fn confirm_dismissed_does_not_apply_and_clears_pending() {
        let mut model = Model::default();
        model.pending = Some(PendingKey::PackAdd);
        model.confirm = Some(Confirm {
            prompt: "...".into(),
            actions: vec![],
            plan: PlanRef::Pack("add-plan".into()),
        });
        let effect = reduce(&mut model, Message::ConfirmDismissed);
        assert_eq!(effect, Effect::None);
        assert!(model.confirm.is_none());
        assert!(model.pending.is_none());
    }

    // --- Integrations ---

    #[test]
    fn failed_target_prepare_surfaces_the_error() {
        let mut model = Model::default();
        model.pending = Some(PendingKey::Target {
            target_id: "claude-code".into(),
            operation: TargetOperation::Install,
        });
        let effect = reduce(
            &mut model,
            Message::TargetPrepared(Err("checksum mismatch".to_string())),
        );
        assert_eq!(effect, Effect::None);
        assert_eq!(model.error.as_deref(), Some("checksum mismatch"));
        assert!(model.pending.is_none());
    }

    #[test]
    fn target_prepared_confirmation_names_the_operation() {
        let mut model = Model::default();
        reduce(
            &mut model,
            Message::TargetPrepared(Ok(PlanPreview {
                plan_id: "install-plan".into(),
                target_id: "claude-code".into(),
                operation: TargetOperation::Install,
                actions: vec!["write hook".into()],
                expires_in_seconds: 300,
            })),
        );
        let confirm = model.confirm.expect("confirm should be set");
        assert!(confirm.prompt.contains("Apply this install plan?"));
    }

    #[test]
    fn unverified_targets_have_actions_disabled() {
        let unverified = target_summary("codex", false, true);
        let verified = target_summary("claude-code", true, true);
        assert!(!target_actions_enabled(&unverified));
        assert!(target_actions_enabled(&verified));
    }

    #[test]
    fn duplicate_target_change_while_pending_is_a_no_op() {
        let mut model = Model::default();
        let first = reduce(
            &mut model,
            Message::TargetChangeRequested {
                target_id: "claude-code".into(),
                operation: TargetOperation::Install,
            },
        );
        assert!(matches!(first, Effect::Call(Request::PrepareTarget { .. })));
        let second = reduce(
            &mut model,
            Message::TargetChangeRequested {
                target_id: "claude-code".into(),
                operation: TargetOperation::Install,
            },
        );
        assert_eq!(
            second,
            Effect::None,
            "a second click while pending must be a no-op"
        );
    }

    // --- Settings ---

    #[test]
    fn default_level_off_sends_the_off_sentinel_and_nothing_else() {
        let mut model = Model::default();
        let effect = reduce(&mut model, Message::DefaultLevelSelected("off".into()));
        assert_eq!(
            effect,
            Effect::Call(Request::UpdateSettings(UserSettingsPatch {
                default_level: Some(Some("off".to_string())),
                launch_at_login: None,
                close_to_tray: None,
            }))
        );
    }

    #[test]
    fn launch_at_login_toggle_also_requests_os_autostart_after_saving() {
        let mut model = Model::default();
        reduce(&mut model, Message::LaunchAtLoginToggled(true));
        assert_eq!(model.pending, Some(PendingKey::LaunchAtLogin(true)));
        let effect = reduce(
            &mut model,
            Message::SettingsSaved(Ok(UserSettings::default())),
        );
        assert_eq!(
            effect,
            Effect::Batch(vec![
                Effect::Call(Request::Snapshot { generation: 1 }),
                Effect::SetAutostart(true),
            ])
        );
    }

    #[test]
    fn close_to_tray_toggle_does_not_touch_autostart() {
        let mut model = Model::default();
        reduce(&mut model, Message::CloseToTrayToggled(false));
        let effect = reduce(
            &mut model,
            Message::SettingsSaved(Ok(UserSettings::default())),
        );
        assert_eq!(effect, Effect::Call(Request::Snapshot { generation: 1 }));
    }

    #[test]
    fn autostart_failure_reuses_the_original_error_wording() {
        let mut model = Model::default();
        reduce(
            &mut model,
            Message::AutostartApplied {
                enabled: true,
                result: Err("os denied it".to_string()),
            },
        );
        assert_eq!(
            model.error.as_deref(),
            Some("settings saved, but autostart could not be enabled: os denied it")
        );
    }

    // --- Polling ---

    #[test]
    fn polling_is_disabled_while_tray_only() {
        let mut model = Model::default();
        model.window_open = false;
        assert_eq!(poll_interval(&model), None);
    }

    #[test]
    fn polling_is_every_two_seconds_while_a_window_is_open() {
        let mut model = Model::default();
        model.window_open = true;
        assert_eq!(poll_interval(&model), Some(Duration::from_secs(2)));
    }

    // --- Tray / lifecycle ---

    #[test]
    fn tray_open_opens_the_window() {
        let mut model = Model::default();
        let effect = reduce(&mut model, Message::Tray(TrayMessage::Open));
        assert_eq!(effect, Effect::OpenWindow);
        assert!(model.window_open);
    }

    #[test]
    fn close_requested_hides_by_default() {
        let mut model = model_with_snapshot(Some("full"));
        model.window_open = true;
        let effect = reduce(&mut model, Message::CloseRequested);
        assert_eq!(effect, Effect::HideWindow);
        assert!(!model.window_open);
    }

    #[test]
    fn close_requested_quits_when_close_to_tray_is_disabled() {
        let mut model = model_with_snapshot(Some("full"));
        model.snapshot.as_mut().unwrap().settings.gui.close_to_tray = false;
        let effect = reduce(&mut model, Message::CloseRequested);
        assert_eq!(effect, Effect::Quit);
    }

    #[test]
    fn runtime_ready_installs_the_tray() {
        let mut model = Model::default();
        let effect = reduce(&mut model, Message::RuntimeReady);
        assert_eq!(effect, Effect::InstallTray);
    }

    #[test]
    fn tick_is_a_pure_no_op() {
        let mut model = model_with_snapshot(Some("full"));
        let before = model.clone();
        let effect = reduce(&mut model, Message::Tick);
        assert_eq!(effect, Effect::None);
        assert_eq!(model.page, before.page);
        assert_eq!(model.snapshot, before.snapshot);
    }

    // --- Labels ---

    #[test]
    fn working_labels_distinguish_toggle_from_prepared_actions() {
        assert_eq!(working_label(&PendingKey::Toggle), "Working…");
        assert_eq!(working_label(&PendingKey::PackAdd), "Adding…");
        assert_eq!(working_label(&PendingKey::DefaultLevel), "Preparing…");
        assert_eq!(
            working_label(&PendingKey::LaunchAtLogin(true)),
            "Preparing…"
        );
        assert_eq!(working_label(&PendingKey::CloseToTray), "Preparing…");
        assert_eq!(
            working_label(&PendingKey::PackUse {
                selector: "x".into()
            }),
            "Preparing…"
        );
        assert_eq!(
            working_label(&PendingKey::PackRemove {
                selector: "x".into()
            }),
            "Preparing…"
        );
        assert_eq!(
            working_label(&PendingKey::Target {
                target_id: "x".into(),
                operation: TargetOperation::Install,
            }),
            "Preparing…"
        );
    }
}
