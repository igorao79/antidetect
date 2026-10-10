"use client";

import { useId } from "react";
import { useTranslation } from "react-i18next";
import { LuRefreshCw } from "react-icons/lu";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";
import type {
  CamoufoxOS,
  CloakBrowserConfig,
  CloakBrowserWebRtcMode,
} from "@/types";

/** The range CloakBrowser's own wrapper draws seeds from. */
const SEED_MIN = 10000;
const SEED_MAX = 99999;

export function randomCloakBrowserSeed(): number {
  return Math.floor(Math.random() * (SEED_MAX - SEED_MIN + 1)) + SEED_MIN;
}

const OS_OPTIONS: CamoufoxOS[] = ["windows", "macos", "linux"];
const WEBRTC_OPTIONS: CloakBrowserWebRtcMode[] = ["auto", "real", "block"];

const FIELD_LABEL_CLASS = "text-xs font-medium text-muted-foreground";

interface CloakBrowserConfigFormProps {
  config: CloakBrowserConfig;
  onConfigChange: (key: keyof CloakBrowserConfig, value: unknown) => void;
  className?: string;
  readOnly?: boolean;
}

function numberOrNull(value: string): number | null {
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
}

export function CloakBrowserConfigForm({
  config,
  onConfigChange,
  className,
  readOnly = false,
}: CloakBrowserConfigFormProps) {
  const { t } = useTranslation();
  const id = useId();
  const text = (key: keyof CloakBrowserConfig) => ({
    value: (config[key] as string | null | undefined) ?? "",
    onChange: (event: React.ChangeEvent<HTMLInputElement>) =>
      onConfigChange(key, event.target.value.trim() || null),
    disabled: readOnly,
    variant: "soft" as const,
  });
  const number = (key: keyof CloakBrowserConfig) => ({
    type: "number" as const,
    min: 1,
    value: (config[key] as number | null | undefined) ?? "",
    onChange: (event: React.ChangeEvent<HTMLInputElement>) =>
      onConfigChange(key, numberOrNull(event.target.value)),
    disabled: readOnly,
    variant: "soft" as const,
  });

  return (
    <div className={cn("space-y-4", className)}>
      <div className="grid gap-4 sm:grid-cols-2">
        <div className="min-w-0 space-y-1.5">
          <Label htmlFor={`${id}-os`} className={FIELD_LABEL_CLASS}>
            {t("config.wayfern.os.title")}
          </Label>
          <Select
            value={config.os ?? "macos"}
            onValueChange={(value) => onConfigChange("os", value)}
            disabled={readOnly}
          >
            <SelectTrigger id={`${id}-os`} variant="soft" className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {OS_OPTIONS.map((os) => (
                <SelectItem key={os} value={os}>
                  {t(`config.wayfern.os.${os}`)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="min-w-0 space-y-1.5">
          <Label htmlFor={`${id}-seed`} className={FIELD_LABEL_CLASS}>
            {t("cloakbrowser.seed")}
          </Label>
          <div className="flex gap-2">
            <Input
              id={`${id}-seed`}
              {...number("seed")}
              placeholder={t("cloakbrowser.seedPlaceholder")}
            />
            <Button
              type="button"
              variant="soft"
              size="icon"
              aria-label={t("cloakbrowser.newSeed")}
              title={t("cloakbrowser.newSeed")}
              disabled={readOnly}
              onClick={() => onConfigChange("seed", randomCloakBrowserSeed())}
            >
              <LuRefreshCw className="size-4" />
            </Button>
          </div>
        </div>
      </div>
      <p className="text-xs text-muted-foreground">
        {t("cloakbrowser.seedDescription")}
      </p>

      <div className="grid gap-4 sm:grid-cols-2">
        <div className="min-w-0 space-y-1.5">
          <Label htmlFor={`${id}-timezone`} className={FIELD_LABEL_CLASS}>
            {t("cloakbrowser.timezone")}
          </Label>
          <Input
            id={`${id}-timezone`}
            {...text("timezone")}
            placeholder={t("cloakbrowser.followsProxy")}
          />
        </div>
        <div className="min-w-0 space-y-1.5">
          <Label htmlFor={`${id}-locale`} className={FIELD_LABEL_CLASS}>
            {t("cloakbrowser.locale")}
          </Label>
          <Input
            id={`${id}-locale`}
            {...text("locale")}
            placeholder={t("cloakbrowser.followsProxy")}
          />
        </div>
      </div>

      <div className="min-w-0 space-y-1.5">
        <Label htmlFor={`${id}-webrtc`} className={FIELD_LABEL_CLASS}>
          {t("cloakbrowser.webrtc.label")}
        </Label>
        <Select
          value={config.webrtc_mode ?? "auto"}
          onValueChange={(value) => onConfigChange("webrtc_mode", value)}
          disabled={readOnly}
        >
          <SelectTrigger id={`${id}-webrtc`} variant="soft" className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {WEBRTC_OPTIONS.map((mode) => (
              <SelectItem key={mode} value={mode}>
                {t(`cloakbrowser.webrtc.${mode}`)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <div className="space-y-2">
        <div className="flex items-center gap-2">
          <Checkbox
            id={`${id}-noise`}
            checked={config.noise !== false}
            onCheckedChange={(checked) =>
              onConfigChange("noise", checked === true ? null : false)
            }
            disabled={readOnly}
          />
          <Label htmlFor={`${id}-noise`}>{t("cloakbrowser.noise")}</Label>
        </div>
        <div className="flex items-center gap-2">
          <Checkbox
            id={`${id}-randomize`}
            checked={config.randomize_fingerprint_on_launch === true}
            onCheckedChange={(checked) =>
              onConfigChange(
                "randomize_fingerprint_on_launch",
                checked === true ? true : null,
              )
            }
            disabled={readOnly}
          />
          <Label htmlFor={`${id}-randomize`}>
            {t("config.wayfern.fingerprint.randomize")}
          </Label>
        </div>
      </div>

      <div className="space-y-3">
        <div>
          <p className="text-sm font-medium">
            {t("cloakbrowser.overrides.title")}
          </p>
          <p className="text-xs text-muted-foreground">
            {t("cloakbrowser.overrides.description")}
          </p>
        </div>
        <div className="grid gap-4 sm:grid-cols-2">
          <div className="min-w-0 space-y-1.5">
            <Label htmlFor={`${id}-gpu-vendor`} className={FIELD_LABEL_CLASS}>
              {t("cloakbrowser.overrides.gpuVendor")}
            </Label>
            <Input id={`${id}-gpu-vendor`} {...text("gpu_vendor")} />
          </div>
          <div className="min-w-0 space-y-1.5">
            <Label htmlFor={`${id}-gpu-renderer`} className={FIELD_LABEL_CLASS}>
              {t("cloakbrowser.overrides.gpuRenderer")}
            </Label>
            <Input id={`${id}-gpu-renderer`} {...text("gpu_renderer")} />
          </div>
          <div className="min-w-0 space-y-1.5">
            <Label htmlFor={`${id}-cores`} className={FIELD_LABEL_CLASS}>
              {t("cloakbrowser.overrides.cpuCores")}
            </Label>
            <Input id={`${id}-cores`} {...number("hardware_concurrency")} />
          </div>
          <div className="min-w-0 space-y-1.5">
            <Label htmlFor={`${id}-memory`} className={FIELD_LABEL_CLASS}>
              {t("cloakbrowser.overrides.memory")}
            </Label>
            <Input id={`${id}-memory`} {...number("device_memory")} />
          </div>
          <div className="min-w-0 space-y-1.5">
            <Label htmlFor={`${id}-screen-w`} className={FIELD_LABEL_CLASS}>
              {t("cloakbrowser.overrides.screenWidth")}
            </Label>
            <Input id={`${id}-screen-w`} {...number("screen_width")} />
          </div>
          <div className="min-w-0 space-y-1.5">
            <Label htmlFor={`${id}-screen-h`} className={FIELD_LABEL_CLASS}>
              {t("cloakbrowser.overrides.screenHeight")}
            </Label>
            <Input id={`${id}-screen-h`} {...number("screen_height")} />
          </div>
        </div>
      </div>
    </div>
  );
}
