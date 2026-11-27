import { useState } from "react";

export interface DonutSegment {
  key: string;
  label: string;
  value: number;
  valueLabel: string;
  color: string;
}

interface DonutChartProps {
  segments: DonutSegment[];
  centerValue: string;
  centerLabel: string;
  size?: number;
  strokeWidth?: number;
  onSelectSegment?: (key: string) => void;
}

export function DonutChart({
  segments,
  centerValue,
  centerLabel,
  size = 136,
  strokeWidth = 16,
  onSelectSegment,
}: DonutChartProps) {
  const [hoveredKey, setHoveredKey] = useState<string | null>(null);
  const nonZero = segments.filter((s) => s.value > 0);
  const total = nonZero.reduce((acc, s) => acc + s.value, 0);
  const radius = (size - strokeWidth) / 2;
  const circumference = 2 * Math.PI * radius;

  let cumulative = 0;
  const arcs = nonZero.map((seg) => {
    const frac = total > 0 ? seg.value / total : 0;
    const len = frac * circumference;
    const offset = -cumulative;
    cumulative += len;
    return { seg, frac, len, offset };
  });

  const activeSeg = nonZero.find((s) => s.key === hoveredKey);

  return (
    <div className="flex items-center gap-4 flex-wrap sm:flex-nowrap">
      <div
        className="relative shrink-0 grid place-items-center"
        style={{ width: size, height: size }}
      >
        <svg
          width={size}
          height={size}
          viewBox={`0 0 ${size} ${size}`}
          className="-rotate-90"
          role="img"
          aria-label={`${centerLabel}: ${centerValue}`}
        >
          <circle
            cx={size / 2}
            cy={size / 2}
            r={radius}
            fill="none"
            stroke="var(--border-default)"
            strokeWidth={strokeWidth}
          />
          {arcs.map(({ seg, len, offset }) => {
            const isDimmed = hoveredKey !== null && hoveredKey !== seg.key;
            return (
              <circle
                key={seg.key}
                cx={size / 2}
                cy={size / 2}
                r={radius}
                fill="none"
                stroke={seg.color}
                strokeWidth={hoveredKey === seg.key ? strokeWidth + 2 : strokeWidth}
                strokeDasharray={`${Math.max(1.5, len - 1.5)} ${circumference}`}
                strokeDashoffset={offset}
                className="transition-[stroke-dashoffset,stroke-dasharray,opacity] duration-300 ease-out cursor-pointer outline-none"
                style={{ opacity: isDimmed ? 0.35 : 1 }}
                tabIndex={onSelectSegment ? 0 : undefined}
                onMouseEnter={() => setHoveredKey(seg.key)}
                onMouseLeave={() => setHoveredKey(null)}
                onClick={() => onSelectSegment?.(seg.key)}
                onKeyDown={(e) => {
                  if (onSelectSegment && (e.key === "Enter" || e.key === " ")) {
                    e.preventDefault();
                    onSelectSegment(seg.key);
                  }
                }}
              >
                <title>
                  {seg.label}: {seg.valueLabel}
                </title>
              </circle>
            );
          })}
        </svg>

        <div className="absolute inset-0 flex flex-col items-center justify-center text-center pointer-events-none px-3">
          <span className="mono text-[13.5px] font-semibold text-fg-primary leading-tight truncate max-w-full">
            {activeSeg ? activeSeg.valueLabel : centerValue}
          </span>
          <span className="text-[10.5px] text-fg-muted truncate max-w-full mt-0.5">
            {activeSeg ? activeSeg.label : centerLabel}
          </span>
        </div>
      </div>

      <div className="flex-1 min-w-[140px] flex flex-col gap-1.5">
        {nonZero.slice(0, 6).map((seg) => {
          const pct = total > 0 ? (seg.value / total) * 100 : 0;
          const active = hoveredKey === seg.key;
          return (
            <button
              key={seg.key}
              type="button"
              onMouseEnter={() => setHoveredKey(seg.key)}
              onMouseLeave={() => setHoveredKey(null)}
              onClick={() => onSelectSegment?.(seg.key)}
              className={`flex items-center justify-between gap-2 px-2 py-1 rounded-[7px] text-left border-0 cursor-pointer transition-colors ${
                active ? "bg-elevated" : "bg-transparent hover:bg-elevated/60"
              }`}
            >
              <span className="flex items-center gap-2 min-w-0">
                <span
                  className="w-2.5 h-2.5 rounded-[3px] shrink-0"
                  style={{ backgroundColor: seg.color }}
                />
                <span className="text-[12px] text-fg-primary truncate">{seg.label}</span>
              </span>
              <span className="mono text-[11.5px] text-fg-secondary shrink-0">
                {seg.valueLabel} <span className="text-fg-muted">({pct.toFixed(0)}%)</span>
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
