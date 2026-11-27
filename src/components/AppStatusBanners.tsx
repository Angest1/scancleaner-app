import { Trash2 } from "lucide-react";
import { type DeletionJobStatus, api } from "../lib/ipc";
import type { NavPage } from "./AppHeader";
import { Button } from "./ui/Button";
import { Collapse } from "./ui/Collapse";
import { AlertBanner } from "./ui/Feedback";

interface AppStatusBannersProps {
  showPartial: boolean;
  partialMessage?: string;
  activeBgJobs: DeletionJobStatus[];
  onGoSettings: (page: NavPage) => void;
  onDismissPartial: () => void;
}

export function AppStatusBanners({
  showPartial,
  partialMessage,
  activeBgJobs,
  onGoSettings,
  onDismissPartial,
}: AppStatusBannersProps) {
  return (
    <>
      <Collapse open={showPartial && Boolean(partialMessage)}>
        {partialMessage && (
          <AlertBanner
            tone="warning"
            action={
              <div className="flex items-center gap-1.5">
                <Button size="sm" onClick={() => onGoSettings("settings")}>
                  Configurar servicio MFT
                </Button>
                <Button size="sm" variant="ghost" onClick={onDismissPartial}>
                  Ocultar
                </Button>
              </div>
            }
          >
            {partialMessage}
          </AlertBanner>
        )}
      </Collapse>

      <Collapse open={activeBgJobs.length > 0}>
        <div className="flex flex-col gap-2">
          {activeBgJobs.map((job) => (
            <AlertBanner
              key={job.jobId}
              tone="info"
              action={
                <Button
                  size="sm"
                  variant="danger"
                  onClick={() => api.cancelDeletionJob(job.jobId).catch(() => {})}
                >
                  Cancelar
                </Button>
              }
            >
              <span className="inline-flex items-center gap-1.5">
                <Trash2 size={13} aria-hidden="true" />
                Vaciando en segundo plano ({job.completedItems}/{job.totalItems} lotes)…
              </span>
            </AlertBanner>
          ))}
        </div>
      </Collapse>
    </>
  );
}
