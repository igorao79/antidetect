"use client";

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { CloakBrowserConfigForm } from "@/components/cloakbrowser-config-form";
import { LoadingButton } from "@/components/loading-button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { SoftFields } from "@/components/ui/field-variant";
import { RippleButton } from "@/components/ui/ripple";
import { ScrollArea } from "@/components/ui/scroll-area";
import { translateBackendError } from "@/lib/backend-errors";
import { showErrorToast } from "@/lib/toast-utils";
import type { BrowserProfile, CloakBrowserConfig } from "@/types";

interface CloakBrowserConfigDialogProps {
  isOpen: boolean;
  onClose: () => void;
  profile: BrowserProfile | null;
  onSave: (
    profile: BrowserProfile,
    config: CloakBrowserConfig,
  ) => Promise<void>;
  isRunning?: boolean;
}

export function CloakBrowserConfigDialog({
  isOpen,
  onClose,
  profile,
  onSave,
  isRunning = false,
}: CloakBrowserConfigDialogProps) {
  const { t } = useTranslation();
  const [config, setConfig] = useState<CloakBrowserConfig>({});
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    if (isOpen && profile) setConfig(profile.cloakbrowser_config ?? {});
  }, [isOpen, profile]);

  if (!profile) return null;

  const handleSave = async () => {
    setIsSaving(true);
    try {
      await onSave(profile, config);
      onClose();
    } catch (error) {
      console.error("Failed to save CloakBrowser config:", error);
      showErrorToast(t("camoufoxDialog.saveFailed"), {
        description: translateBackendError(t, error),
      });
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="flex max-h-[min(85vh,52rem)] max-w-2xl flex-col">
        <DialogHeader className="shrink-0">
          <DialogTitle>
            {t(
              isRunning
                ? "camoufoxDialog.titleView"
                : "camoufoxDialog.titleConfigure",
              { name: profile.name, browser: "CloakBrowser" },
            )}
          </DialogTitle>
        </DialogHeader>

        <ScrollArea className="min-h-0 flex-1">
          <SoftFields>
            <CloakBrowserConfigForm
              className="py-2 pr-3"
              config={config}
              onConfigChange={(key, value) =>
                setConfig((previous) => ({ ...previous, [key]: value }))
              }
              readOnly={isRunning}
            />
          </SoftFields>
        </ScrollArea>

        <DialogFooter className="shrink-0 border-t pt-4">
          <RippleButton variant="outline" onClick={onClose}>
            {isRunning ? t("common.buttons.close") : t("common.buttons.cancel")}
          </RippleButton>
          {!isRunning && (
            <LoadingButton isLoading={isSaving} onClick={handleSave}>
              {t("common.buttons.save")}
            </LoadingButton>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
