import { ChevronDown, Code2, HardDrive, RefreshCw, ShieldCheck } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { DevCleanupModal } from "../components/DevCleanupModal";
import { DevCandidateRow } from "../components/dev-cleaner/DevCandidateRow";
import { DevProjectRootsCard } from "../components/dev-cleaner/DevProjectRootsCard";
import { Button } from "../components/ui/Button";
import { Card } from "../components/ui/Card";
import { Collapse } from "../components/ui/Collapse";
import { AlertBanner, EmptyState, Skeleton } from "../components/ui/Feedback";
import { SegmentedControl } from "../components/ui/SegmentedControl";
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
  const [collapsedCats, setCollapsedCats] = useState<Set<DevRuleCategory>>(new Set());
  const [levelFilter, setLevelFilter] = useState<"all" | "seguro" | "revisar">("all");
  const [showDockerGuide, setShowDockerGuide] = useState(false);

  const toggleCategory = (cat: DevRuleCategory) => {
    setCollapsedCats((prev) => {
      const next = new Set(prev);
      if (next.has(cat)) next.delete(cat);
      else next.add(cat);
      return next;
    });
  };

  // Dry-run & execution modal state
  const [useRecycleBin, setUseRecycleBin] = useState(false);
  const [activePlan, setActivePlan] = useState<DevCleanupPlan | null>(null);
  const [executing, setExecuting] = useState(false);
  const [result, setResult] = useState<DevCleanupResult | null>(null);
  const runScan = useCallback((cfg: DevCleanerConfig, preserveSelection = false) => {
    if (!preserveSelection) setLoading(true);
    api
      .devScan(cfg)
      .then((res) => {
        setReport(res);
        if (preserveSelection) {
          const valid = new Set(res.candidates.map((c) => c.id));
          setSelectedIds((prev) => {
            const kept = new Set<string>();
            for (const id of prev) {
              if (valid.has(id)) kept.add(id);
            }
            return kept;
          });
        } else {
          const pre = new Set<string>();
          for (const c of res.candidates) {
            if (c.preselected) pre.add(c.id);
          }
          setSelectedIds(pre);
        }
      })
      .catch((err) => alert(String(err)))
      .finally(() => setLoading(false));
  }, []);

  const configRef = useRef(config);
  configRef.current = config;

  useEffect(() => {
    api.devGetConfig().then((cfg) => {
      setConfig(cfg);
      runScan(cfg, false);
    });
  }, [runScan]);

  useEffect(() => {
    if (syncRevision > 0) {
      runScan(configRef.current, true);
    }
  }, [syncRevision, runScan]);

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
        persistConfig({ ...config, projectRoots: [...config.projectRoots, picked] }, true);
      })
      .catch((err) => alert(String(err)));
  };

  const handleAddManualRoot = (trimmed: string) => {
    if (config.projectRoots.some((r) => r.toLowerCase() === trimmed.toLowerCase())) return;
    persistConfig({ ...config, projectRoots: [...config.projectRoots, trimmed] }, true);
  };

  const handleRemoveRoot = (root: string) => {
    persistConfig({ ...config, projectRoots: config.projectRoots.filter((r) => r !== root) }, true);
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
    <div className="flex flex-col gap-4">
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
            {loading ? "Escaneando..." : "Reescanear"}
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

      {/* Summary metrics */}
      <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
        <Card className="py-3.5">
          <div className="text-[11.5px] text-fg-muted">Total recuperable</div>
          <div className="mono text-[21px] font-semibold text-accent mt-0.5">
            {formatBytes(report?.totalRecoverableBytes ?? 0)}
          </div>
        </Card>
        <Card className="py-3.5">
          <div className="text-[11.5px] text-fg-muted">Nivel Seguro (preseleccionado)</div>
          <div className="mono text-[21px] font-semibold text-success mt-0.5">
            {formatBytes(report?.safeRecoverableBytes ?? 0)}
          </div>
        </Card>
        <Card className="py-3.5">
          <div className="text-[11.5px] text-fg-muted">Nivel Revisar (desmarcado)</div>
          <div className="mono text-[21px] font-semibold text-warning mt-0.5">
            {formatBytes(report?.reviewRecoverableBytes ?? 0)}
          </div>
        </Card>
      </div>
      <DevProjectRootsCard
        config={config}
        onPickFolder={handlePickFolder}
        onAddManualRoot={handleAddManualRoot}
        onRemoveRoot={handleRemoveRoot}
        onChangeAgeDays={(days) => persistConfig({ ...config, minProjectAgeDays: days }, false)}
      />

      {report && report.runningDevProcesses.length > 0 && (
        <AlertBanner tone="warning">
          Procesos activos detectados:{" "}
          <strong className="mono">{report.runningDevProcesses.join(", ")}</strong>. Las reglas
          bloqueadas por estos procesos se omitirán automáticamente sin forzar cierres.
        </AlertBanner>
      )}

      {report?.warnings.map((w) => (
        <AlertBanner key={w} tone="info">
          {w}
        </AlertBanner>
      ))}

      {report?.dockerWslInfo.dockerAvailable && (
        <Card className="flex flex-col gap-2">
          <div className="flex items-center justify-between flex-wrap gap-2">
            <div className="flex items-center gap-2 font-semibold text-[13.5px] text-fg-primary">
              <HardDrive size={15} className="text-accent" />
              Compactación de disco virtual Docker / WSL (acción aparte)
            </div>
            <Button size="sm" onClick={() => setShowDockerGuide((v) => !v)}>
              {showDockerGuide ? "Ocultar instrucciones" : "Ver instrucciones"}
            </Button>
          </div>
          <div className="text-[12.5px] text-fg-secondary">{report.dockerWslInfo.vhdxNotice}</div>
          <Collapse open={showDockerGuide}>
            <div className="rounded-[8px] bg-canvas border border-border-subtle p-3 mt-1 flex flex-col gap-1 mono text-[12px] text-fg-secondary">
              {report.dockerWslInfo.compactInstructions.map((step) => (
                <div key={step}>{step}</div>
              ))}
            </div>
          </Collapse>
        </Card>
      )}

      {/* Filter bar */}
      <div className="flex items-center gap-2.5">
        <span className="text-[12px] text-fg-muted">Nivel</span>
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
      </div>

      {loading && !report ? (
        <Card className="flex flex-col gap-2.5">
          {[0, 1, 2, 3].map((i) => (
            <Skeleton key={i} className="h-14 w-full" />
          ))}
        </Card>
      ) : groupedByCategory.length === 0 ? (
        <EmptyState
          icon={<Code2 size={18} />}
          title="Sin elementos limpiables"
          description="No se detectaron artefactos ni cachés de desarrollo con los filtros y raíces actuales."
        />
      ) : (
        groupedByCategory.map(({ category, items }, idx) => {
          const isOpen = !collapsedCats.has(category);
          return (
            <Card
              key={category}
              style={{ animationDelay: `${Math.min(idx, 6) * 40}ms` }}
              className="flex flex-col animate-fade-up"
            >
              <button
                type="button"
                aria-expanded={isOpen}
                onClick={() => toggleCategory(category)}
                className="w-full flex items-center justify-between gap-2 bg-transparent border-0 p-0 cursor-pointer text-left group"
              >
                <div className="flex items-center gap-2">
                  <ChevronDown
                    size={15}
                    className={`text-fg-muted transition-transform duration-200 ease-out ${
                      isOpen ? "rotate-0" : "-rotate-90"
                    }`}
                  />
                  <h2 className="m-0 text-[14px] font-semibold text-fg-primary group-hover:text-accent transition-colors">
                    {category}
                  </h2>
                </div>
                <span className="mono text-[12px] font-medium text-accent">
                  {formatBytes(items.reduce((a, b) => a + b.sizeBytes, 0))} · {items.length} ítems
                </span>
              </button>

              <Collapse open={isOpen}>
                <div className="flex flex-col gap-1.5 pt-3">
                  {items.map((item) => {
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
              </Collapse>
            </Card>
          );
        })
      )}

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
