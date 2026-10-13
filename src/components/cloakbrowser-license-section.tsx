"use client";

import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { LoadingButton } from "@/components/loading-button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { translateBackendError } from "@/lib/backend-errors";
import { showErrorToast, showSuccessToast } from "@/lib/toast-utils";
import { RippleButton } from "./ui/ripple";

interface LicenseStatus {
  source: "environment" | "file" | null;
  hint: string | null;
  valid: boolean | null;
  plan: string | null;
  expires: string | null;
}

interface CloakBrowserLicenseSectionProps {
  isOpen: boolean;
  hidden: boolean;
}

/** Settings section for the CloakBrowser Pro licence key (fork). */
export function CloakBrowserLicenseSection({
  isOpen,
  hidden,
}: CloakBrowserLicenseSectionProps) {
  const { t, i18n } = useTranslation();
  const fieldId = useId();
  const [status, setStatus] = useState<LicenseStatus | null>(null);
  const [keyText, setKeyText] = useState("");
  const [isEditing, setIsEditing] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [isRemoving, setIsRemoving] = useState(false);

  useEffect(() => {
    if (!isOpen) return;
    setKeyText("");
    setIsEditing(false);
    void invoke<LicenseStatus>("get_cloakbrowser_license", { check: true })
      .then(setStatus)
      .catch((err: unknown) => {
        console.error("Failed to read the CloakBrowser licence:", err);
      });
  }, [isOpen]);

  const handleSave = useCallback(async () => {
    setIsSaving(true);
    try {
      const saved = await invoke<LicenseStatus>(
        "set_cloakbrowser_license_key",
        { key: keyText },
      );
      setStatus(saved);
      setKeyText("");
      setIsEditing(false);
      showSuccessToast(t("settings.cloakbrowserLicense.saved"));
      void invoke("ensure_active_browsers_downloaded").catch((err: unknown) => {
        console.error("Failed to start the CloakBrowser Pro download:", err);
      });
    } catch (err) {
      showErrorToast(translateBackendError(t, err));
    } finally {
      setIsSaving(false);
    }
  }, [keyText, t]);

  const handleRemove = useCallback(async () => {
    setIsRemoving(true);
    try {
      setStatus(await invoke<LicenseStatus>("remove_cloakbrowser_license_key"));
      showSuccessToast(t("settings.cloakbrowserLicense.removed"));
    } catch (err) {
      showErrorToast(translateBackendError(t, err));
    } finally {
      setIsRemoving(false);
    }
  }, [t]);

  const hasKey = status?.hint != null;
  const fromEnvironment = status?.source === "environment";
  const showField = !hasKey || isEditing;
  const expires = status?.expires
    ? new Date(status.expires).toLocaleDateString(i18n.language, {
        dateStyle: "medium",
      })
    : null;

  return (
    <div
      data-settings-section="cloakbrowserLicense"
      tabIndex={-1}
      hidden={hidden}
      className="scroll-mt-2 space-y-4 focus:outline-none"
    >
      <Label className="text-base font-medium">
        {t("settings.cloakbrowserLicense.title")}
      </Label>
      <p className="text-xs text-muted-foreground">
        {t("settings.cloakbrowserLicense.description")}
      </p>

      {hasKey && (
        <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border p-3">
          <div className="min-w-0">
            <div className="text-sm font-medium tabular-nums">
              {t("settings.cloakbrowserLicense.endsWith", {
                hint: status.hint,
              })}
            </div>
            <div className="text-xs text-muted-foreground">
              {fromEnvironment
                ? t("settings.cloakbrowserLicense.fromEnvironment")
                : t("settings.cloakbrowserLicense.removeNote")}
            </div>
          </div>
          <div className="flex items-center gap-x-2">
            {status.valid === true && (
              <Badge variant="soft" className="bg-success/10 text-success-text">
                {expires
                  ? t("settings.cloakbrowserLicense.activeUntil", {
                      date: expires,
                    })
                  : t("common.status.active")}
              </Badge>
            )}
            {status.valid === false && (
              <Badge
                variant="soft"
                className="bg-destructive/10 text-destructive-text"
              >
                {t("settings.cloakbrowserLicense.refused")}
              </Badge>
            )}
            {status.valid === null && (
              <Badge variant="soft">
                {t("settings.cloakbrowserLicense.notChecked")}
              </Badge>
            )}
            {!fromEnvironment && !isEditing && (
              <>
                <RippleButton
                  size="sm"
                  variant="outline"
                  onClick={() => {
                    setIsEditing(true);
                  }}
                >
                  {t("settings.cloakbrowserLicense.replace")}
                </RippleButton>
                <LoadingButton
                  size="sm"
                  variant="outline"
                  isLoading={isRemoving}
                  onClick={() => void handleRemove()}
                >
                  {t("common.buttons.delete")}
                </LoadingButton>
              </>
            )}
          </div>
        </div>
      )}

      {showField && !fromEnvironment && (
        <div className="grid gap-2">
          <Label htmlFor={`${fieldId}-key`}>
            {t("settings.cloakbrowserLicense.keyLabel")}
          </Label>
          <div className="flex items-center gap-2">
            <Input
              id={`${fieldId}-key`}
              type="password"
              value={keyText}
              onChange={(e) => {
                setKeyText(e.target.value);
              }}
              placeholder={t("settings.cloakbrowserLicense.placeholder")}
              autoComplete="off"
              autoCapitalize="none"
              spellCheck={false}
              className="font-mono"
            />
            <LoadingButton
              isLoading={isSaving}
              disabled={keyText.trim().length === 0}
              onClick={() => void handleSave()}
            >
              {t("common.buttons.save")}
            </LoadingButton>
            {isEditing && (
              <RippleButton
                variant="outline"
                onClick={() => {
                  setIsEditing(false);
                  setKeyText("");
                }}
              >
                {t("common.buttons.cancel")}
              </RippleButton>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
