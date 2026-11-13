"use client";

import { invoke } from "@tauri-apps/api/core";
import * as React from "react";
import { useTranslation } from "react-i18next";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { translateBackendError } from "@/lib/backend-errors";
import { showErrorToast, showSuccessToast } from "@/lib/toast-utils";
import type { BrowserProfile } from "@/types";
import { type CountHintLine, CountStepper } from "./create-profile-dialog";
import { LoadingButton } from "./loading-button";
import { RippleButton } from "./ui/ripple";

/** The most copies one clone makes; the backend refuses more. */
const MAX_COPIES = 50;

interface CloneProfileDialogProps {
  isOpen: boolean;
  onClose: () => void;
  profile: BrowserProfile | null;
  onCloneComplete?: () => void;
}

export function CloneProfileDialog({
  isOpen,
  onClose,
  profile,
  onCloneComplete,
}: CloneProfileDialogProps) {
  const { t } = useTranslation();
  const [name, setName] = React.useState("");
  const [count, setCount] = React.useState(1);
  const [isLoading, setIsLoading] = React.useState(false);
  const inputRef = React.useRef<HTMLInputElement>(null);

  React.useEffect(() => {
    if (!(isOpen && profile)) {
      setIsLoading(false);
      return;
    }
    setName(profile.name);
    setCount(1);
    const handle = window.setTimeout(() => {
      inputRef.current?.focus();
      inputRef.current?.select();
    }, 0);
    return () => {
      window.clearTimeout(handle);
    };
  }, [isOpen, profile]);

  if (!profile) return null;

  const handleClone = async () => {
    if (!name.trim() || isLoading) return;
    setIsLoading(true);
    try {
      const copies = await invoke<BrowserProfile[]>("clone_profile_copies", {
        profileId: profile.id,
        name: name.trim(),
        count,
      });
      showSuccessToast(
        copies.length === 1
          ? t("profiles.duplicated", { name: copies[0].name })
          : t("profileInfo.clone.madeMany", { count: copies.length }),
      );
      onClose();
      onCloneComplete?.();
    } catch (err: unknown) {
      const errorMessage = translateBackendError(t, err);
      showErrorToast(t("errors.cloneProfileFailed", { error: errorMessage }));
    } finally {
      setIsLoading(false);
    }
  };

  const base = name.trim() || profile.name;
  const countHint: CountHintLine[] = [];
  if (count > 1) {
    countHint.push({
      text: t("profileInfo.clone.namesHint", {
        first: `${base} 2`,
        second: `${base} 3`,
      }),
    });
  }
  countHint.push({
    text: t("profileInfo.clone.maxHint", { max: MAX_COPIES }),
    limit: true,
    atLimitOnly: true,
  });

  return (
    <Dialog
      open={isOpen}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t("profileInfo.clone.title")}</DialogTitle>
          <DialogDescription>
            {t("profileInfo.clone.description")}
          </DialogDescription>
        </DialogHeader>
        <Input
          ref={inputRef}
          value={name}
          onChange={(e) => {
            setName(e.target.value);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") void handleClone();
          }}
          placeholder={t("profileInfo.clone.namePlaceholder")}
          disabled={isLoading}
        />
        <p className="text-xs text-muted-foreground">
          {t("profileInfo.clone.copiedNote")}
        </p>
        <DialogFooter>
          <RippleButton
            variant="outline"
            onClick={onClose}
            disabled={isLoading}
          >
            {t("common.buttons.cancel")}
          </RippleButton>
          <CountStepper
            value={count}
            max={MAX_COPIES}
            onChange={setCount}
            disabled={isLoading}
            hint={countHint}
            onEnter={() => void handleClone()}
          />
          <LoadingButton
            onClick={() => void handleClone()}
            isLoading={isLoading}
            disabled={!name.trim()}
          >
            {count > 1
              ? t("profileInfo.clone.buttonMany", { count })
              : t("profileInfo.clone.button")}
          </LoadingButton>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
