import { FolderOpen, GitCompare, History, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { HorizontalBarChart } from "../components/charts/HorizontalBarChart";
import { CleanupLogsTable } from "../components/history/CleanupLogsTable";
import { HistoryAnalyticsBento } from "../components/history/HistoryAnalyticsBento";
import { InlineActivityBanner } from "../components/ui/ActionMotion";
import { Badge } from "../components/ui/Badge";
import { Button } from "../components/ui/Button";
import { Card } from "../components/ui/Card";
import { Collapse } from "../components/ui/Collapse";
import { EmptyState } from "../components/ui/Feedback";
import { SegmentedControl } from "../components/ui/SegmentedControl";
import { DensityToggle, type TableDensity } from "../components/ui/TableToolbar";
import { formatBytes } from "../lib/format";
import { type CleanupLogEntry, type ScanSnapshot, type SnapshotDiff, api } from "../lib/ipc";

type HistoryTab = "snapshots" | "logs" | "diagnostics";

export function HistoryView() {
  const [tab, setTab] = useState<HistoryTab>("snapshots");
  const [snapshots, setSnapshots] = useState<ScanSnapshot[]>([]);
  const [logs, setLogs] = useState<CleanupLogEntry[]>([]);
  const [diagLines, setDiagLines] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [olderId, setOlderId] = useState<number | null>(null);
  const [newerId, setNewerId] = useState<number | null>(null);
  const [diff, setDiff] = useState<SnapshotDiff | null>(null);
  const [density, setDensity] = useState<TableDensity>("comfortable");
  const loadedTabsRef = useRef<Set<HistoryTab>>(new Set());

  const loadTab = useCallback((target: HistoryTab, force = false) => {
    if (!force && loadedTabsRef.current.has(target)) return;
    setLoading(true);
    const req =
      target === "snapshots"
        ? api.listSnapshots(null).then((snaps) => {
            setSnapshots(snaps);
            if (snaps.length >= 2) {
              setNewerId((prev) => prev ?? snaps[0].id);
              setOlderId((prev) => prev ?? snaps[1].id);
            }
          })
        : target === "logs"
          ? api.listCleanupLogs(200).then(setLogs)
          : api
              .listDiagnosticLogs(200)
              .then(setDiagLines)
              .catch(() => setDiagLines([]));

    req.then(() => loadedTabsRef.current.add(target)).finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    loadTab(tab, false);
  }, [tab, loadTab]);

  const runDiff = () => {
    if (!olderId || !newerId || olderId === newerId) return;
    api
      .diffSnapshots(olderId, newerId)
      .then(setDiff)
      .catch((err) => alert(String(err)));
  };

  const freedByCategory = useMemo(() => {
    const map = new Map<string, number>();
    let total = 0;
    for (const l of logs) {
      if (l.status === "failed" || l.status === "blocked") continue;
      total += l.bytes;
      map.set(l.category, (map.get(l.category) ?? 0) + l.bytes);
    }
    const palette = ["#3b82f6", "#10b981", "#8b5cf6", "#f59e0b", "#06b6d4", "#ec4899"];
    const items = Array.from(map.entries())
      .sort((a, b) => b[1] - a[1])
      .map(([cat, bytes], idx) => ({
        key: cat,
        label: cat,
        value: bytes,
        valueLabel: formatBytes(bytes),
        color: palette[idx % palette.length],
      }));
    return { total, items };
  }, [logs]);

  const rowPad = density === "compact" ? "py-1.5" : "py-2.5";

  return (
    <div className="flex flex-col gap-4 min-h-[calc(100vh-110px)]">
      <div className="flex items-start justify-between flex-wrap gap-3">
        <div>
          <h1 className="m-0 text-[18px] font-semibold tracking-[-0.02em] text-fg-primary">
            Historial y auditoría
          </h1>
          <p className="m-0 mt-1 text-[13px] text-fg-secondary">
            Snapshots locales de escaneos, tendencia histórica y registro JSONL de limpiezas.
          </p>
        </div>

        <div className="flex items-center gap-2 flex-wrap">
          <SegmentedControl
            ariaLabel="Sección de historial"
            value={tab}
            onChange={setTab}
            options={[
              { value: "snapshots", label: "Snapshots", count: snapshots.length || undefined },
              { value: "logs", label: "Logs", count: logs.length || undefined },
              { value: "diagnostics", label: "Diagnóstico", count: diagLines.length || undefined },
            ]}
          />
          {tab !== "diagnostics" && <DensityToggle density={density} onChange={setDensity} />}
          <Button size="sm" onClick={() => loadTab(tab, true)} disabled={loading}>
            <RefreshCw size={13} className={loading ? "animate-spin" : ""} />
            {loading ? "Cargando…" : "Actualizar"}
          </Button>
        </div>
      </div>

      <InlineActivityBanner
        active={loading}
        variant="floating"
        delayMs={240}
        title="Actualizando historial y auditoría…"
        detail="Leyendo registros locales sin bloquear la interfaz."
      />

      {tab === "snapshots" ? (
        <div key="snapshots" className="animate-view-in flex flex-col gap-4 flex-1">
          <Collapse open={snapshots.length >= 1}>
            {snapshots.length >= 1 && <HistoryAnalyticsBento snapshots={snapshots} />}
          </Collapse>

          <Collapse open={snapshots.length >= 2}>
            {snapshots.length >= 2 && (
              <Card className="flex flex-col gap-3">
                <div className="flex items-center gap-2 font-semibold text-[13.5px] text-fg-primary">
                  <GitCompare size={15} className="text-accent" />
                  Comparar dos snapshots
                </div>

                <div className="flex items-center gap-2.5 flex-wrap">
                  <label className="text-[12px] text-fg-secondary flex items-center gap-1.5">
                    Anterior
                    <select
                      className="h-8 px-2.5 rounded-[7px] bg-canvas border border-border-default text-[12px] text-fg-primary focus:border-accent focus:outline-none"
                      value={olderId ?? ""}
                      onChange={(e) => setOlderId(Number(e.target.value))}
                    >
                      {snapshots.map((s) => (
                        <option key={s.id} value={s.id}>
                          #{s.id} · {s.driveRoot} · {new Date(s.scannedAt).toLocaleString("es-ES")}{" "}
                          ({formatBytes(s.scannedAllocatedBytes)})
                        </option>
                      ))}
                    </select>
                  </label>

                  <label className="text-[12px] text-fg-secondary flex items-center gap-1.5">
                    Posterior
                    <select
                      className="h-8 px-2.5 rounded-[7px] bg-canvas border border-border-default text-[12px] text-fg-primary focus:border-accent focus:outline-none"
                      value={newerId ?? ""}
                      onChange={(e) => setNewerId(Number(e.target.value))}
                    >
                      {snapshots.map((s) => (
                        <option key={s.id} value={s.id}>
                          #{s.id} · {s.driveRoot} · {new Date(s.scannedAt).toLocaleString("es-ES")}{" "}
                          ({formatBytes(s.scannedAllocatedBytes)})
                        </option>
                      ))}
                    </select>
                  </label>

                  <Button
                    variant="primary"
                    size="sm"
                    onClick={runDiff}
                    disabled={!olderId || !newerId || olderId === newerId}
                  >
                    Comparar
                  </Button>
                </div>

                <Collapse open={Boolean(diff)}>
                  {diff && (
                    <div className="flex flex-col gap-3 pt-2 border-t border-border-subtle">
                      <div className="flex items-center gap-4 flex-wrap text-[12.5px]">
                        <span className="text-fg-secondary">
                          Cambio total:{" "}
                          <strong
                            className={`mono ${
                              diff.allocatedDeltaBytes > 0
                                ? "text-danger-text"
                                : "text-success-text"
                            }`}
                          >
                            {diff.allocatedDeltaBytes >= 0 ? "+" : "-"}
                            {formatBytes(Math.abs(diff.allocatedDeltaBytes))}
                          </strong>
                        </span>
                        <span className="text-fg-secondary">
                          Archivos:{" "}
                          <strong className="mono text-fg-primary">
                            {diff.fileCountDelta >= 0 ? "+" : ""}
                            {diff.fileCountDelta.toLocaleString()}
                          </strong>
                        </span>
                      </div>

                      {diff.topDirDeltas.length > 0 && (
                        <div className="flex flex-col gap-1">
                          <div className="text-[11.5px] font-medium text-fg-muted">
                            Carpetas con mayor variación
                          </div>
                          {diff.topDirDeltas.slice(0, 8).map((d) => (
                            <div
                              key={d.name}
                              className="flex items-center justify-between gap-3 px-2.5 py-1.5 rounded-[6px] bg-canvas border border-border-subtle text-[12px]"
                            >
                              <span className="mono text-fg-secondary truncate" title={d.name}>
                                {d.name}
                              </span>
                              <span
                                className={`mono font-medium shrink-0 ${
                                  d.deltaBytes > 0 ? "text-danger-text" : "text-success-text"
                                }`}
                              >
                                {d.deltaBytes >= 0 ? "+" : "-"}
                                {formatBytes(Math.abs(d.deltaBytes))}
                              </span>
                            </div>
                          ))}
                        </div>
                      )}
                    </div>
                  )}
                </Collapse>
              </Card>
            )}
          </Collapse>

          {loading && snapshots.length === 0 ? null : snapshots.length === 0 ? (
            <EmptyState
              icon={<History size={18} />}
              title="Aún no hay snapshots"
              description="Completa un escaneo desde Inicio para guardar un snapshot y comparar cambios con el tiempo."
            />
          ) : (
            <Card padded={false} className="sc-reveal overflow-hidden flex-1">
              <div className="min-h-[360px] h-[calc(100vh-360px)] overflow-y-auto overflow-x-hidden">
                <div className="sticky top-0 z-10 grid grid-cols-[minmax(0,1fr)_90px] sm:grid-cols-[54px_76px_minmax(0,1fr)_104px_92px_72px] items-center gap-2 px-3.5 py-2.5 border-b border-border-default bg-shell backdrop-blur-md text-[11.5px] font-medium text-fg-muted">
                  <span className="hidden sm:inline">ID</span>
                  <span className="hidden sm:inline">Unidad</span>
                  <span>Fecha</span>
                  <span className="text-right">Medido</span>
                  <span className="hidden sm:inline text-right">Archivos</span>
                  <span className="hidden sm:inline text-right">Duración</span>
                </div>
                {snapshots.map((s) => (
                  <div
                    key={s.id}
                    className={`grid grid-cols-[minmax(0,1fr)_90px] sm:grid-cols-[54px_76px_minmax(0,1fr)_104px_92px_72px] items-center gap-2 px-3.5 ${rowPad} border-b border-border-subtle text-[12.5px] hover:bg-elevated/55 transition-colors`}
                  >
                    <span className="hidden sm:inline mono text-fg-muted">#{s.id}</span>
                    <div className="hidden sm:block">
                      <Badge tone="accent">{s.driveRoot}</Badge>
                    </div>
                    <div className="min-w-0">
                      <div className="text-fg-primary tabular truncate">
                        {new Date(s.scannedAt).toLocaleString("es-ES")}
                      </div>
                      <div className="sm:hidden text-[11px] text-fg-muted mono mt-0.5">
                        #{s.id} · {s.driveRoot} · {s.fileCount.toLocaleString()} archivos
                      </div>
                    </div>
                    <span className="mono text-right font-medium text-fg-primary">
                      {formatBytes(s.scannedAllocatedBytes)}
                    </span>
                    <span className="hidden sm:inline mono text-right text-fg-secondary">
                      {s.fileCount.toLocaleString()}
                    </span>
                    <span className="hidden sm:inline mono text-right text-fg-muted">
                      {(s.durationMs / 1000).toFixed(1)}s
                    </span>
                  </div>
                ))}
              </div>
            </Card>
          )}
        </div>
      ) : tab === "logs" ? (
        <div key="logs" className="animate-view-in flex flex-col gap-4 flex-1">
          <Collapse open={freedByCategory.items.length > 0}>
            {freedByCategory.items.length > 0 && (
              <Card className="flex flex-col gap-2">
                <div className="flex items-center justify-between flex-wrap gap-2">
                  <span className="text-[13px] font-semibold text-fg-primary">
                    Espacio liberado acumulado por categoría
                  </span>
                  <span className="mono text-[12.5px] font-semibold text-success">
                    {formatBytes(freedByCategory.total)}
                  </span>
                </div>
                <HorizontalBarChart items={freedByCategory.items} maxItems={6} />
              </Card>
            )}
          </Collapse>
          <CleanupLogsTable logs={logs} density={density} />
        </div>
      ) : (
        <Card key="diagnostics" className="animate-view-in flex flex-col gap-3 flex-1">
          <div className="flex items-center justify-between flex-wrap gap-2">
            <div>
              <h2 className="m-0 text-[14px] font-semibold text-fg-primary">
                Logs de diagnóstico de aplicación
              </h2>
              <p className="m-0 mt-0.5 text-[12px] text-fg-secondary">
                Últimas entradas de{" "}
                <code className="mono">%APPDATA%\ScanCleaner\logs\app-*.log</code>.
              </p>
            </div>
            <Button size="sm" onClick={() => api.openLogsFolder().catch(() => {})}>
              <FolderOpen size={13} />
              Abrir carpeta de logs
            </Button>
          </div>
          {diagLines.length === 0 ? (
            <div className="py-8 text-center text-[12.5px] text-fg-muted">
              Aún no hay entradas de diagnóstico registradas.
            </div>
          ) : (
            <div className="min-h-[380px] h-[calc(100vh-300px)] overflow-y-auto rounded-[10px] bg-canvas border border-border-default p-3 mono text-[11.5px] text-fg-secondary flex flex-col gap-1">
              {diagLines.map((line, i) => (
                <div
                  key={`${i}-${line.slice(0, 24)}`}
                  className={
                    line.includes("[ERROR]")
                      ? "text-danger-text"
                      : line.includes("[WARN]")
                        ? "text-warning-text"
                        : ""
                  }
                >
                  {line}
                </div>
              ))}
            </div>
          )}
        </Card>
      )}
    </div>
  );
}
