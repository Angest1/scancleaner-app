import { useVirtualizer } from "@tanstack/react-virtual";
import { ExternalLink, RefreshCw, Search, Trash2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { formatBytes } from "../lib/format";
import { type InstalledApp, api } from "../lib/ipc";

export function AppsView() {
  const [apps, setApps] = useState<InstalledApp[]>([]);
  const [loading, setLoading] = useState(false);
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<"all" | "win32" | "store_msix">("all");
  const [statusMsg, setStatusMsg] = useState<string | null>(null);
  const parentRef = useRef<HTMLDivElement>(null);

  const loadApps = useCallback(() => {
    setLoading(true);
    api
      .listInstalledApps()
      .then(setApps)
      .catch((err) => setStatusMsg(String(err)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    loadApps();
  }, [loadApps]);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return apps.filter((a) => {
      if (filter !== "all" && a.source !== filter) return false;
      if (!q) return true;
      return a.name.toLowerCase().includes(q) || a.publisher.toLowerCase().includes(q);
    });
  }, [apps, search, filter]);

  const virtualizer = useVirtualizer({
    count: filtered.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 56,
    overscan: 8,
  });

  const handleUninstall = (app: InstalledApp) => {
    api
      .uninstallApp(app.source, app.uninstallString)
      .then((msg) => setStatusMsg(msg))
      .catch((err) => setStatusMsg(String(err)));
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
          <h2 style={{ margin: 0, fontSize: 20 }}>Aplicaciones instaladas</h2>
          <p style={{ margin: "4px 0 0", color: "var(--text-secondary)", fontSize: 13 }}>
            Inventario combinado Win32 (Registro) y Microsoft Store / MSIX. La desinstalación usa
            siempre el flujo nativo de Windows.
          </p>
        </div>

        <div style={{ display: "flex", gap: 8 }}>
          <button
            type="button"
            className="btn"
            onClick={() => api.openWindowsAppsSettings().catch((e) => setStatusMsg(String(e)))}
          >
            <ExternalLink size={14} />
            Configuración de Windows
          </button>
          <button type="button" className="btn btn-primary" onClick={loadApps} disabled={loading}>
            <RefreshCw size={14} />
            {loading ? "Cargando..." : "Actualizar"}
          </button>
        </div>
      </div>

      {statusMsg && (
        <div
          className="card"
          style={{ padding: "10px 14px", borderColor: "rgba(56, 189, 248, 0.45)", fontSize: 13 }}
        >
          {statusMsg}
        </div>
      )}

      <div
        className="card"
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          flexWrap: "wrap",
          gap: 10,
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 8, flex: "1 1 260px" }}>
          <Search size={15} color="#64748b" />
          <input
            className="input"
            style={{ width: "100%", maxWidth: 360 }}
            placeholder="Buscar aplicación o editor..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>

        <div style={{ display: "flex", gap: 6 }}>
          <button
            type="button"
            className={`btn ${filter === "all" ? "btn-primary" : ""}`}
            onClick={() => setFilter("all")}
          >
            Todas ({apps.length})
          </button>
          <button
            type="button"
            className={`btn ${filter === "win32" ? "btn-primary" : ""}`}
            onClick={() => setFilter("win32")}
          >
            Win32
          </button>
          <button
            type="button"
            className={`btn ${filter === "store_msix" ? "btn-primary" : ""}`}
            onClick={() => setFilter("store_msix")}
          >
            Store / MSIX
          </button>
        </div>
      </div>

      <div className="card" style={{ padding: 0, overflow: "hidden" }}>
        <div className="table-header" style={{ gridTemplateColumns: "1fr 110px 145px 150px" }}>
          <span>Aplicación</span>
          <span>Origen</span>
          <span style={{ textAlign: "right" }}>Tamaño</span>
          <span style={{ textAlign: "right" }}>Desinstalación</span>
        </div>

        <div ref={parentRef} style={{ height: 460, overflow: "auto" }}>
          <div style={{ height: `${virtualizer.getTotalSize()}px`, position: "relative" }}>
            {virtualizer.getVirtualItems().map((vRow) => {
              const app = filtered[vRow.index];
              return (
                <div
                  key={app.id}
                  className="table-row"
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    height: `${vRow.size}px`,
                    transform: `translateY(${vRow.start}px)`,
                    gridTemplateColumns: "1fr 110px 145px 150px",
                  }}
                >
                  <div style={{ overflow: "hidden", paddingRight: 10 }}>
                    <div
                      style={{
                        fontWeight: 600,
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {app.name}{" "}
                      {app.version ? (
                        <span style={{ color: "var(--text-muted)", fontWeight: 400 }}>
                          v{app.version}
                        </span>
                      ) : null}
                    </div>
                    <div
                      style={{
                        fontSize: 11.5,
                        color: "var(--text-muted)",
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {app.publisher || "Editor desconocido"}
                      {app.installLocation ? ` · ${app.installLocation}` : ""}
                    </div>
                  </div>

                  <div>
                    <span className={`badge ${app.source === "store_msix" ? "badge-info" : ""}`}>
                      {app.source === "store_msix" ? "MSIX" : "Win32"}
                    </span>
                  </div>

                  <div style={{ textAlign: "right" }}>
                    <div className="mono" style={{ fontWeight: 600 }}>
                      {app.sizeSource === "unknown" ? "N/D" : formatBytes(app.sizeBytes)}
                    </div>
                    <div style={{ fontSize: 11, color: "var(--text-muted)" }}>
                      {app.sizeSource === "measured"
                        ? "Medido en disco"
                        : app.sizeSource === "estimated"
                          ? "Estimado"
                          : "Acceso restringido"}
                    </div>
                  </div>

                  <div style={{ display: "flex", justifyContent: "flex-end" }}>
                    <button
                      type="button"
                      className="btn btn-danger"
                      style={{ padding: "5px 10px" }}
                      onClick={() => handleUninstall(app)}
                    >
                      <Trash2 size={13} />
                      Desinstalar
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}
