"use client";

import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { LoadingButton } from "@/components/loading-button";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { WorkspaceInfo } from "@/hooks/use-workspace";
import { translateBackendError } from "@/lib/backend-errors";
import { showErrorToast, showSuccessToast } from "@/lib/toast-utils";
import { type BrowserProfile, isSyncEnabled } from "@/types";

interface WorkspaceShareDialogProps {
  profile: BrowserProfile | null;
  workspace: WorkspaceInfo;
  onClose: () => void;
}

/** Share a profile on the joined Owl server, or stop sharing it. */
export function WorkspaceShareDialog({
  profile,
  workspace,
  onClose,
}: WorkspaceShareDialogProps) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  if (!profile) return null;

  const shared = isSyncEnabled(profile);
  const sharedBy = profile.shared_by ?? null;
  const mayUnshare =
    workspace.member.role === "owner" ||
    !sharedBy ||
    sharedBy.id === workspace.member.id;

  const apply = async () => {
    setBusy(true);
    try {
      await invoke("set_profile_sync_mode", {
        profileId: profile.id,
        syncMode: shared ? "Disabled" : "Regular",
      });
      showSuccessToast(
        t(shared ? "workspace.share.unshared" : "workspace.share.shared", {
          profile: profile.name,
        }),
      );
      onClose();
    } catch (error) {
      showErrorToast(translateBackendError(t, error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>
            {t(
              shared
                ? "workspace.share.titleUnshare"
                : "workspace.share.titleShare",
            )}
          </DialogTitle>
          <DialogDescription>
            {shared && !mayUnshare && sharedBy
              ? t("workspace.share.notYours", { name: sharedBy.name })
              : t(
                  shared
                    ? "workspace.share.unshareBody"
                    : "workspace.share.shareBody",
                  { profile: profile.name, server: workspace.name },
                )}
          </DialogDescription>
        </DialogHeader>
        {shared && sharedBy && (
          <p className="text-xs text-muted-foreground">
            {t("workspace.share.sharedBy", { name: sharedBy.name })}
          </p>
        )}
        <DialogFooter>
          <Button variant="soft" onClick={onClose}>
            {t("common.buttons.cancel")}
          </Button>
          {(!shared || mayUnshare) && (
            <LoadingButton
              isLoading={busy}
              variant={shared ? "destructive" : "default"}
              onClick={() => void apply()}
            >
              {t(shared ? "workspace.share.unshare" : "workspace.share.share")}
            </LoadingButton>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
