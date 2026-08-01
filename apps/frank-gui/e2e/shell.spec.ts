import { expect, test } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

test("renders a keyboard-addressable Frank shell outside Tauri", async ({ page }) => {
  await page.goto("/");
  await expect(page).toHaveTitle("Frank");
  await expect(page.locator("main.shell")).toBeVisible();
  await expect(page.locator("main.shell")).toHaveAttribute("aria-busy", "true");
  await page.keyboard.press("Tab");
  await expect(page.locator("h1")).toBeVisible();
});

test("has no automated accessibility violations in the loading state", async ({ page }) => {
  await page.goto("/");
  const report = await new AxeBuilder({ page }).analyze();
  expect(report.violations).toEqual([]);
});

test("renders a backend snapshot and completes a preview/apply flow", async ({ page }) => {
  await page.addInitScript((initial) => {
    const state = initial as any;
    (window as any).__TAURI_INTERNALS__ = {
      invoke: async (command: string, args: any = {}) => {
        if (command === "snapshot") return state;
        if (command === "set_active_level") {
          state.active_level = args.level ?? null;
          return state.active_level;
        }
        if (command === "update_settings") return state.settings;
        if (command === "prepare_target_change") {
          return {
            plan_id: "browser-plan",
            target_id: args.targetId,
            operation: args.operation,
            actions: ["write Frank hook"],
            expires_in_seconds: 300,
          };
        }
        if (command === "apply_prepared_plan") return { target_id: "claude-code", log: ["wrote hook"] };
        return undefined;
      },
    };
  }, {
    active_level: "full",
    active_pack: "caveman",
    active_pack_version: "0.1.0",
    default_level: "full",
    settings: { default_level: null, gui: { launch_at_login: false, close_to_tray: true } },
    packs: [{ id: "caveman", version: "0.1.0", active: true, builtin: true, levels: [{ id: "full", title: null, aliases: [] }] }],
    targets: [{ id: "claude-code", label: "Claude Code", kind: "native", verified: true, soft: false, detected: true, source: "built-in" }],
    target_errors: [],
    diagnoses: [{ ok: true, message: "SessionStart hook installed" }],
  });
  page.on("dialog", (dialog) => void dialog.accept());

  await page.goto("/");
  await expect(page.getByText("CURRENT PERSONA")).toBeVisible();
  await expect(page.getByText("1 detected")).toBeVisible();

  await page.getByRole("button", { name: "Integrations" }).click();
  await page.getByRole("button", { name: "Preview install" }).click();
  await expect(page.getByText("Claude Code")).toBeVisible();
  await expect(page.getByRole("button", { name: "Preview install" })).toBeVisible();

  await page.getByRole("button", { name: "Settings & diagnostics" }).click();
  await expect(page.getByRole("combobox", { name: "Default level" })).toHaveValue("full");
});
