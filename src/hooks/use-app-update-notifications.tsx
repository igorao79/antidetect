"use client";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { AppUpdateToast } from "@/components/app-update-toast";
import { translateBackendError } from "@/lib/backend-errors";
import { showErrorToast, showSuccessToast, showToast } from "@/lib/toast-utils";
import { playUiSound } from "@/lib/ui-sounds";
import type { AppUpdateInfo, AppUpdateProgress } from "@/types";

export function useAppUpdateNotifications() {
  const { t } = useTranslation();
  const [updateInfo, setUpdateInfo] = useState<AppUpdateInfo | null>(null);
  const [isUpdating, setIsUpdating] = useState(false);
  const [updateProgress, setUpdateProgress] =
    useState<AppUpdateProgress | null>(null);
  const [updateReady, setUpdateReady] = useState(false);
  const [isClient, setIsClient] = useState(false);
  const [dismissedVersion, setDismissedVersion] = useState<string | null>(null);
  const autoDownloadedVersion = useRef<string | null>(null);

  // Ensure we're on the client side to prevent hydration mismatches
  useEffect(() => {
    setIsClient(true);
  }, []);

  const checkForAppUpdates = useCallback(async () => {
    if (!isClient) return;

    try {
      const update = await invoke<AppUpdateInfo | null>(
        "check_for_app_updates",
      );

      if (update) {
        setUpdateInfo(update);
      }
    } catch (error) {
      console.error("Failed to check for app updates:", error);
    }
  }, [isClient]);

  /** Checks now, even for a dismissed version. Throws when the check fails. */
  const checkForAppUpdatesManual = useCallback(async () => {
    const update = await invoke<AppUpdateInfo | null>(
      "check_for_app_updates_manual",
    );
    autoDownloadedVersion.current = null;
    setDismissedVersion(null);
    setUpdateInfo(update);
    return update;
  }, []);

  /** A check asked for outside About, which says how it went in a toast. */
  const checkForAppUpdatesNow = useCallback(async () => {
    const id = "app-update-check";
    showToast({
      id,
      type: "loading",
      title: t("about.update.checking"),
      duration: Number.POSITIVE_INFINITY,
    });
    try {
      const found = await checkForAppUpdatesManual();
      if (!found) {
        showSuccessToast(t("about.update.current"), { id });
      } else if (found.auto_install) {
        showToast({
          id,
          type: "info",
          title: t("about.update.available", { version: found.new_version }),
        });
      } else {
        toast.dismiss(id);
      }
    } catch (error) {
      console.error("Failed to check for app updates:", error);
      showErrorToast(t("about.update.failed"), { id });
    }
  }, [t, checkForAppUpdatesManual]);

  const handleAppUpdate = useCallback(
    async (appUpdateInfo: AppUpdateInfo) => {
      try {
        setIsUpdating(true);
        setUpdateProgress({
          stage: "downloading",
          percentage: 0,
          speed: undefined,
          eta: undefined,
          message: t("appUpdate.toast.startingUpdate"),
        });

        await invoke("download_and_prepare_app_update", {
          updateInfo: appUpdateInfo,
        });
      } catch (error) {
        console.error("Failed to update app:", error);
        showToast({
          type: "error",
          title: t("appUpdate.toast.updateFailed"),
          description: translateBackendError(t, error),
          duration: 6000,
        });
        setIsUpdating(false);
        setUpdateProgress(null);
        // Deliberately NOT resetting autoDownloadedVersion here: the
        // auto-download effect re-runs as soon as isUpdating flips back to
        // false, so clearing the marker now would retry in a tight loop.
        // Retries are re-armed when the next backend check delivers a fresh
        // update event instead.
      }
    },
    [t],
  );

  const handleRestart = useCallback(async () => {
    try {
      await invoke("restart_application");
    } catch (error) {
      console.error("Failed to restart app:", error);
      showToast({
        type: "error",
        title: t("appUpdate.toast.restartFailed"),
        description: translateBackendError(t, error),
        duration: 6000,
      });
    }
  }, [t]);

  const dismissAppUpdate = useCallback(() => {
    if (!isClient) return;

    // Only the toast goes: the update stays known, for the rail's button.
    if (updateInfo) {
      setDismissedVersion(updateInfo.new_version);
    }

    toast.dismiss("app-update");
  }, [isClient, updateInfo]);

  // Listen for app update events
  useEffect(() => {
    if (!isClient) return;

    const unlistenUpdate = listen<AppUpdateInfo>(
      "app-update-available",
      (event) => {
        console.log("App update available:", event.payload);
        // A fresh backend check re-arms auto-download, so a version whose
        // earlier attempt failed (e.g. a transient checksum-fetch error) is
        // retried once per periodic check instead of staying blocked until
        // restart. The effect's updateReady guard keeps an already-prepared
        // update from being downloaded again.
        autoDownloadedVersion.current = null;
        setUpdateInfo(event.payload);
      },
    );

    const unlistenProgress = listen<AppUpdateProgress>(
      "app-update-progress",
      (event) => {
        setUpdateProgress(event.payload);
      },
    );

    const unlistenReady = listen<string>("app-update-ready", (event) => {
      console.log("App update ready:", event.payload);
      playUiSound("attention");
      setUpdateReady(true);
      setIsUpdating(false);
      setUpdateProgress(null);
    });

    return () => {
      void unlistenUpdate.then((unlisten) => {
        unlisten();
      });
      void unlistenProgress.then((unlisten) => {
        unlisten();
      });
      void unlistenReady.then((unlisten) => {
        unlisten();
      });
    };
  }, [isClient]);

  // Auto-download update in background when found
  useEffect(() => {
    if (
      !isClient ||
      !updateInfo?.auto_install ||
      isUpdating ||
      updateReady ||
      autoDownloadedVersion.current === updateInfo.new_version
    )
      return;

    autoDownloadedVersion.current = updateInfo.new_version;
    console.log("Auto-downloading app update:", updateInfo.new_version);
    void handleAppUpdate(updateInfo);
  }, [isClient, updateInfo, isUpdating, updateReady, handleAppUpdate]);

  // A toast when the update is ready, or when it waits for the person: a
  // manual update, or automatic updates turned off.
  useEffect(() => {
    if (!isClient || !updateInfo) return;
    if (!updateReady && updateInfo.auto_install) return;
    if (updateInfo.new_version === dismissedVersion) return;

    toast.custom(
      () => (
        <AppUpdateToast
          updateInfo={updateInfo}
          onInstall={() => void handleAppUpdate(updateInfo)}
          onRestart={handleRestart}
          onDismiss={dismissAppUpdate}
          updateReady={updateReady}
          isUpdating={isUpdating}
        />
      ),
      {
        id: "app-update",
        duration: Number.POSITIVE_INFINITY,
        position: "top-left",
        style: {
          zIndex: 99999,
          pointerEvents: "auto",
          marginTop: "16px",
        },
      },
    );
  }, [
    updateInfo,
    handleAppUpdate,
    handleRestart,
    dismissAppUpdate,
    updateReady,
    isUpdating,
    isClient,
    dismissedVersion,
  ]);

  // Check for app updates on startup
  useEffect(() => {
    if (!isClient) return;

    // Check for updates immediately on startup
    void checkForAppUpdates();
  }, [isClient, checkForAppUpdates]);

  return {
    updateInfo,
    isUpdating,
    updateProgress,
    updateReady,
    checkForAppUpdates,
    checkForAppUpdatesManual,
    checkForAppUpdatesNow,
    dismissAppUpdate,
    installAppUpdate: handleAppUpdate,
    restartToUpdate: handleRestart,
  };
}

export type AppUpdateState = ReturnType<typeof useAppUpdateNotifications>;
