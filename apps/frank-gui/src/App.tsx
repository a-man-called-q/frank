import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "./api";
import { t } from "./i18n";
import { Integrations } from "./pages/Integrations";
import { Overview } from "./pages/Overview";
import { Personas } from "./pages/Personas";
import { Settings } from "./pages/Settings";
import type { DashboardSnapshot } from "./types";

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

export default App;
