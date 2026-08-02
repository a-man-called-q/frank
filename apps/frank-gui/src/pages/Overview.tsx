import { api } from "../api";
import { Card } from "../components/Card";
import { useAsyncAction } from "../hooks/useAsyncAction";
import type { DashboardSnapshot } from "../types";

export function Overview({
  snapshot,
  refresh,
  onError,
}: {
  snapshot: DashboardSnapshot;
  refresh: () => Promise<void>;
  onError: (reason: unknown) => void;
}) {
  const { pending, run } = useAsyncAction(onError);
  const toggle = () =>
    run("toggle", async () => {
      await api.setLevel(snapshot.active_level ? null : snapshot.default_level);
      await refresh();
    });

  return (
    <div className="stack">
      <section className="hero-card">
        <div>
          <p className="eyebrow">CURRENT PERSONA</p>
          <h2>
            {snapshot.active_pack} <small>v{snapshot.active_pack_version}</small>
          </h2>
          <p>
            {snapshot.active_level
              ? `Level ${snapshot.active_level} is reinforcing every turn.`
              : "Frank is ready when you are."}
          </p>
        </div>
        <button
          className={snapshot.active_level ? "switch on" : "switch"}
          onClick={() => void toggle()}
          aria-pressed={Boolean(snapshot.active_level)}
          aria-busy={Boolean(pending)}
          disabled={Boolean(pending)}
        >
          {pending ? "Working…" : snapshot.active_level ? "Turn off" : "Turn on"}
        </button>
      </section>
      <div className="grid two">
        <Card title="Default level">
          <strong>{snapshot.default_level}</strong>
          <p>Used when Frank is activated without an explicit level.</p>
        </Card>
        <Card title="Integrations">
          <strong>{snapshot.targets.filter((target) => target.detected).length} detected</strong>
          <p>
            {snapshot.targets.filter((target) => target.verified && target.detected).length} verified
            integrations ready.
          </p>
        </Card>
      </div>
    </div>
  );
}
