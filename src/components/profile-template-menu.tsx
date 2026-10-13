"use client";

import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { LuChevronDown, LuLayoutTemplate, LuTrash2 } from "react-icons/lu";
import { LoadingButton } from "@/components/loading-button";
import { Input } from "@/components/ui/input";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { translateBackendError } from "@/lib/backend-errors";
import { showErrorToast, showSuccessToast } from "@/lib/toast-utils";
import { cn } from "@/lib/utils";
import type {
  CamoufoxConfig,
  CloakBrowserConfig,
  WayfernConfig,
} from "@/types";

/**
 * What a template keeps: the create dialog's choices without the name and
 * without anything that pins one device (a fingerprint or a seed).
 */
export interface ProfileTemplateSettings {
  engine: "camoufox" | "cloakbrowser" | "wayfern";
  /** A stored proxy's id, or `vpn-<id>` for a VPN, as the dialog selects it. */
  proxyId?: string;
  extensionGroupId?: string;
  ephemeral: boolean;
  dnsBlocklist: string;
  launchHook: string;
  camoufoxConfig: CamoufoxConfig;
  cloakbrowserConfig: CloakBrowserConfig;
  wayfernConfig: WayfernConfig;
}

interface ProfileTemplate {
  id: string;
  name: string;
  settings: ProfileTemplateSettings;
  updated_at: number;
}

interface ProfileTemplateMenuProps {
  /** The dialog is open: the list is read again each time it opens. */
  isOpen: boolean;
  disabled?: boolean;
  /** The dialog's pill styling, for the chosen and the resting state. */
  pillClassName: (on: boolean) => string;
  /** The dialog's settings as they stand, ready to be kept. */
  capture: () => ProfileTemplateSettings;
  onApply: (settings: ProfileTemplateSettings) => void;
}

/** Saved sets of the create dialog's choices: apply one, keep a new one (fork). */
export function ProfileTemplateMenu({
  isOpen,
  disabled,
  pillClassName,
  capture,
  onApply,
}: ProfileTemplateMenuProps) {
  const { t } = useTranslation();
  const nameId = useId();
  const [open, setOpen] = useState(false);
  const [templates, setTemplates] = useState<ProfileTemplate[]>([]);
  const [appliedName, setAppliedName] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    if (!isOpen) {
      setOpen(false);
      setAppliedName(null);
      setName("");
      return;
    }
    void invoke<ProfileTemplate[]>("list_profile_templates")
      .then(setTemplates)
      .catch((err: unknown) => {
        console.error("Failed to load profile templates:", err);
      });
  }, [isOpen]);

  const handleSave = useCallback(async () => {
    const wanted = name.trim();
    if (!wanted) return;
    setIsSaving(true);
    try {
      setTemplates(
        await invoke<ProfileTemplate[]>("save_profile_template", {
          name: wanted,
          settings: capture(),
        }),
      );
      setAppliedName(wanted);
      setName("");
      showSuccessToast(t("createProfile.templates.saved", { name: wanted }));
    } catch (err) {
      showErrorToast(translateBackendError(t, err));
    } finally {
      setIsSaving(false);
    }
  }, [capture, name, t]);

  const handleDelete = useCallback(
    async (template: ProfileTemplate) => {
      try {
        setTemplates(
          await invoke<ProfileTemplate[]>("delete_profile_template", {
            id: template.id,
          }),
        );
        setAppliedName((current) =>
          current === template.name ? null : current,
        );
      } catch (err) {
        showErrorToast(translateBackendError(t, err));
      }
    },
    [t],
  );

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          id="profile-template"
          type="button"
          aria-label={t("createProfile.templates.label")}
          className={pillClassName(appliedName !== null)}
          disabled={disabled}
        >
          <LuLayoutTemplate aria-hidden="true" />
          <span className="truncate">
            {appliedName ?? t("createProfile.templates.label")}
          </span>
          <LuChevronDown
            className={cn(
              "-mr-0.5 opacity-60 transition-transform duration-200 motion-reduce:transition-none",
              open && "rotate-180",
            )}
            aria-hidden="true"
          />
        </button>
      </PopoverTrigger>
      <PopoverContent
        className="w-72 space-y-2 p-2"
        align="start"
        sideOffset={6}
      >
        {templates.length === 0 ? (
          <p className="px-1 py-1.5 text-xs text-muted-foreground">
            {t("createProfile.templates.none")}
          </p>
        ) : (
          <ul className="max-h-48 space-y-0.5 overflow-y-auto">
            {templates.map((template) => (
              <li key={template.id} className="group flex items-center gap-1">
                <button
                  type="button"
                  data-template-name={template.name}
                  className={cn(
                    "min-w-0 flex-1 cursor-pointer truncate rounded-md px-2 py-1.5 text-left text-sm outline-none transition-colors duration-100 hover:bg-foreground/5 focus-visible:bg-foreground/5",
                    template.name === appliedName &&
                      "bg-primary/10 text-foreground",
                  )}
                  onClick={() => {
                    onApply(template.settings);
                    setAppliedName(template.name);
                    setOpen(false);
                  }}
                >
                  {template.name}
                </button>
                <button
                  type="button"
                  aria-label={t("createProfile.templates.deleteAria", {
                    name: template.name,
                  })}
                  className="grid size-7 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none transition-colors duration-100 hover:bg-destructive/10 hover:text-destructive-text focus-visible:bg-destructive/10 focus-visible:text-destructive-text"
                  onClick={() => void handleDelete(template)}
                >
                  <LuTrash2 className="size-3.5" aria-hidden="true" />
                </button>
              </li>
            ))}
          </ul>
        )}
        <div className="flex items-center gap-2 border-t border-border pt-2">
          <label htmlFor={nameId} className="sr-only">
            {t("createProfile.templates.namePlaceholder")}
          </label>
          <Input
            id={nameId}
            variant="soft"
            value={name}
            maxLength={80}
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => {
              setName(event.target.value);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.nativeEvent.isComposing) {
                event.preventDefault();
                event.stopPropagation();
                void handleSave();
              }
            }}
            placeholder={t("createProfile.templates.namePlaceholder")}
            className="h-8 text-sm"
          />
          <LoadingButton
            size="sm"
            isLoading={isSaving}
            disabled={name.trim().length === 0}
            onClick={() => void handleSave()}
          >
            {t("common.buttons.save")}
          </LoadingButton>
        </div>
        <p className="px-1 text-xs text-muted-foreground">
          {t("createProfile.templates.hint")}
        </p>
      </PopoverContent>
    </Popover>
  );
}
