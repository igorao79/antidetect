"use client";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { translateBackendError } from "@/lib/backend-errors";
import { showErrorToast, showSuccessToast } from "@/lib/toast-utils";
import type { BrowserProfile } from "@/types";

/** A quick profile whose browser has stopped and awaits a decision. */
export interface StoppedQuickProfile {
  id: string;
  name: string;
}

type Launch = (profile: BrowserProfile) => Promise<{ status: string }>;

/**
 * Quick profiles: a temporary Camoufox profile made and launched at once.
 * When its browser stops the backend keeps it and announces it here, and the
 * prompt asks whether to keep it as a regular profile or discard it.
 */
export function useQuickProfile(launch: Launch) {
  const { t } = useTranslation();
  const [starting, setStarting] = useState(false);
  const [stopped, setStopped] = useState<StoppedQuickProfile[]>([]);

  useEffect(() => {
    const pending = listen<StoppedQuickProfile>(
      "quick-profile-stopped",
      (event) => {
        setStopped((current) =>
          current.some((item) => item.id === event.payload.id)
            ? current
            : [...current, event.payload],
        );
      },
    );
    return () => {
      void pending.then((unlisten) => unlisten());
    };
  }, []);

  const discard = useCallback(
    async (id: string) => {
      try {
        await invoke("discard_quick_profile", { profileId: id });
      } catch (error) {
        showErrorToast(translateBackendError(t, error));
      } finally {
        setStopped((current) => current.filter((item) => item.id !== id));
      }
    },
    [t],
  );

  const keep = useCallback(
    async (id: string, name: string) => {
      try {
        const kept = await invoke<BrowserProfile>("keep_quick_profile", {
          profileId: id,
          name,
        });
        setStopped((current) => current.filter((item) => item.id !== id));
        showSuccessToast(t("quickProfile.kept", { name: kept.name }));
      } catch (error) {
        // The prompt stays, so a taken name can be changed.
        showErrorToast(translateBackendError(t, error));
      }
    },
    [t],
  );

  const start = useCallback(async () => {
    if (starting) return;
    setStarting(true);
    try {
      let created: BrowserProfile;
      try {
        created = await invoke<BrowserProfile>("create_quick_profile");
      } catch (error) {
        showErrorToast(translateBackendError(t, error));
        return;
      }
      // A failed launch has already told the person why; what is left is a
      // profile no browser will ever stop, so it goes.
      const launched = await launch(created).then(
        (result) => result.status === "launched",
        () => false,
      );
      if (!launched) {
        await invoke("discard_quick_profile", { profileId: created.id }).catch(
          () => undefined,
        );
      }
    } finally {
      setStarting(false);
    }
  }, [launch, starting, t]);

  return { start, starting, stopped, keep, discard };
}
