import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import { Card } from "../components/Card";
import { useAsyncAction } from "../hooks/useAsyncAction";
import type { DashboardSnapshot } from "../types";

export function Personas({
  snapshot,
  refresh,
  onError,
}: {
  snapshot: DashboardSnapshot;
  refresh: () => Promise<void>;
  onError: (reason: unknown) => void;
}) {
  const { pending, run } = useAsyncAction(onError);

  const install = () =>
    run("add", async () => {
      const selected = await open({ directory: true, multiple: false });
      if (typeof selected !== "string") return;
      const preview = await api.preparePack({ kind: "add", source: selected });
      if (!window.confirm(`${preview.actions.join("\n")}\n\nApply this pack plan?`)) return;
      await api.applyPackPlan(preview.plan_id);
      await refresh();
    });

  const applyPackChange = (selector: string, operation: "use" | "remove") =>
    run(`${operation}:${selector}`, async () => {
      const preview = await api.preparePack({ kind: operation, selector });
      if (!window.confirm(`${preview.actions.join("\n")}\n\nApply this pack plan?`)) return;
      await api.applyPackPlan(preview.plan_id);
      await refresh();
    });

  return (
    <div className="stack">
      <div className="section-head">
        <div>
          <p className="eyebrow">PACK STORE</p>
          <p>Choose how Frank speaks for this workspace.</p>
        </div>
        <button className="primary" onClick={() => void install()} disabled={Boolean(pending)}>
          {pending === "add" ? "Adding…" : "Add local pack"}
        </button>
      </div>
      <div className="stack">
        {snapshot.packs.map((pack) => (
          <Card
            key={`${pack.id}@${pack.version}`}
            title={`${pack.id} · v${pack.version}`}
            className={pack.active ? "selected" : ""}
          >
            <div className="row">
              <div>
                <span className="pill">{pack.builtin ? "Built-in" : "Installed"}</span>
                {pack.active && <span className="pill green">Active</span>}
                <p>
                  {pack.levels.length} levels · {pack.levels.map((level) => level.id).join(", ")}
                </p>
              </div>
              <div className="actions">
                {!pack.active && (
                  <button
                    className="ghost"
                    onClick={() => void applyPackChange(`${pack.id}@${pack.version}`, "use")}
                    disabled={Boolean(pending)}
                  >
                    {pending === `use:${pack.id}@${pack.version}` ? "Preparing…" : "Use pack"}
                  </button>
                )}
                {!pack.builtin && (
                  <button
                    className="ghost danger"
                    onClick={() => void applyPackChange(`${pack.id}@${pack.version}`, "remove")}
                    disabled={Boolean(pending)}
                  >
                    {pending === `remove:${pack.id}@${pack.version}` ? "Preparing…" : "Remove"}
                  </button>
                )}
              </div>
            </div>
          </Card>
        ))}
      </div>
    </div>
  );
}
