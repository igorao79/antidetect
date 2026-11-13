"use client";

import { useCallback, useSyncExternalStore } from "react";
import { allPinned, pinnedProfilesStore } from "@/lib/pinned-profiles";

const NONE: ReadonlySet<string> = new Set();

export function usePinnedProfiles() {
  const pinned = useSyncExternalStore(
    pinnedProfilesStore.subscribe,
    pinnedProfilesStore.getSnapshot,
    () => NONE,
  );
  const setPinned = useCallback((ids: readonly string[], value: boolean) => {
    pinnedProfilesStore.set(ids, value);
  }, []);
  const toggle = useCallback((id: string) => {
    pinnedProfilesStore.set([id], !pinnedProfilesStore.getSnapshot().has(id));
  }, []);
  /** Pins the ids, or unpins them when they all are pinned already. */
  const toggleMany = useCallback((ids: readonly string[]) => {
    const everyPinned = allPinned(pinnedProfilesStore.getSnapshot(), ids);
    pinnedProfilesStore.set(ids, !everyPinned);
  }, []);
  return { pinned, setPinned, toggle, toggleMany };
}
