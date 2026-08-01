import { describe, expect, it, beforeEach, afterEach, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import { api } from "./api";
import { open } from "@tauri-apps/plugin-dialog";
import type { DashboardSnapshot } from "./types";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const baseSnapshot: DashboardSnapshot = {
  active_level: "full",
  active_pack: "caveman",
  active_pack_version: "0.1.0",
  default_level: "full",
  settings: { default_level: null, gui: { launch_at_login: false, close_to_tray: true } },
  packs: [
    { id: "caveman", version: "0.1.0", active: true, builtin: true, levels: [{ id: "full", title: null, aliases: [] }] },
    { id: "local", version: "1.0.0", active: false, builtin: false, levels: [{ id: "compact", title: "Compact", aliases: ["c"] }] },
  ],
  targets: [
    { id: "claude-code", label: "Claude Code", kind: "native", verified: true, soft: false, detected: true, source: "built-in" },
    { id: "unverified", label: "Unverified target", kind: "generic", verified: false, soft: true, detected: false, source: "local" },
  ],
  target_errors: [],
  diagnoses: [
    { ok: true, message: "SessionStart hook installed" },
    { ok: false, message: "Settings file is missing" },
  ],
};

const inactiveSnapshot: DashboardSnapshot = {
  ...baseSnapshot,
  active_level: null,
  settings: { ...baseSnapshot.settings, gui: { launch_at_login: false, close_to_tray: false } },
};

function mockSnapshot(snapshot: DashboardSnapshot = baseSnapshot) {
  vi.spyOn(api, "snapshot").mockResolvedValue(snapshot);
}

async function renderReady(snapshot: DashboardSnapshot = baseSnapshot) {
  mockSnapshot(snapshot);
  render(<App />);
  await screen.findByText("CURRENT PERSONA");
}

describe("Frank control panel", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.clearAllMocks();
    vi.useRealTimers();
    window.confirm = vi.fn(() => true);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("renders active overview, refreshes, and turns Frank off", async () => {
    await renderReady();
    expect(screen.getByText("Turn off")).toBeInTheDocument();
    expect(screen.getByText("1 detected")).toBeInTheDocument();
    expect(screen.getByText("1 verified integrations ready.")).toBeInTheDocument();

    const setLevel = vi.spyOn(api, "setLevel").mockResolvedValue(null);
    await userEvent.setup().click(screen.getByRole("button", { name: "Turn off" }));
    expect(setLevel).toHaveBeenCalledWith(null);
  });

  it("turns Frank on from an inactive state and exposes refresh errors", async () => {
    await renderReady(inactiveSnapshot);
    expect(screen.getByText("Frank is off")).toBeInTheDocument();
    const setLevel = vi.spyOn(api, "setLevel").mockResolvedValue("full");
    await userEvent.setup().click(screen.getByRole("button", { name: "Turn on" }));
    expect(setLevel).toHaveBeenCalledWith("full");

    vi.spyOn(api, "snapshot").mockRejectedValueOnce(new Error("backend timeout"));
    await userEvent.setup().click(screen.getByRole("button", { name: "Refresh snapshot" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("backend timeout");
  });

  it("renders a loading failure without hiding the backend error", async () => {
    vi.spyOn(api, "snapshot").mockRejectedValue(new Error("permission denied"));
    render(<App />);
    expect(await screen.findByText(/permission denied/)).toBeInTheDocument();
  });

  it("fails closed when the backend returns a malformed snapshot", async () => {
    vi.spyOn(api, "snapshot").mockResolvedValue(undefined as never);
    render(<App />);
    expect(await screen.findByText(/malformed snapshot/)).toBeInTheDocument();
  });

  it("switches between every page", async () => {
    await renderReady();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Personas" }));
    expect(screen.getByText("PACK STORE")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Integrations" }));
    expect(screen.getByText("Claude Code")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Settings & diagnostics" }));
    expect(screen.getByText("Behavior")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Overview" }));
    expect(screen.getByText("CURRENT PERSONA")).toBeInTheDocument();
  });

  it("adds a selected local pack and activates another pack", async () => {
    await renderReady();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Personas" }));
    const preparePack = vi.spyOn(api, "preparePack")
      .mockResolvedValueOnce({ plan_id: "add-plan", operation: "add", selector: "my-pack@1.0.0", actions: ["install pack my-pack@1.0.0"], expires_in_seconds: 300 })
      .mockResolvedValueOnce({ plan_id: "pack-plan", operation: "use", selector: "local@1.0.0", actions: ["use pack local@1.0.0"], expires_in_seconds: 300 });
    const applyPackPlan = vi.spyOn(api, "applyPackPlan").mockResolvedValue(undefined);
    const refresh = vi.spyOn(api, "snapshot");

    vi.mocked(open).mockResolvedValueOnce(null);
    await user.click(screen.getByRole("button", { name: "Add local pack" }));
    expect(preparePack).not.toHaveBeenCalled();

    vi.mocked(open).mockResolvedValueOnce("/tmp/my pack");
    await user.click(screen.getByRole("button", { name: "Add local pack" }));
    await waitFor(() => expect(preparePack).toHaveBeenCalledWith({ kind: "add", source: "/tmp/my pack" }));
    await waitFor(() => expect(applyPackPlan).toHaveBeenCalledWith("add-plan"));

    await user.click(screen.getByRole("button", { name: "Use pack" }));
    await waitFor(() => expect(preparePack).toHaveBeenCalledWith({ kind: "use", selector: "local@1.0.0" }));
    await waitFor(() => expect(applyPackPlan).toHaveBeenCalledWith("pack-plan"));
    expect(refresh).toHaveBeenCalled();
  });

  it("requires confirmation before removing an installed pack", async () => {
    await renderReady();
    await userEvent.setup().click(screen.getByRole("button", { name: "Personas" }));
    const preparePack = vi.spyOn(api, "preparePack").mockResolvedValue({ plan_id: "remove-plan", operation: "remove", selector: "local@1.0.0", actions: ["remove pack local@1.0.0"], expires_in_seconds: 300 });
    const applyPackPlan = vi.spyOn(api, "applyPackPlan").mockResolvedValue(undefined);
    await userEvent.setup().click(screen.getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(preparePack).toHaveBeenCalledWith({ kind: "remove", selector: "local@1.0.0" }));
    await waitFor(() => expect(applyPackPlan).toHaveBeenCalledWith("remove-plan"));
    expect(window.confirm).toHaveBeenCalledWith(expect.stringContaining("remove pack local@1.0.0"));
  });

  it("previews, confirms, and applies an integration plan", async () => {
    await renderReady();
    await userEvent.setup().click(screen.getByRole("button", { name: "Integrations" }));
    const prepare = vi.spyOn(api, "prepareTarget").mockResolvedValue({ plan_id: "plan-1", target_id: "claude-code", operation: "install", actions: ["write hook"], expires_in_seconds: 300 });
    const apply = vi.spyOn(api, "applyPlan").mockResolvedValue(undefined);
    const refresh = vi.spyOn(api, "snapshot");

    await userEvent.setup().click(screen.getAllByRole("button", { name: "Preview install" })[0]);
    await waitFor(() => expect(prepare).toHaveBeenCalledWith("claude-code", "install"));
    expect(window.confirm).toHaveBeenCalledWith(expect.stringContaining("write hook"));
    await waitFor(() => expect(apply).toHaveBeenCalledWith("plan-1"));
    expect(refresh).toHaveBeenCalled();

    vi.mocked(window.confirm).mockReturnValue(false);
    await userEvent.setup().click(screen.getAllByRole("button", { name: "Uninstall" })[0]);
    await waitFor(() => expect(prepare).toHaveBeenCalledWith("claude-code", "uninstall"));
    expect(apply).toHaveBeenCalledTimes(1);
  });

  it("reports a failed integration preview", async () => {
    await renderReady();
    await userEvent.setup().click(screen.getByRole("button", { name: "Integrations" }));
    vi.spyOn(api, "prepareTarget").mockRejectedValue(new Error("checksum mismatch"));
    await userEvent.setup().click(screen.getAllByRole("button", { name: "Preview install" })[0]);
    expect(await screen.findByRole("alert")).toHaveTextContent("checksum mismatch");
  });

  it("disables unverified integration actions until they are reviewed", async () => {
    await renderReady();
    await userEvent.setup().click(screen.getByRole("button", { name: "Integrations" }));
    const installButtons = screen.getAllByRole("button", { name: "Preview install" });
    expect(installButtons[0]).not.toBeDisabled();
    expect(installButtons[1]).toBeDisabled();
    expect(screen.getAllByRole("button", { name: "Uninstall" })[1]).toBeDisabled();
  });

  it("updates both settings and renders healthy and unhealthy diagnoses", async () => {
    await renderReady();
    await userEvent.setup().click(screen.getByRole("button", { name: "Settings & diagnostics" }));
    expect(screen.getByText("✓ SessionStart hook installed")).toBeInTheDocument();
    expect(screen.getByText("! Settings file is missing")).toBeInTheDocument();
    const setSettings = vi.spyOn(api, "setSettings").mockResolvedValue(baseSnapshot.settings);
    await userEvent.setup().click(screen.getByRole("checkbox", { name: /Launch at login/ }));
    await waitFor(() => expect(setSettings).toHaveBeenCalledWith({ launch_at_login: true }));
    await userEvent.setup().click(screen.getByRole("checkbox", { name: /Close to tray/ }));
    await waitFor(() => expect(setSettings).toHaveBeenCalledWith({ close_to_tray: false }));
  });

  it("updates the default level through the typed settings patch", async () => {
    await renderReady();
    await userEvent.setup().click(screen.getByRole("button", { name: "Settings & diagnostics" }));
    const setSettings = vi.spyOn(api, "setSettings").mockResolvedValue(baseSnapshot.settings);
    await userEvent.setup().selectOptions(screen.getByRole("combobox", { name: "Default level" }), "off");
    await waitFor(() => expect(setSettings).toHaveBeenCalledWith({ default_level: "off" }));
  });

  it("refreshes on the two-second polling interval", async () => {
    vi.useFakeTimers();
    const snapshotCall = vi.spyOn(api, "snapshot").mockResolvedValue(baseSnapshot);
    render(<App />);
    await vi.waitFor(() => expect(snapshotCall).toHaveBeenCalledTimes(1));
    await vi.advanceTimersByTimeAsync(2000);
    expect(snapshotCall).toHaveBeenCalledTimes(2);
  });

  it("keeps the newest snapshot when an older refresh resolves late", async () => {
    let resolveFirst!: (snapshot: DashboardSnapshot) => void;
    let resolveSecond!: (snapshot: DashboardSnapshot) => void;
    const first = new Promise<DashboardSnapshot>((resolve) => { resolveFirst = resolve; });
    const second = new Promise<DashboardSnapshot>((resolve) => { resolveSecond = resolve; });
    const snapshotCall = vi.spyOn(api, "snapshot").mockResolvedValueOnce(baseSnapshot);
    render(<App />);
    await screen.findByText("CURRENT PERSONA");
    snapshotCall.mockReturnValueOnce(first).mockReturnValueOnce(second);
    await userEvent.setup().click(screen.getByRole("button", { name: "Refresh snapshot" }));
    await userEvent.setup().click(screen.getByRole("button", { name: "Refresh snapshot" }));
    resolveSecond(baseSnapshot);
    await waitFor(() => expect(snapshotCall).toHaveBeenCalledTimes(3));
    resolveFirst(inactiveSnapshot);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.getByRole("button", { name: "Turn off" })).toBeInTheDocument();
  });

  it("ignores duplicate integration clicks while a preview is pending", async () => {
    await renderReady();
    await userEvent.setup().click(screen.getByRole("button", { name: "Integrations" }));
    let resolvePrepare!: (preview: { plan_id: string; target_id: string; operation: "install"; actions: string[]; expires_in_seconds: number }) => void;
    const pending = new Promise<{ plan_id: string; target_id: string; operation: "install"; actions: string[]; expires_in_seconds: number }>((resolve) => { resolvePrepare = resolve; });
    const prepare = vi.spyOn(api, "prepareTarget").mockReturnValue(pending);
    const button = screen.getAllByRole("button", { name: "Preview install" })[0];
    await userEvent.setup().click(button);
    await userEvent.setup().click(button);
    expect(prepare).toHaveBeenCalledTimes(1);
    resolvePrepare({ plan_id: "plan-1", target_id: "claude-code", operation: "install", actions: [], expires_in_seconds: 300 });
  });

  it("keeps the typed Tauri bridge calls explicit", async () => {
    const bridge = vi.mocked(invoke).mockResolvedValue(undefined);
    await api.snapshot();
    await api.doctor();
    await api.settings();
    await api.setLevel("full");
    await api.setSettings({ close_to_tray: false });
    await api.prepareTarget("claude-code", "install");
    await api.applyPlan("plan-1");
    await api.addPack("/tmp/pack");
    await api.usePack("caveman@0.1.0");
    await api.removePack("local@1.0.0");
    await api.preparePack({ kind: "use", selector: "local@1.0.0" });
    await api.applyPackPlan("pack-plan");
    expect(bridge).toHaveBeenNthCalledWith(1, "snapshot");
    expect(bridge).toHaveBeenNthCalledWith(2, "doctor");
    expect(bridge).toHaveBeenNthCalledWith(3, "settings");
    expect(bridge).toHaveBeenNthCalledWith(4, "set_active_level", { level: "full" });
    expect(bridge).toHaveBeenNthCalledWith(5, "update_settings", { patch: { close_to_tray: false } });
    expect(bridge).toHaveBeenNthCalledWith(6, "prepare_target_change", { targetId: "claude-code", operation: "install" });
    expect(bridge).toHaveBeenNthCalledWith(7, "apply_prepared_plan", { planId: "plan-1" });
    expect(bridge).toHaveBeenNthCalledWith(8, "add_local_pack", { source: "/tmp/pack" });
    expect(bridge).toHaveBeenNthCalledWith(9, "use_pack", { selector: "caveman@0.1.0" });
    expect(bridge).toHaveBeenNthCalledWith(10, "remove_pack", { selector: "local@1.0.0" });
    expect(bridge).toHaveBeenNthCalledWith(11, "prepare_pack_change", { operation: { kind: "use", selector: "local@1.0.0" } });
    expect(bridge).toHaveBeenNthCalledWith(12, "apply_prepared_pack", { planId: "pack-plan" });
  });
});
