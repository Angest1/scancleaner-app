import { formatBytes } from "../../lib/format";
import type { UninstallLeftoverSuggestion } from "../../lib/ipc";
import { Button } from "../ui/Button";
import { Collapse } from "../ui/Collapse";
import { AlertBanner } from "../ui/Feedback";

interface AppLeftoversBannerProps {
  leftovers: UninstallLeftoverSuggestion[];
  onQueueCleanup?: (path: string) => void;
}

export function AppLeftoversBanner({ leftovers, onQueueCleanup }: AppLeftoversBannerProps) {
  return (
    <Collapse open={leftovers.length > 0}>
      {leftovers.length > 0 && (
        <AlertBanner tone="warning">
          <div className="font-semibold text-fg-primary mb-1">
            Restos detectados tras desinstalar (nivel Revisar)
          </div>
          {leftovers.map((lv) => (
            <div
              key={lv.path}
              className="flex items-center justify-between gap-2 text-[12px] flex-wrap mt-1"
            >
              <span className="mono truncate">
                {lv.appName}: {lv.path} ({formatBytes(lv.sizeBytes)})
              </span>
              {onQueueCleanup && (
                <Button size="sm" variant="warning" onClick={() => onQueueCleanup(lv.path)}>
                  Revisar en Dry-run
                </Button>
              )}
            </div>
          ))}
        </AlertBanner>
      )}
    </Collapse>
  );
}
