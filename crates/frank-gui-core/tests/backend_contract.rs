//! Replaces the vitest "keeps the typed Tauri bridge calls explicit" test
//! (`apps/frank-gui/src/App.test.tsx`), which asserted the exact ordered
//! sequence of `invoke(...)` calls by ordinal since TypeScript could not
//! enforce anything across the untyped IPC boundary. With no IPC boundary
//! that specific test technique is gone, but the property it protected --
//! "the GUI drives the backend through exactly these calls, in this order,
//! and apply never precedes prepare" -- still matters, so it moves here as
//! a `RecordingBackend` that records every `Request` it receives.

use std::cell::RefCell;
use std::path::PathBuf;

use frank_app::{
    AppError, DashboardSnapshot, DiagnosisView, GuiSettings, OperationResult, PackOperation,
    PackOperationKind, PackOperationResult, PackPlanPreview, PackSummary, PlanPreview,
    TargetOperation, UserSettings, UserSettingsPatch,
};
use frank_gui_core::{Backend, Effect, Message, Model, Request, perform_request, reduce};

#[derive(Default)]
struct RecordingBackend {
    calls: RefCell<Vec<Request>>,
}

fn fixture_snapshot() -> DashboardSnapshot {
    DashboardSnapshot {
        active_level: Some("full".into()),
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
        targets: vec![],
        target_errors: vec![],
        diagnoses: vec![DiagnosisView {
            ok: true,
            message: "ok".into(),
        }],
    }
}

impl Backend for RecordingBackend {
    fn snapshot(&self) -> Result<DashboardSnapshot, AppError> {
        self.calls
            .borrow_mut()
            .push(Request::Snapshot { generation: 0 });
        Ok(fixture_snapshot())
    }

    fn set_active_level(&self, level: Option<&str>) -> Result<Option<String>, AppError> {
        self.calls
            .borrow_mut()
            .push(Request::SetActiveLevel(level.map(str::to_string)));
        Ok(level.map(str::to_string))
    }

    fn update_settings(&self, patch: UserSettingsPatch) -> Result<UserSettings, AppError> {
        self.calls.borrow_mut().push(Request::UpdateSettings(patch));
        Ok(UserSettings::default())
    }

    fn prepare_target_change(
        &self,
        target_id: &str,
        operation: TargetOperation,
    ) -> Result<PlanPreview, AppError> {
        self.calls.borrow_mut().push(Request::PrepareTarget {
            target_id: target_id.to_string(),
            operation,
        });
        Ok(PlanPreview {
            plan_id: "plan-1".into(),
            target_id: target_id.to_string(),
            operation,
            actions: vec!["do the thing".into()],
            expires_in_seconds: 300,
        })
    }

    fn apply_prepared_plan(&self, plan_id: &str) -> Result<OperationResult, AppError> {
        self.calls
            .borrow_mut()
            .push(Request::ApplyPlan(plan_id.to_string()));
        Ok(OperationResult {
            target_id: "claude-code".into(),
            log: vec![],
        })
    }

    fn prepare_pack_change(&self, operation: PackOperation) -> Result<PackPlanPreview, AppError> {
        self.calls
            .borrow_mut()
            .push(Request::PreparePack(operation.clone()));
        let selector = match &operation {
            PackOperation::Add { .. } => "local".to_string(),
            PackOperation::Use { selector } | PackOperation::Remove { selector } => {
                selector.clone()
            }
        };
        Ok(PackPlanPreview {
            plan_id: "pack-plan-1".into(),
            operation: PackOperationKind::Add,
            selector,
            actions: vec!["install local@1.0.0".into()],
            expires_in_seconds: 300,
        })
    }

    fn apply_prepared_pack(&self, plan_id: &str) -> Result<PackOperationResult, AppError> {
        self.calls
            .borrow_mut()
            .push(Request::ApplyPack(plan_id.to_string()));
        Ok(PackOperationResult {
            operation: PackOperationKind::Add,
            selector: "local".into(),
            pack: None,
        })
    }
}

/// Drives `reduce` -> `perform_request` -> `reduce` until no more `Call`
/// effects are produced by the given starting message, recording every
/// `Request` the backend actually saw, in order.
fn drive(model: &mut Model, backend: &RecordingBackend, start: Message) {
    let mut pending = vec![reduce(model, start)];
    while let Some(effect) = pending.pop() {
        match effect {
            Effect::Call(request) => {
                let response = perform_request(backend, request);
                pending.push(reduce(model, response));
            }
            Effect::Batch(effects) => pending.extend(effects),
            _ => {}
        }
    }
}

#[test]
fn full_target_install_flow_hits_the_backend_in_prepare_then_apply_order() {
    let backend = RecordingBackend::default();
    let mut model = Model::default();

    drive(&mut model, &backend, Message::RefreshRequested);
    drive(
        &mut model,
        &backend,
        Message::TargetChangeRequested {
            target_id: "claude-code".into(),
            operation: TargetOperation::Install,
        },
    );
    // Prepare landed as a confirmation, not an immediate apply.
    assert!(model.confirm.is_some());
    drive(&mut model, &backend, Message::ConfirmAccepted);

    assert_eq!(
        *backend.calls.borrow(),
        vec![
            Request::Snapshot { generation: 0 },
            Request::PrepareTarget {
                target_id: "claude-code".into(),
                operation: TargetOperation::Install,
            },
            Request::ApplyPlan("plan-1".into()),
            // Applying refreshes.
            Request::Snapshot { generation: 0 },
        ]
    );
}

#[test]
fn dismissing_the_confirmation_never_calls_apply() {
    let backend = RecordingBackend::default();
    let mut model = Model::default();

    drive(
        &mut model,
        &backend,
        Message::TargetChangeRequested {
            target_id: "claude-code".into(),
            operation: TargetOperation::Uninstall,
        },
    );
    drive(&mut model, &backend, Message::ConfirmDismissed);

    let calls = backend.calls.borrow();
    assert_eq!(calls.len(), 1, "only the prepare call should have happened");
    assert!(matches!(calls[0], Request::PrepareTarget { .. }));
    assert!(model.pending.is_none());
}

#[test]
fn add_pack_flow_matches_the_directory_picker_source() {
    let backend = RecordingBackend::default();
    let mut model = Model::default();

    drive(&mut model, &backend, Message::AddPackRequested);
    drive(
        &mut model,
        &backend,
        Message::DirectoryPicked(Some(PathBuf::from("/tmp/my pack"))),
    );
    drive(&mut model, &backend, Message::ConfirmAccepted);

    let calls = backend.calls.borrow();
    assert_eq!(
        calls[0],
        Request::PreparePack(PackOperation::Add {
            source: PathBuf::from("/tmp/my pack"),
            expected_sha256: None,
        })
    );
    assert_eq!(calls[1], Request::ApplyPack("pack-plan-1".into()));
}

#[test]
fn toggle_active_round_trips_through_set_active_level() {
    let backend = RecordingBackend::default();
    let mut model = Model {
        snapshot: Some(fixture_snapshot()),
        ..Model::default()
    };

    drive(&mut model, &backend, Message::ToggleActive);

    assert_eq!(
        *backend.calls.borrow(),
        vec![
            Request::SetActiveLevel(None),
            Request::Snapshot { generation: 0 },
        ],
        "an already-active snapshot should turn off, not on, then refresh"
    );
    assert!(model.pending.is_none());
}

#[test]
fn settings_change_round_trips_through_update_settings() {
    let backend = RecordingBackend::default();
    let mut model = Model::default();

    drive(
        &mut model,
        &backend,
        Message::DefaultLevelSelected("off".into()),
    );

    assert_eq!(
        *backend.calls.borrow(),
        vec![
            Request::UpdateSettings(UserSettingsPatch {
                default_level: Some(Some("off".to_string())),
                launch_at_login: None,
                close_to_tray: None,
            }),
            Request::Snapshot { generation: 0 },
        ]
    );
}
