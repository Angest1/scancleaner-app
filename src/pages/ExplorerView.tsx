import { AlertTriangle, FolderTree, Home, RefreshCw, ShieldAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { BreadcrumbBar } from "../components/BreadcrumbBar";
import { CategoryCharts } from "../components/CategoryCharts";
import { SystemFilesPanel } from "../components/SystemFilesPanel";
import { TopEntriesTable } from "../components/TopEntriesTable";
import { TreemapCanvas } from "../components/TreemapCanvas";
import { Badge } from "../components/ui/Badge";
import { Button } from "../components/ui/Button";
import { Card } from "../components/ui/Card";
import { Collapse } from "../components/ui/Collapse";
import { EmptyState } from "../components/ui/Feedback";
import { SegmentedControl } from "../components/ui/SegmentedControl";
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
  onGoHome: () => void;
  onRelaunchAdmin: () => void;
  onQueueCleanup: (path: string, category: string) => void;
}

export function ExplorerView({
  scanSummary,
  rulesPath,
  isElevated,
  onGoHome,
  onRelaunchAdmin,
  onQueueCleanup,
}: ExplorerViewProps) {
  const [nodeId, setNodeId] = useState(0);
  const [treemap, setTreemap] = useState<TreemapViewDto | null>(null);
  const [rankingKind, setRankingKind] = useState<"dirs" | "files">("dirs");
  const [topEntries, setTopEntries] = useState<TopEntryDto[]>([]);
  const [breakdown, setBreakdown] = useState<CategoryBreakdown | null>(null);
  const [colorBy, setColorBy] = useState<"type" | "origin">("type");
  const [tab, setTab] = useState<"ranking" | "categories" | "system">("ranking");
  const [deniedPaths, setDeniedPaths] = useState<string[]>([]);
  const [showDenied, setShowDenied] = useState(false);
  const [protectionRevision, setProtectionRevision] = useState(0);

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
    void protectionRevision;
    api
      .getTreemapChildren(nodeId, 140)
      .then(setTreemap)
      .catch(() => {});
    api
      .getTopEntries(nodeId, rankingKind, 100)
      .then(setTopEntries)
      .catch(() => {});
  }, [scanSummary, nodeId, rankingKind, protectionRevision]);

  if (!scanSummary) {
    return (
      <EmptyState
        fullHeight
        icon={<FolderTree size={20} />}
        title="No hay ningún escaneo cargado"
        description="Ve a Inicio y escanea una unidad para explorar su Treemap, ranking de espacio y categorías."
        action={
          <Button variant="primary" onClick={onGoHome}>
            <Home size={13} />
            Ir a Inicio
          </Button>
        }
      />
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

  const handleProtectPath = (fullPath: string, isDir: boolean) => {
    api
      .protectionAdd(
        fullPath,
        isDir ? "carpeta-recursiva" : "archivo",
        "Protegida desde el Explorador.",
        false,
      )
      .then(() => setProtectionRevision((v) => v + 1))
      .catch((err) => alert(String(err)));
  };

  const skippedLinks = scanSummary.stats.symlinksSkipped + scanSummary.stats.junctionsSkipped;

  return (
    <div className="flex flex-col gap-4">
      {/* Scan summary strip */}
      <Card className="flex items-center justify-between flex-wrap gap-3">
        <div>
          <div className="text-[12px] text-fg-muted">
            Escaneo de <span className="mono text-fg-secondary">{scanSummary.rootPath}</span> (
            {scanSummary.fsType.toUpperCase()})
          </div>
          <div className="flex items-center gap-4 flex-wrap mt-1 text-[13px] tabular">
            <span>
              <strong className="mono font-semibold text-fg-primary">
                {formatBytes(scanSummary.stats.totalAllocatedBytes)}
              </strong>{" "}
              <span className="text-fg-secondary">en disco</span>
            </span>
            <span className="text-fg-secondary">
              {scanSummary.stats.totalFiles.toLocaleString()} archivos
            </span>
            <span className="text-fg-secondary">
              {scanSummary.stats.totalDirs.toLocaleString()} carpetas
            </span>
            <span className="text-fg-secondary">
              Duración:{" "}
              <span className="mono">{formatDurationMs(scanSummary.stats.durationMs)}</span>
            </span>
            {scanSummary.stats.hardlinksDeduplicated > 0 && (
              <Badge tone="accent">
                {scanSummary.stats.hardlinksDeduplicated.toLocaleString()} hardlinks deduplicados
              </Badge>
            )}
            {skippedLinks > 0 && <Badge>{skippedLinks.toLocaleString()} enlaces omitidos</Badge>}
          </div>
        </div>

        {scanSummary.stats.accessDeniedCount > 0 && (
          <Button variant="warning" size="sm" onClick={() => setShowDenied((v) => !v)}>
            <AlertTriangle size={13} />
            Omitidas por permisos ({scanSummary.stats.accessDeniedCount})
          </Button>
        )}
      </Card>

      <Collapse open={showDenied && deniedPaths.length > 0}>
        <Card className="border-warning-border bg-warning-soft/50">
          <div className="flex items-center justify-between flex-wrap gap-2 mb-2">
            <div className="flex items-center gap-2 font-semibold text-[13px] text-fg-primary">
              <ShieldAlert size={15} className="text-warning" />
              Rutas sin acceso durante el escaneo (mostrando hasta {deniedPaths.length})
            </div>
            {!isElevated && (
              <Button variant="warning" size="sm" onClick={onRelaunchAdmin}>
                Reiniciar como admin para acceso amplio
              </Button>
            )}
          </div>
          <div className="max-h-[140px] overflow-auto mono text-[11.5px] text-fg-secondary flex flex-col gap-0.5">
            {deniedPaths.map((p) => (
              <div key={p} className="truncate">
                {p}
              </div>
            ))}
          </div>
        </Card>
      </Collapse>

      {/* Treemap container */}
      <Card className="flex flex-col gap-3">
        <div className="flex items-center justify-between flex-wrap gap-2.5">
          <BreadcrumbBar
            breadcrumbs={treemap?.breadcrumbs ?? []}
            onSelectNode={(id) => setNodeId(id)}
          />
          <div className="flex items-center gap-2">
            <span className="text-[12px] text-fg-muted">Color por</span>
            <SegmentedControl
              ariaLabel="Colorear treemap por"
              value={colorBy}
              onChange={setColorBy}
              options={[
                { value: "type", label: "Tipo" },
                { value: "origin", label: "Origen" },
              ]}
            />
          </div>
        </div>

        <TreemapCanvas
          nodes={treemap?.children ?? []}
          colorBy={colorBy}
          onDrillDown={(id) => setNodeId(id)}
          onOpenExplorer={handleOpenExplorer}
          onSendToCleanup={(n: TreemapNodeDto) => onQueueCleanup(n.fullPath, n.originCat)}
          onProtectPath={(n: TreemapNodeDto) => handleProtectPath(n.fullPath, n.kind === "dir")}
        />
      </Card>

      {/* Bottom analysis tabs */}
      <Card className="flex flex-col gap-3.5">
        <div className="flex items-center justify-between flex-wrap gap-2.5">
          <SegmentedControl
            ariaLabel="Panel de análisis"
            value={tab}
            onChange={setTab}
            options={[
              { value: "ranking", label: "Ranking Top 100" },
              { value: "categories", label: "Categorías" },
              { value: "system", label: "Sistema (no eliminable)" },
            ]}
          />

          {tab === "ranking" ? (
            <SegmentedControl
              ariaLabel="Tipo de ranking"
              value={rankingKind}
              onChange={setRankingKind}
              options={[
                { value: "dirs", label: "Carpetas" },
                { value: "files", label: "Archivos" },
              ]}
            />
          ) : (
            <div className="flex items-center gap-2">
              <span className="mono text-[11.5px] text-fg-muted" title={rulesPath}>
                rules.json
              </span>
              <Button size="sm" onClick={handleReloadRules}>
                <RefreshCw size={12} />
                Recargar reglas
              </Button>
            </div>
          )}
        </div>

        <div key={tab} className="animate-view-in">
          {tab === "ranking" ? (
            <TopEntriesTable
              entries={topEntries}
              kind={rankingKind}
              onDrillDown={(id) => setNodeId(id)}
              onOpenExplorer={handleOpenExplorer}
              onSendToCleanup={(entry) => onQueueCleanup(entry.fullPath, entry.originCat)}
              onProtectPath={(entry) => handleProtectPath(entry.fullPath, rankingKind === "dirs")}
            />
          ) : tab === "categories" ? (
            <CategoryCharts breakdown={breakdown} />
          ) : (
            <SystemFilesPanel driveRoot={scanSummary.rootPath} />
          )}
        </div>
      </Card>
    </div>
  );
}
