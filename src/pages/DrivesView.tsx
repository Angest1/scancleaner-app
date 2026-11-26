import { HardDrive, Network, Play, RefreshCw, Square, Usb } from "lucide-react";
import { formatBytes, formatDurationMs } from "../lib/format";
import type { DriveInfo, ScanProgress } from "../lib/ipc";

interface DrivesViewProps {
  drives: DriveInfo[];
  loading: boolean;
  includeNetwork: boolean;
  scanningDrive: string | null;
  progress: ScanProgress | null;
  onToggleNetwork: (include: boolean) => void;
  onRefresh: () => void;
  onStartScan: (rootPath: string) => void;
  onCancelScan: () => void;
}

export function DrivesView({
  drives,
  loading,
  includeNetwork,
  scanningDrive,
  progress,
  onToggleNetwork,
  onRefresh,
  onStartScan,
  onCancelScan,
}: DrivesViewProps) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 18 }}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          flexWrap: "wrap",
          gap: 12,
        }}
      >
        <div>
          <h2 style={{ margin: 0, fontSize: 20 }}>Unidades de almacenamiento</h2>
          <p style={{ margin: "4px 0 0", color: "var(--text-secondary)", fontSize: 13 }}>
            Selecciona una unidad para analizar su estructura, categorías y oportunidades de
            limpieza.
          </p>
        </div>

        <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
          <label
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 7,
              fontSize: 13,
              cursor: "pointer",
            }}
          >
            <input
              type="checkbox"
              checked={includeNetwork}
              onChange={(e) => onToggleNetwork(e.target.checked)}
            />
            Incluir unidades de red
          </label>
          <button type="button" className="btn" onClick={onRefresh} disabled={loading}>
            <RefreshCw size={14} />
            Actualizar
          </button>
        </div>
      </div>

      {scanningDrive && (
        <div className="card" style={{ borderColor: "rgba(56, 189, 248, 0.5)" }}>
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              marginBottom: 10,
            }}
          >
            <div>
              <div style={{ fontWeight: 700, fontSize: 14.5 }}>
                Escaneando unidad <span className="mono">{scanningDrive}</span>
              </div>
              <div
                className="mono"
                style={{ fontSize: 12, color: "var(--text-secondary)", marginTop: 3 }}
              >
                {progress
                  ? `${progress.scannedFiles.toLocaleString()} archivos · ${progress.scannedDirs.toLocaleString()} carpetas · ${formatBytes(progress.totalAllocatedBytes)} · ${formatDurationMs(progress.elapsedMs)}`
                  : "Iniciando enumeración de archivos..."}
              </div>
            </div>
            <button type="button" className="btn btn-danger" onClick={onCancelScan}>
              <Square size={13} />
              Cancelar
            </button>
          </div>
          <div className="progress-track">
            <div className="progress-fill" style={{ width: "100%", opacity: 0.85 }} />
          </div>
          {progress?.currentPath && (
            <div
              className="mono"
              style={{
                marginTop: 8,
                fontSize: 11.5,
                color: "var(--text-muted)",
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
              }}
            >
              {progress.currentPath}
            </div>
          )}
        </div>
      )}

      <div className="grid-cards">
        {drives.map((drive) => {
          const usedPct =
            drive.totalBytes > 0 ? Math.min(100, (drive.usedBytes / drive.totalBytes) * 100) : 0;
          const Icon =
            drive.kind === "removable" ? Usb : drive.kind === "network" ? Network : HardDrive;

          return (
            <div
              key={drive.rootPath}
              className="card"
              style={{ display: "flex", flexDirection: "column", gap: 12 }}
            >
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "flex-start",
                }}
              >
                <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                  <div
                    style={{
                      width: 38,
                      height: 38,
                      borderRadius: 9,
                      background: "rgba(56, 189, 248, 0.14)",
                      display: "grid",
                      placeItems: "center",
                    }}
                  >
                    <Icon size={20} color="#38bdf8" />
                  </div>
                  <div>
                    <div style={{ fontWeight: 700, fontSize: 15 }}>
                      {drive.label ? `${drive.label} (${drive.rootPath})` : drive.rootPath}
                    </div>
                    <div style={{ fontSize: 12, color: "var(--text-muted)" }}>
                      {drive.kind === "fixed"
                        ? "Disco fijo"
                        : drive.kind === "removable"
                          ? "Unidad removible"
                          : "Unidad de red"}
                    </div>
                  </div>
                </div>
                <span className="badge badge-info">{drive.fsName || "N/D"}</span>
              </div>

              <div>
                <div className="progress-track">
                  <div
                    className="progress-fill"
                    style={{
                      width: `${usedPct}%`,
                      background:
                        usedPct > 90
                          ? "linear-gradient(90deg, #f59e0b, #ef4444)"
                          : "linear-gradient(90deg, #0ea5e9, #38bdf8)",
                    }}
                  />
                </div>
                <div
                  style={{
                    display: "flex",
                    justifyContent: "space-between",
                    fontSize: 12,
                    color: "var(--text-secondary)",
                    marginTop: 6,
                  }}
                >
                  <span>Usado: {formatBytes(drive.usedBytes)}</span>
                  <span>Libre: {formatBytes(drive.freeBytes)}</span>
                </div>
                <div style={{ fontSize: 11.5, color: "var(--text-muted)", marginTop: 2 }}>
                  Capacidad total: {formatBytes(drive.totalBytes)} ({usedPct.toFixed(1)}%)
                </div>
              </div>

              <button
                type="button"
                className="btn btn-primary"
                disabled={!drive.available || Boolean(scanningDrive)}
                onClick={() => onStartScan(drive.rootPath)}
              >
                <Play size={14} />
                {drive.available ? "Escanear unidad" : "No disponible"}
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
}
