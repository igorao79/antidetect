"use client";

import { invoke } from "@tauri-apps/api/core";
import { motion } from "motion/react";
import * as React from "react";
import { useTranslation } from "react-i18next";
import { LuLock } from "react-icons/lu";
import { DeleteConfirmationDialog } from "@/components/delete-confirmation-dialog";
import { Button } from "@/components/ui/button";
import { useMotionAllowed } from "@/hooks/use-motion-allowed";
import { useTeamLocks } from "@/hooks/use-team-locks";
import { useWorkspace } from "@/hooks/use-workspace";
import { translateBackendError } from "@/lib/backend-errors";
import { DONUT_CLOUD } from "@/lib/cloud-only-pages";
import { MOTION_SPRING } from "@/lib/motion";
import { showErrorToast, showSuccessToast } from "@/lib/toast-utils";
import { type BrowserProfile, isSyncEnabled } from "@/types";

/**
 * A shared profile that another session of an Owl server has open, with the
 * way out for a session that went away: the server's owner and the member
 * holding the lock may release it after a warning about unsynced changes.
 */
export function LockHeldBanner({
  profile,
  runningHere,
}: {
  profile: BrowserProfile;
  runningHere: boolean;
}) {
  const { t } = useTranslation();
  const motionAllowed = useMotionAllowed();
  const { workspace } = useWorkspace();
  const { getLockInfo, refetchLocks } = useTeamLocks(workspace?.member.id);
  const [confirming, setConfirming] = React.useState(false);
  const [busy, setBusy] = React.useState(false);

  const lock = getLockInfo(profile.id);
  if (
    DONUT_CLOUD ||
    !workspace ||
    !lock ||
    runningHere ||
    !isSyncEnabled(profile)
  ) {
    return null;
  }

  const mine = lock.lockedBy === workspace.member.id;
  const mayRelease = workspace.member.role === "owner" || mine;

  const release = async () => {
    setBusy(true);
    try {
      await invoke("release_team_lock", { profileId: profile.id });
      showSuccessToast(
        t("profileInfo.lockHeld.released", { profile: profile.name }),
      );
      setConfirming(false);
    } catch (error) {
      showErrorToast(translateBackendError(t, error));
    } finally {
      setBusy(false);
      void refetchLocks();
    }
  };

  return (
    <>
      <motion.div
        role="status"
        data-slot="profile-info-lock-held-banner"
        initial={motionAllowed ? { y: -4 } : false}
        animate={{ y: 0 }}
        transition={MOTION_SPRING.snappy}
        className="mx-3 mb-1 flex shrink-0 items-center gap-2 rounded-lg bg-warning/10 px-3 py-1.5 text-xs"
      >
        <LuLock aria-hidden="true" className="size-3.5 shrink-0 text-warning" />
        <span className="min-w-0 flex-1">
          {mine
            ? t("profileInfo.lockHeld.byYou")
            : t("profileInfo.lockHeld.byMember", { name: lock.lockedByEmail })}
        </span>
        {mayRelease && (
          <Button
            variant="soft"
            size="sm"
            data-slot="profile-info-lock-release"
            className="h-6 px-2 text-xs"
            onClick={() => setConfirming(true)}
          >
            {t("profileInfo.lockHeld.release")}
          </Button>
        )}
      </motion.div>
      <DeleteConfirmationDialog
        isOpen={confirming}
        onClose={() => setConfirming(false)}
        onConfirm={release}
        title={t("profileInfo.lockHeld.confirmTitle")}
        description={t("profileInfo.lockHeld.confirmBody", {
          profile: profile.name,
          name: lock.lockedByEmail,
        })}
        confirmButtonText={t("profileInfo.lockHeld.release")}
        isLoading={busy}
      />
    </>
  );
}
