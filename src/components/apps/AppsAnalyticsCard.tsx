import { BarChart3, ChevronDown } from "lucide-react";
import { useState } from "react";
import { formatBytes } from "../../lib/format";
import type { InstalledApp } from "../../lib/ipc";
import { DonutChart } from "../charts/DonutChart";
import { HorizontalBarChart } from "../charts/HorizontalBarChart";
import { Card } from "../ui/Card";
import { Collapse } from "../ui/Collapse";

interface AppsAnalyticsCardProps {
  apps: InstalledApp[];
  totalBytes: number;
  win32Bytes: number;
  msixBytes: number;
  win32Count: number;
  msixCount: number;
  topApps: InstalledApp[];
  onFilterSource: (source: "win32" | "store_msix") => void;
  onInspectApp: (app: InstalledApp) => void;
}

export function AppsAnalyticsCard({
  apps,
  totalBytes,
  win32Bytes,
  msixBytes,
  win32Count,
  msixCount,
  topApps,
  onFilterSource,
  onInspectApp,
}: AppsAnalyticsCardProps) {
  const [open, setOpen] = useState(true);
  const hasData = apps.length > 0;

  return (
    <Collapse open={hasData}>
      {hasData && (
        <Card className="flex flex-col gap-2.5 py-3">
          <button
            type="button"
            onClick={() => setOpen((v) => !v)}
            aria-expanded={open}
            className="w-full flex items-center justify-between gap-2 bg-transparent border-0 p-0 cursor-pointer text-left group"
          >
            <span className="flex items-center gap-2 text-[13px] font-semibold text-fg-primary group-hover:text-accent transition-colors">
              <BarChart3 size={14} className="text-accent" />
              Resumen visual ({apps.length} apps · {formatBytes(totalBytes)})
            </span>
            <ChevronDown
              size={14}
              className={`text-fg-muted transition-transform duration-150 ${open ? "rotate-0" : "-rotate-90"}`}
            />
          </button>

          <Collapse open={open}>
            <div className="grid grid-cols-1 lg:grid-cols-12 gap-4 pt-1 items-center">
              <div className="lg:col-span-5 p-2.5 rounded-[10px] bg-canvas/60 border border-border-subtle">
                <DonutChart
                  size={124}
                  strokeWidth={15}
                  centerValue={formatBytes(totalBytes)}
                  centerLabel={`${apps.length} apps`}
                  onSelectSegment={(k) => onFilterSource(k as "win32" | "store_msix")}
                  segments={[
                    {
                      key: "win32",
                      label: `Win32 (${win32Count})`,
                      value: win32Bytes,
                      valueLabel: formatBytes(win32Bytes),
                      color: "#3b82f6",
                    },
                    {
                      key: "store_msix",
                      label: `Store / MSIX (${msixCount})`,
                      value: msixBytes,
                      valueLabel: formatBytes(msixBytes),
                      color: "#8b5cf6",
                    },
                  ]}
                />
              </div>
              <div className="lg:col-span-7 p-2.5 rounded-[10px] bg-canvas/60 border border-border-subtle">
                <div className="text-[11px] font-medium text-fg-muted mb-1.5">
                  Top 6 aplicaciones más pesadas
                </div>
                <HorizontalBarChart
                  maxItems={6}
                  onSelectItem={(id) => {
                    const hit = apps.find((a) => a.id === id);
                    if (hit) onInspectApp(hit);
                  }}
                  items={topApps.map((a) => ({
                    key: a.id,
                    label: a.name,
                    sublabel: a.source === "store_msix" ? "MSIX" : "Win32",
                    value: a.sizeBytes,
                    valueLabel: formatBytes(a.sizeBytes),
                    color: a.source === "store_msix" ? "#8b5cf6" : "#3b82f6",
                  }))}
                />
              </div>
            </div>
          </Collapse>
        </Card>
      )}
    </Collapse>
  );
}
