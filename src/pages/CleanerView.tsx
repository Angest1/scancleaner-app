import { ChevronDown, Lock, RefreshCw, ShieldCheck, Sparkles, Trash2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Button } from "../components/ui/Button";
import { Card } from "../components/ui/Card";
import { Collapse } from "../components/ui/Collapse";
import { AlertBanner, EmptyState, Skeleton } from "../components/ui/Feedback";
import { PathDisplay } from "../components/ui/PathDisplay";
import { formatBytes, formatUnixTs } from "../lib/format";
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

const CATEGORY_TITLES: Record<CleanCategory, string> = {
  user_temp: "Temporales de usuario",
  system_temp: "Temporales del sistema",
  recycle_bin: "Papelera de reciclaje",
  browser_cache: "Cachés de navegadores",
  dev_artifact: "Artefactos y cachés de desarrollo",
  large_old_file: "Archivos grandes y antiguos",
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
  const [collapsedCats, setCollapsedCats] = useState<Set<CleanCategory>>(new Set());
  const [minLargeMb, setMinLargeMb] = useState(100);
  const [minAgeDays, setMinAgeDays] = useState(30);

  const toggleCategory = (cat: CleanCategory) => {
    setCollapsedCats((prev) => {
      const next = new Set(prev);
      if (next.has(cat)) next.delete(cat);
      else next.add(cat);
      return next;
    });
  };

  const loadCandidates = useCallback(
    (preserveSelection = false) => {
      if (!preserveSelection) setLoading(true);
      api
        .discoverCleanupCandidates(minLargeMb, minAgeDays)
        .then((res) => {
          setReport(res);
          if (preserveSelection) {
            const validIds = new Set(res.candidates.map((c) => c.id));
            setSelectedIds((prev) => {
              const kept = new Set<string>();
              for (const id of prev) {
                if (validIds.has(id)) kept.add(id);
              }
              return kept;
            });
          } else {
            setSelectedIds(new Set());
          }
        })
        .finally(() => setLoading(false));
    },
    [minLargeMb, minAgeDays],
  );

  useEffect(() => {
    loadCandidates(false);
  }, [loadCandidates]);

  useEffect(() => {
    if (syncRevision > 0) {
      loadCandidates(true);
    }
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

  const selectedCandidates = useMemo(
    () => (report?.candidates ?? []).filter((c) => selectedIds.has(c.id)),
    [report, selectedIds],
  );

  const selectedBytes = useMemo(
    () => selectedCandidates.reduce((acc, c) => acc + c.sizeBytes, 0),
    [selectedCandidates],
  );

  const grouped = useMemo(() => {
    const map = new Map<CleanCategory, CleanCandidate[]>();
    for (const c of report?.candidates ?? []) {
      const list = map.get(c.category) ?? [];
      list.push(c);
      map.set(c.category, list);
    }
    return map;
  }, [report]);

  const handlePreview = () => {
    const targets: CleanupTargetInput[] = selectedCandidates.map((c) => ({
      path: c.path,
      category: c.category,
      isTempDirContents: c.category === "user_temp" || c.category === "system_temp",
    }));
    onStartPreview(targets, "recycle_bin");
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-start justify-between flex-wrap gap-3">
        <div>
          <h1 className="m-0 text-[18px] font-semibold tracking-[-0.02em] text-fg-primary">
            Limpieza segura
          </h1>
          <p className="m-0 mt-1 text-[13px] text-fg-secondary">
            Todo pasa por vista previa (Dry-run), lista negra de rutas protegidas y Papelera por
            defecto.
          </p>
        </div>

        <div className="flex items-center gap-2 flex-wrap">
          <label className="text-[12px] text-fg-secondary flex items-center gap-1.5">
            Mín. MB
            <input
              type="number"
              className="w-[72px] h-8 px-2 rounded-[7px] bg-canvas border border-border-default mono text-[12px] text-fg-primary focus:border-accent focus:outline-none"
              min={10}
              value={minLargeMb}
              onChange={(e) => setMinLargeMb(Math.max(10, Number(e.target.value) || 100))}
            />
          </label>
          <label className="text-[12px] text-fg-secondary flex items-center gap-1.5">
            Días
            <input
              type="number"
              className="w-[64px] h-8 px-2 rounded-[7px] bg-canvas border border-border-default mono text-[12px] text-fg-primary focus:border-accent focus:outline-none"
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

      {report && report.runningBrowsers.length > 0 && (
        <AlertBanner tone="warning">
          Navegadores en ejecución: <strong>{report.runningBrowsers.join(", ")}</strong>. Cierra el
          navegador antes de limpiar su caché para evitar bloqueos de archivo.
        </AlertBanner>
      )}

      {/* Recycle Bin summary card */}
      <Card className="flex items-center justify-between flex-wrap gap-3">
        <div>
          <div className="font-semibold text-[14px] text-fg-primary">Papelera de reciclaje</div>
          <div className="text-[12.5px] text-fg-secondary mt-0.5 tabular">
            {report
              ? `${report.recycleBinItems.toLocaleString()} elementos · ${formatBytes(report.recycleBinBytes)}`
              : "Consultando Papelera..."}
          </div>
        </div>
        <Button
          variant="danger"
          size="sm"
          disabled={!report || report.recycleBinItems === 0}
          onClick={() => onStartPreview([], "empty_recycle_bin")}
        >
          <Trash2 size={13} />
          Vaciar Papelera
        </Button>
      </Card>

      {loading && !report ? (
        <Card className="flex flex-col gap-2.5">
          {[0, 1, 2, 3].map((i) => (
            <Skeleton key={i} className="h-12 w-full" />
          ))}
        </Card>
      ) : grouped.size === 0 ? (
        <EmptyState
          icon={<Sparkles size={18} />}
          title="Sin elementos pendientes"
          description="No se encontraron temporales ni archivos grandes que coincidan con los umbrales actuales."
        />
      ) : (
        Array.from(grouped.entries()).map(([cat, items], idx) => {
          const isOpen = !collapsedCats.has(cat);
          return (
            <Card
              key={cat}
              style={{ animationDelay: `${Math.min(idx, 6) * 40}ms` }}
              className="flex flex-col animate-fade-up"
            >
              <button
                type="button"
                aria-expanded={isOpen}
                onClick={() => toggleCategory(cat)}
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
                    {CATEGORY_TITLES[cat]}
                  </h2>
                  <span className="mono text-[11.5px] text-fg-muted">({items.length})</span>
                </div>
                <span className="mono text-[12px] font-medium text-accent">
                  {formatBytes(items.reduce((a, b) => a + b.sizeBytes, 0))}
                </span>
              </button>

              <Collapse open={isOpen}>
                <div className="flex flex-col gap-1.5 pt-3">
                  {items.map((item) => {
                    const locked = item.requiresAdmin && !isElevated;
                    const checked = selectedIds.has(item.id);
                    return (
                      <div
                        key={item.id}
                        className={`flex items-center justify-between gap-3 px-3 py-2.5 rounded-[8px] bg-canvas border transition-colors duration-140 ${
                          checked
                            ? "border-accent-border bg-accent-soft/30"
                            : "border-border-subtle"
                        } ${locked ? "opacity-65" : ""}`}
                      >
                        <label
                          className={`flex items-start gap-2.5 flex-1 min-w-0 ${
                            locked ? "cursor-not-allowed" : "cursor-pointer"
                          }`}
                        >
                          <input
                            type="checkbox"
                            className="mt-0.5 accent-accent shrink-0"
                            checked={checked}
                            disabled={locked}
                            onChange={() => toggleItem(item)}
                          />
                          <div className="min-w-0 flex-1">
                            <div className="font-medium text-[13px] text-fg-primary truncate">
                              {item.displayName}
                            </div>
                            <PathDisplay
                              path={item.path}
                              maxChars={58}
                              className="text-[11.5px] text-fg-muted"
                            />
                            {item.warning && (
                              <div className="text-[11.5px] text-warning-text mt-0.5">
                                {item.warning}
                              </div>
                            )}
                          </div>
                        </label>

                        <div className="flex items-center gap-2.5 shrink-0">
                          {item.lastModifiedTs > 0 && (
                            <span className="hidden sm:inline text-[11.5px] text-fg-muted tabular">
                              {formatUnixTs(item.lastModifiedTs)}
                            </span>
                          )}
                          <span className="mono font-medium text-[12.5px] text-fg-primary min-w-[78px] text-right">
                            {formatBytes(item.sizeBytes)}
                          </span>
                          {locked && (
                            <Button variant="warning" size="sm" onClick={onRelaunchAdmin}>
                              <Lock size={12} />
                              Admin
                            </Button>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </Collapse>
            </Card>
          );
        })
      )}
    </div>
  );
}
