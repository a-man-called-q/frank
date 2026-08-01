export type GuiSettings = {
  launch_at_login: boolean;
  close_to_tray: boolean;
};

export type UserSettings = {
  default_level: string | null;
  gui: GuiSettings;
};

/** Patch shape sent through the typed Tauri bridge. An omitted key preserves
 * the existing value; `default_level: null` explicitly clears the override. */
export type UserSettingsPatch = {
  default_level?: string | null;
  launch_at_login?: boolean;
  close_to_tray?: boolean;
};

export type LevelSummary = { id: string; title: string | null; aliases: string[] };
export type PackSummary = { id: string; version: string; active: boolean; builtin: boolean; levels: LevelSummary[] };
export type TargetSummary = { id: string; label: string; kind: string; verified: boolean; soft: boolean; detected: boolean; source: string };
export type Diagnosis = { ok: boolean; message: string };
export type DoctorReport = { ok: boolean; checks: Diagnosis[] };

export type DashboardSnapshot = {
  active_level: string | null;
  active_pack: string;
  active_pack_version: string;
  default_level: string;
  settings: UserSettings;
  packs: PackSummary[];
  targets: TargetSummary[];
  target_errors: string[];
  diagnoses: Diagnosis[];
};

export type TargetOperation = "install" | "uninstall";
export type PlanPreview = { plan_id: string; target_id: string; operation: TargetOperation; actions: string[]; expires_in_seconds: number };
export type PackOperation =
  | { kind: "add"; source: string; expected_sha256?: string | null }
  | { kind: "use"; selector: string }
  | { kind: "remove"; selector: string };
export type PackPlanPreview = { plan_id: string; operation: "add" | "use" | "remove"; selector: string; actions: string[]; expires_in_seconds: number };
