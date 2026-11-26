import { Bug, FolderOpen, GitCompare, History, RefreshCw, ScrollText } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { CleanupLogsTable } from "../components/history/CleanupLogsTable";
import { Button } from "../components/ui/Button";
import { Card } from "../components/ui/Card";
import { SegmentedControl } from "../components/ui/SegmentedControl";
import {
  DensityToggle,
  type SortDir,
  SortHeaderButton,
  type TableDensity,
} from "../components/ui/TableToolbar";
import { formatBytes, formatDurationMs, formatSignedBytes } from "../lib/format";
import { type CleanupLogEntry, type ScanSnapshot, type SnapshotDiff, api } from "../lib/ipc";

type SnapSortKey = "date" | "bytes" | "files";

export function HistoryView() {
  const [tab, setTab] = useState<"snapshots" | "logs" | "diagnostics">("snapshots");
  const [snapshots, setSnapshots] = useState<ScanSnapshot[]>([]);
  const [logs, setLogs] = useState<CleanupLogEntry[]>([]);
  const [diagLogs, setDiagLogs] = useState<string[]>([]);
  const [olderId, setOlderId] = useState<number | null>(null);
  const [newerId, setNewerId] = useState<number | null>(null);
  const [diff, setDiff] = useState<SnapshotDiff | null>(null);
  const [density, setDensity] = useState<TableDensity>("comfortable");
  const [snapSort, setSnapSort] = useState<{ key: SnapSortKey; dir: SortDir }>({
    key: "date",
    dir: "desc",
  });

  const refreshLogs = useCallback(() => {
    api.listCleanupLogs(200).then(setLogs);
    api.listDiagnosticLogs(300).then(setDiagLogs);
  }, []);

  useEffect(() => {
    api.listSnapshots(null).then((list) => {
      setSnapshots(list);
      if (list.length >= 2) {
        setNewerId(list[0].id);
        setOlderId(list[1].id);
      }
    });
    refreshLogs();
  }, [refreshLogs]);

  const sortedSnapshots = useMemo(() => {
    const mult = snapSort.dir === "asc" ? 1 : -1;
    return [...snapshots].sort((a, b) => {
      if (snapSort.key === "bytes")
        return (a.scannedAllocatedBytes - b.scannedAllocatedBytes) * mult;
      if (snapSort.key === "files") return (a.fileCount - b.fileCount) * mult;
      return (new Date(a.scannedAt).getTime() - new Date(b.scannedAt).getTime()) * mult;
    });
  }, [snapshots, snapSort]);

  const toggleSnapSort = (key: SnapSortKey) =>
    setSnapSort((s) => ({
      key,
      dir: s.key === key ? (s.dir === "asc" ? "desc" : "asc") : "desc",
    }));

  const handleCompare = () => {
    if (!olderId || !newerId || olderId === newerId) return;
    api
      .diffSnapshots(olderId, newerId)
      .then(setDiff)
      .catch((e) => alert(String(e)));
  };

  const rowPad = density === "compact" ? "py-1.5" : "py-2.5";

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-start justify-between flex-wrap gap-3">
        <div>
          <h1 className="m-0 text-[18px] font-semibold tracking-[-0.02em] text-fg-primary">
            Historial y auditoría
          </h1>
          <p className="m-0 mt-1 text-[13px] text-fg-secondary text-pretty">
            Evolución de espacio entre escaneos (SQLite) y registro auditable de limpieza (JSONL).
          </p>
        </div>

        <div className="flex items-center gap-2 flex-wrap">
          <SegmentedControl
            ariaLabel="Sección de historial"
            value={tab}
            onChange={setTab}
            options={[
              {
                value: "snapshots",
                label: "Snapshots",
                icon: <History size={13} />,
                count: snapshots.length,
              },
              {
                value: "logs",
                label: "Limpieza",
                icon: <ScrollText size={13} />,
                count: logs.length,
              },
              {
                value: "diagnostics",
                label: "Diagnóstico",
                icon: <Bug size={13} />,
                count: diagLogs.length,
              },
            ]}
          />
          {tab !== "diagnostics" && <DensityToggle density={density} onChange={setDensity} />}
          <Button
            onClick={() => api.openLogsFolder().catch((e) => alert(String(e)))}
            title="Abrir %APPDATA%\ScanCleaner\logs"
          >
            <FolderOpen size={13} />
            Abrir carpeta
          </Button>
        </div>
      </div>

      <div key={tab} className="flex flex-col gap-4 animate-view-in">
        {tab === "snapshots" ? (
          <>
            {snapshots.length >= 2 && (
              <Card className="flex items-center gap-2.5 flex-wrap py-3">
                <span className="font-medium text-[12.5px] text-fg-primary">
                  Comparar snapshots
                </span>
                <select
                  aria-label="Snapshot anterior"
                  className="h-8 px-2.5 rounded-[8px] bg-canvas border border-border-default text-[12px] text-fg-primary focus:border-accent focus:outline-none"
                  value={olderId ?? ""}
                  onChange={(e) => setOlderId(Number(e.target.value))}
                >
                  {snapshots.map((s) => (
                    <option key={s.id} value={s.id}>
                      #{s.id} · {s.driveRoot} · {new Date(s.scannedAt).toLocaleString("es-ES")}
                    </option>
                  ))}
                </select>
                <span className="text-fg-muted">→</span>
                <select
                  aria-label="Snapshot reciente"
                  className="h-8 px-2.5 rounded-[8px] bg-canvas border border-border-default text-[12px] text-fg-primary focus:border-accent focus:outline-none"
                  value={newerId ?? ""}
                  onChange={(e) => setNewerId(Number(e.target.value))}
                >
                  {snapshots.map((s) => (
                    <option key={s.id} value={s.id}>
                      #{s.id} · {s.driveRoot} · {new Date(s.scannedAt).toLocaleString("es-ES")}
                    </option>
                  ))}
                </select>
                <Button variant="primary" size="sm" onClick={handleCompare}>
                  <GitCompare size={13} />
                  Calcular diferencia
                </Button>
              </Card>
            )}

            {diff && (
              <Card className="border-accent-border">
                <div className="font-semibold text-[13.5px] text-fg-primary mb-2.5 tabular">
                  Variación en <span className="mono">{diff.driveRoot}</span>:{" "}
                  <span className={diff.allocatedDeltaBytes > 0 ? "text-danger" : "text-success"}>
                    {formatSignedBytes(diff.allocatedDeltaBytes)}
                  </span>{" "}
                  <span className="text-fg-secondary font-normal">
                    ({diff.fileCountDelta >= 0 ? "+" : ""}
                    {diff.fileCountDelta.toLocaleString()} archivos)
                  </span>
                </div>
                <div className="flex flex-col gap-1">
                  {diff.topDirDeltas.slice(0, 12).map((d) => (
                    <div
                      key={d.name}
                      className="flex items-center justify-between gap-2 text-[12px] px-2.5 py-1.5 rounded-[6px] bg-canvas border border-border-subtle"
                    >
                      <span className="mono text-fg-secondary truncate">{d.name}</span>
                      <span
                        className={`mono font-medium shrink-0 ${
                          d.deltaBytes > 0 ? "text-danger" : "text-success"
                        }`}
                      >
                        {formatSignedBytes(d.deltaBytes)}
                      </span>
                    </div>
                  ))}
                </div>
              </Card>
            )}

            <Card padded={false} className="overflow-hidden">
              <div className="max-h-[460px] overflow-y-auto overflow-x-hidden">
                <div className="sticky top-0 z-10 grid grid-cols-[minmax(0,1fr)_105px] md:grid-cols-[64px_84px_minmax(0,1fr)_120px_100px_86px] items-center gap-2 px-3.5 py-2.5 border-b border-border-default bg-shell backdrop-blur-md text-[11.5px] font-medium text-fg-muted">
                  <span className="hidden md:inline">ID</span>
                  <span className="hidden md:inline">Unidad</span>
                  <SortHeaderButton
                    label="Fecha / Unidad"
                    active={snapSort.key === "date"}
                    dir={snapSort.dir}
                    onClick={() => toggleSnapSort("date")}
                  />
                  <SortHeaderButton
                    label="Escaneado"
                    active={snapSort.key === "bytes"}
                    dir={snapSort.dir}
                    align="right"
                    onClick={() => toggleSnapSort("bytes")}
                  />
                  <div className="hidden md:flex justify-end">
                    <SortHeaderButton
                      label="Archivos"
                      active={snapSort.key === "files"}
                      dir={snapSort.dir}
                      align="right"
                      onClick={() => toggleSnapSort("files")}
                    />
                  </div>
                  <span className="hidden md:inline text-right">Duración</span>
                </div>
                {sortedSnapshots.length === 0 ? (
                  <div className="py-10 text-center text-[12.5px] text-fg-muted">
                    Aún no hay snapshots guardados. Completa un escaneo para registrar el primero.
                  </div>
                ) : (
                  sortedSnapshots.map((s) => (
                    <div
                      key={s.id}
                      className={`grid grid-cols-[minmax(0,1fr)_105px] md:grid-cols-[64px_84px_minmax(0,1fr)_120px_100px_86px] items-center gap-2 px-3.5 ${rowPad} border-b border-border-subtle text-[12.5px] hover:bg-elevated/55 transition-colors`}
                    >
                      <span className="hidden md:inline mono text-fg-muted">#{s.id}</span>
                      <span className="hidden md:inline mono font-medium text-fg-primary">
                        {s.driveRoot}
                      </span>
                      <div className="min-w-0">
                        <div className="text-fg-primary md:text-fg-secondary tabular truncate">
                          <span className="md:hidden mono font-semibold mr-1.5">{s.driveRoot}</span>
                          {new Date(s.scannedAt).toLocaleString("es-ES")}
                        </div>
                        <div className="md:hidden text-[11px] text-fg-muted tabular">
                          #{s.id} · {s.fileCount.toLocaleString()} archivos ·{" "}
                          {formatDurationMs(s.durationMs)}
                        </div>
                      </div>
                      <span className="mono text-right font-medium text-fg-primary">
                        {formatBytes(s.scannedAllocatedBytes)}
                      </span>
                      <span className="hidden md:inline mono text-right text-fg-secondary">
                        {s.fileCount.toLocaleString()}
                      </span>
                      <span className="hidden md:inline mono text-right text-fg-muted">
                        {formatDurationMs(s.durationMs)}
                      </span>
                    </div>
                  ))
                )}
              </div>
            </Card>
          </>
        ) : tab === "logs" ? (
          <CleanupLogsTable logs={logs} density={density} />
        ) : (
          <Card className="flex flex-col gap-3">
            <div className="flex items-center justify-between flex-wrap gap-2">
              <span className="text-[12.5px] text-fg-secondary">
                Registra inicio, tiempos de escaneo, operaciones lentas (&gt;500 ms) y
                panics/crashes con stacktrace en{" "}
                <code className="text-fg-primary">%APPDATA%\ScanCleaner\logs\</code>.
              </span>
              <Button size="sm" onClick={refreshLogs}>
                <RefreshCw size={12} />
                Actualizar
              </Button>
            </div>
            {diagLogs.length === 0 ? (
              <div className="py-6 text-center text-[12.5px] text-fg-muted">
                No hay registros de diagnóstico todavía.
              </div>
            ) : (
              <div className="mono max-h-[420px] overflow-auto rounded-[8px] bg-canvas border border-border-subtle p-3 text-[11.5px] flex flex-col gap-1">
                {diagLogs.map((line, idx) => (
                  <div
                    key={`${idx}-${line.slice(0, 24)}`}
                    className={`whitespace-pre-wrap break-words ${
                      line.includes("[CRASH]") || line.includes("[ERROR]")
                        ? "text-danger-text"
                        : line.includes("[WARN]")
                          ? "text-warning-text"
                          : "text-fg-secondary"
                    }`}
                  >
                    {line}
                  </div>
                ))}
              </div>
            )}
          </Card>
        )}
      </div>
    </div>
  );
}
