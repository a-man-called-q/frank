use frank_app::{
    AppError, DashboardSnapshot, FrankService, OperationResult, PackOperation, PackOperationResult,
    PackPlanPreview, PlanPreview, TargetOperation, UserSettings, UserSettingsPatch,
};

/// The exact seven `FrankService` entry points the desktop GUI is allowed to
/// call. This is the replacement for the vitest "keeps the typed bridge
/// calls explicit" test: with no IPC boundary, TypeScript's untyped
/// `invoke("command_name", ...)` calls have nothing to mirror, so the
/// contract moves to the type system instead. Drift is a compile error
/// rather than a test that has to be kept in sync by hand.
///
/// Combined with `xtask architecture-check`'s `"frank-gui-core" =>
/// &["frank-app"]` policy, this guarantees the GUI cannot reach past the
/// facade into `frank-target`/`frank-pack`/`frank-safeio` directly.
pub trait Backend {
    fn snapshot(&self) -> Result<DashboardSnapshot, AppError>;
    fn set_active_level(&self, level: Option<&str>) -> Result<Option<String>, AppError>;
    fn update_settings(&self, patch: UserSettingsPatch) -> Result<UserSettings, AppError>;
    fn prepare_target_change(
        &self,
        target_id: &str,
        operation: TargetOperation,
    ) -> Result<PlanPreview, AppError>;
    fn apply_prepared_plan(&self, plan_id: &str) -> Result<OperationResult, AppError>;
    fn prepare_pack_change(&self, operation: PackOperation) -> Result<PackPlanPreview, AppError>;
    fn apply_prepared_pack(&self, plan_id: &str) -> Result<PackOperationResult, AppError>;
}

impl Backend for FrankService {
    fn snapshot(&self) -> Result<DashboardSnapshot, AppError> {
        FrankService::snapshot(self)
    }

    fn set_active_level(&self, level: Option<&str>) -> Result<Option<String>, AppError> {
        FrankService::set_active_level(self, level)
    }

    fn update_settings(&self, patch: UserSettingsPatch) -> Result<UserSettings, AppError> {
        FrankService::update_settings(self, patch)
    }

    fn prepare_target_change(
        &self,
        target_id: &str,
        operation: TargetOperation,
    ) -> Result<PlanPreview, AppError> {
        FrankService::prepare_target_change(self, target_id, operation)
    }

    fn apply_prepared_plan(&self, plan_id: &str) -> Result<OperationResult, AppError> {
        FrankService::apply_prepared_plan(self, plan_id)
    }

    fn prepare_pack_change(&self, operation: PackOperation) -> Result<PackPlanPreview, AppError> {
        FrankService::prepare_pack_change(self, operation)
    }

    fn apply_prepared_pack(&self, plan_id: &str) -> Result<PackOperationResult, AppError> {
        FrankService::apply_prepared_pack(self, plan_id)
    }
}
