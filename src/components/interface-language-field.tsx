"use client";

import { useId } from "react";
import { useTranslation } from "react-i18next";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

const FOLLOW_PROFILE = "profile";

function languageName(code: string, displayIn: string): string {
  try {
    return (
      new Intl.DisplayNames([displayIn], { type: "language" }).of(code) ?? code
    );
  } catch {
    return code;
  }
}

interface InterfaceLanguageFieldProps {
  /** The stored interface language; unset follows the profile's own language. */
  value?: string | null;
  onChange: (value: string | undefined) => void;
  disabled?: boolean;
  labelClassName?: string;
}

/**
 * The language of the browser's own menus (fork): the profile's language, or
 * this app's when the user would rather read the menus than hide the choice.
 */
export function InterfaceLanguageField({
  value,
  onChange,
  disabled,
  labelClassName,
}: InterfaceLanguageFieldProps) {
  const { t, i18n } = useTranslation();
  const id = useId();
  const appLanguage = i18n.language;
  const chosen = value || FOLLOW_PROFILE;
  // A profile set up under another app language keeps the language it was given.
  const languages =
    value && value !== appLanguage ? [appLanguage, value] : [appLanguage];

  return (
    <div className="min-w-0 space-y-1.5">
      <Label htmlFor={id} className={labelClassName}>
        {t("interfaceLanguage.label")}
      </Label>
      <Select
        value={chosen}
        onValueChange={(next) => {
          onChange(next === FOLLOW_PROFILE ? undefined : next);
        }}
        disabled={disabled}
      >
        <SelectTrigger id={id} data-interface-language={chosen}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={FOLLOW_PROFILE}>
            {t("interfaceLanguage.profile")}
          </SelectItem>
          {languages.map((code) => (
            <SelectItem key={code} value={code}>
              {languageName(code, appLanguage)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <p className="text-xs text-muted-foreground">
        {chosen === FOLLOW_PROFILE
          ? t("interfaceLanguage.profileHint")
          : t("interfaceLanguage.otherHint")}
      </p>
    </div>
  );
}
