import { ArrowDown, ArrowUp, ArrowUpDown, Rows3, Rows4 } from "lucide-react";

export type TableDensity = "compact" | "comfortable";
export type SortDir = "asc" | "desc";

export function DensityToggle({
  density,
  onChange,
}: {
  density: TableDensity;
  onChange: (next: TableDensity) => void;
}) {
  return (
    <fieldset
      aria-label="Densidad de filas"
      className="m-0 inline-flex items-center gap-0.5 p-0.5 rounded-[8px] bg-elevated border border-border-default"
    >
      <button
        type="button"
        title="Vista compacta"
        aria-pressed={density === "compact"}
        onClick={() => onChange("compact")}
        className={`inline-flex items-center gap-1 h-6.5 px-2 rounded-[6px] text-[11.5px] font-medium border cursor-pointer transition-colors ${
          density === "compact"
            ? "bg-tile text-fg-primary border-border-default"
            : "bg-transparent text-fg-secondary border-transparent hover:text-fg-primary"
        }`}
      >
        <Rows4 size={12} />
        <span className="hidden sm:inline">Compacta</span>
      </button>
      <button
        type="button"
        title="Vista cómoda"
        aria-pressed={density === "comfortable"}
        onClick={() => onChange("comfortable")}
        className={`inline-flex items-center gap-1 h-6.5 px-2 rounded-[6px] text-[11.5px] font-medium border cursor-pointer transition-colors ${
          density === "comfortable"
            ? "bg-tile text-fg-primary border-border-default"
            : "bg-transparent text-fg-secondary border-transparent hover:text-fg-primary"
        }`}
      >
        <Rows3 size={12} />
        <span className="hidden sm:inline">Cómoda</span>
      </button>
    </fieldset>
  );
}

export function SortHeaderButton({
  label,
  active,
  dir,
  align = "left",
  onClick,
}: {
  label: string;
  active: boolean;
  dir: SortDir;
  align?: "left" | "right";
  onClick: () => void;
}) {
  const Icon = !active ? ArrowUpDown : dir === "asc" ? ArrowUp : ArrowDown;
  return (
    <button
      type="button"
      onClick={onClick}
      className={`inline-flex items-center gap-1 bg-transparent border-0 p-0 text-[11.5px] font-medium cursor-pointer transition-colors ${
        active ? "text-accent" : "text-fg-muted hover:text-fg-primary"
      } ${align === "right" ? "justify-end ml-auto" : "justify-start"}`}
    >
      <span>{label}</span>
      <Icon size={11} className={active ? "opacity-100" : "opacity-55"} />
    </button>
  );
}
