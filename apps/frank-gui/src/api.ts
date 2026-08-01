import { invoke } from "@tauri-apps/api/core";
import type { DashboardSnapshot, DoctorReport, PackOperation, PackPlanPreview, PlanPreview, TargetOperation, UserSettings, UserSettingsPatch } from "./types";

export const api = {
  snapshot: () => invoke<DashboardSnapshot>("snapshot"),
  doctor: () => invoke<DoctorReport>("doctor"),
  settings: () => invoke<UserSettings>("settings"),
  setLevel: (level: string | null) => invoke<string | null>("set_active_level", { level }),
  setSettings: (patch: UserSettingsPatch) => invoke<UserSettings>("update_settings", { patch }),
  prepareTarget: (targetId: string, operation: TargetOperation) => invoke<PlanPreview>("prepare_target_change", { targetId, operation }),
  applyPlan: (planId: string) => invoke("apply_prepared_plan", { planId }),
  addPack: (source: string) => invoke("add_local_pack", { source }),
  usePack: (selector: string) => invoke("use_pack", { selector }),
  removePack: (selector: string) => invoke("remove_pack", { selector }),
  preparePack: (operation: PackOperation) => invoke<PackPlanPreview>("prepare_pack_change", { operation }),
  applyPackPlan: (planId: string) => invoke("apply_prepared_pack", { planId }),
};
