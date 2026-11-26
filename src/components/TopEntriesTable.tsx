import { useVirtualizer } from "@tanstack/react-virtual";
import { File, Folder, FolderOpen, Lock, ShieldPlus, Trash2 } from "lucide-react";
import { useRef } from "react";
import { formatBytes, formatUnixTs } from "../lib/format";
import type { TopEntryDto } from "../lib/ipc";

interface TopEntriesTableProps {
  entries: TopEntryDto[];
  kind: "dirs" | "files";
  onDrillDown: (nodeId: number) => void;
  onOpenExplorer: (path: string) => void;
  onSendToCleanup: (entry: TopEntryDto) => void;
  onProtectPath: (entry: TopEntryDto) => void;
}

export function TopEntriesTable({
  entries,
  kind,
  onDrillDown,
  onOpenExplorer,
  onSendToCleanup,
  onProtectPath,
}: TopEntriesTableProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: entries.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 44,
    overscan: 8,
  });

  if (entries.length === 0) {
    return (
      <div style={{ padding: 24, textAlign: "center", color: "var(--text-muted)" }}>
        No hay elementos para mostrar en este nivel.
      </div>
    );
  }

  return (
    <div style={{ border: "1px solid var(--border)", borderRadius: 8, overflow: "hidden" }}>
      <div className="table-header" style={{ gridTemplateColumns: "1fr 120px 110px 150px" }}>
        <span>Ruta</span>
        <span style={{ textAlign: "right" }}>En disco</span>
        <span style={{ textAlign: "right" }}>Modificado</span>
        <span style={{ textAlign: "right" }}>Acciones</span>
      </div>

      <div ref={parentRef} style={{ height: 320, overflow: "auto" }}>
        <div style={{ height: `${virtualizer.getTotalSize()}px`, position: "relative" }}>
          {virtualizer.getVirtualItems().map((vRow) => {
            const item = entries[vRow.index];
            return (
              <div
                key={item.nodeId}
                className="table-row"
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  height: `${vRow.size}px`,
                  transform: `translateY(${vRow.start}px)`,
                  gridTemplateColumns: "1fr 120px 110px 150px",
                }}
              >
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 8,
                    overflow: "hidden",
                    cursor: kind === "dirs" ? "pointer" : "default",
                  }}
                  onClick={() => {
                    if (kind === "dirs") onDrillDown(item.nodeId);
                  }}
                  onKeyDown={(e) => {
                    if (kind === "dirs" && e.key === "Enter") onDrillDown(item.nodeId);
                  }}
                  title={item.fullPath}
                >
                  {kind === "dirs" ? (
                    <Folder size={15} color="#38bdf8" style={{ flexShrink: 0 }} />
                  ) : (
                    <File size={15} color="#94a3b8" style={{ flexShrink: 0 }} />
                  )}
                  <div
                    style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}
                  >
                    <div
                      style={{
                        fontWeight: 500,
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        display: "flex",
                        alignItems: "center",
                        gap: 6,
                      }}
                    >
                      <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>
                        {item.name}
                      </span>
                      {item.isProtected && (
                        <span
                          className="badge badge-warning"
                          title={item.protectionReason ?? "Ruta protegida"}
                          style={{
                            display: "inline-flex",
                            alignItems: "center",
                            gap: 3,
                            flexShrink: 0,
                          }}
                        >
                          <Lock size={10} />
                          Protegida
                        </span>
                      )}
                    </div>
                    <div
                      style={{
                        fontSize: 11,
                        color: "var(--text-muted)",
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                      }}
                    >
                      {item.fullPath}
                    </div>
                  </div>
                </div>

                <div className="mono" style={{ textAlign: "right", fontWeight: 600 }}>
                  {formatBytes(item.allocatedBytes)}
                </div>

                <div style={{ textAlign: "right", color: "var(--text-secondary)", fontSize: 12 }}>
                  {formatUnixTs(item.lastWriteTs)}
                </div>

                <div style={{ display: "flex", justifyContent: "flex-end", gap: 6 }}>
                  <button
                    type="button"
                    className="btn"
                    style={{ padding: "4px 8px" }}
                    onClick={() => onOpenExplorer(item.fullPath)}
                    title="Abrir en Explorador"
                  >
                    <FolderOpen size={13} />
                  </button>
                  {!item.isProtected && (
                    <button
                      type="button"
                      className="btn"
                      style={{ padding: "4px 8px" }}
                      onClick={() => onProtectPath(item)}
                      title="Proteger esta ruta"
                    >
                      <ShieldPlus size={13} color="#38bdf8" />
                    </button>
                  )}
                  <button
                    type="button"
                    className="btn btn-danger"
                    style={{
                      padding: "4px 8px",
                      opacity: item.isProtected ? 0.45 : 1,
                      cursor: item.isProtected ? "not-allowed" : "pointer",
                    }}
                    disabled={item.isProtected}
                    onClick={() => {
                      if (!item.isProtected) onSendToCleanup(item);
                    }}
                    title={
                      item.isProtected
                        ? (item.protectionReason ?? "Ruta protegida por lista negra")
                        : "Enviar a Limpieza (Dry-run)"
                    }
                  >
                    {item.isProtected ? <Lock size={13} /> : <Trash2 size={13} />}
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
