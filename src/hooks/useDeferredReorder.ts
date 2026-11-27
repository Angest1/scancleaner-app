import { useCallback, useEffect, useRef, useState } from "react";

/**
 * Prevents a list from reordering underneath the user's cursor while interacting.
 * Live updates to item values (e.g. sizes) still reflect cleanly in-place, while order changes
 * are deferred until the pointer leaves the list or after `idleMs` (default 1000ms) of pointer inactivity.
 */
export function useDeferredReorder<T>(
  items: T[],
  getKey: (item: T) => string | number,
  idleMs = 1000,
) {
  const [interacting, setInteracting] = useState(false);
  const [displayItems, setDisplayItems] = useState<T[]>(items);
  const latestItemsRef = useRef<T[]>(items);
  const idleTimerRef = useRef<number | null>(null);

  latestItemsRef.current = items;

  const clearIdleTimer = useCallback(() => {
    if (idleTimerRef.current !== null) {
      window.clearTimeout(idleTimerRef.current);
      idleTimerRef.current = null;
    }
  }, []);

  useEffect(() => {
    if (!interacting) {
      setDisplayItems(items);
      return;
    }

    // While interacting: preserve current visual order for existing keys and update their payloads in-place
    setDisplayItems((prev) => {
      const byKey = new Map<string | number, T>();
      for (const item of items) {
        byKey.set(getKey(item), item);
      }
      const next: T[] = [];
      const seen = new Set<string | number>();
      for (const oldItem of prev) {
        const k = getKey(oldItem);
        const updated = byKey.get(k);
        if (updated !== undefined) {
          next.push(updated);
          seen.add(k);
        }
      }
      // Append newly arrived items at the end so visible rows under the cursor don't jump
      for (const item of items) {
        const k = getKey(item);
        if (!seen.has(k)) {
          next.push(item);
        }
      }
      return next;
    });
  }, [items, interacting, getKey]);

  const scheduleIdleFlush = useCallback(() => {
    clearIdleTimer();
    idleTimerRef.current = window.setTimeout(() => {
      setInteracting(false);
      setDisplayItems(latestItemsRef.current);
    }, idleMs);
  }, [clearIdleTimer, idleMs]);

  const onPointerEnter = useCallback(() => {
    setInteracting(true);
    scheduleIdleFlush();
  }, [scheduleIdleFlush]);

  const onPointerMove = useCallback(() => {
    setInteracting(true);
    scheduleIdleFlush();
  }, [scheduleIdleFlush]);

  const onPointerLeave = useCallback(() => {
    clearIdleTimer();
    setInteracting(false);
    setDisplayItems(latestItemsRef.current);
  }, [clearIdleTimer]);

  useEffect(() => clearIdleTimer, [clearIdleTimer]);

  return {
    items: displayItems,
    isOrderDeferred: interacting,
    containerProps: {
      onPointerEnter,
      onPointerMove,
      onPointerLeave,
    },
  };
}
