import { Bug, FolderOpen, GitCompare, History, RefreshCw, ScrollText } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { formatBytes, formatDurationMs, formatSignedBytes } from "../lib/format";
import { type CleanupLogEntry, type ScanSnapshot, type SnapshotDiff, api } from "../lib/ipc";

export function HistoryView() {
  const [tab, setTab] = useState<"snapshots" | "logs" | "diagnostics">("snapshots");
  const [snapshots, setSnapshots] = useState<ScanSnapshot[]>([]);
  const [logs, setLogs] = useState<CleanupLogEntry[]>([]);
  const [diagLogs, setDiagLogs] = useState<string[]>([]);
  const [olderId, setOlderId] = useState<number | null>(null);
  const [newerId, setNewerId] = useState<number | null>(null);
  const [diff, setDiff] = useState<SnapshotDiff | null>(null);

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

  const handleCompare = () => {
    if (!olderId || !newerId || olderId === newerId) return;
    api
      .diffSnapshots(olderId, newerId)
      .then(setDiff)
      .catch((e) => alert(String(e)));
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
          <h2 style={{ margin: 0, fontSize: 20 }}>Historial y auditoría</h2>
          <p style={{ margin: "4px 0 0", color: "var(--text-secondary)", fontSize: 13 }}>
            Evolución de espacio entre escaneos (SQLite) y registro auditable de limpieza (JSONL).
          </p>
        </div>

        <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
          <button
            type="button"
            className={`btn ${tab === "snapshots" ? "btn-primary" : ""}`}
            onClick={() => setTab("snapshots")}
          >
            <History size={14} />
            Snapshots ({snapshots.length})
          </button>
          <button
            type="button"
            className={`btn ${tab === "logs" ? "btn-primary" : ""}`}
            onClick={() => setTab("logs")}
          >
            <ScrollText size={14} />
            Logs de limpieza ({logs.length})
          </button>
          <button
            type="button"
            className={`btn ${tab === "diagnostics" ? "btn-primary" : ""}`}
            onClick={() => setTab("diagnostics")}
          >
            <Bug size={14} />
            Diagnóstico / Crashes ({diagLogs.length})
          </button>
          <button
            type="button"
            className="btn"
            onClick={() => api.openLogsFolder().catch((e) => alert(String(e)))}
            title="Abrir %APPDATA%\ScanCleaner\logs"
          >
            <FolderOpen size={14} />
            Abrir carpeta
          </button>
        </div>
      </div>

      {tab === "snapshots" ? (
        <>
          {snapshots.length >= 2 && (
            <div
              className="card"
              style={{ display: "flex", alignItems: "center", gap: 10, flexWrap: "wrap" }}
            >
              <span style={{ fontWeight: 600 }}>Comparar snapshots:</span>
              <select
                className="select"
                value={olderId ?? ""}
                onChange={(e) => setOlderId(Number(e.target.value))}
              >
                {snapshots.map((s) => (
                  <option key={s.id} value={s.id}>
                    #{s.id} · {s.driveRoot} · {new Date(s.scannedAt).toLocaleString("es-ES")}
                  </option>
                ))}
              </select>
              <span>→</span>
              <select
                className="select"
                value={newerId ?? ""}
                onChange={(e) => setNewerId(Number(e.target.value))}
              >
                {snapshots.map((s) => (
                  <option key={s.id} value={s.id}>
                    #{s.id} · {s.driveRoot} · {new Date(s.scannedAt).toLocaleString("es-ES")}
                  </option>
                ))}
              </select>
              <button type="button" className="btn btn-primary" onClick={handleCompare}>
                <GitCompare size={14} />
                Calcular diferencia
              </button>
            </div>
          )}

          {diff && (
            <div className="card" style={{ borderColor: "rgba(56, 189, 248, 0.5)" }}>
              <div style={{ fontWeight: 700, marginBottom: 8 }}>
                Variación en <span className="mono">{diff.driveRoot}</span>:{" "}
                <span style={{ color: diff.allocatedDeltaBytes > 0 ? "#fca5a5" : "#86efac" }}>
                  {formatSignedBytes(diff.allocatedDeltaBytes)}
                </span>{" "}
                ({diff.fileCountDelta >= 0 ? "+" : ""}
                {diff.fileCountDelta.toLocaleString()} archivos)
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                {diff.topDirDeltas.slice(0, 12).map((d) => (
                  <div
                    key={d.name}
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      fontSize: 12.5,
                      padding: "4px 8px",
                      background: "#0f172a",
                      borderRadius: 6,
                    }}
                  >
                    <span className="mono">{d.name}</span>
                    <span
                      className="mono"
                      style={{ color: d.deltaBytes > 0 ? "#fca5a5" : "#86efac" }}
                    >
                      {formatSignedBytes(d.deltaBytes)}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}

          <div className="card" style={{ padding: 0, overflow: "hidden" }}>
            <div
              className="table-header"
              style={{ gridTemplateColumns: "80px 110px 1fr 140px 120px 100px" }}
            >
              <span>ID</span>
              <span>Unidad</span>
              <span>Fecha</span>
              <span style={{ textAlign: "right" }}>Escaneado</span>
              <span style={{ textAlign: "right" }}>Archivos</span>
              <span style={{ textAlign: "right" }}>Duración</span>
            </div>
            {snapshots.length === 0 ? (
              <div style={{ padding: 24, textAlign: "center", color: "var(--text-muted)" }}>
                Aún no hay snapshots guardados. Completa un escaneo para registrar el primero.
              </div>
            ) : (
              snapshots.map((s) => (
                <div
                  key={s.id}
                  className="table-row"
                  style={{ gridTemplateColumns: "80px 110px 1fr 140px 120px 100px" }}
                >
                  <span className="mono">#{s.id}</span>
                  <span className="mono" style={{ fontWeight: 600 }}>
                    {s.driveRoot}
                  </span>
                  <span>{new Date(s.scannedAt).toLocaleString("es-ES")}</span>
                  <span className="mono" style={{ textAlign: "right" }}>
                    {formatBytes(s.scannedAllocatedBytes)}
                  </span>
                  <span className="mono" style={{ textAlign: "right" }}>
                    {s.fileCount.toLocaleString()}
                  </span>
                  <span style={{ textAlign: "right", color: "var(--text-secondary)" }}>
                    {formatDurationMs(s.durationMs)}
                  </span>
                </div>
              ))
            )}
          </div>
        </>
      ) : tab === "logs" ? (
        <div className="card" style={{ padding: 0, overflow: "hidden" }}>
          <div
            className="table-header"
            style={{ gridTemplateColumns: "165px 1fr 110px 110px 110px" }}
          >
            <span>Fecha</span>
            <span>Ruta</span>
            <span>Modo</span>
            <span style={{ textAlign: "right" }}>Bytes</span>
            <span style={{ textAlign: "right" }}>Estado</span>
          </div>
          {logs.length === 0 ? (
            <div style={{ padding: 24, textAlign: "center", color: "var(--text-muted)" }}>
              No hay registros de limpieza todavía.
            </div>
          ) : (
            logs.map((l, idx) => (
              <div
                key={`${l.planId}-${idx}`}
                className="table-row"
                style={{ gridTemplateColumns: "165px 1fr 110px 110px 110px" }}
              >
                <span style={{ fontSize: 12, color: "var(--text-secondary)" }}>
                  {new Date(l.timestamp).toLocaleString("es-ES")}
                </span>
                <span
                  className="mono"
                  style={{
                    fontSize: 12,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                  title={l.path}
                >
                  {l.path}
                </span>
                <span style={{ fontSize: 12 }}>{l.mode}</span>
                <span className="mono" style={{ textAlign: "right" }}>
                  {formatBytes(l.bytes)}
                </span>
                <div style={{ textAlign: "right" }}>
                  <span
                    className={`badge ${l.status === "failed" ? "badge-danger" : "badge-success"}`}
                  >
                    {l.status}
                  </span>
                </div>
              </div>
            ))
          )}
        </div>
      ) : (
        <div className="card" style={{ display: "flex", flexDirection: "column", gap: 10 }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <span style={{ fontSize: 13, color: "var(--text-secondary)" }}>
              Registra inicio, tiempos de escaneo, operaciones lentas (&gt;500 ms) y panics/crashes
              con stacktrace en <code>%APPDATA%\ScanCleaner\logs\</code>.
            </span>
            <button type="button" className="btn" onClick={refreshLogs}>
              <RefreshCw size={13} />
              Actualizar
            </button>
          </div>
          {diagLogs.length === 0 ? (
            <div style={{ padding: 20, textAlign: "center", color: "var(--text-muted)" }}>
              No hay registros de diagnóstico todavía.
            </div>
          ) : (
            <div
              className="mono"
              style={{
                maxHeight: 420,
                overflow: "auto",
                background: "#0f172a",
                borderRadius: 6,
                padding: 10,
                fontSize: 11.5,
                display: "flex",
                flexDirection: "column",
                gap: 4,
              }}
            >
              {diagLogs.map((line, idx) => (
                <div
                  key={`${idx}-${line.slice(0, 24)}`}
                  style={{
                    color:
                      line.includes("[CRASH]") || line.includes("[ERROR]")
                        ? "#fca5a5"
                        : line.includes("[WARN]")
                          ? "#fcd34d"
                          : "var(--text-secondary)",
                    whiteSpace: "pre-wrap",
                    wordBreak: "break-word",
                  }}
                >
                  {line}
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
