import { api } from "../api";
import { Card } from "../components/Card";
import { useAsyncAction } from "../hooks/useAsyncAction";
import { t } from "../i18n";
import type { DashboardSnapshot } from "../types";

export function Settings({
  snapshot,
  refresh,
  onError,
}: {
  snapshot: DashboardSnapshot;
  refresh: () => Promise<void>;
  onError: (reason: unknown) => void;
}) {
  const { pending, run } = useAsyncAction(onError);
  const update = (key: "launch_at_login" | "close_to_tray", value: boolean) =>
    run(key, async () => {
      await api.setSettings({ [key]: value });
      await refresh();
    });
  const updateDefaultLevel = (value: string) =>
    run("default_level", async () => {
      await api.setSettings({ default_level: value });
      await refresh();
    });
  const activePack = snapshot.packs.find((pack) => pack.active);
  const selectedDefault = snapshot.settings.default_level ?? snapshot.default_level;

  return (
    <div className="stack">
      <Card title={t("settings.behavior")}>
        <label className="setting">
          <span>
            <strong>{t("settings.defaultLevel")}</strong>
            <small>{t("settings.defaultLevelHelp")}</small>
          </span>
          <select
            aria-label={t("settings.defaultLevel")}
            value={selectedDefault}
            disabled={Boolean(pending)}
            onChange={(event) => void updateDefaultLevel(event.target.value)}
          >
            <option value="off">{t("settings.off")}</option>
            {activePack?.levels.map((level) => (
              <option key={level.id} value={level.id}>
                {level.title ? `${level.id} · ${level.title}` : level.id}
              </option>
            ))}
          </select>
        </label>
        <label className="setting">
          <span>
            <strong>Launch at login</strong>
            <small>Start Frank hidden in the tray.</small>
          </span>
          <input
            type="checkbox"
            checked={snapshot.settings.gui.launch_at_login}
            disabled={Boolean(pending)}
            onChange={(event) => void update("launch_at_login", event.target.checked)}
          />
        </label>
        <label className="setting">
          <span>
            <strong>Close to tray</strong>
            <small>Keep hooks and quick toggles available when the window closes.</small>
          </span>
          <input
            type="checkbox"
            checked={snapshot.settings.gui.close_to_tray}
            disabled={Boolean(pending)}
            onChange={(event) => void update("close_to_tray", event.target.checked)}
          />
        </label>
      </Card>
      <Card title="Doctor">
        <div className="stack small">
          {snapshot.diagnoses.map((diagnosis) => (
            <p key={diagnosis.message} className={diagnosis.ok ? "diagnosis ok" : "diagnosis bad"}>
              {diagnosis.ok ? "✓" : "!"} {diagnosis.message}
            </p>
          ))}
          {snapshot.target_errors.map((error) => (
            <p key={error} className="diagnosis bad">
              ! Target manifest: {error}
            </p>
          ))}
        </div>
      </Card>
    </div>
  );
}
