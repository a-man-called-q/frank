import { describe, expect, it } from "vitest";
import { t } from "./i18n";

describe("translation keys", () => {
  it("returns English MVP copy for every navigation key", () => {
    expect(t("nav.overview")).toBe("Overview");
    expect(t("nav.personas")).toBe("Personas");
    expect(t("nav.integrations")).toBe("Integrations");
    expect(t("nav.settings")).toBe("Settings & diagnostics");
  });
});
