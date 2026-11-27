import {
  CheckSquare,
  RefreshCw,
  Search,
  ShieldCheck,
  Sparkles,
  Square,
  Trash2,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { DonutChart } from "../components/charts/DonutChart";
import { HorizontalBarChart } from "../components/charts/HorizontalBarChart";
import { CleanCandidateRow } from "../components/cleaner/CleanCandidateRow";
import { InlineActivityBanner } from "../components/ui/ActionMotion";
import { Button } from "../components/ui/Button";
import { Card } from "../components/ui/Card";
import { type CategoryTreeNode, CategoryTreeSidebar } from "../components/ui/CategoryTreeSidebar";
import { Collapse } from "../components/ui/Collapse";
import { AlertBanner, EmptyState } from "../components/ui/Feedback";
import { SelectionActionBar } from "../components/ui/SelectionActionBar";
import { formatBytes } from "../lib/format";
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
  syncRevision?: number;
  onRelaunchAdmin: () => void;
  onStartPreview: (targets: CleanupTargetInput[], mode: CleanupMode) => void;
}

const CATEGORY_ORDER: CleanCategory[] = [
  "user_temp",
  "system_temp",
  "browser_cache",
  "dev_artifact",
  "large_old_file",
  "recycle_bin",
];

const CATEGORY_META: Record<CleanCategory, { label: string; color: string }> = {
  user_temp: { label: "Temporales de usuario", color: "#3b82f6" },
  system_temp: { label: "Temporales del sistema", color: "#f59e0b" },
  recycle_bin: { label: "Papelera de reciclaje", color: "#ef4444" },
  browser_cache: { label: "Cachés de navegadores", color: "#8b5cf6" },
  dev_artifact: { label: "Artefactos de desarrollo", color: "#10b981" },
  large_old_file: { label: "Archivos grandes y antiguos", color: "#06b6d4" },
};

export function CleanerView({
  isElevated,
  syncRevision = 0,
  onRelaunchAdmin,
  onStartPreview,
}: CleanerViewProps) {
  const [report, setReport] = useState<CleanupCandidatesReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [activeCat, setActiveCat] = useState<string>("all");
  const [search, setSearch] = useState("");
  const [minLargeMb, setMinLargeMb] = useState(100);
  const [minAgeDays, setMinAgeDays] = useState(30);

  const loadCandidates = useCallback(
    (preserveSelection = false, silent = false) => {
      if (!silent) setLoading(true);
      api
        .discoverCleanupCandidates(minLargeMb, minAgeDays)
        .then((res) => {
          setReport(res);
          if (preserveSelection) {
            const validIds = new Set(res.candidates.map((c) => c.id));
            setSelectedIds((prev) => new Set([...prev].filter((id) => validIds.has(id))));
          } else {
            setSelectedIds(new Set());
          }
        })
        .finally(() => {
          if (!silent) setLoading(false);
        });
    },
    [minLargeMb, minAgeDays],
  );

  useEffect(() => {
    loadCandidates(false, false);
  }, [loadCandidates]);

  useEffect(() => {
    if (syncRevision > 0) loadCandidates(true, true);
  }, [syncRevision, loadCandidates]);

  const toggleItem = (item: CleanCandidate) => {
    if (item.requiresAdmin && !isElevated) return;
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(item.id)) next.delete(item.id);
      else next.add(item.id);
      return next;
    });
  };

  const allCandidates = report?.candidates ?? [];
  const totalBytes = useMemo(
    () => allCandidates.reduce((acc, c) => acc + c.sizeBytes, 0),
    [allCandidates],
  );

  const groupedByCat = useMemo(() => {
    const map = new Map<CleanCategory, CleanCandidate[]>();
    for (const c of allCandidates) {
      const list = map.get(c.category) ?? [];
      list.push(c);
      map.set(c.category, list);
    }
    return map;
  }, [allCandidates]);

  const treeNodes = useMemo<CategoryTreeNode[]>(() => {
    return CATEGORY_ORDER.filter((cat) => groupedByCat.has(cat)).map((cat) => {
      const items = groupedByCat.get(cat) ?? [];
      return {
        id: cat,
        label: CATEGORY_META[cat].label,
        count: items.length,
        bytes: items.reduce((acc, i) => acc + i.sizeBytes, 0),
        color: CATEGORY_META[cat].color,
      };
    });
  }, [groupedByCat]);

  const visibleCandidates = useMemo(() => {
    const q = search.trim().toLowerCase();
    return allCandidates.filter((c) => {
      if (activeCat !== "all" && c.category !== activeCat) return false;
      if (!q) return true;
      return c.displayName.toLowerCase().includes(q) || c.path.toLowerCase().includes(q);
    });
  }, [allCandidates, activeCat, search]);

  const selectableVisible = useMemo(
    () => visibleCandidates.filter((c) => !(c.requiresAdmin && !isElevated)),
    [visibleCandidates, isElevated],
  );
  const allVisibleSelected =
    selectableVisible.length > 0 && selectableVisible.every((c) => selectedIds.has(c.id));

  const toggleSelectVisible = () => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (allVisibleSelected) {
        for (const c of selectableVisible) next.delete(c.id);
      } else {
        for (const c of selectableVisible) next.add(c.id);
      }
      return next;
    });
  };

  const selectedCandidates = useMemo(
    () => allCandidates.filter((c) => selectedIds.has(c.id)),
    [allCandidates, selectedIds],
  );
  const selectedBytes = useMemo(
    () => selectedCandidates.reduce((acc, c) => acc + c.sizeBytes, 0),
    [selectedCandidates],
  );

  const handlePreview = () => {
    const targets: CleanupTargetInput[] = selectedCandidates.map((c) => ({
      path: c.path,
      category: c.category,
      isTempDirContents: c.category === "user_temp" || c.category === "system_temp",
    }));
    onStartPreview(targets, "recycle_bin");
  };

  return (
    <div className="flex flex-col gap-4 pb-6">
      <div className="flex items-start justify-between flex-wrap gap-3">
        <div>
          <h1 className="m-0 text-[18px] font-semibold tracking-[-0.02em] text-fg-primary">
            Limpieza segura
          </h1>
          <p className="m-0 mt-1 text-[13px] text-fg-secondary">
            Navega por categorías desde el árbol izquierdo. Todo pasa por Dry-run y Papelera por
            defecto.
          </p>
        </div>

        <div className="flex items-center gap-2 flex-wrap">
          <label className="text-[12px] text-fg-secondary flex items-center gap-1.5">
            Mín. MB
            <input
              type="number"
              className="w-[68px] h-8 px-2 rounded-[7px] bg-canvas border border-border-default mono text-[12px] text-fg-primary focus:border-accent focus:outline-none"
              min={10}
              value={minLargeMb}
              onChange={(e) => setMinLargeMb(Math.max(10, Number(e.target.value) || 100))}
            />
          </label>
          <label className="text-[12px] text-fg-secondary flex items-center gap-1.5">
            Días
            <input
              type="number"
              className="w-[60px] h-8 px-2 rounded-[7px] bg-canvas border border-border-default mono text-[12px] text-fg-primary focus:border-accent focus:outline-none"
              min={1}
              value={minAgeDays}
              onChange={(e) => setMinAgeDays(Math.max(1, Number(e.target.value) || 30))}
            />
          </label>
          <Button onClick={() => loadCandidates(true)} disabled={loading}>
            <RefreshCw size={13} className={loading ? "animate-spin" : ""} />
            {loading ? "Analizando…" : "Reanalizar"}
          </Button>
          <Button
            variant="primary"
            disabled={selectedCandidates.length === 0}
            onClick={handlePreview}
          >
            <ShieldCheck size={13} />
            Previsualizar ({selectedCandidates.length} · {formatBytes(selectedBytes)})
          </Button>
        </div>
      </div>

      <InlineActivityBanner
        active={loading}
        variant={!report ? "centered" : "floating"}
        delayMs={!report ? 180 : 280}
        title="Analizando candidatos de limpieza segura…"
        detail="Consultando índices precalculados, cachés de navegadores y Papelera."
      />

      <Collapse open={Boolean(report && report.runningBrowsers.length > 0)}>
        {report && report.runningBrowsers.length > 0 && (
          <AlertBanner tone="warning">
            Navegadores en ejecución: <strong>{report.runningBrowsers.join(", ")}</strong>. Cierra
            el navegador antes de limpiar su caché para evitar bloqueos.
          </AlertBanner>
        )}
      </Collapse>

      {/* Recoverable Space Charts */}
      <Collapse open={totalBytes > 0}>
        {totalBytes > 0 && (
          <Card className="grid grid-cols-1 lg:grid-cols-12 gap-4 items-center">
            <div className="lg:col-span-5">
              <DonutChart
                size={128}
                strokeWidth={16}
                centerValue={formatBytes(totalBytes)}
                centerLabel="Recuperable"
                onSelectSegment={(key) => setActiveCat(key)}
                segments={treeNodes.map((n) => ({
                  key: n.id,
                  label: n.label,
                  value: n.bytes ?? 0,
                  valueLabel: formatBytes(n.bytes ?? 0),
                  color: n.color ?? "var(--accent)",
                }))}
              />
            </div>
            <div className="lg:col-span-7">
              <HorizontalBarChart
                maxItems={6}
                onSelectItem={(key) => setActiveCat(key)}
                items={treeNodes.map((n) => ({
                  key: n.id,
                  label: n.label,
                  sublabel: `${n.count} ítems`,
                  value: n.bytes ?? 0,
                  valueLabel: formatBytes(n.bytes ?? 0),
                  color: n.color ?? "var(--accent)",
                }))}
              />
            </div>
          </Card>
        )}
      </Collapse>

      {/* Main 2-column layout: Category Tree + Candidate List */}
      <div className="flex flex-col lg:flex-row gap-4 items-start">
        <CategoryTreeSidebar
          title="Categorías de limpieza"
          rootLabel="Todas las categorías"
          totalCount={allCandidates.length}
          totalBytes={totalBytes}
          nodes={treeNodes}
          selectedId={activeCat}
          onSelect={setActiveCat}
          footer={
            <div className="flex flex-col gap-2">
              <div className="text-[11.5px] text-fg-secondary tabular">
                <strong className="text-fg-primary">Papelera:</strong>{" "}
                {report
                  ? `${report.recycleBinItems.toLocaleString()} · ${formatBytes(report.recycleBinBytes)}`
                  : "…"}
              </div>
              <Button
                variant="danger"
                size="sm"
                className="w-full justify-center"
                disabled={!report || report.recycleBinItems === 0}
                onClick={() => onStartPreview([], "empty_recycle_bin")}
              >
                <Trash2 size={12} />
                Vaciar Papelera
              </Button>
            </div>
          }
        />

        <div className="flex-1 min-w-0 w-full flex flex-col gap-3">
          {/* Search + bulk select toolbar */}
          <Card className="flex items-center justify-between flex-wrap gap-2.5 py-2.5">
            <div className="relative flex-1 min-w-[180px]">
              <Search
                size={13}
                className="absolute left-2.5 top-1/2 -translate-y-1/2 text-fg-muted pointer-events-none"
              />
              <input
                type="text"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Filtrar por nombre o ruta…"
                className="w-full h-8 pl-8 pr-7 rounded-[8px] bg-canvas border border-border-default text-[12.5px] text-fg-primary placeholder:text-fg-muted focus:border-accent focus:outline-none"
              />
              {search && (
                <button
                  type="button"
                  onClick={() => setSearch("")}
                  aria-label="Limpiar filtro"
                  className="absolute right-2 top-1/2 -translate-y-1/2 text-fg-muted hover:text-fg-primary bg-transparent border-0 p-0.5 cursor-pointer"
                >
                  <X size={12} />
                </button>
              )}
            </div>

            <div className="flex items-center gap-2">
              <span className="mono text-[11.5px] text-fg-muted">
                {visibleCandidates.length} ítems
              </span>
              <Button
                size="sm"
                onClick={toggleSelectVisible}
                disabled={selectableVisible.length === 0}
              >
                {allVisibleSelected ? <Square size={12} /> : <CheckSquare size={12} />}
                {allVisibleSelected ? "Deseleccionar vista" : "Seleccionar vista"}
              </Button>
            </div>
          </Card>

          {loading && !report ? null : visibleCandidates.length === 0 ? (
            <EmptyState
              icon={<Sparkles size={18} />}
              title="Sin elementos en esta vista"
              description="Ajusta el filtro, cambia la categoría en el árbol izquierdo o modifica los umbrales."
            />
          ) : (
            <Card key={activeCat} padded={false} className="sc-reveal overflow-hidden">
              <div className="min-h-[360px] max-h-[calc(100vh-300px)] overflow-y-auto flex flex-col divide-y divide-border-subtle">
                {visibleCandidates.map((item) => (
                  <CleanCandidateRow
                    key={item.id}
                    item={item}
                    color={CATEGORY_META[item.category].color}
                    checked={selectedIds.has(item.id)}
                    locked={item.requiresAdmin && !isElevated}
                    onToggle={toggleItem}
                    onRelaunchAdmin={onRelaunchAdmin}
                  />
                ))}
              </div>
            </Card>
          )}
        </div>
      </div>

      <SelectionActionBar
        count={selectedCandidates.length}
        bytes={selectedBytes}
        noun="elementos seleccionados"
        actionLabel="Previsualizar limpieza"
        onClear={() => setSelectedIds(new Set())}
        onAction={handlePreview}
      />
    </div>
  );
}
