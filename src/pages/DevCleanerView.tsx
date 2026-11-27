import { CheckSquare, Code2, RefreshCw, Search, ShieldCheck, Square, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { DevCleanupModal } from "../components/DevCleanupModal";
import { DevCandidateRow } from "../components/dev-cleaner/DevCandidateRow";
import { DevProjectRootsCard } from "../components/dev-cleaner/DevProjectRootsCard";
import { DEV_CATEGORY_COLORS, DevSummaryCharts } from "../components/dev-cleaner/DevSummaryCharts";
import { DockerWslCard } from "../components/dev-cleaner/DockerWslCard";
import { InlineActivityBanner } from "../components/ui/ActionMotion";
import { Button } from "../components/ui/Button";
import { Card } from "../components/ui/Card";
import { type CategoryTreeNode, CategoryTreeSidebar } from "../components/ui/CategoryTreeSidebar";
import { Collapse } from "../components/ui/Collapse";
import { AlertBanner, EmptyState } from "../components/ui/Feedback";
import { SegmentedControl } from "../components/ui/SegmentedControl";
import { SelectionActionBar } from "../components/ui/SelectionActionBar";
import { formatBytes } from "../lib/format";
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
  syncRevision?: number;
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

type DevScanReportState = Awaited<ReturnType<typeof api.devScan>> | null;

export function DevCleanerView({
  isElevated,
  syncRevision = 0,
  onRelaunchAdmin,
}: DevCleanerViewProps) {
  const [config, setConfig] = useState<DevCleanerConfig>({
    projectRoots: [],
    minProjectAgeDays: 30,
    disabledRuleIds: [],
  });
  const [report, setReport] = useState<DevScanReportState>(null);
  const [loading, setLoading] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [activeCat, setActiveCat] = useState<string>("all");
  const [search, setSearch] = useState("");
  const [levelFilter, setLevelFilter] = useState<"all" | "seguro" | "revisar">("all");

  // Dry-run & execution modal state
  const [useRecycleBin, setUseRecycleBin] = useState(false);
  const [activePlan, setActivePlan] = useState<DevCleanupPlan | null>(null);
  const [executing, setExecuting] = useState(false);
  const [result, setResult] = useState<DevCleanupResult | null>(null);

  const runScan = useCallback((cfg: DevCleanerConfig, preserve = false, silent = false) => {
    if (!silent) setLoading(true);
    api
      .devScan(cfg)
      .then((res) => {
        setReport(res);
        setSelectedIds((prev) =>
          preserve
            ? new Set([...prev].filter((id) => res.candidates.some((c) => c.id === id)))
            : new Set(res.candidates.filter((c) => c.preselected).map((c) => c.id)),
        );
      })
      .catch((err) => alert(String(err)))
      .finally(() => {
        if (!silent) setLoading(false);
      });
  }, []);

  const configRef = useRef(config);
  configRef.current = config;

  useEffect(() => {
    api.devGetConfig().then((cfg) => {
      setConfig(cfg);
      runScan(cfg, false, false);
    });
  }, [runScan]);

  useEffect(() => {
    if (syncRevision > 0) runScan(configRef.current, true, true);
  }, [syncRevision, runScan]);

  const persistConfig = (next: DevCleanerConfig, rescan = false) => {
    setConfig(next);
    api
      .devSaveConfig(next)
      .then(() => rescan && runScan(next))
      .catch((err) => alert(String(err)));
  };

  const addRootIfNew = (root: string) => {
    if (config.projectRoots.some((r) => r.toLowerCase() === root.toLowerCase())) return;
    persistConfig({ ...config, projectRoots: [...config.projectRoots, root] }, true);
  };

  const handlePickFolder = () => {
    api
      .devPickProjectRoot()
      .then((picked) => picked && addRootIfNew(picked))
      .catch((err) => alert(String(err)));
  };

  const handleRemoveRoot = (root: string) =>
    persistConfig({ ...config, projectRoots: config.projectRoots.filter((r) => r !== root) }, true);

  const isLocked = useCallback(
    (c: DevCandidate) => (c.requiresAdmin && !isElevated) || c.blockedReason !== null,
    [isElevated],
  );

  const toggleItem = (c: DevCandidate) => {
    if (isLocked(c)) return;
    setSelectedIds((prev) => {
      const next = new Set(prev);
      next.has(c.id) ? next.delete(c.id) : next.add(c.id);
      return next;
    });
  };

  const allCandidates = report?.candidates ?? [];
  const selectedCandidates = useMemo(
    () => allCandidates.filter((c) => selectedIds.has(c.id) && !isLocked(c)),
    [allCandidates, selectedIds, isLocked],
  );
  const selectedBytes = useMemo(
    () => selectedCandidates.reduce((acc, c) => acc + c.sizeBytes, 0),
    [selectedCandidates],
  );

  const treeNodes = useMemo<CategoryTreeNode[]>(() => {
    const map = new Map<DevRuleCategory, DevCandidate[]>();
    for (const c of allCandidates) {
      if (levelFilter !== "all" && c.nivel !== levelFilter) continue;
      const list = map.get(c.categoria) ?? [];
      list.push(c);
      map.set(c.categoria, list);
    }
    return CATEGORY_ORDER.filter((cat) => map.has(cat)).map((cat) => {
      const items = map.get(cat) ?? [];
      return {
        id: cat,
        label: cat,
        count: items.length,
        bytes: items.reduce((a, b) => a + b.sizeBytes, 0),
        color: DEV_CATEGORY_COLORS[cat],
      };
    });
  }, [allCandidates, levelFilter]);

  const visibleCandidates = useMemo(() => {
    const q = search.trim().toLowerCase();
    return allCandidates.filter((c) => {
      if (levelFilter !== "all" && c.nivel !== levelFilter) return false;
      if (activeCat !== "all" && c.categoria !== activeCat) return false;
      if (!q) return true;
      return (
        c.nombre.toLowerCase().includes(q) ||
        c.path.toLowerCase().includes(q) ||
        c.ruleId.toLowerCase().includes(q)
      );
    });
  }, [allCandidates, levelFilter, activeCat, search]);

  const selectableVisible = useMemo(
    () => visibleCandidates.filter((c) => !isLocked(c)),
    [visibleCandidates, isLocked],
  );
  const allVisibleSelected =
    selectableVisible.length > 0 && selectableVisible.every((c) => selectedIds.has(c.id));

  const toggleSelectVisible = () =>
    setSelectedIds((prev) => {
      const next = new Set(prev);
      for (const c of selectableVisible) {
        if (allVisibleSelected) next.delete(c.id);
        else next.add(c.id);
      }
      return next;
    });

  const openPreview = (recycleOption: boolean) => {
    if (selectedCandidates.length === 0) return;
    setUseRecycleBin(recycleOption);
    setResult(null);
    api
      .devPreview(
        selectedCandidates.map((candidate) => ({ candidate, useRecycleBin: recycleOption })),
      )
      .then(setActivePlan)
      .catch((err) => alert(String(err)));
  };

  const handleExecute = () => {
    if (!activePlan) return;
    setExecuting(true);
    api
      .devExecute(activePlan.planId)
      .then(setResult)
      .catch((err) => alert(String(err)))
      .finally(() => setExecuting(false));
  };

  return (
    <div className="flex flex-col gap-4 pb-6">
      {/* Header */}
      <div className="flex items-start justify-between flex-wrap gap-3">
        <div>
          <h1 className="m-0 text-[18px] font-semibold tracking-[-0.02em] text-fg-primary">
            Limpieza Dev
          </h1>
          <p className="m-0 mt-1 text-[13px] text-fg-secondary text-pretty">
            Solo archivos regenerables y cachés de desarrollo. Lista negra inmutable activa (nunca
            toca código fuente, .git, .env, keystores ni volúmenes Docker).
          </p>
        </div>

        <div className="flex items-center gap-2 flex-wrap">
          <Button onClick={() => runScan(config)} disabled={loading}>
            <RefreshCw size={13} className={loading ? "animate-spin" : ""} />
            {loading ? "Escaneando…" : "Reescanear"}
          </Button>
          <Button
            variant="primary"
            disabled={selectedCandidates.length === 0}
            onClick={() => openPreview(false)}
          >
            <ShieldCheck size={13} />
            Dry-run ({selectedCandidates.length} · {formatBytes(selectedBytes)})
          </Button>
        </div>
      </div>

      <InlineActivityBanner
        active={loading}
        variant={!report ? "centered" : "floating"}
        delayMs={!report ? 180 : 280}
        title="Escaneando artefactos y cachés de desarrollo…"
        detail="Evaluando índices precalculados, detectores especializados y git check-ignore."
      />

      <DevSummaryCharts report={report} onSelectCategory={setActiveCat} />

      <DevProjectRootsCard
        config={config}
        onPickFolder={handlePickFolder}
        onAddManualRoot={addRootIfNew}
        onRemoveRoot={handleRemoveRoot}
        onChangeAgeDays={(days) => persistConfig({ ...config, minProjectAgeDays: days }, false)}
      />

      <Collapse open={Boolean(report && report.runningDevProcesses.length > 0)}>
        {report && report.runningDevProcesses.length > 0 && (
          <AlertBanner tone="warning">
            Procesos activos detectados:{" "}
            <strong className="mono">{report.runningDevProcesses.join(", ")}</strong>. Las reglas
            bloqueadas se omitirán automáticamente sin forzar cierres.
          </AlertBanner>
        )}
      </Collapse>

      <DockerWslCard info={report?.dockerWslInfo} />

      {/* Main 2-column layout: Category Tree + Candidate List */}
      <div className="flex flex-col lg:flex-row gap-4 items-start">
        <CategoryTreeSidebar
          title="Ecosistemas Dev"
          rootLabel="Todos los ecosistemas"
          totalCount={allCandidates.length}
          totalBytes={report?.totalRecoverableBytes ?? 0}
          nodes={treeNodes}
          selectedId={activeCat}
          onSelect={setActiveCat}
        />

        <div className="flex-1 min-w-0 w-full flex flex-col gap-3">
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
                placeholder="Buscar regla, herramienta o ruta…"
                className="w-full h-8 pl-8 pr-7 rounded-[8px] bg-canvas border border-border-default text-[12.5px] text-fg-primary placeholder:text-fg-muted focus:border-accent focus:outline-none"
              />
              {search && (
                <button
                  type="button"
                  onClick={() => setSearch("")}
                  aria-label="Limpiar búsqueda"
                  className="absolute right-2 top-1/2 -translate-y-1/2 text-fg-muted hover:text-fg-primary bg-transparent border-0 p-0.5 cursor-pointer"
                >
                  <X size={12} />
                </button>
              )}
            </div>

            <div className="flex items-center gap-2 flex-wrap">
              <SegmentedControl
                ariaLabel="Filtrar candidatos por nivel"
                value={levelFilter}
                onChange={setLevelFilter}
                options={[
                  { value: "all", label: "Todos" },
                  { value: "seguro", label: "Seguro" },
                  { value: "revisar", label: "Revisar" },
                ]}
              />
              <Button
                size="sm"
                onClick={toggleSelectVisible}
                disabled={selectableVisible.length === 0}
              >
                {allVisibleSelected ? <Square size={12} /> : <CheckSquare size={12} />}
                {allVisibleSelected ? "Deseleccionar" : "Seleccionar vista"}
              </Button>
            </div>
          </Card>

          {loading && !report ? null : visibleCandidates.length === 0 ? (
            <EmptyState
              icon={<Code2 size={18} />}
              title="Sin elementos limpiables"
              description="No se detectaron artefactos ni cachés con los filtros y categoría actual."
            />
          ) : (
            <Card
              key={`${activeCat}-${levelFilter}`}
              padded={false}
              className="sc-reveal overflow-hidden"
            >
              <div className="min-h-[360px] max-h-[calc(100vh-280px)] overflow-y-auto p-2.5 flex flex-col gap-1.5">
                {visibleCandidates.map((item) => {
                  const locked = isLocked(item);
                  return (
                    <DevCandidateRow
                      key={item.id}
                      item={item}
                      locked={locked}
                      checked={selectedIds.has(item.id) && !locked}
                      isElevated={isElevated}
                      onToggle={toggleItem}
                      onRelaunchAdmin={onRelaunchAdmin}
                    />
                  );
                })}
              </div>
            </Card>
          )}
        </div>
      </div>

      <SelectionActionBar
        count={selectedCandidates.length}
        bytes={selectedBytes}
        noun="elementos Dev"
        actionLabel="Abrir Dry-run"
        onClear={() => setSelectedIds(new Set())}
        onAction={() => openPreview(false)}
      />

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
