import { useState } from "react";
import {
  type CleanupMode,
  type CleanupPlan,
  type CleanupResult,
  type CleanupTargetInput,
  api,
} from "../lib/ipc";

export function useGlobalCleanupModal(onAfterExecute: () => void, showToast: (m: string) => void) {
  const [pendingTargets, setPendingTargets] = useState<CleanupTargetInput[]>([]);
  const [activePlan, setActivePlan] = useState<CleanupPlan | null>(null);
  const [executingCleanup, setExecutingCleanup] = useState(false);
  const [cleanupResult, setCleanupResult] = useState<CleanupResult | null>(null);

  const openCleanupPreview = (targets: CleanupTargetInput[], mode: CleanupMode) => {
    setPendingTargets(targets);
    setCleanupResult(null);
    api
      .previewCleanup(targets, mode)
      .then(setActivePlan)
      .catch((err) => showToast(String(err)));
  };

  const handleChangePlanMode = (mode: CleanupMode) => {
    api
      .previewCleanup(pendingTargets, mode)
      .then(setActivePlan)
      .catch((err) => showToast(String(err)));
  };

  const handleExecutePlan = (confirmedPermanent: boolean) => {
    if (!activePlan) return;
    setExecutingCleanup(true);
    api
      .executeCleanup(activePlan.planId, confirmedPermanent)
      .then((res) => {
        setCleanupResult(res);
        onAfterExecute();
      })
      .catch((err) => showToast(String(err)))
      .finally(() => setExecutingCleanup(false));
  };

  const closeModal = () => {
    setActivePlan(null);
    setCleanupResult(null);
  };

  return {
    activePlan,
    executingCleanup,
    cleanupResult,
    openCleanupPreview,
    handleChangePlanMode,
    handleExecutePlan,
    closeModal,
  };
}
