import { AlertTriangle, Lock, RefreshCw, ShieldCheck, Trash2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { formatBytes, formatUnixTs } from "../lib/format";
import {
  type CleanCandidate,
  type CleanCategory,
  type CleanupCandidatesReport,
  type CleanupMode,
  type CleanupTargetInput,
  api,
} from "../lib/ipc";

interface CleanerViewProps {
  isElevated: boolean;
  onRelaunchAdmin: () => void;
  onStartPreview: (targets: CleanupTargetInput[], mode: CleanupMode) => void;
}

const CATEGORY_TITLES: Record<CleanCategory, string> = {
  user_temp: "Temporales de usuario",
  system_temp: "Temporales del sistema",
  recycle_bin: "Papelera de reciclaje",
  browser_cache: "Cachés de navegadores",
  dev_artifact: "Artefactos y cachés de desarrollo",
  large_old_file: "Archivos grandes y antiguos",
};

export function CleanerView({ isElevated, onRelaunchAdmin, onStartPreview }: CleanerViewProps) {
  const [report, setReport] = useState<CleanupCandidatesReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [minLargeMb, setMinLargeMb] = useState(100);
  const [minAgeDays, setMinAgeDays] = useState(30);

  const loadCandidates = useCallback(() => {
    setLoading(true);
    api
      .discoverCleanupCandidates(minLargeMb, minAgeDays)
      .then((res) => {
        setReport(res);
        setSelectedIds(new Set());
      })
      .finally(() => setLoading(false));
  }, [minLargeMb, minAgeDays]);

  useEffect(() => {
    loadCandidates();
  }, [loadCandidates]);

  const toggleItem = (item: CleanCandidate) => {
    if (item.requiresAdmin && !isElevated) return;
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(item.id)) next.delete(item.id);
      else next.add(item.id);
      return next;
    });
  };

  const selectedCandidates = useMemo(
    () => (report?.candidates ?? []).filter((c) => selectedIds.has(c.id)),
    [report, selectedIds],
  );

  const selectedBytes = useMemo(
    () => selectedCandidates.reduce((acc, c) => acc + c.sizeBytes, 0),
    [selectedCandidates],
  );

  const grouped = useMemo(() => {
    const map = new Map<CleanCategory, CleanCandidate[]>();
    for (const c of report?.candidates ?? []) {
      const list = map.get(c.category) ?? [];
      list.push(c);
      map.set(c.category, list);
    }
    return map;
  }, [report]);

  const handlePreview = () => {
    const targets: CleanupTargetInput[] = selectedCandidates.map((c) => ({
      path: c.path,
      category: c.category,
      isTempDirContents: c.category === "user_temp" || c.category === "system_temp",
    }));
    onStartPreview(targets, "recycle_bin");
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          flexWrap: "wrap",
          gap: 12,
        }}
      >
        <div>
          <h2 style={{ margin: 0, fontSize: 20 }}>Limpieza segura</h2>
          <p style={{ margin: "4px 0 0", color: "var(--text-secondary)", fontSize: 13 }}>
            Todo pasa por vista previa (Dry-run), lista negra de rutas protegidas y Papelera por
            defecto.
          </p>
        </div>

        <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
          <label style={{ fontSize: 12.5, display: "flex", alignItems: "center", gap: 6 }}>
            Mín. MB:
            <input
              type="number"
              className="input"
              style={{ width: 78 }}
              min={10}
              value={minLargeMb}
              onChange={(e) => setMinLargeMb(Math.max(10, Number(e.target.value) || 100))}
            />
          </label>
          <label style={{ fontSize: 12.5, display: "flex", alignItems: "center", gap: 6 }}>
            Antigüedad (días):
            <input
              type="number"
              className="input"
              style={{ width: 72 }}
              min={1}
              value={minAgeDays}
              onChange={(e) => setMinAgeDays(Math.max(1, Number(e.target.value) || 30))}
            />
          </label>
          <button type="button" className="btn" onClick={loadCandidates} disabled={loading}>
            <RefreshCw size={14} />
            {loading ? "Analizando..." : "Reanalizar"}
          </button>
          <button
            type="button"
            className="btn btn-primary"
            disabled={selectedCandidates.length === 0}
            onClick={handlePreview}
          >
            <ShieldCheck size={14} />
            Previsualizar ({selectedCandidates.length} · {formatBytes(selectedBytes)})
          </button>
        </div>
      </div>

      {report && report.runningBrowsers.length > 0 && (
        <div
          className="card"
          style={{
            borderColor: "rgba(245, 158, 11, 0.5)",
            background: "rgba(245, 158, 11, 0.08)",
            display: "flex",
            alignItems: "center",
            gap: 10,
            fontSize: 13,
          }}
        >
          <AlertTriangle size={18} color="#f59e0b" />
          <span>
            Navegadores en ejecución: <strong>{report.runningBrowsers.join(", ")}</strong>. Cierra
            el navegador antes de limpiar su caché para evitar bloqueos de archivo.
          </span>
        </div>
      )}

      {/* Recycle Bin summary card */}
      <div
        className="card"
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          flexWrap: "wrap",
          gap: 12,
        }}
      >
        <div>
          <div style={{ fontWeight: 700, fontSize: 15 }}>Papelera de reciclaje</div>
          <div style={{ fontSize: 13, color: "var(--text-secondary)", marginTop: 2 }}>
            {report
              ? `${report.recycleBinItems.toLocaleString()} elementos · ${formatBytes(report.recycleBinBytes)}`
              : "Consultando Papelera..."}
          </div>
        </div>
        <button
          type="button"
          className="btn btn-danger"
          disabled={!report || report.recycleBinItems === 0}
          onClick={() => onStartPreview([], "empty_recycle_bin")}
        >
          <Trash2 size={14} />
          Vaciar Papelera
        </button>
      </div>

      {Array.from(grouped.entries()).map(([cat, items]) => (
        <div
          key={cat}
          className="card"
          style={{ display: "flex", flexDirection: "column", gap: 10 }}
        >
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <h3 style={{ margin: 0, fontSize: 15 }}>{CATEGORY_TITLES[cat]}</h3>
            <span className="mono" style={{ fontSize: 12.5, color: "var(--accent)" }}>
              {formatBytes(items.reduce((a, b) => a + b.sizeBytes, 0))}
            </span>
          </div>

          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            {items.map((item) => {
              const locked = item.requiresAdmin && !isElevated;
              const checked = selectedIds.has(item.id);
              return (
                <div
                  key={item.id}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    gap: 12,
                    padding: "9px 12px",
                    borderRadius: 8,
                    background: "#0f172a",
                    border: checked
                      ? "1px solid rgba(56, 189, 248, 0.55)"
                      : "1px solid var(--border)",
                    opacity: locked ? 0.72 : 1,
                  }}
                >
                  <label
                    style={{
                      display: "flex",
                      alignItems: "flex-start",
                      gap: 10,
                      cursor: locked ? "not-allowed" : "pointer",
                      flex: 1,
                      overflow: "hidden",
                    }}
                  >
                    <input
                      type="checkbox"
                      checked={checked}
                      disabled={locked}
                      onChange={() => toggleItem(item)}
                      style={{ marginTop: 3 }}
                    />
                    <div style={{ overflow: "hidden" }}>
                      <div style={{ fontWeight: 600, fontSize: 13.5 }}>{item.displayName}</div>
                      <div
                        className="mono"
                        style={{
                          fontSize: 11.5,
                          color: "var(--text-muted)",
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                          whiteSpace: "nowrap",
                        }}
                      >
                        {item.path}
                      </div>
                      {item.warning && (
                        <div style={{ fontSize: 11.5, color: "#fcd34d", marginTop: 3 }}>
                          {item.warning}
                        </div>
                      )}
                    </div>
                  </label>

                  <div style={{ display: "flex", alignItems: "center", gap: 10, flexShrink: 0 }}>
                    {item.lastModifiedTs > 0 && (
                      <span style={{ fontSize: 11.5, color: "var(--text-muted)" }}>
                        {formatUnixTs(item.lastModifiedTs)}
                      </span>
                    )}
                    <span
                      className="mono"
                      style={{ fontWeight: 600, minWidth: 85, textAlign: "right" }}
                    >
                      {formatBytes(item.sizeBytes)}
                    </span>
                    {locked && (
                      <button
                        type="button"
                        className="btn btn-warning"
                        style={{ padding: "4px 8px" }}
                        onClick={onRelaunchAdmin}
                      >
                        <Lock size={13} />
                        Admin
                      </button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      ))}
    </div>
  );
}
