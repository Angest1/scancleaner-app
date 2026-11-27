import { ChevronDown, Globe, Lock, Plus, Search, ShieldCheck, Trash2, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { DonutChart } from "../components/charts/DonutChart";
import { HorizontalBarChart } from "../components/charts/HorizontalBarChart";
import { ProtectedRuleForms } from "../components/protection/ProtectedRuleForms";
import { InlineActivityBanner } from "../components/ui/ActionMotion";
import { Badge } from "../components/ui/Badge";
import { Button } from "../components/ui/Button";
import { Card } from "../components/ui/Card";
import { type CategoryTreeNode, CategoryTreeSidebar } from "../components/ui/CategoryTreeSidebar";
import { Collapse } from "../components/ui/Collapse";
import { AlertBanner, EmptyState } from "../components/ui/Feedback";
import { type ProtectedEntry, type ProtectionType, api } from "../lib/ipc";

const TYPE_LABELS: Record<ProtectionType, { label: string; color: string }> = {
  "carpeta-recursiva": { label: "Carpeta recursiva", color: "#3b82f6" },
  archivo: { label: "Archivo exacto", color: "#10b981" },
  glob: { label: "Patrón glob", color: "#a855f7" },
};

export function ProtectedPathsView() {
  const [entries, setEntries] = useState<ProtectedEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedNode, setSelectedNode] = useState<string>("all");
  const [search, setSearch] = useState("");
  const [showForms, setShowForms] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const loadEntries = useCallback(() => {
    setLoading(true);
    api
      .protectionList()
      .then(setEntries)
      .catch((e) => setErrorMsg(String(e)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    loadEntries();
  }, [loadEntries]);

  const handleRemove = (id: string) => {
    setErrorMsg(null);
    api
      .protectionRemove(id)
      .then(setEntries)
      .catch((err) => setErrorMsg(String(err)));
  };

  const userEntries = useMemo(() => entries.filter((e) => e.origen === "usuario"), [entries]);
  const criticalEntries = useMemo(() => entries.filter((e) => e.origen === "crítica"), [entries]);

  const treeNodes = useMemo<CategoryTreeNode[]>(() => {
    const types: ProtectionType[] = ["carpeta-recursiva", "archivo", "glob"];
    const makeChildren = (prefix: string, list: ProtectedEntry[]) =>
      types
        .map((t) => ({
          id: `${prefix}:${t}`,
          label: TYPE_LABELS[t].label,
          count: list.filter((e) => e.tipo === t).length,
          color: TYPE_LABELS[t].color,
        }))
        .filter((c) => c.count > 0);

    return [
      {
        id: "usuario",
        label: "Reglas del usuario",
        count: userEntries.length,
        color: "#3b82f6",
        children: makeChildren("usuario", userEntries),
      },
      {
        id: "crítica",
        label: "Críticas de fábrica",
        count: criticalEntries.length,
        color: "#f59e0b",
        children: makeChildren("crítica", criticalEntries),
      },
    ];
  }, [userEntries, criticalEntries]);

  const typeStats = useMemo(() => {
    const types: ProtectionType[] = ["carpeta-recursiva", "archivo", "glob"];
    return types.map((t) => {
      const count = entries.filter((e) => e.tipo === t).length;
      return {
        key: t,
        label: TYPE_LABELS[t].label,
        value: count,
        valueLabel: `${count} reglas`,
        color: TYPE_LABELS[t].color,
      };
    });
  }, [entries]);

  const filteredEntries = useMemo(() => {
    const q = search.trim().toLowerCase();
    return entries.filter((e) => {
      if (selectedNode === "usuario" && e.origen !== "usuario") return false;
      if (selectedNode === "crítica" && e.origen !== "crítica") return false;
      if (selectedNode.includes(":")) {
        const [orig, tipo] = selectedNode.split(":");
        if (e.origen !== orig || e.tipo !== tipo) return false;
      }
      if (!q) return true;
      return e.patron.toLowerCase().includes(q) || e.motivo.toLowerCase().includes(q);
    });
  }, [entries, selectedNode, search]);

  return (
    <div className="flex flex-col gap-4 min-h-[calc(100vh-110px)]">
      <div className="flex items-start justify-between flex-wrap gap-3">
        <div>
          <h1 className="m-0 text-[18px] font-semibold tracking-[-0.02em] text-fg-primary">
            Rutas protegidas
          </h1>
          <p className="m-0 mt-1 text-[13px] text-fg-secondary text-pretty">
            Ninguna limpieza (Manual, Automática ni Dev) puede tocar rutas coincidentes con estas
            reglas.
          </p>
        </div>

        <Button variant="primary" size="sm" onClick={() => setShowForms((v) => !v)}>
          <Plus size={13} />
          {showForms ? "Ocultar formulario" : "Agregar o probar ruta"}
          <ChevronDown
            size={13}
            className={`transition-transform duration-150 ${showForms ? "rotate-180" : "rotate-0"}`}
          />
        </Button>
      </div>

      <Collapse open={Boolean(errorMsg)}>
        {errorMsg && <AlertBanner tone="danger">{errorMsg}</AlertBanner>}
      </Collapse>

      <InlineActivityBanner
        active={loading}
        variant={entries.length === 0 ? "centered" : "floating"}
        delayMs={entries.length === 0 ? 180 : 280}
        title="Cargando motor de protección…"
        detail="Leyendo reglas críticas de fábrica y reglas personalizadas del usuario."
      />

      <Collapse open={showForms}>
        <div className="pb-1">
          <ProtectedRuleForms
            onAdded={() => {
              setErrorMsg(null);
              loadEntries();
            }}
            onError={setErrorMsg}
          />
        </div>
      </Collapse>

      {/* Visual distribution chart */}
      <Collapse open={entries.length > 0}>
        {entries.length > 0 && (
          <Card className="grid grid-cols-1 lg:grid-cols-12 gap-4 items-center py-3">
            <div className="lg:col-span-5">
              <DonutChart
                size={118}
                strokeWidth={14}
                centerValue={String(entries.length)}
                centerLabel="Reglas"
                onSelectSegment={setSelectedNode}
                segments={[
                  {
                    key: "crítica",
                    label: "Críticas de fábrica",
                    value: criticalEntries.length,
                    valueLabel: `${criticalEntries.length}`,
                    color: "#f59e0b",
                  },
                  {
                    key: "usuario",
                    label: "Usuario",
                    value: userEntries.length,
                    valueLabel: `${userEntries.length}`,
                    color: "#3b82f6",
                  },
                ]}
              />
            </div>
            <div className="lg:col-span-7">
              <HorizontalBarChart items={typeStats} maxItems={4} />
            </div>
          </Card>
        )}
      </Collapse>

      {/* Main 2-column layout: Left Category Tree + Full-height Rules List */}
      <div className="flex flex-col lg:flex-row gap-4 items-start flex-1">
        <CategoryTreeSidebar
          title="Árbol de protección"
          rootLabel="Todas las reglas"
          totalCount={entries.length}
          nodes={treeNodes}
          selectedId={selectedNode}
          onSelect={setSelectedNode}
        />

        <div className="flex-1 min-w-0 w-full flex flex-col gap-3">
          <Card className="flex items-center justify-between flex-wrap gap-2.5 py-2.5">
            <div className="relative flex-1 min-w-[200px]">
              <Search
                size={13}
                className="absolute left-2.5 top-1/2 -translate-y-1/2 text-fg-muted pointer-events-none"
              />
              <input
                type="text"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Buscar patrón, ruta o motivo…"
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
            <span className="mono text-[11.5px] text-fg-muted">
              {filteredEntries.length} / {entries.length} reglas
            </span>
          </Card>

          {loading && entries.length === 0 ? null : filteredEntries.length === 0 ? (
            <EmptyState
              icon={<ShieldCheck size={18} />}
              title="Sin reglas coincidentes"
              description="Cambia el nodo en el árbol izquierdo, limpia la búsqueda o agrega una nueva regla."
            />
          ) : (
            <Card key={selectedNode} padded={false} className="sc-reveal overflow-hidden">
              <div className="min-h-[380px] h-[calc(100vh-360px)] overflow-y-auto divide-y divide-border-subtle">
                {filteredEntries.map((e) => {
                  const isCritical = e.origen === "crítica";
                  return (
                    <div
                      key={e.id}
                      className="px-3.5 py-2.5 flex items-center justify-between gap-3 hover:bg-elevated/55 transition-colors"
                    >
                      <div className="min-w-0">
                        <div className="flex items-center gap-1.5 flex-wrap">
                          <span className="mono font-medium text-[12.5px] text-fg-primary break-all">
                            {e.patron}
                          </span>
                          <Badge tone={isCritical ? "warning" : "accent"}>
                            {isCritical ? (
                              <>
                                <Lock size={10} />
                                Crítica
                              </>
                            ) : (
                              "Usuario"
                            )}
                          </Badge>
                          <Badge tone="neutral">{TYPE_LABELS[e.tipo].label}</Badge>
                          {e.todasLasUnidades && (
                            <Badge tone="neutral">
                              <Globe size={10} />
                              Todas las unidades
                            </Badge>
                          )}
                        </div>
                        <div className="text-[12px] text-fg-secondary mt-0.5">{e.motivo}</div>
                      </div>

                      {!isCritical && (
                        <Button
                          variant="danger"
                          size="sm"
                          onClick={() => handleRemove(e.id)}
                          className="shrink-0"
                        >
                          <Trash2 size={12} />
                          Quitar
                        </Button>
                      )}
                    </div>
                  );
                })}
              </div>
            </Card>
          )}
        </div>
      </div>
    </div>
  );
}
