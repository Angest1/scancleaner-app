import {
  AlertTriangle,
  FolderPlus,
  HardDrive,
  Info,
  Lock,
  RefreshCw,
  ShieldCheck,
  Terminal,
  Trash2,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { DevCleanupModal } from "../components/DevCleanupModal";
import { formatBytes, formatUnixTs } from "../lib/format";
import {
  type DevCandidate,
  type DevCleanerConfig,
  type DevCleanupPlan,
  type DevCleanupResult,
  type DevRuleCategory,
  api,
} from "../lib/ipc";

interface DevCleanerViewProps {
  isElevated: boolean;
  onRelaunchAdmin: () => void;
}

const CATEGORY_ORDER: DevRuleCategory[] = [
  "Node",
  "Python",
  "PHP",
  "Android",
  "Docker",
  "IDE",
  "Testing",
  "Sistema",
];

export function DevCleanerView({ isElevated, onRelaunchAdmin }: DevCleanerViewProps) {
  const [config, setConfig] = useState<DevCleanerConfig>({
    projectRoots: [],
    minProjectAgeDays: 30,
    disabledRuleIds: [],
  });
  const [manualRoot, setManualRoot] = useState("");
  const [report, setReport] = useState<DevScanReportState>(null);
  const [loading, setLoading] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [levelFilter, setLevelFilter] = useState<"all" | "seguro" | "revisar">("all");
  const [showDockerGuide, setShowDockerGuide] = useState(false);

  // Dry-run & execution modal state
  const [useRecycleBin, setUseRecycleBin] = useState(false);
  const [activePlan, setActivePlan] = useState<DevCleanupPlan | null>(null);
  const [executing, setExecuting] = useState(false);
  const [result, setResult] = useState<DevCleanupResult | null>(null);

  const runScan = useCallback((cfg: DevCleanerConfig) => {
    setLoading(true);
    api
      .devScan(cfg)
      .then((res) => {
        setReport(res);
        // Safe items preselected; review items unchecked by default
        const pre = new Set<string>();
        for (const c of res.candidates) {
          if (c.preselected) {
            pre.add(c.id);
          }
        }
        setSelectedIds(pre);
      })
      .catch((err) => alert(String(err)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    api.devGetConfig().then((cfg) => {
      setConfig(cfg);
      runScan(cfg);
    });
  }, [runScan]);

  const persistConfig = (next: DevCleanerConfig, rescan = false) => {
    setConfig(next);
    api
      .devSaveConfig(next)
      .then(() => {
        if (rescan) runScan(next);
      })
      .catch((err) => alert(String(err)));
  };

  const handlePickFolder = () => {
    api
      .devPickProjectRoot()
      .then((picked) => {
        if (!picked) return;
        if (config.projectRoots.some((r) => r.toLowerCase() === picked.toLowerCase())) return;
        const next: DevCleanerConfig = {
          ...config,
          projectRoots: [...config.projectRoots, picked],
        };
        persistConfig(next, true);
      })
      .catch((err) => alert(String(err)));
  };

  const handleAddManualRoot = () => {
    const trimmed = manualRoot.trim();
    if (!trimmed) return;
    if (config.projectRoots.some((r) => r.toLowerCase() === trimmed.toLowerCase())) {
      setManualRoot("");
      return;
    }
    const next: DevCleanerConfig = {
      ...config,
      projectRoots: [...config.projectRoots, trimmed],
    };
    setManualRoot("");
    persistConfig(next, true);
  };

  const handleRemoveRoot = (root: string) => {
    const next: DevCleanerConfig = {
      ...config,
      projectRoots: config.projectRoots.filter((r) => r !== root),
    };
    persistConfig(next, true);
  };

  const isLocked = useCallback(
    (c: DevCandidate) => (c.requiresAdmin && !isElevated) || c.blockedReason !== null,
    [isElevated],
  );

  const toggleItem = (c: DevCandidate) => {
    if (isLocked(c)) return;
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(c.id)) next.delete(c.id);
      else next.add(c.id);
      return next;
    });
  };

  const selectedCandidates = useMemo(
    () => (report?.candidates ?? []).filter((c) => selectedIds.has(c.id) && !isLocked(c)),
    [report, selectedIds, isLocked],
  );

  const selectedBytes = useMemo(
    () => selectedCandidates.reduce((acc, c) => acc + c.sizeBytes, 0),
    [selectedCandidates],
  );

  const groupedByCategory = useMemo(() => {
    const map = new Map<DevRuleCategory, DevCandidate[]>();
    for (const c of report?.candidates ?? []) {
      if (levelFilter !== "all" && c.nivel !== levelFilter) continue;
      const list = map.get(c.categoria) ?? [];
      list.push(c);
      map.set(c.categoria, list);
    }
    return CATEGORY_ORDER.filter((cat) => map.has(cat)).map((cat) => ({
      category: cat,
      items: map.get(cat) ?? [],
    }));
  }, [report, levelFilter]);

  const openPreview = (recycleOption: boolean) => {
    if (selectedCandidates.length === 0) return;
    setUseRecycleBin(recycleOption);
    setResult(null);
    api
      .devPreview(
        selectedCandidates.map((candidate) => ({
          candidate,
          useRecycleBin: recycleOption,
        })),
      )
      .then(setActivePlan)
      .catch((err) => alert(String(err)));
  };

  const handleExecute = () => {
    if (!activePlan) return;
    setExecuting(true);
    api
      .devExecute(activePlan.planId)
      .then((res) => {
        setResult(res);
      })
      .catch((err) => alert(String(err)))
      .finally(() => setExecuting(false));
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
      {/* Header */}
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
          <h2 style={{ margin: 0, fontSize: 20 }}>Limpieza Dev</h2>
          <p style={{ margin: "4px 0 0", color: "var(--text-secondary)", fontSize: 13 }}>
            Solo archivos regenerables y cachés de desarrollo. Lista negra inmutable activa (nunca
            toca código fuente, .git, .env, keystores ni volúmenes Docker).
          </p>
        </div>

        <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
          <button type="button" className="btn" onClick={() => runScan(config)} disabled={loading}>
            <RefreshCw size={14} />
            {loading ? "Escaneando..." : "Reescanear"}
          </button>
          <button
            type="button"
            className="btn btn-primary"
            disabled={selectedCandidates.length === 0}
            onClick={() => openPreview(false)}
          >
            <ShieldCheck size={14} />
            Dry-run ({selectedCandidates.length} · {formatBytes(selectedBytes)})
          </button>
        </div>
      </div>

      {/* Summary metrics */}
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))",
          gap: 12,
        }}
      >
        <div className="card">
          <div style={{ fontSize: 12, color: "var(--text-muted)" }}>Total recuperable</div>
          <div className="mono" style={{ fontSize: 22, fontWeight: 700, color: "#38bdf8" }}>
            {formatBytes(report?.totalRecoverableBytes ?? 0)}
          </div>
        </div>
        <div className="card">
          <div style={{ fontSize: 12, color: "var(--text-muted)" }}>
            Nivel Seguro (preseleccionado)
          </div>
          <div className="mono" style={{ fontSize: 20, fontWeight: 700, color: "#4ade80" }}>
            {formatBytes(report?.safeRecoverableBytes ?? 0)}
          </div>
        </div>
        <div className="card">
          <div style={{ fontSize: 12, color: "var(--text-muted)" }}>Nivel Revisar (desmarcado)</div>
          <div className="mono" style={{ fontSize: 20, fontWeight: 700, color: "#fbbf24" }}>
            {formatBytes(report?.reviewRecoverableBytes ?? 0)}
          </div>
        </div>
      </div>

      {/* Project roots & age configuration */}
      <div className="card" style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            flexWrap: "wrap",
            gap: 10,
          }}
        >
          <div>
            <div style={{ fontWeight: 700, fontSize: 14 }}>
              Raíces de proyectos para artefactos (node_modules, vendor, .venv, build, target)
            </div>
            <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>
              Solo se escanean estas carpetas (nunca el disco completo ni dentro de artefactos o
              symlinks).
            </div>
          </div>
          <label style={{ fontSize: 12.5, display: "flex", alignItems: "center", gap: 6 }}>
            Antigüedad mínima (días):
            <input
              type="number"
              className="input"
              style={{ width: 74 }}
              min={0}
              value={config.minProjectAgeDays}
              onChange={(e) => {
                const days = Math.max(0, Number(e.target.value) || 0);
                persistConfig({ ...config, minProjectAgeDays: days }, false);
              }}
            />
          </label>
        </div>

        <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
          <button type="button" className="btn" onClick={handlePickFolder}>
            <FolderPlus size={14} />
            Elegir carpeta...
          </button>
          <input
            type="text"
            className="input"
            style={{ flex: 1, minWidth: 220 }}
            placeholder="O pega una ruta (ej. D:\proyectos)"
            value={manualRoot}
            onChange={(e) => setManualRoot(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleAddManualRoot();
            }}
          />
          <button type="button" className="btn" onClick={handleAddManualRoot}>
            Agregar
          </button>
        </div>

        {config.projectRoots.length > 0 && (
          <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
            {config.projectRoots.map((root) => (
              <span
                key={root}
                className="mono"
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 6,
                  fontSize: 12,
                  padding: "4px 8px",
                  borderRadius: 6,
                  background: "#0f172a",
                  border: "1px solid var(--border)",
                }}
              >
                {root}
                <button
                  type="button"
                  style={{
                    background: "transparent",
                    border: "none",
                    color: "#fca5a5",
                    cursor: "pointer",
                    padding: 0,
                    display: "inline-flex",
                  }}
                  onClick={() => handleRemoveRoot(root)}
                  title="Quitar raíz"
                >
                  <Trash2 size={12} />
                </button>
              </span>
            ))}
          </div>
        )}
      </div>

      {/* Running processes & warnings */}
      {report && report.runningDevProcesses.length > 0 && (
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
            Procesos activos detectados:{" "}
            <strong className="mono">{report.runningDevProcesses.join(", ")}</strong>. Las reglas
            bloqueadas por estos procesos se omitirán automáticamente sin forzar cierres.
          </span>
        </div>
      )}

      {report?.warnings.map((w) => (
        <div
          key={w}
          className="card"
          style={{
            borderColor: "rgba(56, 189, 248, 0.4)",
            background: "rgba(56, 189, 248, 0.06)",
            display: "flex",
            alignItems: "center",
            gap: 10,
            fontSize: 12.5,
          }}
        >
          <Info size={16} color="#38bdf8" />
          <span>{w}</span>
        </div>
      ))}

      {/* Docker / WSL VHDX compaction card */}
      {report?.dockerWslInfo.dockerAvailable && (
        <div className="card" style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              gap: 12,
              flexWrap: "wrap",
            }}
          >
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <HardDrive size={16} color="#38bdf8" />
              <span style={{ fontWeight: 600, fontSize: 13.5 }}>
                Compactación de disco virtual Docker / WSL (acción aparte)
              </span>
            </div>
            <button type="button" className="btn" onClick={() => setShowDockerGuide((v) => !v)}>
              {showDockerGuide ? "Ocultar instrucciones" : "Ver instrucciones de compactación"}
            </button>
          </div>
          <div style={{ fontSize: 12.5, color: "var(--text-secondary)" }}>
            {report.dockerWslInfo.vhdxNotice}
          </div>
          {showDockerGuide && (
            <div
              style={{
                background: "#0f172a",
                borderRadius: 6,
                padding: "10px 12px",
                display: "flex",
                flexDirection: "column",
                gap: 4,
                fontSize: 12,
              }}
            >
              {report.dockerWslInfo.compactInstructions.map((step) => (
                <div key={step} className="mono">
                  {step}
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Filter pills */}
      <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
        <span style={{ fontSize: 12.5, color: "var(--text-secondary)" }}>Mostrar nivel:</span>
        {(["all", "seguro", "revisar"] as const).map((lvl) => (
          <button
            key={lvl}
            type="button"
            className={`btn ${levelFilter === lvl ? "btn-primary" : ""}`}
            onClick={() => setLevelFilter(lvl)}
          >
            {lvl === "all" ? "Todos" : lvl === "seguro" ? "Seguro" : "Revisar"}
          </button>
        ))}
      </div>

      {/* Grouped candidates by category */}
      {groupedByCategory.length === 0 && !loading && (
        <div
          className="card"
          style={{ textAlign: "center", color: "var(--text-muted)", padding: 24 }}
        >
          No se detectaron elementos limpiables con los filtros actuales.
        </div>
      )}

      {groupedByCategory.map(({ category, items }) => (
        <div
          key={category}
          className="card"
          style={{ display: "flex", flexDirection: "column", gap: 10 }}
        >
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <h3 style={{ margin: 0, fontSize: 15 }}>{category}</h3>
            <span className="mono" style={{ fontSize: 12.5, color: "var(--accent)" }}>
              {formatBytes(items.reduce((a, b) => a + b.sizeBytes, 0))} · {items.length} ítems
            </span>
          </div>

          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            {items.map((item) => {
              const locked = isLocked(item);
              const checked = selectedIds.has(item.id) && !locked;
              return (
                <div
                  key={item.id}
                  style={{
                    display: "flex",
                    alignItems: "flex-start",
                    justifyContent: "space-between",
                    gap: 12,
                    padding: "10px 12px",
                    borderRadius: 8,
                    background: "#0f172a",
                    border: checked
                      ? "1px solid rgba(56, 189, 248, 0.55)"
                      : "1px solid var(--border)",
                    opacity: locked ? 0.7 : 1,
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
                    <div
                      style={{
                        overflow: "hidden",
                        display: "flex",
                        flexDirection: "column",
                        gap: 3,
                      }}
                    >
                      <div
                        style={{ display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" }}
                      >
                        <span style={{ fontWeight: 600, fontSize: 13.5 }}>{item.nombre}</span>
                        <span
                          className={`badge ${
                            item.nivel === "seguro" ? "badge-success" : "badge-warning"
                          }`}
                        >
                          {item.nivel}
                        </span>
                        {item.noGitRepo && (
                          <span className="badge badge-warning">Sin repo Git</span>
                        )}
                        {item.projectAgeDays !== null && (
                          <span className="badge">Sin cambios hace {item.projectAgeDays} d</span>
                        )}
                      </div>

                      <div
                        className="mono"
                        style={{
                          fontSize: 11.5,
                          color: "var(--text-muted)",
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                          whiteSpace: "nowrap",
                        }}
                        title={item.path}
                      >
                        {item.path}
                      </div>

                      <div style={{ fontSize: 11.5, color: "#93c5fd" }}>
                        <strong>Cómo regenerar:</strong> {item.comoRegenerar}
                      </div>

                      {item.command && (
                        <div
                          className="mono"
                          style={{
                            fontSize: 11,
                            color: "var(--text-secondary)",
                            display: "flex",
                            alignItems: "center",
                            gap: 5,
                          }}
                        >
                          <Terminal size={11} />
                          {item.command.join(" ")}
                        </div>
                      )}

                      {item.warning && (
                        <div style={{ fontSize: 11.5, color: "#fcd34d" }}>{item.warning}</div>
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
                      {item.sizeBytes > 0 ? formatBytes(item.sizeBytes) : "—"}
                    </span>
                    {item.requiresAdmin && !isElevated && (
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

      {activePlan && (
        <DevCleanupModal
          plan={activePlan}
          executing={executing}
          result={result}
          useRecycleBin={useRecycleBin}
          onToggleRecycleBin={(val) => openPreview(val)}
          onExecute={handleExecute}
          onClose={() => {
            const hadResult = result !== null;
            setActivePlan(null);
            setResult(null);
            if (hadResult) runScan(config);
          }}
        />
      )}
    </div>
  );
}

type DevScanReportState = Awaited<ReturnType<typeof api.devScan>> | null;
