export interface BarChartItem {
  key: string;
  label: string;
  value: number;
  valueLabel: string;
  sublabel?: string;
  color: string;
}

interface HorizontalBarChartProps {
  items: BarChartItem[];
  emptyText?: string;
  maxItems?: number;
  onSelectItem?: (key: string) => void;
}

export function HorizontalBarChart({
  items,
  emptyText = "Sin datos suficientes para graficar.",
  maxItems = 8,
  onSelectItem,
}: HorizontalBarChartProps) {
  const visible = items.filter((i) => i.value > 0).slice(0, maxItems);
  const maxVal = visible.reduce((m, i) => Math.max(m, i.value), 0);

  if (visible.length === 0) {
    return <div className="py-4 text-[12px] text-fg-muted">{emptyText}</div>;
  }

  return (
    <div className="flex flex-col gap-2">
      {visible.map((item) => {
        const ratio = maxVal > 0 ? Math.max(0.03, Math.min(1, item.value / maxVal)) : 0;
        const InteractiveTag = onSelectItem ? "button" : "div";
        return (
          <InteractiveTag
            key={item.key}
            {...(onSelectItem
              ? {
                  type: "button" as const,
                  onClick: () => onSelectItem(item.key),
                }
              : {})}
            className={`w-full flex flex-col gap-1 text-left bg-transparent border-0 p-0 ${
              onSelectItem ? "cursor-pointer group" : ""
            }`}
          >
            <div className="flex items-center justify-between gap-2 text-[12px]">
              <span className="flex items-center gap-1.5 min-w-0">
                <span
                  className="w-2 h-2 rounded-full shrink-0"
                  style={{ backgroundColor: item.color }}
                />
                <span className="font-medium text-fg-primary truncate group-hover:text-accent transition-colors">
                  {item.label}
                </span>
                {item.sublabel && (
                  <span className="text-[11px] text-fg-muted truncate hidden sm:inline">
                    · {item.sublabel}
                  </span>
                )}
              </span>
              <span className="mono text-[11.5px] font-medium text-fg-primary shrink-0">
                {item.valueLabel}
              </span>
            </div>
            <div className="h-2 w-full rounded-full bg-canvas overflow-hidden">
              <div
                className="h-full w-full rounded-full sc-bar-grow"
                style={{
                  backgroundColor: item.color,
                  transform: `scaleX(${ratio})`,
                }}
              />
            </div>
          </InteractiveTag>
        );
      })}
    </div>
  );
}
