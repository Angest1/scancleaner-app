import type { ReactNode } from "react";

export interface SegmentOption<T extends string> {
  value: T;
  label: string;
  icon?: ReactNode;
  count?: number;
}

interface SegmentedControlProps<T extends string> {
  options: SegmentOption<T>[];
  value: T;
  onChange: (value: T) => void;
  ariaLabel?: string;
  className?: string;
}

export function SegmentedControl<T extends string>({
  options,
  value,
  onChange,
  ariaLabel,
  className = "",
}: SegmentedControlProps<T>) {
  return (
    <div
      role="tablist"
      aria-label={ariaLabel}
      className={`inline-flex items-center gap-0.5 p-1 rounded-[10px] bg-elevated border border-border-default ${className}`}
    >
      {options.map((opt) => {
        const active = opt.value === value;
        return (
          <button
            key={opt.value}
            type="button"
            role="tab"
            aria-selected={active}
            onClick={() => onChange(opt.value)}
            className={`inline-flex items-center gap-1.5 h-7 px-2.5 rounded-[7px] text-[12px] font-medium cursor-pointer transition-[background-color,color,border-color,box-shadow] duration-140 border ${
              active
                ? "bg-tile text-fg-primary border-border-default shadow-[0_2px_6px_-2px_rgba(32,28,76,0.16)]"
                : "bg-transparent text-fg-secondary border-transparent hover:text-fg-primary"
            }`}
          >
            {opt.icon}
            <span>{opt.label}</span>
            {opt.count !== undefined && (
              <span className="mono text-[11px] text-fg-muted">{opt.count}</span>
            )}
          </button>
        );
      })}
    </div>
  );
}
