import { useVirtualizer } from "@tanstack/react-virtual";
import {
  AppWindow,
  ExternalLink,
  MoreHorizontal,
  RefreshCw,
  Search,
  Trash2,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AppDetailDrawer } from "../components/apps/AppDetailDrawer";
import { AppLeftoversBanner } from "../components/apps/AppLeftoversBanner";
import { AppsAnalyticsCard } from "../components/apps/AppsAnalyticsCard";
import { InlineActivityBanner } from "../components/ui/ActionMotion";
import { Badge } from "../components/ui/Badge";
import { Button } from "../components/ui/Button";
import { Card } from "../components/ui/Card";
import { Collapse } from "../components/ui/Collapse";
import { AlertBanner, EmptyState } from "../components/ui/Feedback";
import { SegmentedControl } from "../components/ui/SegmentedControl";
import {
  DensityToggle,
  type SortDir,
  SortHeaderButton,
  type TableDensity,
} from "../components/ui/TableToolbar";
import { useDeferredReorder } from "../hooks/useDeferredReorder";
import { formatBytes } from "../lib/format";
import { type InstalledApp, type UninstallLeftoverSuggestion, api } from "../lib/ipc";

type SortKey = "name" | "source" | "size";

interface AppsViewProps {
  syncRevision?: number;
  onQueueLeftoverCleanup?: (path: string) => void;
}

export function AppsView({ syncRevision = 0, onQueueLeftoverCleanup }: AppsViewProps) {
  const [apps, setApps] = useState<InstalledApp[]>([]);
  const [leftovers, setLeftovers] = useState<UninstallLeftoverSuggestion[]>([]);
  const [loading, setLoading] = useState(false);
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<"all" | "win32" | "store_msix">("all");
  const [sortKey, setSortKey] = useState<SortKey>("size");
  const [sortDir, setSortDir] = useState<SortDir>("desc");
  const [density, setDensity] = useState<TableDensity>("comfortable");
  const [inspected, setInspected] = useState<InstalledApp | null>(null);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);
  const parentRef = useRef<HTMLDivElement>(null);

  const loadApps = useCallback((forceRefresh = false, silent = false) => {
    if (!silent) setLoading(true);
    Promise.all([
      api.listInstalledApps(forceRefresh),
      api.getUninstallLeftovers().catch(() => [] as UninstallLeftoverSuggestion[]),
    ])
      .then(([list, lv]) => {
        setApps(list);
        setLeftovers(lv);
      })
      .catch((err) => setStatusMsg(String(err)))
      .finally(() => !silent && setLoading(false));
  }, []);

  useEffect(() => loadApps(false, false), [loadApps]);
  useEffect(() => {
    if (syncRevision > 0) loadApps(false, true);
  }, [syncRevision, loadApps]);

  const handleSort = (key: SortKey) => {
    setSortDir((d) =>
      sortKey === key ? (d === "asc" ? "desc" : "asc") : key === "size" ? "desc" : "asc",
    );
    setSortKey(key);
  };

  const stats = useMemo(() => {
    const win32 = apps.filter((a) => a.source === "win32");
    const msix = apps.filter((a) => a.source === "store_msix");
    const win32Bytes = win32.reduce((s, a) => s + a.sizeBytes, 0);
    const msixBytes = msix.reduce((s, a) => s + a.sizeBytes, 0);
    return {
      totalBytes: win32Bytes + msixBytes,
      win32Bytes,
      msixBytes,
      win32Count: win32.length,
      msixCount: msix.length,
      topApps: [...apps].sort((a, b) => b.sizeBytes - a.sizeBytes).slice(0, 6),
    };
  }, [apps]);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    const mult = sortDir === "asc" ? 1 : -1;
    return apps
      .filter(
        (a) =>
          (filter === "all" || a.source === filter) &&
          (!q || a.name.toLowerCase().includes(q) || a.publisher.toLowerCase().includes(q)),
      )
      .sort(
        (a, b) =>
          (sortKey === "name"
            ? a.name.localeCompare(b.name)
            : sortKey === "source"
              ? a.source.localeCompare(b.source)
              : a.sizeBytes - b.sizeBytes) * mult,
      );
  }, [apps, search, filter, sortKey, sortDir]);

  const getAppKey = useCallback((a: InstalledApp) => a.id, []);
  const { items: stableApps, containerProps } = useDeferredReorder(filtered, getAppKey, 1000);
  const rowHeight = density === "compact" ? 44 : 58;

  const virtualizer = useVirtualizer({
    count: stableApps.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => rowHeight,
    overscan: 8,
  });

  const handleUninstall = (app: InstalledApp) => {
    api
      .uninstallApp(app.source, app.uninstallString, {
        appId: app.id,
        appName: app.name,
        installLocation: app.installLocation,
      })
      .then((msg) => setStatusMsg(`${msg} (vigilando finalización automática en el Registro…)`))
      .catch((err) => setStatusMsg(String(err)));
  };

  return (
    <div className="flex flex-col gap-4 min-h-[calc(100vh-110px)]">
      <div className="flex items-start justify-between flex-wrap gap-3">
        <div>
          <h1 className="m-0 text-[18px] font-semibold tracking-[-0.02em] text-fg-primary">
            Aplicaciones instaladas
          </h1>
          <p className="m-0 mt-1 text-[13px] text-fg-secondary text-pretty">
            Inventario combinado Win32 y Microsoft Store / MSIX resuelto al instante desde el índice
            en memoria.
          </p>
        </div>

        <div className="flex items-center gap-2 flex-wrap">
          <Button
            onClick={() => api.openWindowsAppsSettings().catch((e) => setStatusMsg(String(e)))}
          >
            <ExternalLink size={13} />
            Configuración de Windows
          </Button>
          <Button variant="primary" onClick={() => loadApps(true)} disabled={loading}>
            <RefreshCw size={13} className={loading ? "animate-spin" : ""} />
            {loading ? "Actualizando…" : "Actualizar"}
          </Button>
        </div>
      </div>

      <Collapse open={Boolean(statusMsg)}>
        {statusMsg && <AlertBanner tone="info">{statusMsg}</AlertBanner>}
      </Collapse>

      <InlineActivityBanner
        active={loading}
        variant={apps.length === 0 ? "centered" : "floating"}
        delayMs={apps.length === 0 ? 180 : 280}
        title="Consultando inventario de aplicaciones instaladas…"
        detail="Leyendo Registro Win32, paquetes Store/MSIX y tamaños desde los volúmenes indexados."
      />

      <AppLeftoversBanner leftovers={leftovers} onQueueCleanup={onQueueLeftoverCleanup} />

      <AppsAnalyticsCard
        apps={apps}
        totalBytes={stats.totalBytes}
        win32Bytes={stats.win32Bytes}
        msixBytes={stats.msixBytes}
        win32Count={stats.win32Count}
        msixCount={stats.msixCount}
        topApps={stats.topApps}
        onFilterSource={setFilter}
        onInspectApp={setInspected}
      />

      <Card className="flex items-center justify-between flex-wrap gap-3 py-2.5">
        <div className="relative flex-1 min-w-[200px] max-w-[360px]">
          <Search
            size={14}
            className="absolute left-2.5 top-1/2 -translate-y-1/2 text-fg-muted pointer-events-none"
          />
          <input
            type="text"
            name="app-search"
            autoComplete="off"
            spellCheck={false}
            aria-label="Buscar aplicación o editor"
            className="w-full h-8 pl-8 pr-8 rounded-[8px] bg-canvas border border-border-default text-[12.5px] text-fg-primary placeholder:text-fg-muted focus:border-accent focus-visible:ring-2 focus-visible:ring-accent/30 transition-colors"
            placeholder="Buscar aplicación o editor…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          {search && (
            <button
              type="button"
              onClick={() => setSearch("")}
              aria-label="Limpiar búsqueda"
              className="absolute right-2 top-1/2 -translate-y-1/2 text-fg-muted hover:text-fg-primary bg-transparent border-0 p-0.5 cursor-pointer"
            >
              <X size={13} />
            </button>
          )}
        </div>

        <div className="flex items-center gap-2 flex-wrap">
          <SegmentedControl
            ariaLabel="Filtrar por origen"
            value={filter}
            onChange={setFilter}
            options={[
              { value: "all", label: "Todas", count: apps.length },
              { value: "win32", label: "Win32", count: stats.win32Count },
              { value: "store_msix", label: "Store / MSIX", count: stats.msixCount },
            ]}
          />
          <DensityToggle density={density} onChange={setDensity} />
        </div>
      </Card>

      {loading && apps.length === 0 ? null : filtered.length === 0 ? (
        <EmptyState
          icon={<AppWindow size={18} />}
          title="Sin aplicaciones coincidentes"
          description="Ajusta el término de búsqueda o cambia el filtro de origen."
          action={
            (search || filter !== "all") && (
              <Button
                onClick={() => {
                  setSearch("");
                  setFilter("all");
                }}
              >
                Restablecer filtros
              </Button>
            )
          }
        />
      ) : (
        <Card
          padded={false}
          className="sc-reveal overflow-hidden flex-1 flex flex-col min-h-[420px]"
        >
          <div
            ref={parentRef}
            {...containerProps}
            className="flex-1 min-h-[420px] h-[calc(100vh-300px)] overflow-y-auto overflow-x-hidden"
          >
            <div className="sticky top-0 z-10 grid grid-cols-[minmax(0,1fr)_90px_40px] md:grid-cols-[minmax(0,1fr)_84px_124px_142px] items-center gap-2 px-3.5 py-2.5 border-b border-border-default bg-shell backdrop-blur-md">
              <SortHeaderButton
                label="Aplicación"
                active={sortKey === "name"}
                dir={sortDir}
                onClick={() => handleSort("name")}
              />
              <div className="hidden md:flex">
                <SortHeaderButton
                  label="Origen"
                  active={sortKey === "source"}
                  dir={sortDir}
                  onClick={() => handleSort("source")}
                />
              </div>
              <SortHeaderButton
                label="Tamaño"
                active={sortKey === "size"}
                dir={sortDir}
                align="right"
                onClick={() => handleSort("size")}
              />
              <span className="text-right text-[11.5px] font-medium text-fg-muted">
                <span className="hidden md:inline">Acciones</span>
              </span>
            </div>

            <div style={{ height: `${virtualizer.getTotalSize()}px`, position: "relative" }}>
              {virtualizer.getVirtualItems().map((vRow) => {
                const app = stableApps[vRow.index];
                if (!app) return null;
                return (
                  <div
                    key={app.id}
                    className="group grid grid-cols-[minmax(0,1fr)_90px_40px] md:grid-cols-[minmax(0,1fr)_84px_124px_142px] items-center gap-2 px-3.5 border-b border-border-subtle hover:bg-elevated/65 transition-colors duration-140"
                    style={{
                      position: "absolute",
                      top: 0,
                      left: 0,
                      width: "100%",
                      height: `${rowHeight}px`,
                      transform: `translateY(${vRow.start}px)`,
                    }}
                  >
                    <button
                      type="button"
                      onClick={() => setInspected(app)}
                      className="min-w-0 text-left bg-transparent border-0 p-0 cursor-pointer"
                    >
                      <div className="font-medium text-[13px] text-fg-primary group-hover:text-accent transition-colors truncate">
                        {app.name}{" "}
                        {app.version ? (
                          <span className="mono text-[11.5px] text-fg-muted font-normal">
                            v{app.version}
                          </span>
                        ) : null}
                      </div>
                      {density === "comfortable" && (
                        <div className="flex items-center gap-1.5 text-[11.5px] text-fg-muted truncate">
                          <span className="md:hidden">
                            {app.source === "store_msix" ? "MSIX" : "Win32"} ·
                          </span>
                          <span className="truncate">{app.publisher || "Editor desconocido"}</span>
                        </div>
                      )}
                    </button>

                    <div className="hidden md:block">
                      <Badge tone={app.source === "store_msix" ? "accent" : "neutral"}>
                        {app.source === "store_msix" ? "MSIX" : "Win32"}
                      </Badge>
                    </div>

                    <div className="text-right">
                      <div className="mono font-medium text-[12.5px] text-fg-primary">
                        {app.sizeSource === "unknown" ? "N/D" : formatBytes(app.sizeBytes)}
                      </div>
                      {density === "comfortable" && (
                        <div className="text-[11px] text-fg-muted">
                          {app.sizeSource === "measured"
                            ? "Medido"
                            : app.sizeSource === "estimated"
                              ? "Estimado"
                              : "Restringido"}
                        </div>
                      )}
                    </div>

                    <div className="flex justify-end items-center gap-1">
                      <div className="hidden md:flex opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity duration-140">
                        <Button variant="danger" size="sm" onClick={() => handleUninstall(app)}>
                          <Trash2 size={12} />
                          Desinstalar
                        </Button>
                      </div>
                      <Button
                        variant="ghost"
                        size="icon"
                        onClick={() => setInspected(app)}
                        title="Ver detalles de la aplicación"
                        aria-label={`Detalles de ${app.name}`}
                      >
                        <MoreHorizontal size={14} />
                      </Button>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        </Card>
      )}

      {inspected && (
        <AppDetailDrawer
          app={inspected}
          onClose={() => setInspected(null)}
          onUninstall={handleUninstall}
        />
      )}
    </div>
  );
}
