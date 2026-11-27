import { useVirtualizer } from "@tanstack/react-virtual";
import { MoreHorizontal } from "lucide-react";
import { useMemo, useRef, useState } from "react";
import { formatBytes } from "../../lib/format";
import type { CleanupLogEntry } from "../../lib/ipc";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { DetailDrawer, DetailField } from "../ui/DetailDrawer";
import { PathDisplay } from "../ui/PathDisplay";
import { type SortDir, SortHeaderButton, type TableDensity } from "../ui/TableToolbar";

type LogSortKey = "date" | "bytes";

interface CleanupLogsTableProps {
  logs: CleanupLogEntry[];
  density: TableDensity;
}

export function CleanupLogsTable({ logs, density }: CleanupLogsTableProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const [sort, setSort] = useState<{ key: LogSortKey; dir: SortDir }>({
    key: "date",
    dir: "desc",
  });
  const [inspected, setInspected] = useState<CleanupLogEntry | null>(null);

  const sorted = useMemo(() => {
    const mult = sort.dir === "asc" ? 1 : -1;
    return [...logs].sort((a, b) => {
      if (sort.key === "bytes") return (a.bytes - b.bytes) * mult;
      return (new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime()) * mult;
    });
  }, [logs, sort]);

  const toggleSort = (key: LogSortKey) =>
    setSort((s) => ({
      key,
      dir: s.key === key ? (s.dir === "asc" ? "desc" : "asc") : "desc",
    }));

  const rowHeight = density === "compact" ? 38 : 48;
  const virtualizer = useVirtualizer({
    count: sorted.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => rowHeight,
    overscan: 10,
  });

  return (
    <>
      <Card padded={false} className="overflow-hidden">
        <div
          ref={parentRef}
          className="min-h-[380px] h-[calc(100vh-340px)] overflow-y-auto overflow-x-hidden"
        >
          <div className="sticky top-0 z-10 grid grid-cols-[minmax(0,1fr)_88px_36px] md:grid-cols-[148px_minmax(0,1fr)_96px_96px_88px_36px] items-center gap-2 px-3.5 py-2.5 border-b border-border-default bg-shell backdrop-blur-md text-[11.5px] font-medium text-fg-muted">
            <div className="hidden md:flex">
              <SortHeaderButton
                label="Fecha"
                active={sort.key === "date"}
                dir={sort.dir}
                onClick={() => toggleSort("date")}
              />
            </div>
            <span>Ruta</span>
            <span className="hidden md:inline">Modo</span>
            <SortHeaderButton
              label="Bytes"
              active={sort.key === "bytes"}
              dir={sort.dir}
              align="right"
              onClick={() => toggleSort("bytes")}
            />
            <span className="hidden md:inline text-right">Estado</span>
            <span />
          </div>
          {sorted.length === 0 ? (
            <div className="py-10 text-center text-[12.5px] text-fg-muted">
              No hay registros de limpieza todavía.
            </div>
          ) : (
            <div style={{ height: `${virtualizer.getTotalSize()}px`, position: "relative" }}>
              {virtualizer.getVirtualItems().map((vRow) => {
                const l = sorted[vRow.index];
                if (!l) return null;
                return (
                  <div
                    key={`${l.planId}-${vRow.index}`}
                    className="group grid grid-cols-[minmax(0,1fr)_88px_36px] md:grid-cols-[148px_minmax(0,1fr)_96px_96px_88px_36px] items-center gap-2 px-3.5 border-b border-border-subtle text-[12.5px] hover:bg-elevated/55 transition-colors"
                    style={{
                      position: "absolute",
                      top: 0,
                      left: 0,
                      width: "100%",
                      height: `${rowHeight}px`,
                      transform: `translateY(${vRow.start}px)`,
                    }}
                  >
                    <span className="hidden md:inline text-[12px] text-fg-secondary tabular truncate">
                      {new Date(l.timestamp).toLocaleString("es-ES")}
                    </span>
                    <div className="min-w-0">
                      <PathDisplay
                        path={l.path}
                        maxChars={48}
                        className="text-[12px] text-fg-primary"
                      />
                      <div className="md:hidden text-[11px] text-fg-muted tabular mt-0.5">
                        {new Date(l.timestamp).toLocaleString("es-ES")} · {l.mode} · {l.status}
                      </div>
                    </div>
                    <span className="hidden md:inline text-[12px] text-fg-secondary truncate">
                      {l.mode}
                    </span>
                    <span className="mono text-right font-medium text-fg-primary">
                      {formatBytes(l.bytes)}
                    </span>
                    <div className="hidden md:block text-right">
                      <Badge tone={l.status === "failed" ? "danger" : "success"}>{l.status}</Badge>
                    </div>
                    <div className="flex justify-end">
                      <Button
                        variant="ghost"
                        size="icon"
                        onClick={() => setInspected(l)}
                        title="Ver detalle del registro"
                        aria-label={`Ver detalle de ${l.path}`}
                      >
                        <MoreHorizontal size={14} />
                      </Button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </Card>

      {inspected && (
        <DetailDrawer
          title="Registro de limpieza"
          subtitle={new Date(inspected.timestamp).toLocaleString("es-ES")}
          badge={
            <Badge tone={inspected.status === "failed" ? "danger" : "success"}>
              {inspected.status}
            </Badge>
          }
          onClose={() => setInspected(null)}
          footer={<Button onClick={() => setInspected(null)}>Cerrar</Button>}
        >
          <DetailField label="Espacio procesado">
            <span className="mono text-[18px] font-semibold text-accent">
              {formatBytes(inspected.bytes)}
            </span>
          </DetailField>
          <DetailField label="Ruta completa">
            <div className="p-2.5 rounded-[8px] bg-canvas border border-border-default">
              <PathDisplay path={inspected.path} maxChars={120} />
            </div>
          </DetailField>
          <DetailField label="Modo de ejecución">
            <Badge>{inspected.mode}</Badge>
          </DetailField>
          <DetailField label="ID de plan">
            <span className="mono text-[12px] text-fg-secondary">{inspected.planId}</span>
          </DetailField>
        </DetailDrawer>
      )}
    </>
  );
}
