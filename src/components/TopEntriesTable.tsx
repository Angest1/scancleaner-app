import { useVirtualizer } from "@tanstack/react-virtual";
import { File, Folder, FolderOpen, Lock, MoreHorizontal, ShieldPlus, Trash2 } from "lucide-react";
import { useCallback, useMemo, useRef, useState } from "react";
import { useDeferredReorder } from "../hooks/useDeferredReorder";
import { formatBytes, formatUnixTs } from "../lib/format";
import type { TopEntryDto } from "../lib/ipc";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import { DetailDrawer, DetailField } from "./ui/DetailDrawer";
import { PathDisplay } from "./ui/PathDisplay";
import {
  DensityToggle,
  type SortDir,
  SortHeaderButton,
  type TableDensity,
} from "./ui/TableToolbar";

interface TopEntriesTableProps {
  entries: TopEntryDto[];
  kind: "dirs" | "files";
  onDrillDown: (nodeId: number) => void;
  onOpenExplorer: (path: string) => void;
  onSendToCleanup: (entry: TopEntryDto) => void;
  onProtectPath: (entry: TopEntryDto) => void;
}

type SortKey = "name" | "size" | "date";

export function TopEntriesTable({
  entries,
  kind,
  onDrillDown,
  onOpenExplorer,
  onSendToCleanup,
  onProtectPath,
}: TopEntriesTableProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const [sortKey, setSortKey] = useState<SortKey>("size");
  const [sortDir, setSortDir] = useState<SortDir>("desc");
  const [density, setDensity] = useState<TableDensity>("comfortable");
  const [inspected, setInspected] = useState<TopEntryDto | null>(null);

  const handleSort = (key: SortKey) => {
    if (sortKey === key) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(key);
      setSortDir(key === "name" ? "asc" : "desc");
    }
  };

  const sorted = useMemo(() => {
    const list = [...entries];
    const mult = sortDir === "asc" ? 1 : -1;
    list.sort((a, b) => {
      if (sortKey === "name") return a.name.localeCompare(b.name) * mult;
      if (sortKey === "date") return (a.lastWriteTs - b.lastWriteTs) * mult;
      return (a.allocatedBytes - b.allocatedBytes) * mult;
    });
    return list;
  }, [entries, sortKey, sortDir]);

  const getRowKey = useCallback((item: TopEntryDto) => item.nodeId, []);
  const { items: stableRows, containerProps } = useDeferredReorder(sorted, getRowKey, 1000);

  const rowHeight = density === "compact" ? 42 : 54;

  const virtualizer = useVirtualizer({
    count: stableRows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => rowHeight,
    overscan: 8,
  });

  if (entries.length === 0) {
    return (
      <div className="py-8 text-center text-[12.5px] text-fg-muted border border-border-subtle rounded-[10px] bg-canvas/60">
        No hay elementos para mostrar en este nivel.
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2 text-[11.5px] text-fg-muted">
        <span className="tabular">{sorted.length} elementos en este nivel</span>
        <DensityToggle density={density} onChange={setDensity} />
      </div>

      <div className="border border-border-default rounded-[10px] overflow-hidden bg-canvas/40">
        <div
          ref={parentRef}
          {...containerProps}
          className="h-[360px] overflow-y-auto overflow-x-hidden"
        >
          {/* Sticky adaptive header (zero horizontal scroll) */}
          <div className="sticky top-0 z-10 grid grid-cols-[minmax(0,1fr)_88px_40px] sm:grid-cols-[minmax(0,1fr)_104px_96px_118px] items-center gap-2 px-3 py-2 border-b border-border-default bg-shell backdrop-blur-md">
            <SortHeaderButton
              label="Ruta"
              active={sortKey === "name"}
              dir={sortDir}
              onClick={() => handleSort("name")}
            />
            <SortHeaderButton
              label="En disco"
              active={sortKey === "size"}
              dir={sortDir}
              align="right"
              onClick={() => handleSort("size")}
            />
            <div className="hidden sm:flex justify-end">
              <SortHeaderButton
                label="Modificado"
                active={sortKey === "date"}
                dir={sortDir}
                align="right"
                onClick={() => handleSort("date")}
              />
            </div>
            <span className="text-right text-[11.5px] font-medium text-fg-muted">
              <span className="hidden sm:inline">Acciones</span>
            </span>
          </div>

          <div style={{ height: `${virtualizer.getTotalSize()}px`, position: "relative" }}>
            {virtualizer.getVirtualItems().map((vRow) => {
              const item = stableRows[vRow.index];
              if (!item) return null;
              return (
                <div
                  key={item.nodeId}
                  className="group grid grid-cols-[minmax(0,1fr)_88px_40px] sm:grid-cols-[minmax(0,1fr)_104px_96px_118px] items-center gap-2 px-3 border-b border-border-subtle hover:bg-elevated/65 transition-colors duration-140"
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    height: `${rowHeight}px`,
                    transform: `translateY(${vRow.start}px)`,
                  }}
                >
                  <div className="flex items-center gap-2.5 min-w-0">
                    {kind === "dirs" ? (
                      <Folder size={15} className="text-accent shrink-0" />
                    ) : (
                      <File size={15} className="text-fg-muted shrink-0" />
                    )}
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-1.5 min-w-0">
                        {kind === "dirs" ? (
                          <button
                            type="button"
                            onClick={() => onDrillDown(item.nodeId)}
                            title={item.fullPath}
                            className="font-medium text-[12.5px] text-fg-primary hover:text-accent transition-colors truncate bg-transparent border-0 p-0 cursor-pointer text-left"
                          >
                            {item.name}
                          </button>
                        ) : (
                          <button
                            type="button"
                            onClick={() => setInspected(item)}
                            title={item.fullPath}
                            className="font-medium text-[12.5px] text-fg-primary hover:text-accent transition-colors truncate bg-transparent border-0 p-0 cursor-pointer text-left"
                          >
                            {item.name}
                          </button>
                        )}
                        {item.isProtected && (
                          <Badge tone="warning" title={item.protectionReason ?? "Ruta protegida"}>
                            <Lock size={10} />
                            <span className="hidden md:inline">Protegida</span>
                          </Badge>
                        )}
                      </div>
                      {density === "comfortable" && (
                        <div className="flex items-center gap-2 text-[11px] text-fg-muted min-w-0">
                          <PathDisplay path={item.fullPath} maxChars={48} />
                          <span className="sm:hidden shrink-0 tabular">
                            · {formatUnixTs(item.lastWriteTs)}
                          </span>
                        </div>
                      )}
                    </div>
                  </div>

                  <div className="mono text-right text-[12.5px] font-medium text-fg-primary">
                    {formatBytes(item.allocatedBytes)}
                  </div>

                  <div className="hidden sm:block text-right text-[12px] text-fg-secondary tabular">
                    {formatUnixTs(item.lastWriteTs)}
                  </div>

                  {/* Progressive hover actions + Inspector trigger */}
                  <div className="flex justify-end items-center gap-1">
                    <div className="hidden sm:flex items-center gap-1 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity duration-140">
                      <Button
                        variant="ghost"
                        size="icon"
                        onClick={() => onOpenExplorer(item.fullPath)}
                        title="Abrir en Explorador"
                        aria-label={`Abrir ${item.name} en Explorador`}
                      >
                        <FolderOpen size={13} />
                      </Button>
                      {!item.isProtected && (
                        <Button
                          variant="ghost"
                          size="icon"
                          onClick={() => onProtectPath(item)}
                          title="Proteger esta ruta"
                          aria-label={`Proteger ${item.name}`}
                        >
                          <ShieldPlus size={13} className="text-accent" />
                        </Button>
                      )}
                      <Button
                        variant="danger"
                        size="icon"
                        disabled={item.isProtected}
                        onClick={() => {
                          if (!item.isProtected) onSendToCleanup(item);
                        }}
                        title={
                          item.isProtected
                            ? (item.protectionReason ?? "Ruta protegida por lista negra")
                            : "Enviar a Limpieza (Dry-run)"
                        }
                        aria-label={
                          item.isProtected
                            ? `${item.name} está protegida`
                            : `Enviar ${item.name} a limpieza`
                        }
                      >
                        {item.isProtected ? <Lock size={13} /> : <Trash2 size={13} />}
                      </Button>
                    </div>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => setInspected(item)}
                      title="Inspeccionar detalles"
                      aria-label={`Inspeccionar ${item.name}`}
                    >
                      <MoreHorizontal size={14} />
                    </Button>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      </div>

      {inspected && (
        <DetailDrawer
          title={inspected.name}
          subtitle={kind === "dirs" ? "Carpeta en disco" : "Archivo en disco"}
          badge={
            inspected.isProtected ? (
              <Badge tone="warning">
                <Lock size={10} />
                Protegida
              </Badge>
            ) : undefined
          }
          onClose={() => setInspected(null)}
          footer={
            <>
              <Button onClick={() => onOpenExplorer(inspected.fullPath)}>
                <FolderOpen size={13} />
                Explorador
              </Button>
              {!inspected.isProtected && (
                <Button
                  onClick={() => {
                    onProtectPath(inspected);
                    setInspected(null);
                  }}
                >
                  <ShieldPlus size={13} className="text-accent" />
                  Proteger
                </Button>
              )}
              <Button
                variant="danger"
                disabled={inspected.isProtected}
                onClick={() => {
                  if (!inspected.isProtected) {
                    onSendToCleanup(inspected);
                    setInspected(null);
                  }
                }}
              >
                <Trash2 size={13} />
                Limpiar
              </Button>
            </>
          }
        >
          <DetailField label="Tamaño asignado en disco">
            <span className="mono text-[18px] font-semibold text-accent">
              {formatBytes(inspected.allocatedBytes)}
            </span>
          </DetailField>
          <DetailField label="Ruta completa">
            <div className="p-2.5 rounded-[8px] bg-canvas border border-border-default mono text-[12px] break-all">
              {inspected.fullPath}
            </div>
          </DetailField>
          <DetailField label="Última modificación">
            <span className="tabular">{formatUnixTs(inspected.lastWriteTs)}</span>
          </DetailField>
          <DetailField label="Categoría de origen">
            <Badge>{inspected.originCat}</Badge>
          </DetailField>
          {inspected.isProtected && (
            <DetailField label="Motivo de protección">
              <span className="text-warning-text">
                {inspected.protectionReason ?? "En lista negra"}
              </span>
            </DetailField>
          )}
        </DetailDrawer>
      )}
    </div>
  );
}
