import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import { t } from "./i18n";
import type { DashboardSnapshot, TargetOperation } from "./types";

type Page = "overview" | "personas" | "integrations" | "settings";

const labels: Record<Page, string> = {
  overview: t("nav.overview"),
  personas: t("nav.personas"),
  integrations: t("nav.integrations"),
  settings: t("nav.settings"),
};

function isDashboardSnapshot(value: unknown): value is DashboardSnapshot {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<DashboardSnapshot>;
  return typeof candidate.active_pack === "string"
    && typeof candidate.active_pack_version === "string"
    && typeof candidate.default_level === "string"
    && Boolean(candidate.settings && typeof candidate.settings === "object")
    && Array.isArray(candidate.packs)
    && Array.isArray(candidate.targets)
    && Array.isArray(candidate.target_errors)
    && Array.isArray(candidate.diagnoses);
}

function App() {
  const [page, setPage] = useState<Page>("overview");
  const [snapshot, setSnapshot] = useState<DashboardSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const latestRequest = useRef(0);
  const headingRef = useRef<HTMLHeadingElement>(null);

  const refresh = useCallback(async () => {
    const request = ++latestRequest.current;
    try {
      const next = await api.snapshot();
      if (!isDashboardSnapshot(next)) {
        throw new Error("Frank backend returned a malformed snapshot");
      }
      if (request === latestRequest.current) {
        setSnapshot(next);
        setError(null);
      }
    } catch (reason) {
      if (request === latestRequest.current) setError(String(reason));
    }
  }, []);

  const reportError = useCallback((reason: unknown) => setError(String(reason)), []);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 2000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    headingRef.current?.focus();
  }, [page]);

  if (!snapshot) {
    return <main className="shell centered" aria-busy="true"><h1 tabIndex={-1}>{t("app.name")}</h1><p role="status">{error ?? t("app.loading")}</p></main>;
  }

  return (
    <main className="shell">
      <aside className="sidebar" aria-label="Frank navigation">
        <div className="brand"><span className="brand-mark">F</span><div><strong>Frank</strong><span>persona engine</span></div></div>
        <nav>{(Object.keys(labels) as Page[]).map((id) => <button key={id} className={page === id ? "nav-item active" : "nav-item"} aria-current={page === id ? "page" : undefined} onClick={() => setPage(id)}>{labels[id]}</button>)}</nav>
        <div className="sidebar-foot"><span className={snapshot.active_level ? "status-dot on" : "status-dot"} />{snapshot.active_level ? `Active · ${snapshot.active_level}` : "Frank is off"}</div>
      </aside>
      <section className="content">
        <header><div><p className="eyebrow">CONTROL PANEL</p><h1 ref={headingRef} tabIndex={-1}>{labels[page]}</h1></div><button className="ghost" onClick={() => void refresh()} aria-label={t("app.refreshSnapshot")}>{t("app.refresh")}</button></header>
        {error && <div className="alert error" role="alert" aria-live="assertive">{error}</div>}
        {page === "overview" && <Overview snapshot={snapshot} refresh={refresh} onError={reportError} />}
        {page === "personas" && <Personas snapshot={snapshot} refresh={refresh} onError={reportError} />}
        {page === "integrations" && <Integrations snapshot={snapshot} refresh={refresh} onError={reportError} />}
        {page === "settings" && <Settings snapshot={snapshot} refresh={refresh} onError={reportError} />}
      </section>
    </main>
  );
}

function Overview({ snapshot, refresh, onError }: { snapshot: DashboardSnapshot; refresh: () => Promise<void>; onError: (reason: unknown) => void }) {
  const [pending, setPending] = useState(false);
  const toggle = async () => {
    if (pending) return;
    setPending(true);
    try { await api.setLevel(snapshot.active_level ? null : snapshot.default_level); await refresh(); } catch (reason) { onError(reason); } finally { setPending(false); }
  };
  return <div className="stack"><section className="hero-card"><div><p className="eyebrow">CURRENT PERSONA</p><h2>{snapshot.active_pack} <small>v{snapshot.active_pack_version}</small></h2><p>{snapshot.active_level ? `Level ${snapshot.active_level} is reinforcing every turn.` : "Frank is ready when you are."}</p></div><button className={snapshot.active_level ? "switch on" : "switch"} onClick={() => void toggle()} aria-pressed={Boolean(snapshot.active_level)} aria-busy={pending} disabled={pending}>{pending ? "Working…" : snapshot.active_level ? "Turn off" : "Turn on"}</button></section><div className="grid two"><Card title="Default level"><strong>{snapshot.default_level}</strong><p>Used when Frank is activated without an explicit level.</p></Card><Card title="Integrations"><strong>{snapshot.targets.filter(t => t.detected).length} detected</strong><p>{snapshot.targets.filter(t => t.verified && t.detected).length} verified integrations ready.</p></Card></div></div>;
}

function Personas({ snapshot, refresh, onError }: { snapshot: DashboardSnapshot; refresh: () => Promise<void>; onError: (reason: unknown) => void }) {
  const [pending, setPending] = useState<string | null>(null);
  const install = async () => {
    if (pending) return;
    setPending("add");
    try {
      const selected = await open({ directory: true, multiple: false });
      if (typeof selected === "string") {
        const preview = await api.preparePack({ kind: "add", source: selected });
        if (!window.confirm(`${preview.actions.join("\n")}\n\nApply this pack plan?`)) return;
        await api.applyPackPlan(preview.plan_id);
        await refresh();
      }
    } catch (reason) { onError(reason); } finally { setPending(null); }
  };
  const applyPackChange = async (selector: string, operation: "use" | "remove") => {
    if (pending) return;
    setPending(`${operation}:${selector}`);
    try {
      const preview = await api.preparePack({ kind: operation, selector });
      if (!window.confirm(`${preview.actions.join("\n")}\n\nApply this pack plan?`)) return;
      await api.applyPackPlan(preview.plan_id);
      await refresh();
    } catch (reason) { onError(reason); } finally { setPending(null); }
  };
  return <div className="stack"><div className="section-head"><div><p className="eyebrow">PACK STORE</p><p>Choose how Frank speaks for this workspace.</p></div><button className="primary" onClick={() => void install()} disabled={Boolean(pending)}>{pending === "add" ? "Adding…" : "Add local pack"}</button></div><div className="stack">{snapshot.packs.map(pack => <Card key={`${pack.id}@${pack.version}`} title={`${pack.id} · v${pack.version}`} className={pack.active ? "selected" : ""}><div className="row"><div><span className="pill">{pack.builtin ? "Built-in" : "Installed"}</span>{pack.active && <span className="pill green">Active</span>}<p>{pack.levels.length} levels · {pack.levels.map(level => level.id).join(", ")}</p></div><div className="actions">{!pack.active && <button className="ghost" onClick={() => void applyPackChange(`${pack.id}@${pack.version}`, "use")} disabled={Boolean(pending)}>{pending === `use:${pack.id}@${pack.version}` ? "Preparing…" : "Use pack"}</button>}{!pack.builtin && <button className="ghost danger" onClick={() => void applyPackChange(`${pack.id}@${pack.version}`, "remove")} disabled={Boolean(pending)}>{pending === `remove:${pack.id}@${pack.version}` ? "Preparing…" : "Remove"}</button>}</div></div></Card>)}</div></div>;
}

function Integrations({ snapshot, refresh, onError }: { snapshot: DashboardSnapshot; refresh: () => Promise<void>; onError: (reason: unknown) => void }) {
  const [pending, setPending] = useState<string | null>(null);
  const change = async (targetId: string, operation: TargetOperation) => { const key = `${targetId}:${operation}`; if (pending) return; setPending(key); try { const preview = await api.prepareTarget(targetId, operation); if (!window.confirm(`${preview.actions.join("\n")}\n\nApply this ${operation} plan?`)) return; await api.applyPlan(preview.plan_id); await refresh(); } catch (reason) { onError(reason); } finally { setPending(null); } };
  return <div className="stack">{snapshot.targets.map(target => <Card key={target.id} title={target.label}><div className="row"><div><span className={target.detected ? "pill green" : "pill"}>{target.detected ? "Detected" : "Not detected"}</span>{!target.verified && <span className="pill warning">Unverified</span>}<p>{target.kind} · {target.source}</p></div><div className="actions"><button className="ghost" onClick={() => void change(target.id, "install")} disabled={!target.verified || Boolean(pending)} title={!target.verified ? "Unverified targets require manual review" : undefined}>{pending === `${target.id}:install` ? "Preparing…" : "Preview install"}</button><button className="ghost" onClick={() => void change(target.id, "uninstall")} disabled={!target.verified || Boolean(pending)} title={!target.verified ? "Unverified targets require manual review" : undefined}>{pending === `${target.id}:uninstall` ? "Preparing…" : "Uninstall"}</button></div></div></Card>)}</div>;
}

function Settings({ snapshot, refresh, onError }: { snapshot: DashboardSnapshot; refresh: () => Promise<void>; onError: (reason: unknown) => void }) {
  const [pending, setPending] = useState<string | null>(null);
  const update = async (key: "launch_at_login" | "close_to_tray", value: boolean) => { if (pending) return; setPending(key); try { await api.setSettings({ [key]: value }); await refresh(); } catch (reason) { onError(reason); } finally { setPending(null); } };
  const updateDefaultLevel = async (value: string) => { if (pending) return; setPending("default_level"); try { await api.setSettings({ default_level: value }); await refresh(); } catch (reason) { onError(reason); } finally { setPending(null); } };
  const activePack = snapshot.packs.find(pack => pack.active);
  const selectedDefault = snapshot.settings.default_level ?? snapshot.default_level;
  return <div className="stack"><Card title={t("settings.behavior")}><label className="setting"><span><strong>{t("settings.defaultLevel")}</strong><small>{t("settings.defaultLevelHelp")}</small></span><select aria-label={t("settings.defaultLevel")} value={selectedDefault} disabled={Boolean(pending)} onChange={e => void updateDefaultLevel(e.target.value)}><option value="off">{t("settings.off")}</option>{activePack?.levels.map(level => <option key={level.id} value={level.id}>{level.title ? `${level.id} · ${level.title}` : level.id}</option>)}</select></label><label className="setting"><span><strong>Launch at login</strong><small>Start Frank hidden in the tray.</small></span><input type="checkbox" checked={snapshot.settings.gui.launch_at_login} disabled={Boolean(pending)} onChange={e => void update("launch_at_login", e.target.checked)} /></label><label className="setting"><span><strong>Close to tray</strong><small>Keep hooks and quick toggles available when the window closes.</small></span><input type="checkbox" checked={snapshot.settings.gui.close_to_tray} disabled={Boolean(pending)} onChange={e => void update("close_to_tray", e.target.checked)} /></label></Card><Card title="Doctor"><div className="stack small">{snapshot.diagnoses.map(d => <p key={d.message} className={d.ok ? "diagnosis ok" : "diagnosis bad"}>{d.ok ? "✓" : "!"} {d.message}</p>)}{snapshot.target_errors.map(error => <p key={error} className="diagnosis bad">! Target manifest: {error}</p>)}</div></Card></div>;
}

function Card({ title, children, className = "" }: { title: string; children: ReactNode; className?: string }) { return <section className={`card ${className}`}><h3>{title}</h3>{children}</section>; }

export default App;
