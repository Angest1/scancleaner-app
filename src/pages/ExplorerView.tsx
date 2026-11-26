import { AlertTriangle, RefreshCw, ShieldAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { BreadcrumbBar } from "../components/BreadcrumbBar";
import { CategoryCharts } from "../components/CategoryCharts";
import { TopEntriesTable } from "../components/TopEntriesTable";
import { TreemapCanvas } from "../components/TreemapCanvas";
import { formatBytes, formatDurationMs } from "../lib/format";
import {
  type CategoryBreakdown,
  type ScanSummary,
  type TopEntryDto,
  type TreemapNodeDto,
  type TreemapViewDto,
  api,
} from "../lib/ipc";

interface ExplorerViewProps {
  scanSummary: ScanSummary | null;
  rulesPath: string;
  isElevated: boolean;
  onRelaunchAdmin: () => void;
  onQueueCleanup: (path: string, category: string) => void;
}

export function ExplorerView({
  scanSummary,
  rulesPath,
  isElevated,
  onRelaunchAdmin,
  onQueueCleanup,
}: ExplorerViewProps) {
  const [nodeId, setNodeId] = useState(0);
  const [treemap, setTreemap] = useState<TreemapViewDto | null>(null);
  const [rankingKind, setRankingKind] = useState<"dirs" | "files">("dirs");
  const [topEntries, setTopEntries] = useState<TopEntryDto[]>([]);
  const [breakdown, setBreakdown] = useState<CategoryBreakdown | null>(null);
  const [colorBy, setColorBy] = useState<"type" | "origin">("type");
  const [tab, setTab] = useState<"ranking" | "categories">("ranking");
  const [deniedPaths, setDeniedPaths] = useState<string[]>([]);
  const [showDenied, setShowDenied] = useState(false);

  useEffect(() => {
    if (!scanSummary) return;
    setNodeId(0);
    api
      .getCategoryBreakdown()
      .then(setBreakdown)
      .catch(() => {});
    api
      .getAccessDenied(200)
      .then(setDeniedPaths)
      .catch(() => {});
  }, [scanSummary]);

  useEffect(() => {
    if (!scanSummary) return;
    api
      .getTreemapChildren(nodeId, 140)
      .then(setTreemap)
      .catch(() => {});
    api
      .getTopEntries(nodeId, rankingKind, 100)
      .then(setTopEntries)
      .catch(() => {});
  }, [scanSummary, nodeId, rankingKind]);

  if (!scanSummary) {
    return (
      <div className="card" style={{ padding: 36, textAlign: "center" }}>
        <h3 style={{ margin: "0 0 8px" }}>No hay ningún escaneo cargado</h3>
        <p style={{ margin: 0, color: "var(--text-secondary)" }}>
          Ve a la pestaña <strong>Unidades</strong> y escanea un disco para explorar su Treemap,
          ranking y categorías.
        </p>
      </div>
    );
  }

  const handleOpenExplorer = (path: string) => {
    api.openInExplorer(path).catch((err) => alert(String(err)));
  };

  const handleReloadRules = () => {
    api
      .reloadRules()
      .then(setBreakdown)
      .catch((err) => alert(String(err)));
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
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
          <div style={{ fontSize: 12, color: "var(--text-muted)" }}>
            Escaneo de <span className="mono">{scanSummary.rootPath}</span> (
            {scanSummary.fsType.toUpperCase()})
          </div>
          <div style={{ display: "flex", gap: 16, flexWrap: "wrap", marginTop: 4, fontSize: 13 }}>
            <span>
              <strong>{formatBytes(scanSummary.stats.totalAllocatedBytes)}</strong> en disco
            </span>
            <span>{scanSummary.stats.totalFiles.toLocaleString()} archivos</span>
            <span>{scanSummary.stats.totalDirs.toLocaleString()} carpetas</span>
            <span>Duración: {formatDurationMs(scanSummary.stats.durationMs)}</span>
            {scanSummary.stats.hardlinksDeduplicated > 0 && (
              <span className="badge badge-info">
                {scanSummary.stats.hardlinksDeduplicated.toLocaleString()} hardlinks deduplicados
              </span>
            )}
            {scanSummary.stats.symlinksSkipped + scanSummary.stats.junctionsSkipped > 0 && (
              <span className="badge">
                {(
                  scanSummary.stats.symlinksSkipped + scanSummary.stats.junctionsSkipped
                ).toLocaleString()}{" "}
                enlaces omitidos
              </span>
            )}
          </div>
        </div>

        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          {scanSummary.stats.accessDeniedCount > 0 && (
            <button
              type="button"
              className="btn btn-warning"
              onClick={() => setShowDenied((v) => !v)}
            >
              <AlertTriangle size={14} />
              Omitidas por permisos ({scanSummary.stats.accessDeniedCount})
            </button>
          )}
        </div>
      </div>

      {showDenied && deniedPaths.length > 0 && (
        <div className="card" style={{ borderColor: "rgba(245, 158, 11, 0.5)" }}>
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              marginBottom: 8,
            }}
          >
            <div style={{ display: "flex", alignItems: "center", gap: 8, fontWeight: 600 }}>
              <ShieldAlert size={16} color="#f59e0b" />
              Rutas sin acceso durante el escaneo (mostrando hasta {deniedPaths.length})
            </div>
            {!isElevated && (
              <button type="button" className="btn btn-warning" onClick={onRelaunchAdmin}>
                Reiniciar como admin para acceso amplio
              </button>
            )}
          </div>
          <div style={{ maxHeight: 140, overflow: "auto", fontSize: 12 }} className="mono">
            {deniedPaths.map((p) => (
              <div key={p} style={{ color: "var(--text-secondary)", padding: "2px 0" }}>
                {p}
              </div>
            ))}
          </div>
        </div>
      )}

      <div className="card" style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            flexWrap: "wrap",
            gap: 10,
          }}
        >
          <BreadcrumbBar
            breadcrumbs={treemap?.breadcrumbs ?? []}
            onSelectNode={(id) => setNodeId(id)}
          />
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span style={{ fontSize: 12, color: "var(--text-muted)" }}>Color por:</span>
            <button
              type="button"
              className={`btn ${colorBy === "type" ? "btn-primary" : ""}`}
              style={{ padding: "5px 10px" }}
              onClick={() => setColorBy("type")}
            >
              Tipo
            </button>
            <button
              type="button"
              className={`btn ${colorBy === "origin" ? "btn-primary" : ""}`}
              style={{ padding: "5px 10px" }}
              onClick={() => setColorBy("origin")}
            >
              Origen
            </button>
          </div>
        </div>

        <TreemapCanvas
          nodes={treemap?.children ?? []}
          colorBy={colorBy}
          onDrillDown={(id) => setNodeId(id)}
          onOpenExplorer={handleOpenExplorer}
          onSendToCleanup={(n: TreemapNodeDto) => onQueueCleanup(n.fullPath, n.originCat)}
        />
      </div>

      <div className="card" style={{ display: "flex", flexDirection: "column", gap: 14 }}>
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            flexWrap: "wrap",
            gap: 10,
          }}
        >
          <div style={{ display: "flex", gap: 8 }}>
            <button
              type="button"
              className={`btn ${tab === "ranking" ? "btn-primary" : ""}`}
              onClick={() => setTab("ranking")}
            >
              Ranking Top 100
            </button>
            <button
              type="button"
              className={`btn ${tab === "categories" ? "btn-primary" : ""}`}
              onClick={() => setTab("categories")}
            >
              Categorías
            </button>
          </div>

          {tab === "ranking" ? (
            <div style={{ display: "flex", gap: 8 }}>
              <button
                type="button"
                className={`btn ${rankingKind === "dirs" ? "btn-primary" : ""}`}
                onClick={() => setRankingKind("dirs")}
              >
                Carpetas más grandes
              </button>
              <button
                type="button"
                className={`btn ${rankingKind === "files" ? "btn-primary" : ""}`}
                onClick={() => setRankingKind("files")}
              >
                Archivos más grandes
              </button>
            </div>
          ) : (
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span
                className="mono"
                style={{ fontSize: 11.5, color: "var(--text-muted)" }}
                title={rulesPath}
              >
                Reglas: rules.json
              </span>
              <button type="button" className="btn" onClick={handleReloadRules}>
                <RefreshCw size={13} />
                Recargar reglas
              </button>
            </div>
          )}
        </div>

        {tab === "ranking" ? (
          <TopEntriesTable
            entries={topEntries}
            kind={rankingKind}
            onDrillDown={(id) => setNodeId(id)}
            onOpenExplorer={handleOpenExplorer}
            onSendToCleanup={(entry) => onQueueCleanup(entry.fullPath, entry.originCat)}
          />
        ) : (
          <CategoryCharts breakdown={breakdown} />
        )}
      </div>
    </div>
  );
}
