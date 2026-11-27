import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { type DeletionJobStatus, type SyncStatusDto, api } from "../lib/ipc";

interface UseAppLiveEventsOptions {
  includeNetwork: boolean;
  fetchDrives: (withNet: boolean) => void;
  setSyncRevision: React.Dispatch<React.SetStateAction<number>>;
  setSyncStatus: React.Dispatch<React.SetStateAction<SyncStatusDto>>;
  setBgJobs: React.Dispatch<React.SetStateAction<DeletionJobStatus[]>>;
  showToast: (msg: string) => void;
}

export function useAppLiveEvents({
  includeNetwork,
  fetchDrives,
  setSyncRevision,
  setSyncStatus,
  setBgJobs,
  showToast,
}: UseAppLiveEventsOptions) {
  useEffect(() => {
    const unsubs: (() => void)[] = [];
    const register = async () => {
      try {
        unsubs.push(
          await listen("sync-updated", () => {
            setSyncRevision((r) => r + 1);
            fetchDrives(includeNetwork);
          }),
        );
        unsubs.push(
          await listen<SyncStatusDto>("sync-status-changed", (ev) => {
            if (ev.payload) setSyncStatus(ev.payload);
          }),
        );
        unsubs.push(
          await listen("drives-changed", () => {
            fetchDrives(includeNetwork);
            setSyncRevision((r) => r + 1);
          }),
        );
        unsubs.push(
          await listen("apps-updated", () => {
            setSyncRevision((r) => r + 1);
            showToast("Desinstalación detectada: inventario de aplicaciones actualizado.");
          }),
        );
        unsubs.push(
          await listen<DeletionJobStatus>("deletion-job-updated", (ev) => {
            if (!ev.payload) return;
            setBgJobs((prev) => {
              const filtered = prev.filter((j) => j.jobId !== ev.payload.jobId);
              if (ev.payload.phase === "completed") return filtered;
              return [...filtered, ev.payload];
            });
          }),
        );
        unsubs.push(
          await listen("window-focused", () => {
            api
              .triggerFocusSync()
              .then((res) => {
                if (res.systemFileNotice) showToast(res.systemFileNotice);
              })
              .catch(() => {});
          }),
        );
      } catch {
        // Non-Tauri browser preview
      }
    };
    void register();
    return () => {
      for (const fn of unsubs) fn();
    };
  }, [fetchDrives, includeNetwork, setBgJobs, setSyncRevision, setSyncStatus, showToast]);
}
