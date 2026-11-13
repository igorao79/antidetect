"use client";

import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { LoadingButton } from "@/components/loading-button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { RippleButton } from "@/components/ui/ripple";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { translateBackendError } from "@/lib/backend-errors";
import { showErrorToast, showSuccessToast } from "@/lib/toast-utils";
import type { StoredProxy } from "@/types";

interface ProxyReplaceDialogProps {
  /** The proxy whose profiles move; the dialog is open while it is set. */
  proxy: StoredProxy | null;
  proxies: StoredProxy[];
  /** How many profiles use `proxy` now. */
  usage: number;
  onClose: () => void;
}

/** Moves every profile that uses one proxy onto another in one step. */
export function ProxyReplaceDialog({
  proxy,
  proxies,
  usage,
  onClose,
}: ProxyReplaceDialogProps) {
  const { t } = useTranslation();
  const [targetId, setTargetId] = useState("");
  const [isBusy, setIsBusy] = useState(false);
  const proxyId = proxy?.id;

  useEffect(() => {
    if (proxyId) setTargetId("");
  }, [proxyId]);

  const targets = proxies.filter((candidate) => candidate.id !== proxyId);

  const handleReplace = async () => {
    if (!proxy || !targetId) return;
    setIsBusy(true);
    try {
      const moved = await invoke<number>("replace_proxy_in_profiles", {
        fromProxyId: proxy.id,
        toProxyId: targetId,
      });
      const target = proxies.find((candidate) => candidate.id === targetId);
      showSuccessToast(
        t("proxies.replace.done", { count: moved, name: target?.name ?? "" }),
      );
      onClose();
    } catch (error: unknown) {
      showErrorToast(translateBackendError(t, error));
    } finally {
      setIsBusy(false);
    }
  };

  return (
    <Dialog
      open={proxy !== null}
      onOpenChange={(open) => {
        if (!open && !isBusy) onClose();
      }}
    >
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t("proxies.replace.title")}</DialogTitle>
          <DialogDescription>
            {t("proxies.replace.description", { name: proxy?.name ?? "" })}
          </DialogDescription>
        </DialogHeader>
        <div className="grid gap-2">
          <Label htmlFor="proxy-replace-target">
            {t("proxies.replace.target")}
          </Label>
          <Select
            value={targetId}
            onValueChange={setTargetId}
            disabled={isBusy || targets.length === 0}
          >
            <SelectTrigger id="proxy-replace-target">
              <SelectValue
                placeholder={t("proxies.replace.targetPlaceholder")}
              />
            </SelectTrigger>
            <SelectContent>
              {targets.map((candidate) => (
                <SelectItem key={candidate.id} value={candidate.id}>
                  {candidate.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <p className="text-xs text-muted-foreground">
            {usage > 0
              ? t("proxies.replace.usedBy", { count: usage })
              : t("proxies.replace.noneUsing")}
          </p>
        </div>
        <DialogFooter>
          <RippleButton variant="outline" onClick={onClose} disabled={isBusy}>
            {t("common.buttons.cancel")}
          </RippleButton>
          <LoadingButton
            isLoading={isBusy}
            disabled={!targetId || usage === 0}
            onClick={() => void handleReplace()}
          >
            {t("proxies.replace.confirm")}
          </LoadingButton>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
