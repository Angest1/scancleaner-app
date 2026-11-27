import { ChevronDown, FolderTree } from "lucide-react";
import { useState } from "react";
import { formatBytes } from "../../lib/format";

export interface CategoryTreeNode {
  id: string;
  label: string;
  count: number;
  bytes?: number;
  color?: string;
  children?: CategoryTreeNode[];
}

interface CategoryTreeSidebarProps {
  title: string;
  rootLabel?: string;
  totalCount: number;
  totalBytes?: number;
  nodes: CategoryTreeNode[];
  selectedId: string;
  onSelect: (id: string) => void;
  footer?: React.ReactNode;
}

export function CategoryTreeSidebar({
  title,
  rootLabel = "Todas las categorías",
  totalCount,
  totalBytes,
  nodes,
  selectedId,
  onSelect,
  footer,
}: CategoryTreeSidebarProps) {
  const [expanded, setExpanded] = useState<Set<string>>(
    () => new Set(nodes.filter((n) => n.children?.length).map((n) => n.id)),
  );

  const toggleExpand = (id: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const maxBytes = Math.max(1, totalBytes ?? 0);

  return (
    <aside
      aria-label={title}
      className="w-full lg:w-[264px] lg:shrink-0 rounded-[14px] bg-panel border border-border-default shadow-[var(--shadow-neu)] p-3 flex flex-col gap-2 lg:sticky lg:top-0 lg:self-start"
    >
      <div className="flex items-center gap-2 px-1.5 py-1 border-b border-border-subtle">
        <FolderTree size={14} className="text-accent shrink-0" />
        <span className="text-[12px] font-semibold uppercase tracking-[0.04em] text-fg-muted">
          {title}
        </span>
      </div>

      {/* Root "All" node */}
      <button
        type="button"
        onClick={() => onSelect("all")}
        aria-current={selectedId === "all" ? "true" : undefined}
        className={`w-full flex items-center justify-between gap-2 px-2.5 py-2 rounded-[9px] text-[12.5px] font-medium border cursor-pointer transition-colors ${
          selectedId === "all"
            ? "bg-accent-soft text-accent border-accent-border"
            : "bg-transparent text-fg-primary border-transparent hover:bg-elevated"
        }`}
      >
        <span className="truncate">{rootLabel}</span>
        <span className="mono text-[11px] shrink-0">
          {totalBytes !== undefined ? `${formatBytes(totalBytes)} · ` : ""}
          {totalCount}
        </span>
      </button>

      {/* Tree branches */}
      <nav className="flex flex-row lg:flex-col gap-1 overflow-x-auto lg:overflow-x-visible pb-1 lg:pb-0">
        {nodes.map((node) => {
          const isSelected = selectedId === node.id;
          const hasChildren = Boolean(node.children && node.children.length > 0);
          const isOpen = expanded.has(node.id);
          const byteRatio =
            node.bytes !== undefined && totalBytes ? Math.min(1, node.bytes / maxBytes) : 0;

          return (
            <div key={node.id} className="shrink-0 lg:shrink flex flex-col gap-0.5">
              <div className="flex items-center gap-0.5">
                {hasChildren && (
                  <button
                    type="button"
                    aria-label={isOpen ? `Contraer ${node.label}` : `Expandir ${node.label}`}
                    onClick={() => toggleExpand(node.id)}
                    className="w-5 h-7 grid place-items-center text-fg-muted hover:text-fg-primary bg-transparent border-0 p-0 cursor-pointer shrink-0"
                  >
                    <ChevronDown
                      size={13}
                      className={`transition-transform duration-150 ${isOpen ? "rotate-0" : "-rotate-90"}`}
                    />
                  </button>
                )}

                <button
                  type="button"
                  onClick={() => onSelect(node.id)}
                  aria-current={isSelected ? "true" : undefined}
                  className={`flex-1 min-w-0 flex flex-col gap-1 px-2.5 py-1.5 rounded-[8px] text-left border cursor-pointer transition-colors ${
                    isSelected
                      ? "bg-accent-soft text-accent border-accent-border"
                      : "bg-transparent text-fg-secondary border-transparent hover:bg-elevated hover:text-fg-primary"
                  }`}
                >
                  <div className="flex items-center justify-between gap-2 w-full">
                    <span className="flex items-center gap-1.5 min-w-0">
                      <span
                        className="w-2 h-2 rounded-full shrink-0"
                        style={{ backgroundColor: node.color ?? "var(--accent)" }}
                      />
                      <span className="text-[12.5px] font-medium truncate">{node.label}</span>
                    </span>
                    <span className="mono text-[11px] shrink-0 opacity-85">{node.count}</span>
                  </div>

                  {node.bytes !== undefined && (
                    <div className="flex items-center justify-between gap-2 w-full">
                      <div className="flex-1 h-1 rounded-full bg-canvas overflow-hidden">
                        <div
                          className="h-full w-full rounded-full origin-left"
                          style={{
                            backgroundColor: node.color ?? "var(--accent)",
                            transform: `scaleX(${Math.max(0.04, byteRatio)})`,
                          }}
                        />
                      </div>
                      <span className="mono text-[10.5px] text-fg-muted shrink-0">
                        {formatBytes(node.bytes)}
                      </span>
                    </div>
                  )}
                </button>
              </div>

              {hasChildren && isOpen && (
                <div className="hidden lg:flex flex-col gap-0.5 pl-5 ml-2.5 border-l border-border-subtle">
                  {node.children?.map((child) => {
                    const childSel = selectedId === child.id;
                    return (
                      <button
                        key={child.id}
                        type="button"
                        onClick={() => onSelect(child.id)}
                        aria-current={childSel ? "true" : undefined}
                        className={`w-full flex items-center justify-between gap-2 px-2 py-1.5 rounded-[7px] text-[12px] border cursor-pointer transition-colors ${
                          childSel
                            ? "bg-accent-soft text-accent border-accent-border font-medium"
                            : "bg-transparent text-fg-secondary border-transparent hover:bg-elevated hover:text-fg-primary"
                        }`}
                      >
                        <span className="flex items-center gap-1.5 min-w-0">
                          <span
                            className="w-1.5 h-1.5 rounded-full shrink-0"
                            style={{ backgroundColor: child.color ?? "var(--text-muted)" }}
                          />
                          <span className="truncate">{child.label}</span>
                        </span>
                        <span className="mono text-[11px] shrink-0 opacity-80">{child.count}</span>
                      </button>
                    );
                  })}
                </div>
              )}
            </div>
          );
        })}
      </nav>

      {footer && <div className="pt-2 border-t border-border-subtle">{footer}</div>}
    </aside>
  );
}
