import { api } from "../api";
import { Card } from "../components/Card";
import { useAsyncAction } from "../hooks/useAsyncAction";
import type { DashboardSnapshot, TargetOperation } from "../types";

export function Integrations({
  snapshot,
  refresh,
  onError,
}: {
  snapshot: DashboardSnapshot;
  refresh: () => Promise<void>;
  onError: (reason: unknown) => void;
}) {
  const { pending, run } = useAsyncAction(onError);
  const change = (targetId: string, operation: TargetOperation) =>
    run(`${targetId}:${operation}`, async () => {
      const preview = await api.prepareTarget(targetId, operation);
      if (!window.confirm(`${preview.actions.join("\n")}\n\nApply this ${operation} plan?`)) return;
      await api.applyPlan(preview.plan_id);
      await refresh();
    });

  return (
    <div className="stack">
      {snapshot.targets.map((target) => (
        <Card key={target.id} title={target.label}>
          <div className="row">
            <div>
              <span className={target.detected ? "pill green" : "pill"}>
                {target.detected ? "Detected" : "Not detected"}
              </span>
              {!target.verified && <span className="pill warning">Unverified</span>}
              <p>
                {target.kind} · {target.source}
              </p>
            </div>
            <div className="actions">
              {(["install", "uninstall"] as const).map((operation) => (
                <button
                  key={operation}
                  className="ghost"
                  onClick={() => void change(target.id, operation)}
                  disabled={!target.verified || Boolean(pending)}
                  title={!target.verified ? "Unverified targets require manual review" : undefined}
                >
                  {pending === `${target.id}:${operation}`
                    ? "Preparing…"
                    : operation === "install"
                      ? "Preview install"
                      : "Uninstall"}
                </button>
              ))}
            </div>
          </div>
        </Card>
      ))}
    </div>
  );
}
