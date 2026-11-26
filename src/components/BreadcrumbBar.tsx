import { ArrowUp, ChevronRight } from "lucide-react";
import type { BreadcrumbItem } from "../lib/ipc";

interface BreadcrumbBarProps {
  breadcrumbs: BreadcrumbItem[];
  onSelectNode: (nodeId: number) => void;
}

export function BreadcrumbBar({ breadcrumbs, onSelectNode }: BreadcrumbBarProps) {
  const canGoUp = breadcrumbs.length > 1;
  const parentId = canGoUp ? breadcrumbs[breadcrumbs.length - 2].nodeId : 0;

  return (
    <div style={{ display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" }}>
      <button
        type="button"
        className="btn"
        disabled={!canGoUp}
        onClick={() => onSelectNode(parentId)}
        title="Subir un nivel"
      >
        <ArrowUp size={14} />
        Subir
      </button>

      <div style={{ display: "flex", alignItems: "center", flexWrap: "wrap", gap: 4 }}>
        {breadcrumbs.map((bc, idx) => {
          const isLast = idx === breadcrumbs.length - 1;
          return (
            <span key={bc.nodeId} style={{ display: "inline-flex", alignItems: "center", gap: 4 }}>
              {idx > 0 && <ChevronRight size={14} color="#64748b" />}
              <button
                type="button"
                className="btn"
                style={{
                  padding: "4px 9px",
                  background: isLast ? "rgba(56, 189, 248, 0.16)" : "transparent",
                  borderColor: isLast ? "rgba(56, 189, 248, 0.4)" : "transparent",
                  color: isLast ? "#38bdf8" : "#cbd5e1",
                }}
                onClick={() => onSelectNode(bc.nodeId)}
              >
                {bc.name}
              </button>
            </span>
          );
        })}
      </div>
    </div>
  );
}
