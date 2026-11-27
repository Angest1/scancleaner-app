import { BarChart3, ChevronDown } from "lucide-react";
import { useState } from "react";
import { formatBytes } from "../../lib/format";
import type { DevRuleCategory, DevScanReport } from "../../lib/ipc";
import { DonutChart } from "../charts/DonutChart";
import { HorizontalBarChart } from "../charts/HorizontalBarChart";
import { Card } from "../ui/Card";
import { Collapse } from "../ui/Collapse";

export const DEV_CATEGORY_COLORS: Record<DevRuleCategory, string> = {
  Node: "#22c55e",
  Python: "#3b82f6",
  PHP: "#8b5cf6",
  Android: "#10b981",
  Docker: "#0ea5e9",
  IDE: "#f59e0b",
  Testing: "#ec4899",
  Sistema: "#64748b",
};

interface DevSummaryChartsProps {
  report: DevScanReport | null;
  onSelectCategory: (cat: string) => void;
}

export function DevSummaryCharts({ report, onSelectCategory }: DevSummaryChartsProps) {
  const [open, setOpen] = useState(true);
  const hasData = Boolean(report && report.totalRecoverableBytes > 0);

  const byCat = new Map<DevRuleCategory, { bytes: number; count: number }>();
  for (const c of report?.candidates ?? []) {
    const prev = byCat.get(c.categoria) ?? { bytes: 0, count: 0 };
    byCat.set(c.categoria, { bytes: prev.bytes + c.sizeBytes, count: prev.count + 1 });
  }
  const catItems = Array.from(byCat.entries())
    .sort((a, b) => b[1].bytes - a[1].bytes)
    .map(([cat, info]) => ({
      key: cat,
      label: cat,
      sublabel: `${info.count} ítems`,
      value: info.bytes,
      valueLabel: formatBytes(info.bytes),
      color: DEV_CATEGORY_COLORS[cat],
    }));

  return (
    <Collapse open={hasData}>
      {hasData && report && (
        <Card className="flex flex-col gap-3">
          <button
            type="button"
            onClick={() => setOpen((v) => !v)}
            aria-expanded={open}
            className="w-full flex items-center justify-between gap-2 bg-transparent border-0 p-0 cursor-pointer text-left group"
          >
            <div className="flex items-center gap-2">
              <BarChart3 size={15} className="text-accent" />
              <span className="text-[13.5px] font-semibold text-fg-primary group-hover:text-accent transition-colors">
                Distribución recuperable (Seguro vs. Revisar y por ecosistema)
              </span>
            </div>
            <ChevronDown
              size={15}
              className={`text-fg-muted transition-transform duration-150 ${open ? "rotate-0" : "-rotate-90"}`}
            />
          </button>

          <Collapse open={open}>
            <div className="grid grid-cols-1 lg:grid-cols-12 gap-4 pt-1 items-center">
              <div className="lg:col-span-5 p-3 rounded-[10px] bg-canvas/60 border border-border-subtle">
                <DonutChart
                  size={132}
                  strokeWidth={16}
                  centerValue={formatBytes(report.totalRecoverableBytes)}
                  centerLabel="Recuperable"
                  segments={[
                    {
                      key: "seguro",
                      label: "Nivel Seguro",
                      value: report.safeRecoverableBytes,
                      valueLabel: formatBytes(report.safeRecoverableBytes),
                      color: "var(--success)",
                    },
                    {
                      key: "revisar",
                      label: "Nivel Revisar",
                      value: report.reviewRecoverableBytes,
                      valueLabel: formatBytes(report.reviewRecoverableBytes),
                      color: "var(--warning)",
                    },
                  ]}
                />
              </div>
              <div className="lg:col-span-7 p-3 rounded-[10px] bg-canvas/60 border border-border-subtle">
                <HorizontalBarChart items={catItems} maxItems={6} onSelectItem={onSelectCategory} />
              </div>
            </div>
          </Collapse>
        </Card>
      )}
    </Collapse>
  );
}
