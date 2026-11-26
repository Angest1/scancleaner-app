import { ArrowUp, ChevronRight } from "lucide-react";
import type { BreadcrumbItem } from "../lib/ipc";
import { Button } from "./ui/Button";

interface BreadcrumbBarProps {
  breadcrumbs: BreadcrumbItem[];
  onSelectNode: (nodeId: number) => void;
}

export function BreadcrumbBar({ breadcrumbs, onSelectNode }: BreadcrumbBarProps) {
  const canGoUp = breadcrumbs.length > 1;
  const parentId = canGoUp ? breadcrumbs[breadcrumbs.length - 2].nodeId : 0;

  return (
    <div className="flex items-center gap-2 flex-wrap min-w-0">
      <Button
        variant="default"
        size="sm"
        disabled={!canGoUp}
        onClick={() => onSelectNode(parentId)}
        title="Subir un nivel"
      >
        <ArrowUp size={13} />
        Subir
      </Button>

      <nav aria-label="Ruta actual" className="min-w-0">
        <ol className="flex items-center flex-wrap gap-0.5 m-0 p-0 list-none">
          {breadcrumbs.map((bc, idx) => {
            const isLast = idx === breadcrumbs.length - 1;
            return (
              <li key={bc.nodeId} className="inline-flex items-center gap-0.5">
                {idx > 0 && <ChevronRight size={13} className="text-fg-muted shrink-0" />}
                <button
                  type="button"
                  aria-current={isLast ? "page" : undefined}
                  onClick={() => onSelectNode(bc.nodeId)}
                  className={`h-7 px-2 rounded-[6px] text-[12px] font-medium border cursor-pointer transition-[background-color,color,border-color] duration-140 ${
                    isLast
                      ? "bg-accent-soft border-accent-border text-accent"
                      : "bg-transparent border-transparent text-fg-secondary hover:bg-elevated hover:text-fg-primary"
                  }`}
                >
                  {bc.name}
                </button>
              </li>
            );
          })}
        </ol>
      </nav>
    </div>
  );
}
