"use client";

import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { useEffect, useId, useState } from "react";
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
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { RippleButton } from "@/components/ui/ripple";
import { translateBackendError } from "@/lib/backend-errors";
import {
  PROFILE_CSV_COLUMNS,
  type ProfileCsvColumn,
  type ProfilesCsvLookups,
  profilesToCsv,
} from "@/lib/profiles-csv";
import { showErrorToast, showSuccessToast } from "@/lib/toast-utils";
import type {
  BrowserProfile,
  ProfileGroup,
  ProfileStatus,
  StoredProxy,
  VpnConfig,
} from "@/types";

type ExportScope = "selected" | "shown";

interface ProfilesExportDialogProps {
  isOpen: boolean;
  onClose: () => void;
  selectedProfiles: BrowserProfile[];
  /** The profiles the table shows now, with its search and filters applied. */
  shownProfiles: BrowserProfile[];
}

async function loadLookups(): Promise<ProfilesCsvLookups> {
  const [groups, proxies, vpns, statuses] = await Promise.all([
    invoke<ProfileGroup[]>("get_profile_groups"),
    invoke<StoredProxy[]>("get_stored_proxies"),
    invoke<VpnConfig[]>("list_vpn_configs"),
    invoke<ProfileStatus[]>("list_profile_statuses"),
  ]);
  return { groups, proxies, vpns, statuses };
}

/**
 * Writes the profiles table to a CSV file: the selected profiles or every
 * profile the table shows, with the columns a spreadsheet user looks for.
 */
export function ProfilesExportDialog({
  isOpen,
  onClose,
  selectedProfiles,
  shownProfiles,
}: ProfilesExportDialogProps) {
  const { t } = useTranslation();
  const fieldId = useId();
  const hasSelection = selectedProfiles.length > 0;
  const [scope, setScope] = useState<ExportScope>("shown");
  const [isExporting, setIsExporting] = useState(false);

  useEffect(() => {
    if (isOpen) setScope(hasSelection ? "selected" : "shown");
  }, [isOpen, hasSelection]);

  const profiles = scope === "selected" ? selectedProfiles : shownProfiles;

  const handleExport = async () => {
    setIsExporting(true);
    try {
      const path = await save({
        defaultPath: `profiles-${new Date().toISOString().slice(0, 10)}.csv`,
        filters: [{ name: "CSV", extensions: ["csv"] }],
      });
      if (!path) return;
      const headers = Object.fromEntries(
        PROFILE_CSV_COLUMNS.map((column) => [
          column,
          t(`profilesExport.columns.${column}`),
        ]),
      ) as Record<ProfileCsvColumn, string>;
      const content = profilesToCsv(profiles, await loadLookups(), headers);
      await invoke("write_profiles_csv", { path, content });
      showSuccessToast(t("profilesExport.saved", { count: profiles.length }));
      onClose();
    } catch (error) {
      showErrorToast(t("profilesExport.failed"), {
        description: translateBackendError(t, error),
      });
    } finally {
      setIsExporting(false);
    }
  };

  const options: { value: ExportScope; label: string; count: number }[] = [
    {
      value: "selected",
      label: t("profilesExport.scopeSelected", {
        count: selectedProfiles.length,
      }),
      count: selectedProfiles.length,
    },
    {
      value: "shown",
      label: t("profilesExport.scopeShown", { count: shownProfiles.length }),
      count: shownProfiles.length,
    },
  ];

  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t("profilesExport.title")}</DialogTitle>
          <DialogDescription>
            {t("profilesExport.description")}
          </DialogDescription>
        </DialogHeader>
        <RadioGroup
          value={scope}
          disabled={isExporting}
          onValueChange={(value) => {
            setScope(value as ExportScope);
          }}
        >
          {options.map((option) => (
            <label
              key={option.value}
              htmlFor={`${fieldId}-${option.value}`}
              className="flex cursor-pointer items-center gap-2 text-sm"
            >
              <RadioGroupItem
                id={`${fieldId}-${option.value}`}
                value={option.value}
                disabled={option.count === 0}
              />
              {option.label}
            </label>
          ))}
        </RadioGroup>
        <p className="text-xs text-muted-foreground">
          {t("profilesExport.columnsHint")}
        </p>
        <DialogFooter>
          <RippleButton variant="outline" onClick={onClose}>
            {t("common.buttons.cancel")}
          </RippleButton>
          <LoadingButton
            isLoading={isExporting}
            disabled={profiles.length === 0}
            onClick={() => void handleExport()}
          >
            {t("common.buttons.export")}
          </LoadingButton>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
